#[path = "discord-server/interactions.rs"]
mod interactions;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use discord_text::*;
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Deserialize;
use serde_json::json;
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Clone)]
struct Db(Arc<Mutex<Connection>>);
struct Error(StatusCode, i32, &'static str);
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"code":self.1,"message":self.2}))).into_response()
    }
}
type Result<T> = std::result::Result<T, Error>;
fn internal() -> Error {
    Error(StatusCode::INTERNAL_SERVER_ERROR, 0, "Storage unavailable")
}
impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        eprintln!("{e}");
        internal()
    }
}
impl Db {
    fn lock(&self) -> Result<MutexGuard<'_, Connection>> {
        self.0.lock().map_err(|_| internal())
    }
}
fn invalid() -> Error {
    Error(StatusCode::BAD_REQUEST, 50035, "Invalid form body")
}
fn read_channel(row: &Row) -> rusqlite::Result<Channel> {
    Ok(Channel {
        id: row.get::<_, i64>(0)?.to_string(),
        name: row.get(1)?,
        guild_id: GUILD.into(),
        kind: 0,
    })
}
fn channel(db: &Connection, id: &str) -> Result<Channel> {
    db.query_row(
        "SELECT id,name FROM channels WHERE id=?",
        [id],
        read_channel,
    )
    .optional()?
    .ok_or(Error(StatusCode::NOT_FOUND, 10003, "Unknown Channel"))
}
fn user(id: i64, name: String) -> User {
    User {
        id: id.to_string(),
        username: name,
        avatar: None,
        discriminator: "0".into(),
    }
}
fn identity(db: &Connection, h: &HeaderMap) -> Result<User> {
    let unauthorized = || Error(StatusCode::UNAUTHORIZED, 0, "Unauthorized");
    let token = h
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or_else(unauthorized)?;
    db.query_row("SELECT id,name FROM users WHERE token=?", [token], |r| {
        Ok(user(r.get(0)?, r.get(1)?))
    })
    .optional()?
    .ok_or_else(unauthorized)
}
// SQLite serializes allocation with the enclosing insert transaction across connections.
fn allocate(db: &Connection) -> Result<i64> {
    let floor = (chrono::Utc::now().timestamp_millis() - 1420070400000) << 22;
    Ok(db.query_row(
        "UPDATE counter SET id=max(id+1,?) RETURNING id",
        [floor],
        |r| r.get(0),
    )?)
}
#[derive(Deserialize)]
struct Login {
    username: String,
}
async fn session(State(db): State<Db>, Json(body): Json<Login>) -> Result<Json<Session>> {
    let name = body.username.trim();
    if !(1..=32).contains(&name.chars().count()) {
        return Err(invalid());
    }
    let mut db = db.lock()?;
    let tx = db.transaction()?;
    let id = allocate(&tx)?;
    let token = tx.query_row(
        "INSERT INTO users(id,name,token) VALUES (?,?,lower(hex(randomblob(32)))) RETURNING token",
        params![id, name],
        |r| r.get(0),
    )?;
    tx.commit()?;
    Ok(Json(Session {
        token,
        user: user(id, name.into()),
    }))
}
async fn channels(
    State(db): State<Db>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Vec<Channel>>> {
    let db = db.lock()?;
    identity(&db, &h)?;
    if id != GUILD {
        return Err(Error(StatusCode::NOT_FOUND, 10004, "Unknown Guild"));
    }
    let mut query = db.prepare("SELECT id,name FROM channels ORDER BY id")?;
    Ok(Json(
        query
            .query_map([], read_channel)?
            .collect::<rusqlite::Result<_>>()?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewChannel {
    name: String,
    #[serde(rename = "type", default)]
    kind: u8,
}
async fn create_channel(
    State(db): State<Db>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<NewChannel>,
) -> Result<Json<Channel>> {
    let mut db = db.lock()?;
    identity(&db, &headers)?;
    if id != GUILD {
        return Err(Error(StatusCode::NOT_FOUND, 10004, "Unknown Guild"));
    }
    let name = body
        .name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase();
    if body.kind != 0
        || !(1..=100).contains(&name.chars().count())
        || name.chars().any(char::is_control)
    {
        return Err(invalid());
    }
    let tx = db.transaction()?;
    let channel_id = allocate(&tx)?;
    tx.execute(
        "INSERT INTO channels(id,name) VALUES (?,?)",
        params![channel_id, name],
    )?;
    let result = channel(&tx, &channel_id.to_string())?;
    tx.commit()?;
    Ok(Json(result))
}
fn read_message(r: &Row) -> rusqlite::Result<Message> {
    Ok(Message {
        id: r.get::<_, i64>(0)?.to_string(),
        channel_id: r.get(1)?,
        author: user(r.get(2)?, r.get(3)?),
        content: r.get(4)?,
        timestamp: r.get(5)?,
        edited_timestamp: r.get(6)?,
        tts: false,
        mention_everyone: false,
        mentions: vec![],
        mention_roles: vec![],
        attachments: vec![],
        embeds: vec![],
        pinned: false,
        kind: 0,
    })
}
const SELECT: &str = "SELECT m.id,m.channel,u.id,u.name,m.content,m.created,m.edited FROM messages m JOIN users u ON u.id=m.author WHERE m.channel=?";
fn one(db: &Connection, c: &str, id: &str) -> Result<Message> {
    db.query_row(
        &format!("{SELECT} AND m.id=?"),
        params![c, id],
        read_message,
    )
    .optional()?
    .ok_or(Error(StatusCode::NOT_FOUND, 10008, "Unknown Message"))
}
#[derive(Deserialize)]
struct Page {
    limit: Option<u8>,
    before: Option<u64>,
    after: Option<u64>,
    around: Option<u64>,
}
async fn history(
    State(db): State<Db>,
    h: HeaderMap,
    Path(c): Path<String>,
    Query(q): Query<Page>,
) -> Result<Json<Vec<Message>>> {
    let db = db.lock()?;
    identity(&db, &h)?;
    channel(&db, &c)?;
    let limit = q.limit.unwrap_or(50);
    if !(1..=100).contains(&limit) || [q.before, q.after, q.around].iter().flatten().count() > 1 {
        return Err(invalid());
    }
    let query = |condition: &str, id: u64, n: u8| -> Result<Vec<Message>> {
        // condition is selected from the fixed clauses below. Client values are bound.
        let mut stmt = db.prepare(&format!("{SELECT} AND {condition} LIMIT ?"))?;
        let id = i64::try_from(id).unwrap_or(i64::MAX);
        Ok(stmt
            .query_map(params![c, id, n], read_message)?
            .collect::<rusqlite::Result<_>>()?)
    };
    let older = "m.id < ? ORDER BY m.id DESC";
    let newer = "m.id > ? ORDER BY m.id ASC";
    let rows = if let Some(anchor) = q.around {
        let mut above = query("m.id >= ? ORDER BY m.id ASC", anchor, limit)?;
        let mut below = query(older, anchor, limit)?;
        above.truncate(above.len().min(
            usize::from(limit.div_ceil(2)).max(usize::from(limit).saturating_sub(below.len())),
        ));
        below.truncate(usize::from(limit) - above.len());
        above.reverse();
        above.extend(below);
        above
    } else if let Some(id) = q.after {
        let mut v = query(newer, id, limit)?;
        v.reverse();
        v
    } else {
        query(older, q.before.unwrap_or(u64::MAX), limit)?
    };
    Ok(Json(rows))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Text {
    content: String,
}
fn validate(t: &Text) -> Result<()> {
    if t.content.trim().is_empty() || t.content.chars().count() > 2000 {
        Err(invalid())
    } else {
        Ok(())
    }
}
fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
async fn create(
    State(db): State<Db>,
    h: HeaderMap,
    Path(c): Path<String>,
    Json(t): Json<Text>,
) -> Result<Json<Message>> {
    let mut db = db.lock()?;
    let author = identity(&db, &h)?;
    channel(&db, &c)?;
    validate(&t)?;
    let tx = db.transaction()?;
    let id = allocate(&tx)?;
    tx.execute(
        "INSERT INTO messages(id,channel,author,content,created) VALUES(?,?,?,?,?)",
        params![id, c, author.id, t.content, now()],
    )?;
    let result = one(&tx, &c, &id.to_string())?;
    tx.commit()?;
    Ok(Json(result))
}
async fn fetch(
    State(db): State<Db>,
    h: HeaderMap,
    Path((c, id)): Path<(String, String)>,
) -> Result<Json<Message>> {
    let db = db.lock()?;
    identity(&db, &h)?;
    channel(&db, &c)?;
    Ok(Json(one(&db, &c, &id)?))
}
fn owner(db: &Connection, h: &HeaderMap, c: &str, id: &str) -> Result<()> {
    let u = identity(db, h)?;
    channel(db, c)?;
    if one(db, c, id)?.author.id != u.id {
        return Err(Error(StatusCode::FORBIDDEN, 50013, "Missing Permissions"));
    }
    Ok(())
}
async fn edit(
    State(db): State<Db>,
    h: HeaderMap,
    Path((c, id)): Path<(String, String)>,
    Json(t): Json<Text>,
) -> Result<Json<Message>> {
    let db = db.lock()?;
    owner(&db, &h, &c, &id)?;
    validate(&t)?;
    db.execute(
        "UPDATE messages SET content=?,edited=? WHERE id=?",
        params![t.content, now(), id],
    )?;
    Ok(Json(one(&db, &c, &id)?))
}
async fn remove(
    State(db): State<Db>,
    h: HeaderMap,
    Path((c, id)): Path<(String, String)>,
) -> Result<StatusCode> {
    let db = db.lock()?;
    owner(&db, &h, &c, &id)?;
    db.execute("DELETE FROM messages WHERE id=?", [id])?;
    Ok(StatusCode::NO_CONTENT)
}
async fn preview_interaction(
    State(db): State<Db>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(event): Json<interactions::Interaction>,
) -> Result<Json<interactions::Callback>> {
    let db = db.lock()?;
    identity(&db, &headers)?;
    channel(&db, &id)?;
    interactions::dispatch(event)
}

#[derive(Deserialize)]
#[serde(tag = "cmd", rename_all = "SCREAMING_SNAKE_CASE")]
enum ActivityCommand {
    GetChannel {
        nonce: String,
        args: ActivityChannelArgs,
    },
}
#[derive(Deserialize)]
struct ActivityChannelArgs {
    channel_id: String,
}
async fn activity_rpc(
    State(db): State<Db>,
    headers: HeaderMap,
    Json(command): Json<ActivityCommand>,
) -> Result<Json<serde_json::Value>> {
    let db = db.lock()?;
    identity(&db, &headers)?;
    match command {
        ActivityCommand::GetChannel { nonce, args } => {
            let channel = channel(&db, &args.channel_id)?;
            let mut stmt = db.prepare(&format!("{SELECT} ORDER BY m.id DESC LIMIT 50"))?;
            let messages: Vec<serde_json::Value> = stmt
                .query_map([&channel.id], read_message)?
                .map(|row| -> Result<_> {
                    let message = row?;
                    let mut value = serde_json::to_value(message).map_err(|_| internal())?;
                    value["author"]["flags"] = json!(0);
                    value["author"]["bot"] = json!(false);
                    Ok(value)
                })
                .collect::<Result<_>>()?;
            Ok(Json(json!({
                "cmd": "GET_CHANNEL",
                "evt": null,
                "nonce": nonce,
                "data": {
                    "id": channel.id, "guild_id": channel.guild_id, "type": channel.kind,
                    "name": channel.name, "voice_states": [], "messages": messages
                }
            })))
        }
    }
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let db = Connection::open(
        std::env::var("DISCORD_LOCAL_DB").unwrap_or_else(|_| "discord-local.sqlite".into()),
    )?;
    db.execute_batch("PRAGMA foreign_keys=ON; BEGIN; CREATE TABLE IF NOT EXISTS counter(id INTEGER NOT NULL); INSERT INTO counter SELECT 4194306 WHERE NOT EXISTS(SELECT 1 FROM counter); CREATE TABLE IF NOT EXISTS users(id INTEGER PRIMARY KEY,name TEXT NOT NULL,token TEXT NOT NULL UNIQUE); CREATE TABLE IF NOT EXISTS channels(id INTEGER PRIMARY KEY,name TEXT NOT NULL); INSERT OR IGNORE INTO channels VALUES (4194305,'general'), (4194306,'off-topic'); CREATE TABLE IF NOT EXISTS messages(id INTEGER PRIMARY KEY,channel TEXT NOT NULL,author INTEGER NOT NULL REFERENCES users(id),content TEXT NOT NULL,created TEXT NOT NULL,edited TEXT); CREATE INDEX IF NOT EXISTS history ON messages(channel,id); COMMIT;")?;
    let app = Router::new()
        .route("/local/sessions", post(session))
        .route(
            "/api/v10/guilds/{id}/channels",
            get(channels).post(create_channel),
        )
        .route("/api/v10/channels/{c}/messages", get(history).post(create))
        .route(
            "/api/v10/channels/{c}/messages/{id}",
            get(fetch).patch(edit).delete(remove),
        )
        .route(
            "/local/channels/{id}/interactions",
            post(preview_interaction),
        )
        .route("/local/activities/rpc", post(activity_rpc))
        .with_state(Db(Arc::new(Mutex::new(db))));
    let interaction_routes = Router::new()
        .route("/interactions", post(interactions::webhook))
        .with_state(interactions::SigningKey::load()?);
    let (health_status, health_service) = tonic_health::server::health_reporter();
    health_status
        .set_service_status("discord.local.chat", tonic_health::ServingStatus::Serving)
        .await;
    let app = app
        .merge(interaction_routes)
        .merge(tonic::service::Routes::new(health_service).into_axum_router());
    let port = std::env::var("DISCORD_LOCAL_PORT")
        .unwrap_or_else(|_| "8790".into())
        .parse::<u16>()?;
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    println!("http://{}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
