use std::collections::HashSet;
use std::env;
use std::ops::Deref;
use std::sync::{Arc, Mutex};

use bcrypt::verify;
use dotenv::dotenv;
use serde::{Deserialize, Serialize};
use socketioxide::{
    extract::{Data, SocketRef, State},
    SocketIo,
};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, services::ServeDir};
use tracing::info;
use tracing_subscriber::FmtSubscriber;

#[derive(Debug, Clone)]
struct SharedState {
    db: sqlx::PgPool,
    users: Arc<Mutex<HashSet<String>>>,
    batch_size: i64,
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
    text: String,
}

#[derive(Deserialize)]
struct LoadMoreMessagesData {
    last: Option<i32>,
}

#[derive(Serialize)]
struct StartEvent<'a, T>
where
    T: IntoIterator<Item = String> + Serialize,
{
    users: &'a T,
}

#[derive(Serialize)]
struct UserEvent<'a> {
    nick: &'a str,
}

#[derive(Debug)]
struct OffsetDateTime(sqlx::types::time::OffsetDateTime);

impl Deref for OffsetDateTime {
    type Target = sqlx::types::time::OffsetDateTime;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<sqlx::types::time::PrimitiveDateTime> for OffsetDateTime {
    fn from(value: sqlx::types::time::PrimitiveDateTime) -> Self {
        Self(value.assume_utc())
    }
}

#[derive(Serialize, Debug, sqlx::FromRow)]
struct MessageEvent {
    #[serde(rename = "f")]
    username: String,
    #[serde(rename = "m")]
    message: String,
    id: i32,

    #[serde(with = "time::serde::timestamp::milliseconds")]
    #[serde(rename = "time")]
    sent_at: OffsetDateTime,
}

#[derive(Serialize)]
struct PreviousMsgEvent<'a, T>
where
    T: IntoIterator<Item = MessageEvent>,
{
    msgs: &'a T,
}


async fn on_login(
    s: SocketRef,
    Data(data): Data<LoginData>,
    State(state): State<Arc<SharedState>>,
) {
    let nick = data.nick.trim();
    let password = data.password.trim();

    if nick.is_empty() || password.is_empty() {
        s.emit(
            "force-login",
             "Nick or password can't be empty.",
        )
        .ok();
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

            if !verify(password, &password_hash).unwrap_or(false) {
                s.emit(
                    "force-login",
                     "Invalid credentials.",
                )
                .ok();
            } else {
                let mut is_new = false;
                if let Ok(mut users) = state.users.lock() {
                    is_new = users.insert(nick.to_string());

                    s.extensions.insert(Username(nick.to_string()));
                    s.join("main");

                    s.emit("start", &StartEvent { users: &*users }).ok();
                }

                if is_new {
                    s.to("main")
                        .emit("ue", &UserEvent { nick: nick })
                        .await
                        .ok();
                }

                let view_history: bool = row.view_history;
                if view_history {
                    let rows_query = sqlx::query_as!(
                        MessageEvent,
                        "SELECT username, message, sent_at, id 
                        FROM messages 
                        ORDER BY id 
                        DESC LIMIT $1", state.batch_size
                    ).fetch_all(&state.db).await;

                    if let Ok(msgs) = rows_query {
                        s.emit("previous-msg", 
                            &PreviousMsgEvent { msgs: &msgs })
                            .ok();
                    }
                }
            }
        }
        Ok(None) => {
            s.emit(
                "force-login",
                "Invalid credentials.",
            )
            .ok();
        }
        Err(e) => {
            tracing::error!("Database error: {}", e);
            s.emit(
                "force-login",
                 "Server error during authentication.",
            )
            .ok();
        }
    }
}

async fn on_send_msg(
    s: SocketRef,
    Data(data): Data<SendMsgData>,
    State(state): State<Arc<SharedState>>,
) {
    if let Some(Username(nick)) = s.extensions.get::<Username>() {

        if let Ok(row) = sqlx::query!(
            "INSERT INTO messages (username, message) 
            VALUES ($1, $2) 
            RETURNING id, sent_at",
            nick,
            data.text,
        )
        .fetch_one(&state.db)
        .await
        {
            let msg = MessageEvent {
                username: nick.clone(),
                message: data.text.to_string(),
                id: row.id,
                sent_at: row.sent_at.into(),
            };

            println!("broadcasting the message");

            s.to("main").emit("new-msg", &msg).await.ok();

            s.to("main")
                .emit(
                    "new-msg",
                    &serde_json::json!({
                        "f": nick,
                        "m": data.text, 
                        "time": row.sent_at.assume_utc().unix_timestamp() * 1000 
                            + row.sent_at.assume_utc().millisecond() as i64,
                        "id": row.id
                    }),
                )
                .await
                .ok();

            s.emit(
                    "new-msg",
                    &serde_json::json!({
                        "f": nick,
                        "m": data.text, 
                        "time": row.sent_at.assume_utc().unix_timestamp() * 1000 
                            + row.sent_at.assume_utc().millisecond() as i64,
                        "id": row.id
                    }),
                )
                .ok();
        }
    } else {
        s.emit(
            "force-login",
             "You need to be logged in to send messages.",
        )
        .ok();
    }
}

async fn on_typing(s: SocketRef, Data(data): Data<bool>) {
    if let Some(Username(nick)) = s.extensions.get::<Username>() {
        s.broadcast()
            .to("main")
            .emit(
                "typing",
                &serde_json::json!({
                    "status": data,
                    "nick": nick
                }),
            )
            .await
            .ok();
    }
}

async fn on_load_more_messages(
    s: SocketRef,
    Data(data): Data<LoadMoreMessagesData>,
    State(state): State<Arc<SharedState>>,
) {
    if let Some(Username(_)) = s.extensions.get::<Username>() {
        let rows_query = if let Some(last) = data.last {
            sqlx::query_as!(
                MessageEvent,
                "SELECT username, message, sent_at, id 
                FROM messages WHERE id < $1 
                ORDER BY sent_at
                DESC LIMIT $2",
                last,
                state.batch_size
            )
            .fetch_all(&state.db)
            .await
        } else {
            sqlx::query_as!(
                MessageEvent,
                "SELECT username, message, sent_at, id 
                FROM messages 
                ORDER BY sent_at
                DESC LIMIT $1",
                state.batch_size
            )
            .fetch_all(&state.db)
            .await
        };

        if let Ok(msgs) = rows_query {
            s.emit("older-msgs", &PreviousMsgEvent { msgs: &msgs }).ok();
        } else {
            tracing::error!("Database error: {}", rows_query.unwrap_err());
            return;
        }
    }
}

async fn on_disconnect(s: SocketRef, State(state): State<Arc<SharedState>>) {
    if let Some(Username(nick)) = s.extensions.remove::<Username>() {
        if let Ok(mut users) = state.users.lock() {
            users.remove(&nick);
        }

        s.to("main")
            .emit(
                "ul",
                &UserEvent {
                    nick: nick.as_ref(),
                },
            )
            .await
            .ok();
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();

    let subscriber = FmtSubscriber::builder()
        .with_max_level(tracing::Level::DEBUG)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let db = PgPoolOptions::new().connect(&db_url).await?;

    let batch_size: i64 = match env::var("BATCH_SIZE") {
        Ok(val) => val.parse().unwrap_or(50),
        Err(_) => 50,
    };

    let shared_state = Arc::new(SharedState {
        // Wrap in Arc
        db,
        users: Arc::new(Mutex::new(HashSet::new())),
        batch_size,
    });

    let (layer, io) = SocketIo::builder().with_state(shared_state).build_layer();

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

    let port = std::env::args().nth(1).unwrap_or("8090".to_string());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;

    info!("Starting server on port {}", port);
    axum::serve(listener, app).await?;

    Ok(())
}
