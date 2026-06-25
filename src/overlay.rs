use eframe::egui;

use crate::app::{SlpChatApp, LOCAL_WEBCAM_PEER_ID};
use crate::chat::{format_time, ChatEntry};
use crate::theme::{paint_letter_avatar, rgb_to_color32, Colors};

const MEDIA_W: f32 = 132.0;
const GAP: f32 = 8.0;
const INPUT_H: f32 = 30.0;
const PAD: f32 = 6.0;
const TALK_STROKE: f32 = 3.0;

#[cfg(target_os = "windows")]
fn find_dolphin_rect() -> Option<(usize, i32, i32, i32, i32)> {
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowRect, GetWindowTextW, IsIconic, IsWindowVisible,
    };

    struct CallbackData {
        result: Option<(usize, i32, i32, i32, i32)>,
    }

    unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let data = &mut *(lparam as *mut CallbackData);
        if IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0 {
            return 1;
        }
        let mut title = [0u16; 256];
        let len = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32);
        if len > 0 {
            let lower = String::from_utf16_lossy(&title[..len as usize]).to_lowercase();
            if (lower.contains("dolphin") || lower.contains("slippi") || lower.contains("melee"))
                && !lower.contains("slp chat")
                && !lower.contains("slp overlay")
            {
                let mut rect: RECT = std::mem::zeroed();
                if GetWindowRect(hwnd, &mut rect) != 0 {
                    data.result = Some((
                        hwnd as usize,
                        rect.left,
                        rect.top,
                        rect.right - rect.left,
                        rect.bottom - rect.top,
                    ));
                    return 0;
                }
            }
        }
        1
    }

    let mut data = CallbackData { result: None };
    unsafe { EnumWindows(Some(enum_cb), &mut data as *mut _ as LPARAM) };
    data.result
}

#[cfg(target_os = "linux")]
fn find_dolphin_rect() -> Option<(usize, i32, i32, i32, i32)> {
    find_dolphin_rect_hyprctl().or_else(find_dolphin_rect_xdotool)
}

#[cfg(target_os = "linux")]
fn find_dolphin_rect_hyprctl() -> Option<(usize, i32, i32, i32, i32)> {
    let output = std::process::Command::new("hyprctl")
        .args(["clients", "-j"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let clients = serde_json::from_slice::<serde_json::Value>(&output.stdout).ok()?;
    for client in clients.as_array()? {
        let title = client.get("title").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
        let class = client.get("class").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
        let is_game = title.contains("dolphin")
            || title.contains("slippi")
            || title.contains("melee")
            || class.contains("dolphin")
            || class.contains("slippi");
        if !is_game || title.contains("slp chat") || title.contains("slp overlay") {
            continue;
        }
        let at = client.get("at")?.as_array()?;
        let size = client.get("size")?.as_array()?;
        let x = at.first()?.as_i64()? as i32;
        let y = at.get(1)?.as_i64()? as i32;
        let w = size.first()?.as_i64()? as i32;
        let h = size.get(1)?.as_i64()? as i32;
        if w > 0 && h > 0 {
            return Some((0, x, y, w, h));
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn find_dolphin_rect_xdotool() -> Option<(usize, i32, i32, i32, i32)> {
    let output = std::process::Command::new("xdotool")
        .args(["search", "--name", "(?i)(dolphin|slippi|melee)"])
        .output()
        .ok()?;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let wid: u64 = line.trim().parse().ok()?;
        let name_out = std::process::Command::new("xdotool")
            .args(["getwindowname", &wid.to_string()])
            .output()
            .ok()?;
        let name = String::from_utf8_lossy(&name_out.stdout).to_lowercase();
        if name.contains("slp chat") || name.contains("slp overlay") {
            continue;
        }
        let geom = std::process::Command::new("xdotool")
            .args(["getwindowgeometry", "--shell", &wid.to_string()])
            .output()
            .ok()?;
        let mut x = 0;
        let mut y = 0;
        let mut w = 0;
        let mut h = 0;
        for gline in String::from_utf8_lossy(&geom.stdout).lines() {
            if let Some(v) = gline.strip_prefix("X=") { x = v.parse().unwrap_or(0); }
            if let Some(v) = gline.strip_prefix("Y=") { y = v.parse().unwrap_or(0); }
            if let Some(v) = gline.strip_prefix("WIDTH=") { w = v.parse().unwrap_or(0); }
            if let Some(v) = gline.strip_prefix("HEIGHT=") { h = v.parse().unwrap_or(0); }
        }
        if w > 0 && h > 0 {
            return Some((wid as usize, x, y, w, h));
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn find_dolphin_rect() -> Option<(usize, i32, i32, i32, i32)> {
    let script = r#"
tell application "System Events"
    set allProcs to every process whose visible is true
    repeat with p in allProcs
        set pName to name of p
        set pLower to do shell script "echo " & quoted form of pName & " | tr '[:upper:]' '[:lower:]'"
        if pLower contains "dolphin" or pLower contains "slippi" or pLower contains "melee" then
            if not (pLower contains "slp chat") then
                set wins to windows of p
                if (count of wins) > 0 then
                    set w to item 1 of wins
                    set {px, py} to position of w
                    set {pw, ph} to size of w
                    return (px as text) & "," & (py as text) & "," & (pw as text) & "," & (ph as text)
                end if
            end if
        end if
    end repeat
end tell
"#;
    let output = std::process::Command::new("osascript").arg("-e").arg(script).output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<i32> = stdout.trim().split(',').filter_map(|p| p.trim().parse().ok()).collect();
    if parts.len() == 4 && parts[2] > 0 && parts[3] > 0 {
        Some((0, parts[0], parts[1], parts[2], parts[3]))
    } else {
        None
    }
}

impl SlpChatApp {
    pub(crate) fn render_dolphin_overlay(&mut self, ctx: &egui::Context) {
        if !self.config.overlay_enabled || !self.overlay_visible {
            return;
        }

        self.overlay_hint_passes = self.overlay_hint_passes.saturating_sub(1);

        let Some((_, dx, dy, dw, dh)) = find_dolphin_rect() else {
            return;
        };

        let viewport_pos = egui::pos2(dx as f32, dy as f32);
        let viewport_size = egui::vec2(dw.max(1) as f32, dh.max(1) as f32);
        let content = self.overlay_content_rect(viewport_size);

        let viewport = egui::ViewportBuilder::default()
            .with_title("SLP Overlay")
            .with_app_id("slpauth_app_overlay")
            .with_inner_size([viewport_size.x, viewport_size.y])
            .with_min_inner_size([viewport_size.x, viewport_size.y])
            .with_max_inner_size([viewport_size.x, viewport_size.y])
            .with_position(viewport_pos)
            .with_decorations(false)
            .with_resizable(false)
            .with_transparent(true)
            .with_taskbar(false)
            .with_always_on_top();

        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("slp_overlay"),
            viewport,
            |overlay_ctx, _| self.render_old_style_overlay(overlay_ctx, content),
        );

        if self.overlay_hint_passes % 2 == 0 {
            Self::apply_compositor_overlay_hints(viewport_pos, viewport_size);
        }
    }

    fn overlay_content_rect(&self, viewport_size: egui::Vec2) -> egui::Rect {
        let overlay_w = self.theme.overlay_window_w.clamp(160, 1600) as f32;
        let overlay_h = self.theme.overlay_window_h.clamp(80, 1000) as f32;
        let dx = self.theme.overlay_distance_x.max(0) as f32;
        let dy = self.theme.overlay_distance_y.max(0) as f32;
        let pos = match self.theme.overlay_position {
            1 => egui::pos2((viewport_size.x - overlay_w - dx).max(0.0), dy),
            2 => egui::pos2(dx, (viewport_size.y - overlay_h - dy).max(0.0)),
            3 => egui::pos2((viewport_size.x - overlay_w - dx).max(0.0), (viewport_size.y - overlay_h - dy).max(0.0)),
            _ => egui::pos2(dx, dy),
        };
        egui::Rect::from_min_size(pos, egui::vec2(overlay_w, overlay_h))
    }

    fn render_old_style_overlay(&mut self, ctx: &egui::Context, rect: egui::Rect) {
        if ctx.input(|i| i.viewport().close_requested()) {
            self.overlay_visible = false;
            return;
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::TRANSPARENT))
            .show(ctx, |ui| {
                let full_rect = ui.max_rect();
                let painter = ui.painter_at(full_rect);
                let text_size = self.theme.overlay_text_size.clamp(8, 48) as f32;
                let font_id = egui::FontId::proportional(text_size);
                let small_font = egui::FontId::proportional((text_size - 2.0).max(8.0));
                let text_color = readable_overlay_text_color(rgb_to_color32(&self.theme.text_color));
                let outline_color = rgb_to_color32(&self.theme.overlay_outline_color);
                let outline_t = self.theme.overlay_outline_thickness.max(0) as f32;
                let media_on_right = matches!(self.theme.overlay_position, 0 | 2);
                let participants = self.overlay_participants(ui.ctx());
                let sidebar_w = if participants.is_empty() { 0.0 } else { MEDIA_W };
                let sidebar_x = if media_on_right { rect.max.x - sidebar_w } else { rect.min.x };
                let text_left = if media_on_right { rect.min.x + PAD } else { rect.min.x + sidebar_w + GAP + PAD };
                let text_right = if media_on_right { rect.max.x - sidebar_w - GAP - PAD } else { rect.max.x - PAD };
                let input_top = rect.max.y - INPUT_H;
                let msg_bottom = input_top;
                let wrap_width = (text_right - text_left).max(24.0);

                self.paint_overlay_participants(
                    ui,
                    &painter,
                    sidebar_x,
                    rect.min.y + PAD,
                    sidebar_w,
                    rect.height() - PAD * 2.0,
                    &participants,
                    &small_font,
                );

                let messages = self.overlay_messages();
                let (main_galleys, outline_galleys) = layout_overlay_messages(
                    ui,
                    &messages,
                    &font_id,
                    wrap_width,
                    text_color,
                    outline_color,
                );
                let total_h: f32 = main_galleys.iter().map(|g| g.size().y).sum();
                let avail_h = (msg_bottom - rect.min.y - PAD).max(0.0);
                let start_idx = if total_h > avail_h {
                    let mut skip_h = total_h - avail_h;
                    let mut idx = 0;
                    while idx < main_galleys.len() && skip_h > 0.0 {
                        skip_h -= main_galleys[idx].size().y;
                        idx += 1;
                    }
                    idx.min(main_galleys.len())
                } else {
                    0
                };
                let mut y = if total_h <= avail_h { msg_bottom - total_h } else { rect.min.y + PAD };
                if start_idx > 0 {
                    y = msg_bottom;
                    for i in (start_idx..main_galleys.len()).rev() {
                        y -= main_galleys[i].size().y;
                    }
                }
                for i in start_idx..main_galleys.len() {
                    if y >= msg_bottom { break; }
                    let pos = egui::pos2(text_left, y);
                    if outline_t > 0.0 {
                        if let Some(outline) = &outline_galleys[i] {
                            for ox in [-outline_t, 0.0, outline_t] {
                                for oy in [-outline_t, 0.0, outline_t] {
                                    if ox != 0.0 || oy != 0.0 {
                                        painter.galley(pos + egui::vec2(ox, oy), outline.clone(), outline_color);
                                    }
                                }
                            }
                        }
                    }
                    painter.galley(pos, main_galleys[i].clone(), text_color);
                    y += main_galleys[i].size().y;
                }

                let input_rect = egui::Rect::from_min_max(egui::pos2(rect.min.x, input_top), rect.max);
                painter.rect_filled(input_rect, 0.0, egui::Color32::from_rgba_premultiplied(30, 30, 30, 235));
                painter.line_segment(
                    [input_rect.left_top(), egui::pos2(input_rect.max.x, input_rect.min.y)],
                    egui::Stroke::new(1.0, rgb_to_color32(&self.theme.separator_color)),
                );
                let text_area = input_rect.shrink2(egui::vec2(6.0, 3.0));
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(text_area)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                let response = child.add(
                    egui::TextEdit::singleline(&mut self.input_text)
                        .desired_width(text_area.width())
                        .hint_text("Type a message...")
                        .font(font_id)
                        .frame(false),
                );
                if response.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.send_text();
                    response.request_focus();
                }
            });
    }

    fn overlay_messages(&self) -> Vec<OverlayMessage> {
        let text_color = readable_overlay_text_color(rgb_to_color32(&self.theme.text_color));
        let self_color = rgb_to_color32(&self.name_color);
        let mut messages = Vec::new();
        for entry in self.chat.entries.iter().rev() {
            if messages.len() >= 10 { break; }
            match entry {
                ChatEntry::Text { sender, content, is_self, timestamp } => {
                    let color = if *is_self { self_color } else { self.peer_color_by_name(sender) };
                    messages.push(OverlayMessage {
                        time: format_time(timestamp),
                        sender: Some(sender.clone()),
                        content: content.clone(),
                        color,
                        text_color,
                    });
                }
                ChatEntry::Image { sender, is_self, timestamp, .. } => {
                    let color = if *is_self { self_color } else { self.peer_color_by_name(sender) };
                    messages.push(OverlayMessage {
                        time: format_time(timestamp),
                        sender: Some(sender.clone()),
                        content: "sent an image".to_string(),
                        color,
                        text_color,
                    });
                }
                ChatEntry::System { msg, timestamp } => {
                    messages.push(OverlayMessage {
                        time: format_time(timestamp),
                        sender: None,
                        content: msg.clone(),
                        color: Colors::TEXT_SYSTEM,
                        text_color,
                    });
                }
            }
        }
        messages.reverse();
        messages
    }

    fn peer_color_by_name(&self, sender: &str) -> egui::Color32 {
        self.chat
            .peer_names
            .iter()
            .find(|(_, name)| name.as_str() == sender)
            .and_then(|(id, _)| self.chat.peer_colors.get(id).copied())
            .unwrap_or(Colors::TEXT_NAME_OTHER)
    }

    fn overlay_participants(&mut self, ctx: &egui::Context) -> Vec<OverlayParticipant> {
        let mut participants = Vec::new();
        participants.push(OverlayParticipant {
            peer_id: LOCAL_WEBCAM_PEER_ID.to_string(),
            name: self.username.clone(),
            color: rgb_to_color32(&self.name_color),
            talking: self.voice.is_self_talking(),
            texture: self.overlay_self_media_texture(ctx),
        });
        let peers: Vec<(String, String, egui::Color32, bool)> = self
            .chat
            .peer_names
            .iter()
            .map(|(peer_id, name)| {
                let color = self.chat.peer_colors.get(peer_id).copied().unwrap_or(Colors::TEXT_NAME_OTHER);
                (peer_id.clone(), name.clone(), color, self.voice.is_peer_talking(peer_id))
            })
            .collect();
        for (peer_id, name, color, talking) in peers {
            let texture = self.overlay_peer_media_texture(ctx, &peer_id);
            participants.push(OverlayParticipant { peer_id, name, color, talking, texture });
        }
        participants
    }

    fn overlay_self_media_texture(&mut self, ctx: &egui::Context) -> Option<egui::TextureHandle> {
        if let Some(tex) = self.overlay_video_texture(ctx, LOCAL_WEBCAM_PEER_ID) {
            return Some(tex);
        }
        let avatar_png = self.avatar_png.clone();
        SlpChatApp::load_avatar_texture(&avatar_png, &mut self.avatar_texture, ctx, "overlay_self_avatar");
        self.avatar_texture.clone()
    }

    fn overlay_peer_media_texture(&mut self, ctx: &egui::Context, peer_id: &str) -> Option<egui::TextureHandle> {
        if let Some(tex) = self.overlay_video_texture(ctx, peer_id) {
            return Some(tex);
        }
        self.chat.avatar_texture(peer_id, ctx)
    }

    fn overlay_video_texture(&mut self, ctx: &egui::Context, peer_id: &str) -> Option<egui::TextureHandle> {
        let jpeg_data = self.chat.peer_video_frames.get(peer_id).cloned()?;
        if !self.chat.peer_video_textures.contains_key(peer_id) {
            if let Ok(img) = image::load_from_memory(&jpeg_data) {
                let rgba = img.to_rgba8();
                let tex = ctx.load_texture(
                    format!("overlay_webcam_{peer_id}"),
                    egui::ColorImage::from_rgba_unmultiplied(
                        [rgba.width() as usize, rgba.height() as usize],
                        rgba.as_raw(),
                    ),
                    egui::TextureOptions::LINEAR,
                );
                self.chat.peer_video_textures.insert(peer_id.to_string(), tex);
            }
        }
        self.chat.peer_video_textures.get(peer_id).cloned()
    }

    fn paint_overlay_participants(
        &mut self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        participants: &[OverlayParticipant],
        font: &egui::FontId,
    ) {
        if width <= 0.0 || participants.is_empty() {
            return;
        }
        let avatar_size = 48.0;
        let mut py = y;
        for participant in participants {
            if py + avatar_size > y + height {
                break;
            }
            let avatar_rect = egui::Rect::from_min_size(egui::pos2(x + PAD, py), egui::vec2(avatar_size, avatar_size));
            if let Some(tex) = &participant.texture {
                painter.image(tex.id(), avatar_rect, unit_uv(), egui::Color32::WHITE);
            } else {
                paint_letter_avatar(ui, avatar_rect, &participant.name, participant.color);
            }
            if participant.talking {
                painter.rect_stroke(
                    avatar_rect,
                    5.0,
                    egui::Stroke::new(TALK_STROKE, participant.color),
                    egui::StrokeKind::Inside,
                );
            }
            let display = short_name(&participant.name);
            let text_pos = egui::pos2(avatar_rect.max.x + 5.0, py + (avatar_size - font.size) / 2.0);
            let galley = ui.painter().layout_no_wrap(display, font.clone(), participant.color);
            let max_right = x + width - PAD;
            let clip = egui::Rect::from_min_max(text_pos, egui::pos2(max_right, text_pos.y + galley.size().y));
            painter.with_clip_rect(clip).galley(text_pos, galley, participant.color);
            py += avatar_size + PAD;
        }
    }
}

struct OverlayParticipant {
    #[allow(dead_code)]
    peer_id: String,
    name: String,
    color: egui::Color32,
    talking: bool,
    texture: Option<egui::TextureHandle>,
}

struct OverlayMessage {
    time: String,
    sender: Option<String>,
    content: String,
    color: egui::Color32,
    text_color: egui::Color32,
}

fn layout_overlay_messages(
    ui: &egui::Ui,
    messages: &[OverlayMessage],
    font_id: &egui::FontId,
    wrap_width: f32,
    text_color: egui::Color32,
    outline_color: egui::Color32,
) -> (Vec<std::sync::Arc<egui::Galley>>, Vec<Option<std::sync::Arc<egui::Galley>>>) {
    let mut main = Vec::with_capacity(messages.len());
    let mut outline = Vec::with_capacity(messages.len());
    for msg in messages {
        let full_text = if let Some(sender) = &msg.sender {
            format!("[{}] {}: {}", msg.time, sender, msg.content)
        } else {
            format!("[{}] {}", msg.time, msg.content)
        };
        let mut outline_job = egui::text::LayoutJob::single_section(
            full_text,
            egui::TextFormat { font_id: font_id.clone(), color: outline_color, ..Default::default() },
        );
        outline_job.wrap.max_width = wrap_width;
        outline.push(Some(ui.fonts(|f| f.layout_job(outline_job))));

        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = wrap_width;
        if let Some(sender) = &msg.sender {
            job.append(
                &format!("[{}] ", msg.time),
                0.0,
                egui::TextFormat { font_id: font_id.clone(), color: msg.text_color, ..Default::default() },
            );
            job.append(
                sender,
                0.0,
                egui::TextFormat { font_id: font_id.clone(), color: msg.color, ..Default::default() },
            );
            job.append(
                ": ",
                0.0,
                egui::TextFormat { font_id: font_id.clone(), color: msg.color, ..Default::default() },
            );
            job.append(
                &msg.content,
                0.0,
                egui::TextFormat { font_id: font_id.clone(), color: text_color, ..Default::default() },
            );
        } else {
            job.append(
                &format!("[{}] {}", msg.time, msg.content),
                0.0,
                egui::TextFormat { font_id: font_id.clone(), color: msg.color, ..Default::default() },
            );
        }
        main.push(ui.fonts(|f| f.layout_job(job)));
    }
    (main, outline)
}

fn readable_overlay_text_color(color: egui::Color32) -> egui::Color32 {
    let luma = 0.2126 * color.r() as f32 + 0.7152 * color.g() as f32 + 0.0722 * color.b() as f32;
    if luma < 35.0 { egui::Color32::WHITE } else { color }
}

fn short_name(name: &str) -> String {
    const MAX: usize = 8;
    if name.chars().count() > MAX {
        format!("{}…", name.chars().take(MAX).collect::<String>())
    } else {
        name.to_string()
    }
}

fn unit_uv() -> egui::Rect {
    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0))
}
