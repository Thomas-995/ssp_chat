use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Returns the directory where profile data is stored.
fn profile_dir() -> PathBuf {
    let mut dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    dir.push("slp_chat");
    dir
}

#[cfg(target_os = "macos")]
pub fn instance_pid_path() -> PathBuf {
    profile_dir().join("instance.pid")
}

#[cfg(target_os = "windows")]
pub fn show_signal_path() -> PathBuf {
    profile_dir().join("show_signal")
}

/// Returns the directory where conversation history is stored.
fn history_dir() -> PathBuf {
    profile_dir().join("history")
}

fn username_path() -> PathBuf {
    profile_dir().join("username.txt")
}

fn avatar_path() -> PathBuf {
    profile_dir().join("avatar.png")
}

fn uuid_path() -> PathBuf {
    profile_dir().join("uuid.txt")
}

fn theme_path() -> PathBuf {
    profile_dir().join("theme.json")
}

fn config_path() -> PathBuf {
    profile_dir().join("config.json")
}

// ---------------------------------------------------------------------------
// UUID (persistent player identity)
// ---------------------------------------------------------------------------

/// Load or generate a persistent player UUID.
pub fn load_or_create_uuid() -> String {
    if let Ok(s) = fs::read_to_string(uuid_path()) {
        let s = s.trim().to_string();
        if !s.is_empty() {
            return s;
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    let dir = profile_dir();
    let _ = fs::create_dir_all(&dir);
    let _ = fs::write(uuid_path(), &id);
    id
}

// ---------------------------------------------------------------------------
// Theme configuration
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThemeConfig {
    /// Primary app/window background color [r, g, b]
    pub bg_color: [u8; 3],
    /// Main chat/text panel background color [r, g, b]
    #[serde(default = "default_text_panel_color", alias = "panel_color")]
    pub text_panel_color: [u8; 3],
    /// Sidebar background color [r, g, b]
    #[serde(default = "default_side_panel_color")]
    pub side_panel_color: [u8; 3],
    /// Separator line color [r, g, b]
    #[serde(default = "default_separator_color", alias = "border_color")]
    pub separator_color: [u8; 3],
    /// Accent color for timestamps, "joined" messages, and dates [r, g, b]
    pub accent_color: [u8; 3],
    /// Button/scrollbar color [r, g, b]
    pub button_color: [u8; 3],
    /// Button corner radius in pixels.
    #[serde(default = "default_button_radius")]
    pub button_radius: f32,
    /// Text color for normal text [r, g, b]
    #[serde(default = "default_text_color")]
    pub text_color: [u8; 3],

    // -- Overlay styling --
    /// Overlay position: 0=top-left, 1=top-right, 2=bottom-left, 3=bottom-right
    #[serde(default)]
    pub overlay_position: u8,
    /// Distance from corner horizontally in pixels
    #[serde(default = "default_overlay_distance")]
    pub overlay_distance_x: i32,
    /// Distance from corner vertically in pixels
    #[serde(default = "default_overlay_distance")]
    pub overlay_distance_y: i32,
    /// Overlay text size in pixels
    #[serde(default = "default_overlay_text_size")]
    pub overlay_text_size: i32,
    /// Overlay window width in pixels
    #[serde(default = "default_overlay_window_w")]
    pub overlay_window_w: i32,
    /// Overlay window height in pixels
    #[serde(default = "default_overlay_window_h")]
    pub overlay_window_h: i32,
    /// Overlay text outline color [r, g, b]
    #[serde(default)]
    pub overlay_outline_color: [u8; 3],
    /// Overlay text outline thickness in pixels
    #[serde(default = "default_overlay_outline_thickness")]
    pub overlay_outline_thickness: i32,

    // -- Talking border --
    /// Border color shown around avatar when a user is talking [r, g, b]
    #[serde(default = "default_talking_border_color")]
    pub talking_border_color: [u8; 3],
    /// Thickness of the talking border in pixels
    #[serde(default = "default_talking_border_thickness")]
    pub talking_border_thickness: f32,
}

fn default_text_panel_color() -> [u8; 3] { [30, 31, 35] }
fn default_side_panel_color() -> [u8; 3] { [25, 25, 28] }
fn default_separator_color() -> [u8; 3] { [48, 50, 55] }
fn default_text_color() -> [u8; 3] { [220, 222, 226] }
fn default_button_radius() -> f32 { 8.0 }
fn default_overlay_distance() -> i32 { 8 }
fn default_overlay_text_size() -> i32 { 16 }
fn default_overlay_window_w() -> i32 { 500 }
fn default_overlay_window_h() -> i32 { 280 }
fn default_overlay_outline_thickness() -> i32 { 1 }
fn default_talking_border_color() -> [u8; 3] { [59, 165, 93] }
fn default_talking_border_thickness() -> f32 { 2.0 }

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            bg_color: [25, 25, 28],
            text_panel_color: [30, 31, 35],
            side_panel_color: [25, 25, 28],
            separator_color: [48, 50, 55],
            accent_color: [88, 101, 242],
            button_color: [88, 101, 242],
            button_radius: 8.0,
            text_color: [220, 222, 226],
            overlay_position: 0,
            overlay_distance_x: 8,
            overlay_distance_y: 8,
            overlay_text_size: 16,
            overlay_window_w: 500,
            overlay_window_h: 280,
            overlay_outline_color: [0, 0, 0],
            overlay_outline_thickness: 1,
            talking_border_color: [59, 165, 93],
            talking_border_thickness: 2.0,
        }
    }
}

pub fn load_theme() -> ThemeConfig {
    fs::read_to_string(theme_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_theme(theme: &ThemeConfig) {
    let dir = profile_dir();
    let _ = fs::create_dir_all(&dir);
    if let Ok(json) = serde_json::to_string_pretty(theme) {
        let _ = fs::write(theme_path(), json);
    }
}

// ---------------------------------------------------------------------------
// App config (persistent settings)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PersistentConfig {
    pub voice_enabled: bool,
    pub images_enabled: bool,
    pub avatars_enabled: bool,
    pub file_transmission_enabled: bool,
    /// Whether to store conversation history to disk.
    pub store_messages: bool,
    #[serde(default = "default_true")]
    pub overlay_enabled: bool,
    #[serde(default)]
    pub start_on_startup: bool,
    /// Playback volume (0.0 – 5.0).
    #[serde(default = "default_volume")]
    pub voice_volume: f32,
    /// Noise gate threshold (0.0 – 1.0).
    #[serde(default = "default_threshold")]
    pub voice_threshold: f32,
    /// Whether push-to-talk is required (true) or open-mic (false).
    #[serde(default = "default_true")]
    pub voice_use_ptt: bool,
    #[serde(default = "default_ptt_key")]
    pub ptt_key: String,
    #[serde(default = "default_true")]
    pub webcam_enabled: bool,
    #[serde(default = "default_webcam_fps")]
    pub webcam_fps: f32,
    #[serde(default = "default_webcam_quality")]
    pub webcam_quality: u8,
    #[serde(default)]
    pub webcam_boxes: HashMap<String, WebcamBoxConfig>,
    #[serde(default)]
    pub audio_input_device: String,
    #[serde(default)]
    pub audio_output_device: String,
    #[serde(default)]
    pub webcam_device: String,
}

fn default_true() -> bool { true }
fn default_volume() -> f32 { 1.0 }
fn default_threshold() -> f32 { 0.01 }
fn default_webcam_fps() -> f32 { 6.0 }
fn default_webcam_quality() -> u8 { 60 }
fn default_ptt_key() -> String { "V".to_string() }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WebcamBoxConfig {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Default for PersistentConfig {
    fn default() -> Self {
        Self {
            voice_enabled: true,
            images_enabled: true,
            avatars_enabled: true,
            file_transmission_enabled: false,
            store_messages: true,
            overlay_enabled: true,
            start_on_startup: false,
            voice_volume: 1.0,
            voice_threshold: 0.01,
            voice_use_ptt: true,
            ptt_key: "V".to_string(),
            webcam_enabled: true,
            webcam_fps: 6.0,
            webcam_quality: 60,
            webcam_boxes: HashMap::new(),
            audio_input_device: String::new(),
            audio_output_device: String::new(),
            webcam_device: String::new(),
        }
    }
}

pub fn load_config() -> PersistentConfig {
    fs::read_to_string(config_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_config(config: &PersistentConfig) {
    let dir = profile_dir();
    let _ = fs::create_dir_all(&dir);
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let _ = fs::write(config_path(), json);
    }
}

// ---------------------------------------------------------------------------
// Conversation history persistence
// ---------------------------------------------------------------------------

/// A stored message in conversation history.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredMessage {
    /// True if this message was sent by us (the local user).
    pub is_self: bool,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub is_image: bool,
    pub image_data: Vec<u8>,
}

/// A stored peer profile snapshot.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredPeerProfile {
    pub uuid: String,
    pub name: String,
    #[serde(default)]
    pub avatar_png: Option<Vec<u8>>,
    #[serde(default = "default_name_color")]
    pub name_color: [u8; 3],
    #[serde(default)]
    pub bio: String,
    pub last_seen: chrono::DateTime<chrono::Utc>,
}

fn default_name_color() -> [u8; 3] { [87, 203, 222] }

/// Conversation with a specific peer (identified by UUID).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConversationHistory {
    pub peer: StoredPeerProfile,
    pub messages: Vec<StoredMessage>,
}

/// Load all stored peer profiles (for the history dropdown).
pub fn load_all_peer_profiles() -> Vec<StoredPeerProfile> {
    let dir = history_dir();
    if !dir.exists() {
        return Vec::new();
    }
    let mut profiles = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(data) = fs::read_to_string(&path) {
                    if let Ok(conv) = serde_json::from_str::<ConversationHistory>(&data) {
                        profiles.push(conv.peer);
                    }
                }
            }
        }
    }
    profiles.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
    profiles
}

/// Load conversation history for a specific peer UUID.
pub fn load_conversation(peer_uuid: &str) -> Option<ConversationHistory> {
    let path = history_dir().join(format!("{}.json", peer_uuid));
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

/// Save/update conversation history for a peer.
pub fn save_conversation(conv: &ConversationHistory) {
    let dir = history_dir();
    let _ = fs::create_dir_all(&dir);
    let path = dir.join(format!("{}.json", conv.peer.uuid));
    if let Ok(json) = serde_json::to_string_pretty(conv) {
        let _ = fs::write(path, json);
    }
}

/// Update a peer's profile in conversation history (upsert).
pub fn update_peer_profile(profile: &StoredPeerProfile) {
    let mut conv = load_conversation(&profile.uuid).unwrap_or_else(|| ConversationHistory {
        peer: profile.clone(),
        messages: Vec::new(),
    });
    conv.peer = profile.clone();
    save_conversation(&conv);
}

/// Append a message to a peer's conversation history.
pub fn append_message(peer_uuid: &str, msg: StoredMessage) {
    if let Some(mut conv) = load_conversation(peer_uuid) {
        conv.messages.push(msg);
        save_conversation(&conv);
    }
}

// ---------------------------------------------------------------------------
// Username / avatar / color / bio (existing)
// ---------------------------------------------------------------------------

/// Load the persisted username, or `None` if not yet set.
pub fn load_username() -> Option<String> {
    fs::read_to_string(username_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Persist the username to disk.
pub fn save_username(name: &str) {
    let dir = profile_dir();
    let _ = fs::create_dir_all(&dir);
    let _ = fs::write(username_path(), name.trim());
}

/// Load the persisted avatar PNG, or `None`.
pub fn load_avatar() -> Option<Vec<u8>> {
    let data = fs::read(avatar_path()).ok().filter(|d| !d.is_empty())?;
    // Ensure the avatar is 128x128 — re-encode if it isn't
    let img = image::load_from_memory(&data).ok()?;
    if img.width() == 128 && img.height() == 128 {
        Some(data)
    } else {
        let img = img.resize_to_fill(128, 128, image::imageops::FilterType::Lanczos3);
        let mut buf = Vec::new();
        let enc = image::codecs::png::PngEncoder::new(&mut buf);
        img.write_with_encoder(enc).ok()?;
        // Persist the corrected version
        let _ = fs::write(avatar_path(), &buf);
        Some(buf)
    }
}

/// Persist avatar PNG to disk.
pub fn save_avatar(png_data: &[u8]) {
    let dir = profile_dir();
    let _ = fs::create_dir_all(&dir);
    let _ = fs::write(avatar_path(), png_data);
}

/// Delete persisted avatar.
pub fn clear_avatar() {
    let _ = fs::remove_file(avatar_path());
}

fn name_color_path() -> PathBuf {
    profile_dir().join("name_color.txt")
}

/// Load the persisted name color as `[r, g, b]`, or `None`.
pub fn load_name_color() -> Option<[u8; 3]> {
    let s = fs::read_to_string(name_color_path()).ok()?;
    let parts: Vec<&str> = s.trim().split(',').collect();
    if parts.len() == 3 {
        let r = parts[0].parse::<u8>().ok()?;
        let g = parts[1].parse::<u8>().ok()?;
        let b = parts[2].parse::<u8>().ok()?;
        Some([r, g, b])
    } else {
        None
    }
}

/// Persist name color to disk.
pub fn save_name_color(rgb: &[u8; 3]) {
    let dir = profile_dir();
    let _ = fs::create_dir_all(&dir);
    let _ = fs::write(name_color_path(), format!("{},{},{}", rgb[0], rgb[1], rgb[2]));
}

fn bio_path() -> PathBuf {
    profile_dir().join("bio.txt")
}

/// Load the persisted bio, or `None`.
pub fn load_bio() -> Option<String> {
    fs::read_to_string(bio_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Persist bio to disk.
pub fn save_bio(bio: &str) {
    let dir = profile_dir();
    let _ = fs::create_dir_all(&dir);
    let _ = fs::write(bio_path(), bio.trim());
}
