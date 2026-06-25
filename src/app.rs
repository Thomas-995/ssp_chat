//! Core application struct, state machine, and data/logic methods.
//!
//! All rendering is delegated to sibling modules (ui.rs, overlay.rs, theme.rs).
//! Connection spawning lives in connect.rs; config & startup in config.rs;
//! stealth window helpers in stealth.rs.

use eframe::egui;
use ssp_client::ConnectionState;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

pub(crate) fn webcam_device_names() -> Vec<String> {
    static CACHED: OnceLock<Vec<String>> = OnceLock::new();
    CACHED
        .get_or_init(|| {
            nokhwa::query(nokhwa::utils::ApiBackend::Auto)
                .ok()
                .unwrap_or_default()
                .into_iter()
                .map(|info| info.human_name().to_string())
                .collect()
        })
        .clone()
}

use crate::chat::ChatHistory;
use crate::config::AppConfig;
use crate::connect::{spawn_connect, ConnectHandle, SharedSession};
use crate::profile::{self, StoredMessage, StoredPeerProfile, ThemeConfig};
use crate::protocol::{ChatMessage, UserInfo};
use crate::theme::{Colors, ThemeHexFields};
use crate::voice::{VoiceEngine, VoiceWorkerHandle};

#[cfg(target_os = "windows")]
use crate::stealth::{find_main_hwnd, stealth_hide_window, stealth_show_window};

// ---------------------------------------------------------------------------
// Application states.
// ---------------------------------------------------------------------------
pub(crate) const LOCAL_WEBCAM_PEER_ID: &str = "__local_webcam__";

pub(crate) enum BackgroundIoEvent {
    Incoming { peer_id: String, data: Vec<u8> },
    VoiceSent(usize),
    VoiceReceived(usize),
}

pub(crate) enum AppScreen {
    FirstRun,
    Editing,
    InSession,
}

// ---------------------------------------------------------------------------
// Main application struct.
// ---------------------------------------------------------------------------

pub struct SlpChatApp {
    pub(crate) screen: AppScreen,

    // Profile
    pub(crate) username: String,
    pub(crate) avatar_png: Option<Vec<u8>>,
    pub(crate) avatar_texture: Option<egui::TextureHandle>,
    pub(crate) name_color: [u8; 3],
    pub(crate) bio: String,
    /// Persistent player UUID.
    pub(crate) player_uuid: String,
    /// Scratch fields used while editing.
    pub(crate) edit_username: String,
    pub(crate) edit_avatar_png: Option<Vec<u8>>,
    pub(crate) edit_avatar_texture: Option<egui::TextureHandle>,
    pub(crate) edit_name_color: [u8; 3],
    pub(crate) edit_color_hex: String,
    pub(crate) edit_bio: String,

    // Theme
    pub(crate) theme: ThemeConfig,
    pub(crate) edit_theme: ThemeConfig,
    pub(crate) edit_theme_hex: ThemeHexFields,

    // Connection
    pub(crate) connect_handle: Option<ConnectHandle>,
    pub(crate) session: Option<SharedSession>,
    /// Keeps the tokio Runtime alive so background networking tasks survive.
    pub(crate) _runtime: Option<tokio::runtime::Runtime>,

    // Chat state (only meaningful in InSession)
    pub(crate) chat: ChatHistory,
    pub(crate) voice: VoiceEngine,
    pub(crate) voice_worker_handle: Arc<Mutex<VoiceWorkerHandle>>,
    pub(crate) background_io_tx: Sender<BackgroundIoEvent>,
    pub(crate) background_io_rx: Receiver<BackgroundIoEvent>,
    pub(crate) background_io_stop: Option<Arc<AtomicBool>>,
    pub(crate) background_io_started: bool,
    pub(crate) background_repaint_started: bool,
    pub(crate) voice_packets_sent: u64,
    pub(crate) voice_packets_received: u64,
    pub(crate) input_text: String,
    pub(crate) info_announced: bool,
    /// True once we've received at least one UserInfo reply from a peer.
    pub(crate) joined: bool,
    pub(crate) reveal_on_peer_connect: bool,
    pub(crate) pending_image: Option<Vec<u8>>,
    pub(crate) webcam_streaming: Arc<AtomicBool>,
    pub(crate) webcam_worker_running: Arc<AtomicBool>,
    pub(crate) webcam_outgoing_frame: Arc<Mutex<Option<Vec<u8>>>>,
    pub(crate) webcam_error_slot: Arc<Mutex<Option<String>>>,
    pub(crate) webcam_error: Option<String>,
    pub(crate) capturing_ptt_key: bool,
    pub(crate) webcam_box_positions: std::collections::HashMap<String, egui::Pos2>,
    pub(crate) webcam_box_sizes: std::collections::HashMap<String, egui::Vec2>,

    // Options
    pub(crate) config: AppConfig,
    pub(crate) show_options: bool,

    /// Peer IDs we've already replied to with our own announce.
    pub(crate) replied_peers: HashSet<String>,
    /// Peer ID whose profile panel is open (None = closed).
    pub(crate) selected_peer_profile: Option<String>,
    /// Sidebar: whether the peer list dropdown is expanded.
    pub(crate) peers_expanded: bool,

    /// Sidebar: whether the history dropdown is expanded.
    pub(crate) history_expanded: bool,
    /// Cached peer profiles for history display.
    pub(crate) history_profiles: Vec<StoredPeerProfile>,
    /// UUID of selected history profile (to view past messages).
    pub(crate) selected_history_uuid: Option<String>,
    /// When viewing a past conversation from history (None = live chat).
    pub(crate) viewing_history_chat: Option<profile::ConversationHistory>,
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    pub(crate) launched_at_startup: bool,
    pub(crate) window_shown: bool,
    pub(crate) overlay_visible: bool,
    pub(crate) overlay_hint_passes: u8,
    #[cfg(target_os = "windows")]
    pub(crate) startup_hide_frame: u8,
    pub(crate) sidebar_width: f32,
    pub(crate) show_theme_editor: bool,
}

fn webcam_worker_loop(
    streaming: Arc<AtomicBool>,
    outgoing: Arc<Mutex<Option<Vec<u8>>>>,
    fps: f32,
    quality: u8,
) -> Result<(), String> {
    use nokhwa::pixel_format::RgbFormat;
    use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};

    let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
    let mut camera = nokhwa::Camera::new(CameraIndex::Index(0), requested)
        .map_err(|err| format!("Could not open webcam: {err}"))?;
    camera
        .open_stream()
        .map_err(|err| format!("Could not start webcam stream: {err}"))?;

    let frame_delay = std::time::Duration::from_secs_f32(1.0 / fps.max(1.0));
    while streaming.load(Ordering::Relaxed) {
        let frame = camera
            .frame()
            .map_err(|err| format!("Could not capture webcam frame: {err}"))?;
        let rgb = frame
            .decode_image::<RgbFormat>()
            .map_err(|err| format!("Could not decode webcam frame: {err}"))?;
        let resized = image::DynamicImage::ImageRgb8(rgb)
            .resize(426, 240, image::imageops::FilterType::Triangle)
            .to_rgb8();
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, quality)
            .encode(
                resized.as_raw(),
                resized.width(),
                resized.height(),
                image::ExtendedColorType::Rgb8,
            )
            .map_err(|err| format!("Could not encode webcam frame: {err}"))?;
        *outgoing.lock().unwrap() = Some(jpeg);
        std::thread::sleep(frame_delay);
    }

    let _ = camera.stop_stream();
    Ok(())
}

impl SlpChatApp {
    pub fn new(launched_at_startup: bool) -> Self {
        let username = profile::load_username().unwrap_or_default();
        let avatar_png = profile::load_avatar();
        let name_color = profile::load_name_color().unwrap_or(Colors::DEFAULT_NAME_COLOR);
        let bio = profile::load_bio().unwrap_or_default();
        let player_uuid = profile::load_or_create_uuid();
        let theme = profile::load_theme();
        let persistent_config = profile::load_config();

        let screen = if username.is_empty() {
            AppScreen::FirstRun
        } else {
            AppScreen::InSession
        };

        let edit_theme = theme.clone();
        let edit_theme_hex = ThemeHexFields::from_theme(&theme);
        let config: AppConfig = persistent_config.into();
        let voice = VoiceEngine::new_with_devices(
            &config.audio_input_device,
            &config.audio_output_device,
        );
        voice.set_volume(config.voice_volume);
        voice.set_threshold(config.voice_threshold);
        voice.set_use_ptt(config.voice_use_ptt);
        let voice_worker_handle = Arc::new(Mutex::new(voice.worker_handle()));
        let (background_io_tx, background_io_rx) = mpsc::channel();

        let mut app = Self {
            screen,
            username: username.clone(),
            avatar_png: avatar_png.clone(),
            avatar_texture: None,
            name_color,
            bio: bio.clone(),
            player_uuid,
            edit_username: username,
            edit_avatar_png: avatar_png,
            edit_avatar_texture: None,
            edit_name_color: name_color,
            edit_color_hex: format!(
                "{:02X}{:02X}{:02X}",
                name_color[0], name_color[1], name_color[2]
            ),
            edit_bio: bio,
            theme,
            edit_theme,
            edit_theme_hex,
            connect_handle: None,
            session: None,
            _runtime: None,
            chat: ChatHistory::new(),
            voice,
            voice_worker_handle,
            background_io_tx,
            background_io_rx,
            background_io_stop: None,
            background_io_started: false,
            background_repaint_started: false,
            voice_packets_sent: 0,
            voice_packets_received: 0,
            input_text: String::new(),
            info_announced: false,
            joined: false,
            reveal_on_peer_connect: false,
            pending_image: None,
            webcam_streaming: Arc::new(AtomicBool::new(false)),
            webcam_worker_running: Arc::new(AtomicBool::new(false)),
            webcam_outgoing_frame: Arc::new(Mutex::new(None)),
            webcam_error_slot: Arc::new(Mutex::new(None)),
            webcam_error: None,
            capturing_ptt_key: false,
            webcam_box_positions: std::collections::HashMap::new(),
            webcam_box_sizes: std::collections::HashMap::new(),
            config,
            show_options: false,
            replied_peers: HashSet::new(),
            selected_peer_profile: None,
            peers_expanded: true,
            history_expanded: false,
            history_profiles: profile::load_all_peer_profiles(),
            selected_history_uuid: None,
            viewing_history_chat: None,
            #[cfg(any(target_os = "windows", target_os = "macos"))]
            launched_at_startup,
            window_shown: !launched_at_startup,
            overlay_visible: false,
            overlay_hint_passes: 0,
            #[cfg(target_os = "windows")]
            startup_hide_frame: 0,
            sidebar_width: 170.0,
            show_theme_editor: false,
        };

        // Start connecting right away if we already have a username.
        if matches!(app.screen, AppScreen::InSession) {
            app.start_connecting();
        }

        app
    }

    // ---- connection management ----

    pub(crate) fn start_connecting(&mut self) {
        self.connect_handle = Some(spawn_connect());
    }

    pub(crate) fn poll_session(&mut self) {
        if self.session.is_some() {
            return;
        }
        let ready = self
            .connect_handle
            .as_ref()
            .map(|h| h.session_slot.lock().unwrap().is_some())
            .unwrap_or(false);
        if ready {
            let handle = self.connect_handle.take().unwrap();
            let (session, rt) = handle.session_slot.lock().unwrap().take().unwrap();
            self.session = Some(session);
            self._runtime = Some(rt);
            self.screen = AppScreen::InSession;
            self.info_announced = false;
            self.joined = false;
            self.reveal_on_peer_connect = false;
            self.replied_peers.clear();
            self.chat = ChatHistory::new();
        }
    }

    // ---- profile helpers ----

    pub(crate) fn load_avatar_texture(
        png: &Option<Vec<u8>>,
        tex: &mut Option<egui::TextureHandle>,
        ctx: &egui::Context,
        label: &str,
    ) {
        if tex.is_none() {
            if let Some(data) = png {
                if let Ok(img) = image::load_from_memory(data) {
                    let rgba = img.to_rgba8();
                    let size = [rgba.width() as usize, rgba.height() as usize];
                    let pixels = rgba.into_raw();
                    let ci = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
                    *tex = Some(ctx.load_texture(label, ci, egui::TextureOptions::LINEAR));
                }
            }
        }
    }

    pub(crate) fn pick_avatar() -> Option<Vec<u8>> {
        let path = rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg", "bmp", "webp"])
            .pick_file()?;
        let data = std::fs::read(&path).ok()?;
        let img = image::load_from_memory(&data).ok()?;
        let img = img.resize_to_fill(128, 128, image::imageops::FilterType::Lanczos3);
        let mut buf = Vec::new();
        let enc = image::codecs::png::PngEncoder::new(&mut buf);
        img.write_with_encoder(enc).ok()?;
        Some(buf)
    }

    pub(crate) fn commit_profile_edit(&mut self) {
        let name = self.edit_username.trim().to_string();
        if name.is_empty() {
            return;
        }
        self.username = name.clone();
        profile::save_username(&name);

        self.avatar_png = self.edit_avatar_png.clone();
        self.avatar_texture = None;
        if let Some(ref data) = self.avatar_png {
            profile::save_avatar(data);
        } else {
            profile::clear_avatar();
        }

        self.name_color = self.edit_name_color;
        profile::save_name_color(&self.name_color);

        self.bio = self.edit_bio.trim().to_string();
        profile::save_bio(&self.bio);

        self.screen = AppScreen::InSession;
        self.info_announced = false;
        if self.connect_handle.is_none() && self.session.is_none() {
            self.start_connecting();
        }
        self.config.save();
        crate::config::sync_startup_registry(self.config.start_on_startup);
    }

    pub(crate) fn show_chat_window(&mut self, ctx: &egui::Context) {
        #[cfg(target_os = "windows")]
        {
            let hwnd = find_main_hwnd();
            if hwnd != 0 as _ {
                stealth_show_window(hwnd);
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        self.window_shown = true;

        if self.config.overlay_enabled {
            self.overlay_visible = true;
            self.overlay_hint_passes = 10;
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    pub(crate) fn apply_compositor_overlay_hints(pos: egui::Pos2, size: egui::Vec2) {
        if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none() {
            return;
        }
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(120));
            let size_w = size.x.round().to_string();
            let size_h = size.y.round().to_string();
            let pos_x = pos.x.round().to_string();
            let pos_y = pos.y.round().to_string();
            let commands: Vec<Vec<String>> = vec![
                vec!["keyword", "windowrulev2", "float,class:^(slpauth_app_overlay)$"],
                vec!["keyword", "windowrulev2", "pin,class:^(slpauth_app_overlay)$"],
                vec!["keyword", "windowrulev2", "noborder,class:^(slpauth_app_overlay)$"],
                vec!["keyword", "windowrulev2", "noshadow,class:^(slpauth_app_overlay)$"],
                vec!["keyword", "windowrulev2", "noanim,class:^(slpauth_app_overlay)$"],
                vec!["keyword", "windowrulev2", "noblur,class:^(slpauth_app_overlay)$"],
                vec!["dispatch", "focuswindow", "class:slpauth_app_overlay"],
                vec!["dispatch", "focuswindow", "title:SLP Chat Overlay"],
                vec!["dispatch", "setfloating"],
                vec!["dispatch", "pin"],
                vec!["dispatch", "alterzorder", "top"],
                vec!["dispatch", "setprop", "active", "noborder", "1", "lock"],
                vec!["dispatch", "setprop", "active", "noshadow", "1", "lock"],
                vec!["dispatch", "setprop", "active", "rounding", "0", "lock"],
                vec!["dispatch", "setprop", "active", "noanim", "1", "lock"],
                vec!["dispatch", "resizeactive", "exact", &size_w, &size_h],
                vec!["dispatch", "movewindowpixel", "exact", &pos_x, &pos_y],
            ]
            .into_iter()
            .map(|v| v.into_iter().map(str::to_string).collect())
            .collect();
            for args in commands {
                let _ = std::process::Command::new("hyprctl")
                    .args(args)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }
        });
    }

    #[cfg(any(not(unix), target_os = "macos"))]
    pub(crate) fn apply_compositor_overlay_hints(_pos: egui::Pos2, _size: egui::Vec2) {}

    pub(crate) fn apply_voice_settings(&self) {
        self.voice.set_volume(self.config.voice_volume);
        self.voice.set_threshold(self.config.voice_threshold);
        self.voice.set_use_ptt(self.config.voice_use_ptt);
    }

    pub(crate) fn rebuild_voice_engine(&mut self) {
        let was_enabled = self.voice.is_enabled();
        let was_ptt_active = self.voice.ptt_active();
        let voice = VoiceEngine::new_with_devices(
            &self.config.audio_input_device,
            &self.config.audio_output_device,
        );
        voice.set_volume(self.config.voice_volume);
        voice.set_threshold(self.config.voice_threshold);
        voice.set_use_ptt(self.config.voice_use_ptt);
        voice.set_enabled(was_enabled);
        voice.set_ptt(was_ptt_active);
        *self.voice_worker_handle.lock().unwrap() = voice.worker_handle();
        self.voice = voice;
    }

    pub(crate) fn start_background_repaint_waker(&mut self, ctx: &egui::Context) {
        if self.background_repaint_started {
            return;
        }
        self.background_repaint_started = true;
        let ctx = ctx.clone();
        thread::spawn(move || loop {
            ctx.request_repaint();
            thread::sleep(Duration::from_millis(16));
        });
    }

    pub(crate) fn start_background_io_worker(&mut self) {
        if self.background_io_started {
            return;
        }
        let Some(session) = self.session.as_ref().cloned() else {
            return;
        };
        let Some(runtime) = self._runtime.as_ref() else {
            return;
        };

        self.background_io_started = true;
        let stop = Arc::new(AtomicBool::new(false));
        self.background_io_stop = Some(stop.clone());
        let voice = self.voice_worker_handle.clone();
        let tx = self.background_io_tx.clone();
        let rt_handle = runtime.handle().clone();

        thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let outgoing = voice.lock().unwrap().drain_outgoing();
                if !outgoing.is_empty() {
                    let sent_count = outgoing.len();
                    let session_for_send = session.clone();
                    rt_handle.spawn(async move {
                        let session = session_for_send.lock().await;
                        for packet in outgoing {
                            let _ = session.send(ChatMessage::Voice(packet).to_bytes()).await;
                        }
                    });
                    let _ = tx.send(BackgroundIoEvent::VoiceSent(sent_count));
                }

                if let Ok(mut session_guard) = session.try_lock() {
                    for _ in 0..100 {
                        match session_guard.try_recv() {
                            Ok(Some(msg)) => {
                                let peer_id = format!("{:?}", msg.from);
                                match ChatMessage::from_bytes(&msg.data) {
                                    Some(ChatMessage::Voice(data)) => {
                                        voice.lock().unwrap().play_incoming(&data, &peer_id);
                                        let _ = tx.send(BackgroundIoEvent::VoiceReceived(1));
                                    }
                                    _ => {
                                        let _ = tx.send(BackgroundIoEvent::Incoming {
                                            peer_id,
                                            data: msg.data,
                                        });
                                    }
                                }
                            }
                            Ok(None) => break,
                            Err(_) => break,
                        }
                    }
                }

                thread::sleep(Duration::from_millis(5));
            }
        });
    }

    // ---- history helpers ----

    pub(crate) fn load_past_messages_for_peer(&mut self, peer_uuid: &str) {
        if !self.config.store_messages {
            return;
        }
        if let Some(conv) = profile::load_conversation(peer_uuid) {
            if conv.messages.is_empty() {
                return;
            }
            let peer_name = &conv.peer.name;
            for msg in &conv.messages {
                let sender_name = if msg.is_self {
                    &self.username
                } else {
                    peer_name
                };
                if msg.is_image {
                    let ts = std::time::UNIX_EPOCH
                        + std::time::Duration::from_secs(msg.timestamp.timestamp().max(0) as u64);
                    self.chat.add_image_with_time(sender_name, msg.image_data.clone(), msg.is_self, ts);
                } else {
                    let ts = std::time::UNIX_EPOCH
                        + std::time::Duration::from_secs(msg.timestamp.timestamp().max(0) as u64);
                    self.chat
                        .add_text_with_time(sender_name, &msg.content, msg.is_self, ts);
                }
            }
        }
    }

    // ---- session messaging ----

    pub(crate) fn send_session_bytes_best_effort(&self, data: Vec<u8>) {
        let Some(session) = self.session.as_ref().cloned() else {
            return;
        };
        if let Some(rt) = &self._runtime {
            rt.spawn(async move {
                let session = session.lock().await;
                let _ = session.send(data).await;
            });
        }
    }

    pub(crate) fn send_user_info(&mut self, is_reply: bool) {
        let info = UserInfo {
            name: self.username.clone(),
            uuid: self.player_uuid.clone(),
            avatar: self.avatar_png.clone(),
            name_color: self.name_color,
            bio: self.bio.clone(),
            is_reply,
        };
        let msg = ChatMessage::UserInfo(info);
        self.send_session_bytes_best_effort(msg.to_bytes());
    }

    pub(crate) fn announce_if_needed(&mut self) {
        if self.info_announced {
            return;
        }
        let discovered = self
            .session
            .as_ref()
            .and_then(|session| session.try_lock().ok().map(|session| session.connection_state()))
            .is_some_and(|state| state == ConnectionState::Discovered);
        if discovered {
            self.reveal_on_peer_connect = true;
            self.send_user_info(false);
            self.info_announced = true;
        }
    }

    pub(crate) fn send_text(&mut self) {
        let text = self.input_text.trim().to_string();
        if text.is_empty() {
            return;
        }
        self.input_text.clear();
        let msg = ChatMessage::Text(text.clone());
        self.send_session_bytes_best_effort(msg.to_bytes());
        self.chat.add_text(&self.username, &text, true);

        if self.config.store_messages {
            for uuid in self.chat.peer_uuids.values() {
                profile::append_message(
                    uuid,
                    StoredMessage {
                        is_self: true,
                        content: text.clone(),
                        timestamp: chrono::Utc::now(),
                        is_image: false,
                        image_data: Vec::new(),
                    },
                );
            }
        }
    }

    pub(crate) fn send_image(&mut self, png_data: Vec<u8>) {
        let msg = ChatMessage::Image(png_data.clone());
        self.send_session_bytes_best_effort(msg.to_bytes());
        self.chat.add_image(&self.username, png_data.clone(), true);

        if self.config.store_messages {
            for uuid in self.chat.peer_uuids.values() {
                profile::append_message(
                    uuid,
                    StoredMessage {
                        is_self: true,
                        content: String::new(),
                        timestamp: chrono::Utc::now(),
                        is_image: true,
                        image_data: png_data.clone(),
                    },
                );
            }
        }
    }

    fn handle_incoming_chat_message(
        &mut self,
        peer_id: String,
        chat_msg: ChatMessage,
        needs_reply_to: &mut Vec<String>,
        load_history_uuids: &mut Vec<String>,
    ) {
        match chat_msg {
            ChatMessage::UserInfo(info) => {
                self.chat.set_peer_name(&peer_id, &info.name);
                if let Some(avatar) = &info.avatar {
                    self.chat.set_peer_avatar(&peer_id, avatar.clone());
                }
                self.chat.set_peer_color(
                    &peer_id,
                    egui::Color32::from_rgb(
                        info.name_color[0],
                        info.name_color[1],
                        info.name_color[2],
                    ),
                );
                if !info.bio.is_empty() {
                    self.chat.set_peer_bio(&peer_id, &info.bio);
                }

                if !info.uuid.is_empty() {
                    self.chat.set_peer_uuid(&peer_id, &info.uuid);

                    if self.config.store_messages {
                        let stored_profile = StoredPeerProfile {
                            uuid: info.uuid.clone(),
                            name: info.name.clone(),
                            avatar_png: info.avatar.clone(),
                            name_color: info.name_color,
                            bio: info.bio.clone(),
                            last_seen: chrono::Utc::now(),
                        };
                        profile::update_peer_profile(&stored_profile);
                        self.history_profiles = profile::load_all_peer_profiles();
                    }

                    load_history_uuids.push(info.uuid.clone());
                }

                if !info.is_reply {
                    if self.replied_peers.insert(peer_id.clone()) {
                        needs_reply_to.push(peer_id);
                    }
                }
                let first_join = !self.joined;
                self.joined = true;
                if first_join {
                    self.reveal_on_peer_connect = true;
                }
            }
            ChatMessage::Text(text) => {
                let name = self.chat.peer_display_name(&peer_id);
                self.chat.add_text(&name, &text, false);

                if self.config.store_messages {
                    if let Some(uuid) = self.chat.peer_uuids.get(&peer_id).cloned() {
                        profile::append_message(
                            &uuid,
                            StoredMessage {
                                is_self: false,
                                content: text,
                                timestamp: chrono::Utc::now(),
                                is_image: false,
                                image_data: Vec::new(),
                            },
                        );
                    }
                }
            }
            ChatMessage::Image(data) => {
                if self.config.images_enabled {
                    let name = self.chat.peer_display_name(&peer_id);
                    self.chat.add_image(&name, data.clone(), false);

                    if self.config.store_messages {
                        if let Some(uuid) = self.chat.peer_uuids.get(&peer_id).cloned() {
                            profile::append_message(
                                &uuid,
                                StoredMessage {
                                    is_self: false,
                                    content: String::new(),
                                    timestamp: chrono::Utc::now(),
                                    is_image: true,
                                    image_data: data,
                                },
                            );
                        }
                    }
                }
            }
            ChatMessage::VideoFrame(data) => {
                if self.config.webcam_enabled {
                    if data.is_empty() {
                        self.chat.remove_peer_video_frame(&peer_id);
                    } else {
                        self.chat.set_peer_video_frame(&peer_id, data);
                    }
                }
            }
            ChatMessage::Voice(data) => {
                if self.config.voice_enabled {
                    self.voice_packets_received = self.voice_packets_received.saturating_add(1);
                    self.voice.play_incoming(&data, &peer_id);
                }
            }
        }
    }

    pub(crate) fn poll_incoming(&mut self) {
        let mut needs_reply_to: Vec<String> = Vec::new();
        let mut load_history_uuids: Vec<String> = Vec::new();

        while let Ok(event) = self.background_io_rx.try_recv() {
            match event {
                BackgroundIoEvent::Incoming { peer_id, data } => {
                    if let Some(chat_msg) = ChatMessage::from_bytes(&data) {
                        self.handle_incoming_chat_message(
                            peer_id,
                            chat_msg,
                            &mut needs_reply_to,
                            &mut load_history_uuids,
                        );
                    }
                }
                BackgroundIoEvent::VoiceSent(count) => {
                    self.voice_packets_sent = self.voice_packets_sent.saturating_add(count as u64);
                }
                BackgroundIoEvent::VoiceReceived(count) => {
                    self.voice_packets_received = self.voice_packets_received.saturating_add(count as u64);
                }
            }
        }

        // Once the background I/O worker is active, it is the only consumer of
        // Session::try_recv(), so hidden/occluded windows cannot starve voice.
        if !self.background_io_started {
            let Some(shared_session) = self.session.as_ref().cloned() else {
                return;
            };
            let Ok(mut session) = shared_session.try_lock() else {
                return;
            };
            for _ in 0..100 {
                match session.try_recv() {
                    Ok(Some(msg)) => {
                        let peer_id = format!("{:?}", msg.from);
                        if let Some(chat_msg) = ChatMessage::from_bytes(&msg.data) {
                            self.handle_incoming_chat_message(
                                peer_id,
                                chat_msg,
                                &mut needs_reply_to,
                                &mut load_history_uuids,
                            );
                        }
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
        }

        if !needs_reply_to.is_empty() {
            self.send_user_info(true);
        }
        for uuid in load_history_uuids {
            self.load_past_messages_for_peer(&uuid);
        }
    }

    pub(crate) fn toggle_webcam_stream(&mut self) {
        if self.webcam_streaming.load(Ordering::Relaxed) {
            self.stop_webcam_stream();
        } else {
            self.start_webcam_stream();
        }
    }

    pub(crate) fn start_webcam_stream(&mut self) {
        if !self.config.webcam_enabled || self.webcam_worker_running.load(Ordering::Relaxed) {
            return;
        }
        self.webcam_error = None;
        self.webcam_streaming.store(true, Ordering::Relaxed);
        self.webcam_worker_running.store(true, Ordering::Relaxed);

        let streaming = self.webcam_streaming.clone();
        let running = self.webcam_worker_running.clone();
        let outgoing = self.webcam_outgoing_frame.clone();
        let err_slot = self.webcam_error_slot.clone();
        let fps = self.config.webcam_fps.clamp(1.0, 30.0);
        let quality = self.config.webcam_quality.clamp(1, 100);

        std::thread::spawn(move || {
            let result = webcam_worker_loop(streaming.clone(), outgoing, fps, quality);
            if let Err(err) = result {
                *err_slot.lock().unwrap() = Some(err);
                streaming.store(false, Ordering::Relaxed);
            }
            running.store(false, Ordering::Relaxed);
        });
    }

    pub(crate) fn stop_webcam_stream(&mut self) {
        self.webcam_streaming.store(false, Ordering::Relaxed);
        self.chat.remove_peer_video_frame(LOCAL_WEBCAM_PEER_ID);
        self.send_session_bytes_best_effort(ChatMessage::VideoFrame(Vec::new()).to_bytes());
    }

    pub(crate) fn poll_webcam_stream(&mut self) {
        if let Some(err) = self.webcam_error_slot.lock().unwrap().take() {
            self.webcam_error = Some(err);
        }
        if !self.config.webcam_enabled && self.webcam_streaming.load(Ordering::Relaxed) {
            self.stop_webcam_stream();
            return;
        }
        let frame = self.webcam_outgoing_frame.lock().unwrap().take();
        if let Some(frame) = frame {
            self.chat.set_peer_video_frame(LOCAL_WEBCAM_PEER_ID, frame.clone());
            self.send_session_bytes_best_effort(ChatMessage::VideoFrame(frame).to_bytes());
        }
        self.chat
            .prune_stale_peer_video_frames(std::time::Duration::from_secs(3));
    }

    pub(crate) fn drain_voice_packets(&mut self) {
        if !self.config.voice_enabled {
            let _ = self.voice.drain_outgoing();
            return;
        }
        let packets = self.voice.drain_outgoing();
        self.voice_packets_sent = self.voice_packets_sent.saturating_add(packets.len() as u64);
        for packet in packets {
            let msg = ChatMessage::Voice(packet);
            self.send_session_bytes_best_effort(msg.to_bytes());
        }
    }
}

// ---------------------------------------------------------------------------
// eframe::App
// ---------------------------------------------------------------------------

impl eframe::App for SlpChatApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        if self.config.overlay_enabled {
            egui::Color32::TRANSPARENT.to_normalized_gamma_f32()
        } else {
            egui::Color32::from_rgba_unmultiplied(12, 12, 12, 180).to_normalized_gamma_f32()
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.start_background_repaint_waker(ctx);
        self.apply_theme(ctx);
        if !self.config.overlay_enabled {
            self.overlay_visible = false;
        }

        // On Windows --startup: we must wait until frame 2 to stealth-hide
        // the window.  On frame 1, eframe's post_rendering() calls winit's
        // set_visible(true) which overwrites extended styles.
        #[cfg(target_os = "windows")]
        if self.launched_at_startup && self.startup_hide_frame < 2 {
            self.startup_hide_frame += 1;
            if self.startup_hide_frame == 2 {
                let hwnd = find_main_hwnd();
                if hwnd != 0 as _ {
                    stealth_hide_window(hwnd);
                }
            }
        }

        // On Windows: when the window is hidden, check for a show-signal
        // file written by the second instance.
        #[cfg(target_os = "windows")]
        if !self.window_shown {
            let signal_path = profile::show_signal_path();
            if signal_path.exists() {
                let _ = std::fs::remove_file(&signal_path);
                self.window_shown = true;
                let hwnd = find_main_hwnd();
                if hwnd != 0 as _ {
                    stealth_show_window(hwnd);
                }
            }
        }

        // On macOS: detect when the app has been activated externally.
        #[cfg(target_os = "macos")]
        if self.launched_at_startup && !self.window_shown {
            let is_active = ctx.input(|i| i.focused);
            if is_active {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                self.window_shown = true;
            }
        }

        // Save theme on shutdown so overlay settings always persist.
        if ctx.input(|i| i.viewport().close_requested()) {
            self.remember_webcam_box_layout();
            self.config.save();
            profile::save_theme(&self.theme);
        }

        // When overlay is enabled, intercept the close button: hide instead of quit.
        if self.config.overlay_enabled {
            if ctx.input(|i| i.viewport().close_requested()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                #[cfg(target_os = "windows")]
                {
                    let hwnd = find_main_hwnd();
                    if hwnd != 0 as _ {
                        stealth_hide_window(hwnd);
                    }
                }
                #[cfg(not(target_os = "windows"))]
                {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                }
                self.window_shown = false;
            }
        }

        match self.screen {
            AppScreen::FirstRun => {
                self.render_first_run(ctx);
            }
            AppScreen::Editing => {
                self.render_editing(ctx);
            }
            AppScreen::InSession => {
                self.poll_session();

                if self.session.is_some() {
                    self.start_background_io_worker();
                    self.poll_webcam_stream();
                    self.announce_if_needed();
                    self.poll_incoming();
                    if self.reveal_on_peer_connect {
                        self.reveal_on_peer_connect = false;
                        if self.config.overlay_enabled || !self.window_shown {
                            self.show_chat_window(ctx);
                        }
                    }
                    if self.joined {
                        // Key capture for PTT binding
                        if self.capturing_ptt_key {
                            if let Some(key) = ctx.input(|i| {
                                i.events.iter().find_map(|e| {
                                    if let egui::Event::Key { key, pressed: true, .. } = e {
                                        Some(*key)
                                    } else {
                                        None
                                    }
                                })
                            }) {
                                self.config.ptt_key = format!("{:?}", key).to_uppercase();
                                self.capturing_ptt_key = false;
                            }
                        }
                        self.handle_ptt_input(ctx);
                        if !self.background_io_started {
                            self.drain_voice_packets();
                        }
                        if let Some(img) = self.pending_image.take() {
                            self.send_image(img);
                        }
                    }
                }

                self.render_webcam_boxes(ctx);
                self.render_session(ctx);
                self.render_options_window(ctx);
                self.render_editing_theme(ctx);
                self.render_dolphin_overlay(ctx);
            }
        }

        if self.session.is_some() || self.connect_handle.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
    }
}
