use axum::{
    Json,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use rand::prelude::IndexedRandom;
use ring::signature::{ED25519, UnparsedPublicKey};
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

use super::Error;

const PING: u8 = 1;
const APPLICATION_COMMAND: u8 = 2;
const PONG: u8 = 1;
const CHANNEL_MESSAGE_WITH_SOURCE: u8 = 4;
const TEXT_DISPLAY: u8 = 10;
const IS_COMPONENTS_V2: u32 = 1 << 15;
const EMOJIS: &[&str] = &["😄", "🤓", "🤖", "🌊", "✨"];

#[derive(Clone, Default)]
pub struct SigningKey(Option<[u8; 32]>);

impl SigningKey {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let Some(key) = std::env::var_os("DISCORD_INTERACTIONS_PUBLIC_KEY") else {
            return Ok(Self::default());
        };
        let mut bytes = [0; 32];
        hex::decode_to_slice(key.to_str().ok_or("Expected a hex public key")?, &mut bytes)?;
        Ok(Self(Some(bytes)))
    }

    fn verify(&self, headers: &HeaderMap, body: &[u8]) -> Result<(), Error> {
        let rejected = || Error(StatusCode::UNAUTHORIZED, 0, "Invalid request signature");
        let key = self.0.ok_or_else(rejected)?;
        let timestamp = headers.get("x-signature-timestamp").ok_or_else(rejected)?;
        let signature = headers.get("x-signature-ed25519").ok_or_else(rejected)?;
        let mut parsed = [0; 64];
        hex::decode_to_slice(signature.as_bytes(), &mut parsed).map_err(|_| rejected())?;
        let mut signed = Vec::with_capacity(timestamp.as_bytes().len() + body.len());
        signed.extend_from_slice(timestamp.as_bytes());
        signed.extend_from_slice(body);
        UnparsedPublicKey::new(&ED25519, key)
            .verify(&signed, &parsed)
            .map_err(|_| rejected())
    }
}

#[derive(Deserialize)]
pub struct Interaction {
    #[serde(rename = "type")]
    kind: u8,
    data: Option<Command>,
    #[serde(default)]
    locale: String,
}

#[derive(Deserialize)]
struct Command {
    name: String,
}

#[derive(Serialize)]
pub struct Callback {
    #[serde(rename = "type")]
    kind: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<CallbackData>,
}

#[derive(Serialize)]
struct CallbackData {
    flags: u32,
    components: [TextDisplay; 1],
}

#[derive(Serialize)]
struct TextDisplay {
    #[serde(rename = "type")]
    kind: u8,
    content: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
struct Messages {
    test_greeting: String,
}

static ENGLISH: LazyLock<Messages> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../locales/en-US.messages.json"))
        .expect("English messages")
});
static SPANISH: LazyLock<Messages> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../locales/es-ES.messages.json"))
        .expect("Spanish messages")
});

pub fn dispatch(event: Interaction) -> Result<Json<Callback>, Error> {
    match event.kind {
        PING => Ok(Json(Callback {
            kind: PONG,
            data: None,
        })),
        APPLICATION_COMMAND if event.data.is_some_and(|data| data.name == "test") => {
            let messages = match event.locale.split('-').next() {
                Some("es") => &*SPANISH,
                _ => &*ENGLISH,
            };
            let emoji = EMOJIS
                .choose(&mut rand::rng())
                .expect("Non-empty emoji list");
            Ok(Json(Callback {
                kind: CHANNEL_MESSAGE_WITH_SOURCE,
                data: Some(CallbackData {
                    flags: IS_COMPONENTS_V2,
                    components: [TextDisplay {
                        kind: TEXT_DISPLAY,
                        content: messages.test_greeting.replace("{emoji}", emoji),
                    }],
                }),
            }))
        }
        APPLICATION_COMMAND => Err(Error(StatusCode::BAD_REQUEST, 0, "Unknown command")),
        _ => Err(Error(
            StatusCode::BAD_REQUEST,
            0,
            "Unknown interaction type",
        )),
    }
}

pub async fn webhook(
    State(key): State<SigningKey>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Callback>, Error> {
    key.verify(&headers, &body)?;
    let event = serde_json::from_slice(&body).map_err(|_| super::invalid())?;
    dispatch(event)
}
