use std::env;
use std::sync::{Arc, Mutex};
use std::collections::HashSet;

use dotenv::dotenv;
use serde::{Deserialize, Serialize};
use socketioxide::{
    extract::{Data, SocketRef},
    SocketIo,
};
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, services::ServeDir};
use tracing::info;
use tracing_subscriber::FmtSubscriber;
use bcrypt::verify;

#[derive(Debug, Clone)]
struct SharedState {
    db: sqlx::PgPool,
    users: Arc<Mutex<HashSet<String>>>,
    batch_size: i32,
}

#[derive(Debug, Clone)]
struct Username(String);

#[derive(Deserialize, Serialize)]
struct LoginData {
    nick: String,
    password: String,
}

#[derive(Deserialize)]
struct SendMsgData {
    m: String,
}

#[derive(Deserialize)]
struct TypingData {
    status: bool,
}

#[derive(Deserialize)]
struct LoadMoreMessagesData {
    last: Option<i32>,
}

#[derive(Serialize)]
struct StartEvent {
    users: Vec<String>,
}

#[derive(Serialize)]
struct UserEvent {
    nick: String,
}

#[derive(Serialize)]
struct MessageEvent {
    f: String,
    m: serde_json::Value,
    id: i32,
    time: i64,
}

#[derive(Serialize)]
struct PreviousMsgEvent {
    msgs: Vec<MessageEvent>,
}

#[derive(Serialize)]
struct ForceLoginEvent {
    #[serde(rename = "type")]
    error_type: String,
    message: String,
}

async fn on_login(s: SocketRef, Data(data): Data<LoginData>) {
    let state = s.extensions.get::<Arc<SharedState>>().unwrap().clone();
    let nick = data.nick.trim().to_string();
    let password = data.password.trim().to_string();

    if nick.is_empty() {
        s.emit("force-login", &ForceLoginEvent {
            error_type: "login".to_string(),
            message: "Nick can't be empty.".to_string(),
        }).ok();
        return;
    }
    
    match sqlx::query!(
        "SELECT username, password_hash, view_history FROM users WHERE username = $1",
        nick
        )
        .fetch_optional(&state.db)
        .await
    {
        Ok(Some(row)) => {
            let password_hash: String = row.password_hash;
            
            if verify(&password, &password_hash).unwrap_or(false) {
                let mut users = state.users.lock().unwrap();
                let is_new = users.insert(nick.clone());
                
                s.extensions.insert(Username(nick.clone()));
                s.join("main");
                
                s.emit("start", &StartEvent {
                    users: users.iter().cloned().collect(),
                }).ok();
                
                /*
                if is_new {
                    s.broadcast().to("main").emit("ue", &UserEvent { nick: nick.clone() }).await.ok();
                }
                
                let view_history: bool = row.view_history;
                if view_history {
                    if let Ok(rows) = sqlx::query("SELECT username, message, sent_at, id FROM messages ORDER BY id DESC LIMIT $1")
                        .bind(state.batch_size)
                        .fetch_all(&state.db)
                        .await
                    {
                        let msgs: Vec<MessageEvent> = rows.into_iter().filter_map(|row| {
                            let message: String = row.get("message");
                            serde_json::from_str(&message).ok().map(|m| MessageEvent {
                                f: row.get("username"),
                                m,
                                id: row.get("id"),
                                time: (row.get::<f64, _>("sent_at") * 1000.0) as i64,
                            })
                        }).collect();
                        
                        s.emit("previous-msg", &PreviousMsgEvent { msgs }).ok();
                    }
                }
                */
            } else {
                s.emit("force-login", &ForceLoginEvent {
                    error_type: "login".to_string(),
                    message: "Invalid credentials.".to_string(),
                }).ok();
            }
        }
        Ok(None) => {
            s.emit("force-login", &ForceLoginEvent {
                error_type: "login".to_string(),
                message: "Invalid credentials.".to_string(),
            }).ok();
        }
        Err(e) => {
            eprintln!("Database error: {}", e);
            s.emit("force-login", &ForceLoginEvent {
                error_type: "login".to_string(),
                message: "Server error during authentication.".to_string(),
            }).ok();
        }
    }
}

async fn on_send_msg(s: SocketRef, Data(data): Data<SendMsgData>) {
    let state = s.extensions.get::<Arc<SharedState>>().unwrap().clone();
    if let Some(Username(nick)) = s.extensions.get::<Username>() {
        let message_json = serde_json::to_string(&data.m).unwrap_or_default();
        
        if let Ok(row) = sqlx::query("INSERT INTO messages (username, message) VALUES ($1, $2) RETURNING id, sent_at")
            .bind(&nick)
            .bind(&message_json)
            .fetch_one(&state.db)
            .await
        {
            let msg = MessageEvent {
                f: nick.clone(),
                m: serde_json::Value::String(data.m),
                id: row.get("id"),
                time: (row.get::<f64, _>("sent_at") * 1000.0) as i64,
            };
            
            s.within("main").emit("new-msg", &msg).await.ok();
        }
    } else {
        s.emit("force-login", &ForceLoginEvent {
            error_type: "auth".to_string(),
            message: "You need to be logged in to send messages.".to_string(),
        }).ok();
    }
}

async fn on_typing(s: SocketRef, Data(data): Data<TypingData>) {
    if let Some(Username(nick)) = s.extensions.get::<Username>() {
        s.broadcast().to("main").emit("typing", &serde_json::json!({
            "status": data.status,
            "nick": nick
        })).await.ok();
    }
}

async fn on_load_more_messages(s: SocketRef, Data(data): Data<LoadMoreMessagesData>) {
    let state = s.extensions.get::<Arc<SharedState>>().unwrap().clone();
    if let Some(Username(_)) = s.extensions.get::<Username>() {
        let query = if let Some(last) = data.last {
            sqlx::query("SELECT username, message, sent_at, id FROM messages WHERE id < $1 ORDER BY id DESC LIMIT $2")
                .bind(last)
                .bind(state.batch_size)
        } else {
            sqlx::query("SELECT username, message, sent_at, id FROM messages ORDER BY id DESC LIMIT $1")
                .bind(state.batch_size)
        };
        
        if let Ok(rows) = query.fetch_all(&state.db).await {
            let msgs: Vec<MessageEvent> = rows.into_iter().filter_map(|row| {
                let message: String = row.get("message");
                serde_json::from_str(&message).ok().map(|m| MessageEvent {
                    f: row.get("username"),
                    m,
                    id: row.get("id"),
                    time: (row.get::<f64, _>("sent_at") * 1000.0) as i64,
                })
            }).collect();
            
            s.emit("older-msgs", &PreviousMsgEvent { msgs }).ok();
        }
    }
}

async fn on_disconnect(s: SocketRef) {
    let state = s.extensions.get::<Arc<SharedState>>().unwrap().clone();
    if let Some(Username(nick)) = s.extensions.remove::<Username>() {
        let mut users = state.users.lock().unwrap();
        users.remove(&nick);
        
        s.to("main").emit("ul", &UserEvent { nick }).await.ok();
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();
    
    let subscriber = FmtSubscriber::new();
    tracing::subscriber::set_global_default(subscriber)?;

    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let db = PgPoolOptions::new()
        .connect(&db_url)
        .await?;

    let batch_size: i32 = match env::var("BATCH_SIZE") {
        Ok(val) => val.parse().unwrap_or(50),
        Err(_) => 50,
    };

    let shared_state = Arc::new(SharedState { // Wrap in Arc
        db,
        users: Arc::new(Mutex::new(HashSet::new())),
        batch_size,
    });

    let (layer, io) = SocketIo::builder()
        .with_state(shared_state)
        .build_layer();

    io.ns("/", |s: SocketRef| {
        s.on("login", on_login);
        s.on("send-msg", on_send_msg);
        s.on("typing", on_typing);
        s.on("load-more-messages", on_load_more_messages);
        s.on_disconnect(on_disconnect);
    });

    let app = axum::Router::new()
        .fallback_service(ServeDir::new("html"))
        .layer(
            ServiceBuilder::new()
                .layer(CorsLayer::permissive())
                .layer(layer),
        );

    let port = std::env::args().nth(1).unwrap_or_else(|| "8090".to_string());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    
    info!("Starting server on port {}", port);
    axum::serve(listener, app).await?;

    Ok(())
}
