use serde::{Deserialize, Serialize};

/// Tag bytes for message types on the wire.
pub const TAG_USER_INFO: u8 = 0x01;
pub const TAG_TEXT: u8 = 0x02;
pub const TAG_IMAGE: u8 = 0x03;
pub const TAG_VIDEO_FRAME: u8 = 0x04;
pub const TAG_VOICE: u8 = 0x05;

/// Profile/identity payload bundled into a single message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub name: String,
    /// Persistent player UUID (survives name/avatar changes).
    #[serde(default)]
    pub uuid: String,
    /// 128x128 PNG avatar bytes (optional).
    pub avatar: Option<Vec<u8>>,
    /// RGB name color.
    pub name_color: [u8; 3],
    /// Biography text (may be empty).
    pub bio: String,
    /// `true` = this is a reply to someone else's announcement (peer was already here).
    /// `false` = this is a fresh join announcement (new peer entering the session).
    pub is_reply: bool,
}

/// Application-level chat message (after parsing the tag byte).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChatMessage {
    /// Bundled user identity announcement / reply.
    UserInfo(UserInfo),
    Text(String),
    Image(Vec<u8>),       // PNG bytes
    VideoFrame(Vec<u8>),  // JPEG bytes; empty payload = stop stream
    Voice(Vec<u8>),       // Opus packet
}

impl ChatMessage {
    /// Serialize to wire format: [tag][payload]
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            ChatMessage::UserInfo(info) => {
                let mut buf = vec![TAG_USER_INFO];
                buf.extend_from_slice(&serde_json::to_vec(info).expect("serialize UserInfo"));
                buf
            }
            ChatMessage::Text(text) => {
                let mut buf = vec![TAG_TEXT];
                buf.extend_from_slice(text.as_bytes());
                buf
            }
            ChatMessage::Image(data) => {
                let mut buf = vec![TAG_IMAGE];
                buf.extend_from_slice(data);
                buf
            }
            ChatMessage::VideoFrame(data) => {
                let mut buf = vec![TAG_VIDEO_FRAME];
                buf.extend_from_slice(data);
                buf
            }
            ChatMessage::Voice(data) => {
                let mut buf = vec![TAG_VOICE];
                buf.extend_from_slice(data);
                buf
            }
        }
    }

    /// Deserialize from wire format.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.is_empty() {
            return None;
        }
        let tag = bytes[0];
        let payload = &bytes[1..];
        match tag {
            TAG_USER_INFO => {
                serde_json::from_slice::<UserInfo>(payload).ok().map(ChatMessage::UserInfo)
            }
            TAG_TEXT => {
                String::from_utf8(payload.to_vec()).ok().map(ChatMessage::Text)
            }
            TAG_IMAGE => Some(ChatMessage::Image(payload.to_vec())),
            TAG_VIDEO_FRAME => Some(ChatMessage::VideoFrame(payload.to_vec())),
            TAG_VOICE => Some(ChatMessage::Voice(payload.to_vec())),
            _ => None,
        }
    }
}
