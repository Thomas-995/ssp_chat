use eframe::egui;

use crate::app::{SlpChatApp, LOCAL_WEBCAM_PEER_ID};
use crate::chat::{format_time, ChatEntry};
use crate::theme::{paint_letter_avatar, rgb_to_color32, Colors};

const MEDIA_W: f32 = 132.0;
const GAP: f32 = 8.0;
const TALK_STROKE: f32 = 3.0;

impl SlpChatApp {
    pub(crate) fn render_dolphin_overlay(&mut self, ctx: &egui::Context) {
        if !self.config.overlay_enabled || !self.overlay_visible {
            return;
        }

        let (pos, size, media_left) = self.overlay_geometry(ctx);
        let viewport = egui::ViewportBuilder::default()
            .with_title("SLP Chat Overlay")
            .with_app_id("slpauth_app_overlay")
            .with_inner_size([size.x, size.y])
            .with_min_inner_size([size.x, size.y])
            .with_max_inner_size([size.x, size.y])
            .with_position(pos)
            .with_decorations(false)
            .with_resizable(false)
            .with_transparent(true)
            .with_taskbar(false)
            .with_always_on_top();

        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("slpauth_overlay_viewport"),
            viewport,
            |overlay_ctx, _| self.render_overlay_contents(overlay_ctx, media_left),
        );

        if self.overlay_hint_passes > 0 {
            self.overlay_hint_passes -= 1;
            if self.overlay_hint_passes % 2 == 0 {
                Self::apply_compositor_overlay_hints(pos, size);
            }
        }
    }

    fn overlay_geometry(&self, ctx: &egui::Context) -> (egui::Pos2, egui::Vec2, bool) {
        let text_w = self.theme.overlay_window_w.clamp(180, 1400) as f32;
        let text_h = self.theme.overlay_window_h.clamp(120, 900) as f32;
        let size = egui::vec2(text_w + MEDIA_W + GAP, text_h);
        let monitor = ctx
            .input(|i| i.viewport().monitor_size)
            .unwrap_or(egui::vec2(1920.0, 1080.0));
        let dx = self.theme.overlay_distance_x.max(0) as f32;
        let dy = self.theme.overlay_distance_y.max(0) as f32;
        let pos = match self.theme.overlay_position {
            1 => egui::pos2((monitor.x - size.x - dx).max(0.0), dy),
            2 => egui::pos2(dx, (monitor.y - size.y - dy).max(0.0)),
            3 => egui::pos2((monitor.x - size.x - dx).max(0.0), (monitor.y - size.y - dy).max(0.0)),
            _ => egui::pos2(dx, dy),
        };
        let media_left = matches!(self.theme.overlay_position, 0 | 2);
        (pos, size, media_left)
    }

    fn render_overlay_contents(&mut self, ctx: &egui::Context, media_left: bool) {
        if ctx.input(|i| i.viewport().close_requested()) {
            self.overlay_visible = false;
            return;
        }

        let text_w = self.theme.overlay_window_w.clamp(180, 1400) as f32;
        let text_h = self.theme.overlay_window_h.clamp(120, 900) as f32;

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::TRANSPARENT))
            .show(ctx, |ui| {
                ui.visuals_mut().override_text_color = Some(rgb_to_color32(&self.theme.text_color));
                ui.spacing_mut().item_spacing = egui::vec2(GAP, 0.0);
                ui.horizontal(|ui| {
                    if media_left {
                        self.render_overlay_media_column(ui, text_h);
                        ui.add_space(GAP);
                    }
                    self.render_overlay_chat_box(ui, text_w, text_h);
                    if !media_left {
                        ui.add_space(GAP);
                        self.render_overlay_media_column(ui, text_h);
                    }
                });
            });
    }

    fn render_overlay_chat_box(&mut self, ui: &mut egui::Ui, width: f32, height: f32) {
        let panel = rgb_to_color32(&self.theme.text_panel_color).linear_multiply(0.72);
        let separator = rgb_to_color32(&self.theme.separator_color);
        let input_h = if self.joined { 34.0 } else { 0.0 };

        egui::Frame::NONE
            .fill(panel)
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.set_min_size(egui::vec2(width, height));
                let scroll_h = (height - input_h - 22.0).max(60.0);
                egui::ScrollArea::vertical()
                    .max_height(scroll_h)
                    .stick_to_bottom(true)
                    .auto_shrink([false; 2])
                    .show(ui, |ui| self.render_overlay_messages(ui));

                if self.joined {
                    ui.painter().hline(
                        ui.min_rect().left()..=ui.max_rect().right(),
                        ui.cursor().top() + 2.0,
                        egui::Stroke::new(1.0, separator),
                    );
                    ui.add_space(6.0);
                    self.render_overlay_input(ui, width);
                }
            });
    }

    fn render_overlay_media_column(&mut self, ui: &mut egui::Ui, height: f32) {
        let media_panel = rgb_to_color32(&self.theme.text_panel_color).linear_multiply(0.55);
        egui::Frame::NONE
            .fill(media_panel)
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(egui::Margin::same(6))
            .show(ui, |ui| {
                ui.set_min_size(egui::vec2(MEDIA_W - 12.0, height));
                egui::ScrollArea::vertical()
                    .max_height(height - 12.0)
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        let ctx = ui.ctx().clone();
                        self.render_overlay_avatar_list(ui, &ctx);
                        self.render_overlay_webcams(ui, &ctx);
                    });
            });
    }

    fn render_overlay_avatar_list(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let self_color = rgb_to_color32(&self.name_color);
        self.render_overlay_avatar(ui, ctx, None, &self.username.clone(), self_color, self.voice.is_self_talking());

        let peers: Vec<(String, String, egui::Color32, bool)> = self
            .chat
            .peer_names
            .iter()
            .map(|(peer_id, name)| {
                let color = self
                    .chat
                    .peer_colors
                    .get(peer_id)
                    .copied()
                    .unwrap_or(Colors::TEXT_NAME_OTHER);
                (peer_id.clone(), name.clone(), color, self.voice.is_peer_talking(peer_id))
            })
            .collect();
        for (peer_id, name, color, talking) in peers {
            self.render_overlay_avatar(ui, ctx, Some(&peer_id), &name, color, talking);
        }
    }

    fn render_overlay_avatar(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        peer_id: Option<&str>,
        name: &str,
        color: egui::Color32,
        talking: bool,
    ) {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(34.0, 34.0), egui::Sense::hover());
            if let Some(peer_id) = peer_id {
                if let Some(tex) = self.chat.avatar_texture(peer_id, ctx) {
                    ui.painter().image(tex.id(), rect, unit_uv(), egui::Color32::WHITE);
                } else {
                    paint_letter_avatar(ui, rect, name, color);
                }
            } else {
                let avatar_png = self.avatar_png.clone();
                SlpChatApp::load_avatar_texture(&avatar_png, &mut self.avatar_texture, ctx, "overlay_self_avatar");
                if let Some(tex) = &self.avatar_texture {
                    ui.painter().image(tex.id(), rect, unit_uv(), egui::Color32::WHITE);
                } else {
                    paint_letter_avatar(ui, rect, name, color);
                }
            }
            if talking {
                ui.painter().rect_stroke(rect, 5.0, egui::Stroke::new(TALK_STROKE, color), egui::StrokeKind::Inside);
            }
            ui.label(egui::RichText::new(short_name(name)).color(color).size(11.0));
        });
        ui.add_space(4.0);
    }

    fn render_overlay_webcams(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let peer_ids: Vec<String> = self.chat.peer_video_frames.keys().cloned().collect();
        if peer_ids.is_empty() {
            return;
        }
        ui.add_space(4.0);
        for peer_id in peer_ids {
            self.render_overlay_webcam(ui, ctx, &peer_id);
            ui.add_space(6.0);
        }
    }

    fn render_overlay_webcam(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, peer_id: &str) {
        let Some(jpeg_data) = self.chat.peer_video_frames.get(peer_id).cloned() else { return; };
        if !self.chat.peer_video_textures.contains_key(peer_id) {
            if let Ok(img) = image::load_from_memory(&jpeg_data) {
                let rgba = img.to_rgba8();
                let tex = ctx.load_texture(
                    format!("overlay_webcam_{peer_id}"),
                    egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()),
                    egui::TextureOptions::LINEAR,
                );
                self.chat.peer_video_textures.insert(peer_id.to_string(), tex);
            }
        }
        let Some(tex) = self.chat.peer_video_textures.get(peer_id) else { return; };

        let size = tex.size();
        let aspect = size[0] as f32 / size[1].max(1) as f32;
        let rect_size = egui::vec2(MEDIA_W - 18.0, (MEDIA_W - 18.0) / aspect).min(egui::vec2(MEDIA_W - 18.0, 88.0));
        let (rect, _) = ui.allocate_exact_size(rect_size, egui::Sense::hover());
        ui.painter().image(tex.id(), rect, unit_uv(), egui::Color32::WHITE);

        let (name, color, talking) = if peer_id == LOCAL_WEBCAM_PEER_ID {
            (self.username.clone(), rgb_to_color32(&self.name_color), self.voice.is_self_talking())
        } else {
            (
                self.chat.peer_display_name(peer_id),
                self.chat.peer_colors.get(peer_id).copied().unwrap_or(Colors::TEXT_NAME_OTHER),
                self.voice.is_peer_talking(peer_id),
            )
        };
        if talking {
            ui.painter().rect_stroke(rect, 3.0, egui::Stroke::new(TALK_STROKE, color), egui::StrokeKind::Inside);
        }

        let galley = ui.painter().layout_no_wrap(short_name(&name), egui::FontId::proportional(10.0), color);
        let bg = egui::Rect::from_min_size(
            egui::pos2(rect.right() - galley.rect.width() - 8.0, rect.top() + 3.0),
            egui::vec2(galley.rect.width() + 6.0, galley.rect.height() + 4.0),
        );
        ui.painter().rect_filled(bg, 3.0, egui::Color32::from_black_alpha(150));
        ui.painter().galley(bg.min + egui::vec2(3.0, 2.0), galley, color);
    }

    fn render_overlay_messages(&mut self, ui: &mut egui::Ui) {
        let text_color = rgb_to_color32(&self.theme.text_color);
        let self_color = rgb_to_color32(&self.name_color);
        let peer_colors = self
            .chat
            .peer_names
            .iter()
            .filter_map(|(peer_id, name)| self.chat.peer_colors.get(peer_id).copied().map(|color| (name.clone(), color)))
            .collect::<std::collections::HashMap<_, _>>();

        for entry in self.chat.entries.iter().rev().take(60).collect::<Vec<_>>().into_iter().rev() {
            match entry {
                ChatEntry::Text { sender, content, is_self, timestamp } => {
                    let color = if *is_self { self_color } else { peer_colors.get(sender).copied().unwrap_or(Colors::TEXT_NAME_OTHER) };
                    overlay_message(ui, &format_time(timestamp), sender, OverlayMessage::Text(content), color, text_color);
                }
                ChatEntry::Image { sender, is_self, timestamp, .. } => {
                    let color = if *is_self { self_color } else { peer_colors.get(sender).copied().unwrap_or(Colors::TEXT_NAME_OTHER) };
                    overlay_message(ui, &format_time(timestamp), sender, OverlayMessage::Image, color, text_color);
                }
                ChatEntry::System { msg, timestamp } => {
                    ui.label(egui::RichText::new(format!("[{}] {}", format_time(timestamp), msg)).color(Colors::TEXT_SYSTEM));
                }
            }
        }
    }

    fn render_overlay_input(&mut self, ui: &mut egui::Ui, width: f32) {
        ui.horizontal(|ui| {
            let response = ui.add_sized(
                [width - 66.0, 30.0],
                egui::TextEdit::singleline(&mut self.input_text).hint_text("Type a message..."),
            );
            let send_clicked = ui.button("Send").clicked();
            let enter_pressed = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if send_clicked || enter_pressed {
                self.send_text();
                response.request_focus();
            }
        });
    }
}

enum OverlayMessage<'a> {
    Text(&'a str),
    Image,
}

fn overlay_message(
    ui: &mut egui::Ui,
    time: &str,
    sender: &str,
    message: OverlayMessage<'_>,
    name_color: egui::Color32,
    text_color: egui::Color32,
) {
    let mut job = egui::text::LayoutJob::default();
    job.append(&format!("[{time}] "), 0.0, text_format(ui, text_color));
    job.append(sender, 0.0, text_format(ui, name_color));
    match message {
        OverlayMessage::Text(content) => {
            job.append(": ", 0.0, text_format(ui, name_color));
            job.append(content, 0.0, text_format(ui, text_color));
        }
        OverlayMessage::Image => job.append(" sent an image", 0.0, text_format(ui, text_color)),
    }
    ui.label(job);
}

fn text_format(ui: &egui::Ui, color: egui::Color32) -> egui::text::TextFormat {
    egui::text::TextFormat {
        font_id: egui::TextStyle::Body.resolve(ui.style()),
        color,
        ..Default::default()
    }
}

fn short_name(name: &str) -> String {
    const MAX: usize = 12;
    if name.chars().count() > MAX {
        format!("{}…", name.chars().take(MAX).collect::<String>())
    } else {
        name.to_string()
    }
}

fn unit_uv() -> egui::Rect {
    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0))
}
