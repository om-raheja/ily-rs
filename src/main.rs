use std::collections::HashSet;
use std::env;
use std::ops::Deref;
use std::sync::{Arc, Mutex};

use bcrypt::verify;
use dashmap::DashMap;
use dotenv::dotenv;
use serde::{Deserialize, Serialize};
use socketioxide::{
    extract::{Data, SocketRef, State},
    SocketIo,
};
use sqlx::postgres::PgPoolOptions;
use tokio::net::{TcpListener, UnixListener};
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, services::ServeDir};
use tracing::{error, info};
use tracing_subscriber::FmtSubscriber;

#[derive(Debug, Clone)]
struct SocketData {
    nick: String,
    channels: Vec<String>,
}

#[derive(Debug, Clone)]
struct SharedState {
    db: sqlx::PgPool,
    users: Arc<DashMap<String, Arc<Mutex<HashSet<String>>>>>,
    batch_size: i64,
}

#[derive(Deserialize, Serialize)]
struct LoginData {
    nick: String,
    password: String,
}

#[derive(Deserialize)]
struct SendMsgData {
    text: String,
    channel: String,
}

#[derive(Deserialize)]
struct TypingData {
    channel: String,
    typing: bool,
}

#[derive(Deserialize)]
struct LoadMoreMessagesData {
    last: Option<i32>,
    channel: String,
}

#[derive(Serialize)]
struct StartEvent<'a, T>
where
    T: IntoIterator<Item = String> + Serialize,
{
    users: &'a T,
    channel: String,
}

#[derive(Serialize)]
struct UserEvent<'a> {
    nick: &'a str,
    channel: String,
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

    channel: String,
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
        "SELECT password_hash, view_history, channels FROM users WHERE username = $1",
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
                for channel in &row.channels {
                    let mut is_new = false;
                    // Use entry API to ensure the channel exists, then clone the Arc
                    let users_arc = state.users
                        .entry(channel.clone())
                        .or_insert_with(|| Arc::new(Mutex::new(HashSet::new())))
                        .clone(); // Clone the Arc to avoid holding the DashMap lock

                    // Lock the Mutex from the cloned Arc
                    if let Ok(mut users) = users_arc.lock() {
                        is_new = users.insert(nick.to_string());
                        s.emit("start", &StartEvent { 
                            users: &*users, 
                            channel: channel.to_string() }).ok();
                    }

                    if is_new {
                        let msg = UserEvent {
                            nick: data.nick.as_ref(),
                            channel: channel.to_string(),
                        };
                        match serde_json::to_string(&msg) {
                            Ok(json_string) => {
                                if let Err(err) = s.to(channel.to_string())
                                    .emit("ue", &json_string).await {
                                    error!("Failed to send message: {}", err);
                                    s.emit("force-login", "Failed to send message.").ok();
                                } 
                                s.emit("ue", &json_string).ok();
                            }
                            Err(err) => {
                                error!("Failed to serialize previous messages: {}", err);
                            }
                        }
                    }
                }

                println!("Viewing history");
                let rows_query = sqlx::query_as!(
                    MessageEvent,
                    "
                    WITH user_channels AS (
                        SELECT 
                            unnest(channels) as channel,
                            unnest(view_history) as can_view
                        FROM users 
                        WHERE username = $1
                    ),
                    allowed_channels AS (
                        SELECT channel 
                        FROM user_channels 
                        WHERE can_view = true
                    )
                    SELECT 
                        username, message, sent_at, id, channel
                    FROM 
                        messages 
                    WHERE 
                        channel IN (SELECT channel FROM allowed_channels)
                    ORDER BY 
                        id DESC 
                    LIMIT 
                        $2
                    ",
                    nick,               // Username to look up permissions for
                    state.batch_size
                ).fetch_all(&state.db).await;

                if let Ok(msgs) = rows_query {
                    match serde_json::to_string(&msgs) {
                        Ok(json_string) => {
                            if let Err(err) = 
                                s.emit("previous-msg", &json_string) {

                                error!("Failed to send previous messages: {}", err);
                            }
                        }
                        Err(err) => {
                            error!("Failed to serialize previous messages: {}", err);
                        }
                    }
                }

                s.extensions.insert(SocketData {
                    nick: nick.to_string(),
                    channels: row.channels,
                });

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
            error!("Database error: {}", e);
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
    Data(payload): Data<SendMsgData>,
    State(state): State<Arc<SharedState>>,
) {
    if let Some(data) = s.extensions.get::<SocketData>() {

        if let Ok(row) = sqlx::query!(
            "INSERT INTO messages (username, message, channel) 
            VALUES ($1, $2, $3) 
            RETURNING id, sent_at",
            data.nick,
            payload.text,
            payload.channel,
        )
        .fetch_one(&state.db)
        .await
        {
            let msg = MessageEvent {
                username: data.nick,
                message: payload.text.to_string(),
                id: row.id,
                sent_at: row.sent_at.into(),
                channel: payload.channel.clone(),
            };

            println!("broadcasting the message");

            match serde_json::to_string(&msg) {
                Ok(json_string) => {
                    if let Err(err) = s.to(payload.channel.clone())
                        .emit("new-msg", &json_string).await {
                        error!("Failed to send message: {}", err);
                        s.emit("force-login", "Failed to send message.").ok();
                    } 
                    s.emit("new-msg", &json_string).ok();
                }
                Err(err) => {
                    error!("Failed to serialize previous messages: {}", err);
                }
            }
        }
    } else {
        s.emit(
            "force-login",
             "You need to be logged in to send messages.",
        )
        .ok();
    }
}

async fn on_typing(s: SocketRef, Data(payload): Data<TypingData>) {
    if let Some(data) = s.extensions.get::<SocketData>() {
        if data.channels.contains(&payload.channel) {
            s.broadcast()
                .to(payload.channel.clone())
                .emit(
                    "typing",
                    &serde_json::json!({
                        "status": payload.typing,
                        "channel": payload.channel.clone(),
                        "nick": data.nick
                    }),
                )
                .await
                .ok();
        }
    }
}

async fn on_load_more_messages(
    s: SocketRef,
    Data(payload): Data<LoadMoreMessagesData>,
    State(state): State<Arc<SharedState>>,
) {
    if let Some(data) = s.extensions.get::<SocketData>() {
        match sqlx::query!(
            "SELECT 
                (view_history[array_position(channels, $2)] = true) 
                AS can_view 
            FROM users 
            WHERE username = $1",
            data.nick, 
            payload.channel,
        )
        .fetch_one(&state.db)
        .await
        {
            Ok(row) => {
                if let None = row.can_view {
                    s.emit("force-login", 
                        "You need to be logged in to view previous messages.")
                        .ok();
                    return;
                }
            }
            Err(e) => {
                error!("Database error: {}", e);
                s.emit("force-login", "Server error during authentication.").ok();
                return;
            }
        }

        let rows_query = if let Some(last) = payload.last {
            sqlx::query_as!(
                MessageEvent,
                "SELECT username, message, sent_at, id, channel
                FROM messages 
                WHERE id < $1 AND channel = $3
                ORDER BY sent_at
                DESC LIMIT $2",
                last,
                state.batch_size,
                payload.channel,
            )
            .fetch_all(&state.db)
            .await
        } else {
            sqlx::query_as!(
                MessageEvent,
                "SELECT username, message, sent_at, id, channel
                FROM messages 
                WHERE channel = $2
                ORDER BY sent_at
                DESC LIMIT $1",
                state.batch_size,
                payload.channel,
            )
            .fetch_all(&state.db)
            .await
        };

        if let Ok(msgs) = rows_query {
            match serde_json::to_string(&msgs) {
                Ok(json_string) => {
                    if let Err(err) = s.emit("older-msgs", &json_string) {
                        error!("Failed to send previous messages: {}", err);
                    }
                }
                Err(err) => {
                    error!("Failed to serialize previous messages: {}", err);
                }
            }
        } else {
            error!("Database error: {}", rows_query.unwrap_err());
            return;
        }
    }
}

async fn on_disconnect(s: SocketRef, State(state): State<Arc<SharedState>>) {
    if let Some(data) = s.extensions.remove::<SocketData>() {
        for entry in state.users.iter() {
            let channel = entry.key();
            let user_set = entry.value();
            
            // Try to lock the Mutex, skip channel if failed
            let mut contains = false;
            if let Ok(mut locked_set) = user_set.lock() {
                contains = locked_set.contains(&data.nick);
                if contains {
                    locked_set.remove(&data.nick);
                } 
            }
            
            if contains {
                // Emit event after releasing the lock
                let msg = UserEvent {
                    nick: data.nick.as_ref(),
                    channel: channel.clone(),
                };
                match serde_json::to_string(&msg) {
                    Ok(json_string) => {
                        if let Err(err) = s.to(channel.clone())
                            .emit("ul", &json_string).await {
                            error!("Failed to send message: {}", err);
                            s.emit("force-login", "Failed to send message.").ok();
                        } 
                        s.emit("ul", &json_string).ok();
                    }
                    Err(err) => {
                        error!("Failed to serialize previous messages: {}", err);
                    }
                }
            }
        } 
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
        db,
        users: Arc::new(DashMap::<String, Arc<Mutex<HashSet<String>>>>::new()),
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


    // Parse command-line arguments
    let args: Vec<String> = std::env::args().collect();
    let mut port = "8090".to_string();
    let mut unix_socket = None;

    let mut i = 1; // Skip program name
    while i < args.len() {
        if args[i] == "--unix" && i + 1 < args.len() {
            unix_socket = Some(args[i + 1].clone());
            i += 2; // Skip both --unix and the socket path
        } else {
            port = args[i].clone();
            i += 1;
        }
    }

    let app = axum::Router::new()
        .fallback_service(ServeDir::new("html"))
        .layer(
            ServiceBuilder::new()
                .layer(CorsLayer::permissive())
                .layer(layer),
        );


    if let Some(socket_path) = unix_socket {
        // delete the file before binding
        tokio::fs::remove_file(&socket_path).await.ok();
        let listener = UnixListener::bind(&socket_path).unwrap();

        info!("Starting server on Unix socket: {}", socket_path);
        axum::serve(listener, app).await?;

    } else {
        let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
        info!("Starting server on port {}", port);
        axum::serve(listener, app).await?;
    }


    Ok(())
}
