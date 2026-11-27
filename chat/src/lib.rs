use serde::{Deserialize, Serialize};
pub const GUILD: &str = "4194304";
pub const GENERAL: &str = "4194305";
pub const OFF_TOPIC: &str = "4194306";

#[derive(Deserialize, Serialize, Clone)]
pub struct User {
    pub id: String,
    pub username: String,
    pub discriminator: String,
    pub avatar: Option<String>,
}
#[derive(Deserialize, Serialize, Clone)]
pub struct Message {
    pub id: String,
    pub channel_id: String,
    pub author: User,
    pub content: String,
    pub timestamp: String,
    pub edited_timestamp: Option<String>,
    pub tts: bool,
    pub mention_everyone: bool,
    pub mentions: Vec<User>,
    pub mention_roles: Vec<String>,
    pub attachments: Vec<serde_json::Value>,
    pub embeds: Vec<serde_json::Value>,
    pub pinned: bool,
    #[serde(rename = "type")]
    pub kind: u8,
}
#[derive(Deserialize, Serialize)]
pub struct Session {
    pub token: String,
    pub user: User,
}
#[derive(Deserialize, Serialize, Clone)]
pub struct Channel {
    pub id: String,
    pub guild_id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: u8,
}
pub mod backoff;
