//! The transport-independent part of a WebTiles connection: one text frame from the server to
//! the messages it carries, in order. A socket (any socket) hands each frame here and each
//! message to [`crate::reduce`]; a `ping` is answered with [`crate::cm::pong`] and not passed on,
//! as connection.ts does. Compressed (binary) frames are not decoded: ask for the
//! `no-compression` subprotocol.

use std::fmt;

use serde_json::Value;

/// Why a frame gave no messages. The texts are the diagnostics connection.ts reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameError {
    /// The frame does not start with `{`; the first 200 characters of it.
    NotJson(String),
    /// The frame starts like JSON but does not parse; the first 200 characters of it.
    Parse(String),
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FrameError::NotJson(t) => write!(f, "non-JSON message ignored: {t}"),
            FrameError::Parse(t) => write!(f, "JSON parse error: {t}"),
        }
    }
}

impl std::error::Error for FrameError {}

fn head(text: &str) -> String {
    text.chars().take(200).collect()
}

/// A server text frame to its messages: the `msgs` list of a `{"msgs": [...]}` batch, in order,
/// or the frame itself when it is a single message.
pub fn unpack_frame(text: &str) -> Result<Vec<Value>, FrameError> {
    if !text.starts_with('{') {
        return Err(FrameError::NotJson(head(text)));
    }
    let mut obj: Value = serde_json::from_str(text).map_err(|_| FrameError::Parse(head(text)))?;
    match obj.get_mut("msgs").map(Value::take) {
        Some(Value::Array(msgs)) => Ok(msgs),
        Some(other) => {
            // not a batch after all: the message as it came
            if let Some(o) = obj.as_object_mut() {
                o.insert("msgs".into(), other);
            }
            Ok(vec![obj])
        }
        None => Ok(vec![obj]),
    }
}

/// Whether a message is the server's keep-alive, which the client answers with
/// [`crate::cm::pong`] and nothing else sees.
pub fn is_ping(m: &Value) -> bool {
    m.get("msg").and_then(Value::as_str) == Some("ping")
}
