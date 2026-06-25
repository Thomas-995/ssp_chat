use eframe::egui;
use std::collections::HashMap;
use std::time::SystemTime;

/// Convert a SystemTime to a chrono Local DateTime (non-Windows) or use Win32 API (Windows).
#[cfg(not(windows))]
fn to_local(time: &SystemTime) -> chrono::DateTime<chrono::Local> {
    let secs = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    use chrono::TimeZone;
    chrono::Local
        .timestamp_opt(secs, 0)
        .single()
        .unwrap_or_else(|| chrono::Local::now())
}

/// Format a SystemTime as HH:MM using platform-specific local time.
pub fn format_time(time: &SystemTime) -> String {
    let secs = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    #[cfg(windows)]
    {
        use std::mem::MaybeUninit;
        // Convert to FILETIME then to UTC SYSTEMTIME, then to local SYSTEMTIME
        const UNIX_EPOCH_FILETIME: u64 = 116444736000000000;
        let ft_ticks = secs * 10_000_000 + UNIX_EPOCH_FILETIME;
        let ft = windows_sys::Win32::Foundation::FILETIME {
            dwLowDateTime: ft_ticks as u32,
            dwHighDateTime: (ft_ticks >> 32) as u32,
        };
        let mut utc_st = MaybeUninit::<windows_sys::Win32::Foundation::SYSTEMTIME>::uninit();
        let mut local_st = MaybeUninit::<windows_sys::Win32::Foundation::SYSTEMTIME>::uninit();
        unsafe {
            windows_sys::Win32::System::Time::FileTimeToSystemTime(&ft, utc_st.as_mut_ptr());
            windows_sys::Win32::System::Time::SystemTimeToTzSpecificLocalTime(
                std::ptr::null(),
                utc_st.as_ptr(),
                local_st.as_mut_ptr(),
            );
            let st = local_st.assume_init();
            format!("{:02}:{:02}", st.wHour, st.wMinute)
        }
    }
    #[cfg(not(windows))]
    {
        let _ = secs; // suppress unused warning, we use to_local() instead
        let dt = to_local(time);
        use chrono::Timelike;
        format!("{:02}:{:02}", dt.hour(), dt.minute())
    }
}

/// A single entry in the chat log.
#[derive(Clone)]
pub enum ChatEntry {
    /// A text message from a peer.
    Text {
        sender: String,
        content: String,
        is_self: bool,
        timestamp: SystemTime,
    },
    /// An image shared by a peer (PNG bytes).
    Image {
        sender: String,
        png_data: Vec<u8>,
        /// Texture handle, lazily loaded on first render.
        texture: Option<egui::TextureHandle>,
        is_self: bool,
        timestamp: SystemTime,
    },
    /// System/status message (peer joined, etc.)
    System {
        msg: String,
        timestamp: SystemTime,
    },
}

/// Manages the chat history and peer names.
pub struct ChatHistory {
    pub entries: Vec<ChatEntry>,
    /// Map from peer node ID (hex) to display name.
    pub peer_names: HashMap<String, String>,
    /// Map from peer node ID to avatar PNG bytes.
    pub peer_avatars: HashMap<String, Vec<u8>>,
    /// Cached avatar textures (peer_id -> TextureHandle).
    pub avatar_textures: HashMap<String, egui::TextureHandle>,
    /// Peer name colors (peer_id -> Color32).
    pub peer_colors: HashMap<String, egui::Color32>,
    /// Peer biographies (peer_id -> bio text).
    pub peer_bios: HashMap<String, String>,
    /// Peer UUIDs (peer_id -> uuid).
    pub peer_uuids: HashMap<String, String>,
    /// Latest webcam frame from each peer (peer_id -> JPEG bytes).
    pub peer_video_frames: HashMap<String, Vec<u8>>,
    /// Cached webcam frame textures (peer_id -> TextureHandle).
    pub peer_video_textures: HashMap<String, egui::TextureHandle>,
    /// Last time a webcam frame was seen for each peer.
    pub peer_video_last_seen: HashMap<String, SystemTime>,
    /// Whether we should auto-scroll to bottom.
    pub scroll_to_bottom: bool,
}

impl ChatHistory {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            peer_names: HashMap::new(),
            peer_avatars: HashMap::new(),
            avatar_textures: HashMap::new(),
            peer_colors: HashMap::new(),
            peer_bios: HashMap::new(),
            peer_uuids: HashMap::new(),
            peer_video_frames: HashMap::new(),
            peer_video_textures: HashMap::new(),
            peer_video_last_seen: HashMap::new(),
            scroll_to_bottom: false,
        }
    }

    pub fn add_text(&mut self, sender: &str, content: &str, is_self: bool) {
        self.add_text_with_time(sender, content, is_self, SystemTime::now());
    }

    pub fn add_text_with_time(&mut self, sender: &str, content: &str, is_self: bool, timestamp: SystemTime) {
        self.entries.push(ChatEntry::Text {
            sender: sender.to_string(),
            content: content.to_string(),
            is_self,
            timestamp,
        });
        self.scroll_to_bottom = true;
    }

    pub fn add_image(&mut self, sender: &str, png_data: Vec<u8>, is_self: bool) {
        self.add_image_with_time(sender, png_data, is_self, SystemTime::now());
    }

    pub fn add_image_with_time(
        &mut self,
        sender: &str,
        png_data: Vec<u8>,
        is_self: bool,
        timestamp: SystemTime,
    ) {
        self.entries.push(ChatEntry::Image {
            sender: sender.to_string(),
            png_data,
            texture: None,
            is_self,
            timestamp,
        });
        self.scroll_to_bottom = true;
    }

    pub fn add_system(&mut self, msg: &str) {
        self.entries.push(ChatEntry::System {
            msg: msg.to_string(),
            timestamp: SystemTime::now(),
        });
        self.scroll_to_bottom = true;
    }

    /// Resolve a peer ID to their display name, or a short hex fallback.
    pub fn peer_display_name(&self, peer_id: &str) -> String {
        self.peer_names
            .get(peer_id)
            .cloned()
            .unwrap_or_else(|| {
                if peer_id.len() > 8 {
                    format!("{}...", &peer_id[..8])
                } else {
                    peer_id.to_string()
                }
            })
    }

    pub fn set_peer_name(&mut self, peer_id: &str, name: &str) {
        let old = self.peer_names.get(peer_id).cloned();
        self.peer_names.insert(peer_id.to_string(), name.to_string());
        if old.as_deref() != Some(name) {
            self.add_system(&format!("{} joined", name));
        }
    }

    pub fn set_peer_avatar(&mut self, peer_id: &str, png_data: Vec<u8>) {
        // Invalidate cached texture so it gets re-created
        self.avatar_textures.remove(peer_id);
        self.peer_avatars.insert(peer_id.to_string(), png_data);
    }

    pub fn set_peer_color(&mut self, peer_id: &str, color: egui::Color32) {
        self.peer_colors.insert(peer_id.to_string(), color);
    }

    pub fn set_peer_bio(&mut self, peer_id: &str, bio: &str) {
        self.peer_bios.insert(peer_id.to_string(), bio.to_string());
    }

    pub fn set_peer_uuid(&mut self, peer_id: &str, uuid: &str) {
        self.peer_uuids.insert(peer_id.to_string(), uuid.to_string());
    }

    pub fn set_peer_video_frame(&mut self, peer_id: &str, jpeg_data: Vec<u8>) {
        self.peer_video_textures.remove(peer_id);
        self.peer_video_frames.insert(peer_id.to_string(), jpeg_data);
        self.peer_video_last_seen.insert(peer_id.to_string(), SystemTime::now());
    }

    pub fn remove_peer_video_frame(&mut self, peer_id: &str) {
        self.peer_video_frames.remove(peer_id);
        self.peer_video_textures.remove(peer_id);
        self.peer_video_last_seen.remove(peer_id);
    }

    pub fn prune_stale_peer_video_frames(&mut self, max_age: std::time::Duration) {
        let now = SystemTime::now();
        let stale: Vec<String> = self
            .peer_video_last_seen
            .iter()
            .filter_map(|(peer_id, seen)| {
                now.duration_since(*seen)
                    .ok()
                    .filter(|age| *age > max_age)
                    .map(|_| peer_id.clone())
            })
            .collect();
        for peer_id in stale {
            self.remove_peer_video_frame(&peer_id);
        }
    }

    /// Get or lazily create the avatar texture for a peer.
    pub fn avatar_texture(&mut self, peer_id: &str, ctx: &egui::Context) -> Option<egui::TextureHandle> {
        if let Some(tex) = self.avatar_textures.get(peer_id) {
            return Some(tex.clone());
        }
        if let Some(png_data) = self.peer_avatars.get(peer_id) {
            if let Ok(img) = image::load_from_memory(png_data) {
                let rgba = img.to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                let pixels = rgba.into_raw();
                let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
                let tex = ctx.load_texture(
                    format!("avatar_{}", peer_id),
                    color_image,
                    egui::TextureOptions::LINEAR,
                );
                self.avatar_textures.insert(peer_id.to_string(), tex.clone());
                return Some(tex);
            }
        }
        None
    }
}
