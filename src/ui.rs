//! Main egui UI rendering.

use eframe::egui;

use crate::app::{webcam_device_names, AppScreen, SlpChatApp};
use crate::chat::{format_time, ChatEntry};
use crate::profile;
use crate::theme::{paint_letter_avatar, rgb_to_color32, save_theme, theme_color_row, Colors, ThemeHexFields};
use crate::voice::VoiceEngine;

#[derive(Clone, Copy)]
enum ButtonIcon {
    Send,
    Image,
    Mic,
    MicMuted,
    Webcam,
    Stop,
}

impl SlpChatApp {
    pub(crate) fn apply_theme(&self, ctx: &egui::Context) {
        let mut visuals = egui::Visuals::dark();
        let bg = rgb_to_color32(&self.theme.bg_color);
        let text_panel = rgb_to_color32(&self.theme.text_panel_color);
        let separator = rgb_to_color32(&self.theme.separator_color);
        let text = rgb_to_color32(&self.theme.text_color);
        let button = rgb_to_color32(&self.theme.button_color);
        let button_radius = egui::CornerRadius::same(self.theme.button_radius.clamp(0.0, 24.0) as u8);

        visuals.panel_fill = bg;
        visuals.window_fill = text_panel;
        visuals.extreme_bg_color = Colors::BG_INPUT;
        visuals.override_text_color = Some(text);
        visuals.widgets.noninteractive.fg_stroke.color = text;
        visuals.widgets.inactive.fg_stroke.color = text;
        visuals.widgets.hovered.fg_stroke.color = text;
        visuals.widgets.active.fg_stroke.color = text;
        visuals.widgets.open.fg_stroke.color = text;
        visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, egui::Color32::TRANSPARENT);
        visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, egui::Color32::TRANSPARENT);
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, egui::Color32::TRANSPARENT);
        visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, egui::Color32::TRANSPARENT);
        visuals.window_stroke = egui::Stroke::new(1.0, separator);
        visuals.widgets.inactive.bg_fill = Colors::BG_INPUT;
        visuals.widgets.hovered.bg_fill = Colors::BG_HOVER;
        // Buttons use weak_bg_fill when framed. Keep noninteractive unchanged so
        // disabled media buttons remain greyed out, but color normal/hover/active
        // buttons from the theme's Button color.
        visuals.widgets.inactive.weak_bg_fill = button;
        visuals.widgets.hovered.weak_bg_fill = button.gamma_multiply(1.18);
        visuals.widgets.active.weak_bg_fill = button.gamma_multiply(0.85);
        visuals.widgets.open.weak_bg_fill = button.gamma_multiply(1.05);
        visuals.widgets.inactive.corner_radius = button_radius;
        visuals.widgets.hovered.corner_radius = button_radius;
        visuals.widgets.active.corner_radius = button_radius;
        visuals.widgets.open.corner_radius = button_radius;
        visuals.selection.bg_fill = button;
        visuals.selection.stroke.color = text;
        ctx.set_visuals(visuals);

        let text_size = self.theme.overlay_text_size.clamp(8, 48) as f32;
        let mut style = (*ctx.style()).clone();
        style.text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(text_size));
        style.text_styles.insert(egui::TextStyle::Button, egui::FontId::proportional(text_size));
        style.text_styles.insert(egui::TextStyle::Monospace, egui::FontId::monospace(text_size));
        style.text_styles.insert(egui::TextStyle::Small, egui::FontId::proportional((text_size - 2.0).max(8.0)));
        style.text_styles.insert(egui::TextStyle::Heading, egui::FontId::proportional(text_size + 6.0));
        ctx.set_style(style);
    }

    /// Parse a key name string into an egui::Key. Returns Key::V as fallback.
    fn ptt_egui_key(&self) -> egui::Key {
        match self.config.ptt_key.to_uppercase().as_str() {
            "A" => egui::Key::A, "B" => egui::Key::B, "C" => egui::Key::C,
            "D" => egui::Key::D, "E" => egui::Key::E, "F" => egui::Key::F,
            "G" => egui::Key::G, "H" => egui::Key::H, "I" => egui::Key::I,
            "J" => egui::Key::J, "K" => egui::Key::K, "L" => egui::Key::L,
            "M" => egui::Key::M, "N" => egui::Key::N, "O" => egui::Key::O,
            "P" => egui::Key::P, "Q" => egui::Key::Q, "R" => egui::Key::R,
            "S" => egui::Key::S, "T" => egui::Key::T, "U" => egui::Key::U,
            "V" => egui::Key::V, "W" => egui::Key::W, "X" => egui::Key::X,
            "Y" => egui::Key::Y, "Z" => egui::Key::Z,
            "SPACE" => egui::Key::Space,
            _ => egui::Key::V,
        }
    }

    pub(crate) fn handle_ptt_input(&mut self, ctx: &egui::Context) {
        if self.config.voice_use_ptt {
            let key = self.ptt_egui_key();
            let active = ctx.input(|i| i.key_down(key));
            self.voice.set_ptt(active);
        } else {
            self.voice.set_ptt(true);
        }
    }

    fn theme_text_color(&self) -> egui::Color32 {
        rgb_to_color32(&self.theme.text_color)
    }

    fn theme_text_panel_color(&self) -> egui::Color32 {
        rgb_to_color32(&self.theme.text_panel_color)
    }

    fn theme_side_panel_color(&self) -> egui::Color32 {
        rgb_to_color32(&self.theme.side_panel_color)
    }

    fn theme_separator_color(&self) -> egui::Color32 {
        rgb_to_color32(&self.theme.separator_color)
    }

    fn themed_separator(&self, ui: &mut egui::Ui) {
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), 6.0),
            egui::Sense::hover(),
        );
        ui.painter().hline(
            rect.left()..=rect.right(),
            rect.center().y,
            egui::Stroke::new(1.0, self.theme_separator_color()),
        );
    }

    fn text_format(ui: &egui::Ui, color: egui::Color32) -> egui::text::TextFormat {
        egui::text::TextFormat {
            font_id: egui::TextStyle::Body.resolve(ui.style()),
            color,
            ..Default::default()
        }
    }

    fn label_text_message(
        ui: &mut egui::Ui,
        time_str: &str,
        sender: &str,
        content: &str,
        name_color: egui::Color32,
        text_color: egui::Color32,
    ) {
        let mut job = egui::text::LayoutJob::default();
        job.append(&format!("[{time_str}] "), 0.0, Self::text_format(ui, text_color));
        job.append(&format!("{sender}:"), 0.0, Self::text_format(ui, name_color));
        job.append(&format!(" {content}"), 0.0, Self::text_format(ui, text_color));
        ui.label(job);
    }

    fn label_image_message(
        ui: &mut egui::Ui,
        time_str: &str,
        sender: &str,
        name_color: egui::Color32,
        text_color: egui::Color32,
    ) {
        let mut job = egui::text::LayoutJob::default();
        job.append(&format!("[{time_str}] "), 0.0, Self::text_format(ui, text_color));
        job.append(sender, 0.0, Self::text_format(ui, name_color));
        job.append(" sent an image", 0.0, Self::text_format(ui, text_color));
        ui.label(job);
    }

    fn back_to_game_button(ui: &mut egui::Ui) -> egui::Response {
        let desired = egui::vec2(116.0, ui.spacing().interact_size.y);
        let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::click());
        let visuals = ui.style().interact(&response);
        let text_color = ui.visuals().text_color();
        let stroke = egui::Stroke::new(1.6, text_color);
        let painter = ui.painter();

        painter.rect(
            rect,
            visuals.corner_radius,
            visuals.weak_bg_fill,
            visuals.bg_stroke,
            egui::StrokeKind::Inside,
        );

        // Draw a simple "return/turn-around" arrow so we don't depend on
        // missing unicode arrow glyphs in the system font.
        let icon = egui::Rect::from_min_size(
            egui::pos2(rect.left() + 9.0, rect.center().y - 7.0),
            egui::vec2(18.0, 14.0),
        );
        let y = icon.center().y;
        painter.line_segment([egui::pos2(icon.right(), icon.top()), egui::pos2(icon.right(), y)], stroke);
        painter.line_segment([egui::pos2(icon.right(), y), egui::pos2(icon.left() + 3.0, y)], stroke);
        painter.line_segment([egui::pos2(icon.left() + 3.0, y), egui::pos2(icon.left() + 7.0, y - 4.0)], stroke);
        painter.line_segment([egui::pos2(icon.left() + 3.0, y), egui::pos2(icon.left() + 7.0, y + 4.0)], stroke);

        painter.text(
            egui::pos2(rect.left() + 34.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            "Back to game",
            egui::FontId::proportional(12.0),
            text_color,
        );

        response
    }

    fn icon_button(
        ui: &mut egui::Ui,
        enabled: bool,
        icon: ButtonIcon,
        tooltip: &str,
    ) -> egui::Response {
        let desired = egui::vec2(32.0, 32.0);
        let sense = if enabled { egui::Sense::click() } else { egui::Sense::hover() };
        let (rect, response) = ui.allocate_exact_size(desired, sense);
        let visuals = if enabled {
            ui.style().interact(&response)
        } else {
            &ui.visuals().widgets.noninteractive
        };
        let fill = if enabled {
            visuals.weak_bg_fill
        } else {
            ui.visuals().widgets.noninteractive.weak_bg_fill
        };
        let icon_color = if enabled {
            ui.visuals().text_color()
        } else {
            ui.visuals().weak_text_color()
        };
        let stroke = egui::Stroke::new(1.7, icon_color);
        let painter = ui.painter();
        painter.rect(
            rect,
            visuals.corner_radius,
            fill,
            visuals.bg_stroke,
            egui::StrokeKind::Inside,
        );

        let r = rect.shrink(7.0);
        let c = r.center();
        match icon {
            ButtonIcon::Send => {
                // Simple paper-plane outline.
                let tip = egui::pos2(r.right(), c.y);
                let top = egui::pos2(r.left(), r.top() + 1.0);
                let bottom = egui::pos2(r.left(), r.bottom() - 1.0);
                let inner = egui::pos2(c.x - 1.0, c.y + 2.0);
                painter.line_segment([top, tip], stroke);
                painter.line_segment([tip, bottom], stroke);
                painter.line_segment([bottom, inner], stroke);
                painter.line_segment([inner, top], stroke);
                painter.line_segment([inner, tip], stroke);
            }
            ButtonIcon::Image => {
                let frame = egui::Rect::from_min_max(
                    egui::pos2(r.left(), r.top() + 1.0),
                    egui::pos2(r.right(), r.bottom() - 1.0),
                );
                painter.rect_stroke(frame, 3.0, stroke, egui::StrokeKind::Inside);
                painter.circle_stroke(egui::pos2(frame.left() + 4.5, frame.top() + 4.5), 1.8, stroke);
                painter.line_segment(
                    [
                        egui::pos2(frame.left() + 3.0, frame.bottom() - 3.0),
                        egui::pos2(frame.left() + 8.0, frame.center().y),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        egui::pos2(frame.left() + 8.0, frame.center().y),
                        egui::pos2(frame.left() + 12.0, frame.bottom() - 4.0),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        egui::pos2(frame.left() + 11.0, frame.bottom() - 5.0),
                        egui::pos2(frame.right() - 3.0, frame.top() + 8.0),
                    ],
                    stroke,
                );
            }
            ButtonIcon::Mic | ButtonIcon::MicMuted => {
                let body = egui::Rect::from_center_size(
                    egui::pos2(c.x, r.top() + 6.5),
                    egui::vec2(8.0, 13.0),
                );
                painter.rect_stroke(body, 5.0, stroke, egui::StrokeKind::Inside);
                painter.line_segment(
                    [egui::pos2(c.x - 7.0, c.y), egui::pos2(c.x - 7.0, c.y + 1.0)],
                    stroke,
                );
                painter.line_segment(
                    [egui::pos2(c.x + 7.0, c.y), egui::pos2(c.x + 7.0, c.y + 1.0)],
                    stroke,
                );
                painter.line_segment([egui::pos2(c.x, body.bottom()), egui::pos2(c.x, r.bottom() - 3.0)], stroke);
                painter.line_segment([egui::pos2(c.x - 5.0, r.bottom() - 3.0), egui::pos2(c.x + 5.0, r.bottom() - 3.0)], stroke);
                if matches!(icon, ButtonIcon::MicMuted) {
                    painter.line_segment(
                        [egui::pos2(r.left() - 1.0, r.bottom()), egui::pos2(r.right() + 1.0, r.top())],
                        egui::Stroke::new(2.0, icon_color),
                    );
                }
            }
            ButtonIcon::Webcam => {
                let body = egui::Rect::from_min_max(
                    egui::pos2(r.left(), c.y - 5.5),
                    egui::pos2(r.left() + 12.5, c.y + 5.5),
                );
                painter.rect_stroke(body, 3.0, stroke, egui::StrokeKind::Inside);
                painter.line_segment([egui::pos2(body.right(), c.y - 3.5), egui::pos2(r.right(), c.y - 7.0)], stroke);
                painter.line_segment([egui::pos2(r.right(), c.y - 7.0), egui::pos2(r.right(), c.y + 7.0)], stroke);
                painter.line_segment([egui::pos2(r.right(), c.y + 7.0), egui::pos2(body.right(), c.y + 3.5)], stroke);
                painter.line_segment([egui::pos2(body.right(), c.y + 3.5), egui::pos2(body.right(), c.y - 3.5)], stroke);
            }
            ButtonIcon::Stop => {
                let stop_rect = egui::Rect::from_center_size(c, egui::vec2(13.0, 13.0));
                painter.rect_filled(stop_rect, 3.0, icon_color);
            }
        }

        response.on_hover_text(tooltip)
    }

    fn back_to_game(&mut self) {
        self.selected_history_uuid = None;
        self.viewing_history_chat = None;
    }

    fn connection_status_text(&self, ctx: &egui::Context) -> Option<String> {
        if self.joined {
            return None;
        }

        if self.session.is_none() {
            if self.connect_handle.is_some() {
                let dots = (ctx.input(|i| i.time * 2.0).floor() as usize % 3) + 1;
                return Some(format!("waiting for game{}", ".".repeat(dots)));
            }
            return None;
        }

        let state = self
            .session
            .as_ref()
            .and_then(|session| session.try_lock().ok().map(|session| session.connection_state()));

        match state {
            Some(ssp_client::ConnectionState::Discovering) => Some("searching for peer...".to_string()),
            Some(ssp_client::ConnectionState::Discovered) => Some("connecting...".to_string()),
            _ => Some("searching for peer...".to_string()),
        }
    }

    pub(crate) fn render_first_run(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(&ctx.style()).fill(self.theme_text_panel_color()))
            .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(120.0);
                ui.heading("Welcome to SLP Chat");
                ui.add_space(12.0);
                ui.label("Choose a display name");
                ui.add_sized(
                    [260.0, 32.0],
                    egui::TextEdit::singleline(&mut self.edit_username).hint_text("Username"),
                );
                ui.add_space(8.0);
                ui.checkbox(&mut self.config.overlay_enabled, "Enable in-game overlay");
                ui.checkbox(&mut self.config.start_on_startup, "Start on startup");
                ui.add_space(8.0);
                if ui.button("Continue").clicked() {
                    self.commit_profile_edit();
                }
            });
        });
    }

    pub(crate) fn render_editing(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(&ctx.style()).fill(self.theme_text_panel_color()))
            .show(ctx, |ui| {
            ui.heading("Edit Profile");
            self.themed_separator(ui);
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(72.0, 72.0), egui::Sense::hover());
                SlpChatApp::load_avatar_texture(
                    &self.edit_avatar_png,
                    &mut self.edit_avatar_texture,
                    ctx,
                    "edit_avatar",
                );
                if let Some(tex) = &self.edit_avatar_texture {
                    ui.painter().image(
                        tex.id(),
                        rect,
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                } else {
                    paint_letter_avatar(
                        ui,
                        rect,
                        &self.edit_username,
                        egui::Color32::from_rgb(
                            self.edit_name_color[0],
                            self.edit_name_color[1],
                            self.edit_name_color[2],
                        ),
                    );
                }
                ui.vertical(|ui| {
                    if ui.button("Choose avatar").clicked() {
                        self.edit_avatar_png = SlpChatApp::pick_avatar();
                        self.edit_avatar_texture = None;
                    }
                    if ui.button("Remove avatar").clicked() {
                        self.edit_avatar_png = None;
                        self.edit_avatar_texture = None;
                    }
                });
            });
            ui.add_space(10.0);
            ui.label("Username");
            ui.text_edit_singleline(&mut self.edit_username);
            ui.label("Bio");
            ui.text_edit_multiline(&mut self.edit_bio);
            theme_color_row(ui, "Name color", &mut self.edit_name_color, &mut self.edit_color_hex);
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    self.commit_profile_edit();
                }
                if ui.button("Cancel").clicked() {
                    self.screen = AppScreen::InSession;
                }
            });
        });
    }

    pub(crate) fn render_session(&mut self, ctx: &egui::Context) {
        let separator = self.theme_separator_color();
        egui::SidePanel::left("sidebar")
            .resizable(true)
            .default_width(self.sidebar_width)
            .frame(
                egui::Frame::side_top_panel(&ctx.style())
                    .fill(self.theme_side_panel_color())
                    .stroke(egui::Stroke::NONE),
            )
            .show(ctx, |ui| {
                let panel_rect = ui.max_rect();
                self.sidebar_width = ui.available_width();
                self.render_sidebar(ui, ctx);
                ui.painter().vline(
                    panel_rect.right(),
                    panel_rect.top()..=panel_rect.bottom(),
                    egui::Stroke::new(1.0, separator),
                );
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::central_panel(&ctx.style())
                    .fill(self.theme_text_panel_color())
                    .stroke(egui::Stroke::new(0.0, egui::Color32::TRANSPARENT)),
            )
            .show(ctx, |ui| {
                self.render_chat_area(ui, ctx);
            });
    }

    fn render_sidebar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(36.0, 36.0), egui::Sense::hover());
            SlpChatApp::load_avatar_texture(&self.avatar_png, &mut self.avatar_texture, ctx, "self_avatar");
            if let Some(tex) = &self.avatar_texture {
                ui.painter().image(
                    tex.id(),
                    rect,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            } else {
                paint_letter_avatar(ui, rect, &self.username, egui::Color32::from_rgb(self.name_color[0], self.name_color[1], self.name_color[2]));
            }
            if self.voice.is_self_talking() {
                ui.painter().rect_stroke(
                    rect,
                    4.0,
                    egui::Stroke::new(3.0, rgb_to_color32(&self.name_color)),
                    egui::StrokeKind::Inside,
                );
            }
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(&self.username).color(rgb_to_color32(&self.name_color)).strong());
                if let Some(status) = self.connection_status_text(ctx) {
                    ui.label(egui::RichText::new(status).size(11.0).color(Colors::TEXT_SECONDARY));
                }
                if ui.small_button("Edit profile").clicked() {
                    self.edit_username = self.username.clone();
                    self.edit_avatar_png = self.avatar_png.clone();
                    self.edit_avatar_texture = None;
                    self.edit_name_color = self.name_color;
                    self.edit_color_hex = format!("{:02X}{:02X}{:02X}", self.name_color[0], self.name_color[1], self.name_color[2]);
                    self.edit_bio = self.bio.clone();
                    self.screen = AppScreen::Editing;
                }
            });
        });
        self.themed_separator(ui);
        if ui.button("⚙ Options").clicked() {
            self.show_options = !self.show_options;
        }
        if ui.button("🎨 Edit Theme").clicked() {
            self.edit_theme = self.theme.clone();
            self.edit_theme_hex = ThemeHexFields::from_theme(&self.theme);
            self.show_theme_editor = !self.show_theme_editor;
        }
        self.themed_separator(ui);
        ui.horizontal(|ui| {
            let row_h = ui.spacing().interact_size.y;
            let (arrow_rect, arrow_resp) = ui.allocate_exact_size(
                egui::vec2(12.0, row_h),
                egui::Sense::click(),
            );
            let arrow_color = self.theme_text_color();
            let c = arrow_rect.center();
            let points = if self.peers_expanded {
                vec![
                    egui::pos2(c.x - 4.0, c.y - 2.0),
                    egui::pos2(c.x + 4.0, c.y - 2.0),
                    egui::pos2(c.x, c.y + 3.0),
                ]
            } else {
                vec![
                    egui::pos2(c.x - 2.0, c.y - 4.0),
                    egui::pos2(c.x - 2.0, c.y + 4.0),
                    egui::pos2(c.x + 3.0, c.y),
                ]
            };
            ui.painter().add(egui::Shape::convex_polygon(
                points,
                arrow_color,
                egui::Stroke::NONE,
            ));

            let peers_resp = ui.add(
                egui::Label::new(egui::RichText::new("Peers"))
                    .sense(egui::Sense::click()),
            );
            if arrow_resp.clicked() || peers_resp.clicked() {
                self.peers_expanded = !self.peers_expanded;
            }

            let should_show_back_to_game = self.viewing_history_chat.is_some()
                || self.selected_history_uuid.is_some();
            if should_show_back_to_game && Self::back_to_game_button(ui).clicked() {
                self.back_to_game();
            }
        });
        if self.peers_expanded {
            ui.indent("peers_list", |ui| {
                if self.chat.peer_names.is_empty() {
                    ui.label(egui::RichText::new("No peers yet").color(Colors::TEXT_SECONDARY));
                }
                for (peer_id, name) in self.chat.peer_names.clone() {
                    let selected = self.selected_peer_profile.as_deref() == Some(&peer_id);
                    if ui.selectable_label(selected, name).clicked() {
                        if selected {
                            self.selected_peer_profile = None;
                        } else {
                            self.selected_peer_profile = Some(peer_id);
                        }
                    }
                }
            });
        }
        ui.horizontal(|ui| {
            let row_h = ui.spacing().interact_size.y;
            let (arrow_rect, arrow_resp) = ui.allocate_exact_size(
                egui::vec2(12.0, row_h),
                egui::Sense::click(),
            );
            let arrow_color = self.theme_text_color();
            let c = arrow_rect.center();
            let points = if self.history_expanded {
                vec![
                    egui::pos2(c.x - 4.0, c.y - 2.0),
                    egui::pos2(c.x + 4.0, c.y - 2.0),
                    egui::pos2(c.x, c.y + 3.0),
                ]
            } else {
                vec![
                    egui::pos2(c.x - 2.0, c.y - 4.0),
                    egui::pos2(c.x - 2.0, c.y + 4.0),
                    egui::pos2(c.x + 3.0, c.y),
                ]
            };
            ui.painter().add(egui::Shape::convex_polygon(
                points,
                arrow_color,
                egui::Stroke::NONE,
            ));

            let history_resp = ui.add(
                egui::Label::new(egui::RichText::new("History"))
                    .sense(egui::Sense::click()),
            );
            if arrow_resp.clicked() || history_resp.clicked() {
                self.history_expanded = !self.history_expanded;
            }
        });
        if self.history_expanded {
            ui.indent("history_list", |ui| {
                for profile in self.history_profiles.clone() {
                    let selected = self.selected_history_uuid.as_deref() == Some(&profile.uuid);
                    if ui.selectable_label(selected, &profile.name).clicked() {
                        self.selected_history_uuid = Some(profile.uuid.clone());
                        self.viewing_history_chat = profile::load_conversation(&profile.uuid);
                    }
                }
            });
        }
    }

    fn render_options_contents(&mut self, ui: &mut egui::Ui) {
        let mut voice_settings_changed = false;
        let mut voice_devices_changed = false;
        let mut save_config = false;

        ui.group(|ui| {
            ui.label(egui::RichText::new("Packet settings").strong());
            if ui.checkbox(&mut self.config.voice_enabled, "Voice packets").changed() {
                if !self.config.voice_enabled {
                    self.voice.set_enabled(false);
                    let _ = self.voice.drain_outgoing();
                }
                save_config = true;
            }
            if ui.checkbox(&mut self.config.images_enabled, "Image packets").changed() {
                save_config = true;
            }
            if ui.checkbox(&mut self.config.webcam_enabled, "Webcam packets").changed() {
                save_config = true;
            }
            if ui.checkbox(&mut self.config.avatars_enabled, "Avatars").changed() {
                save_config = true;
            }
            if ui.checkbox(&mut self.config.store_messages, "Store messages").changed() {
                save_config = true;
            }
            if ui.checkbox(&mut self.config.overlay_enabled, "In-game overlay").changed() {
                save_config = true;
            }
            if ui.checkbox(&mut self.config.start_on_startup, "Start on startup").changed() {
                save_config = true;
                crate::config::sync_startup_registry(self.config.start_on_startup);
            }
            if ui.add(egui::Slider::new(&mut self.config.voice_volume, 0.0..=5.0).text("Voice volume")).changed() {
                voice_settings_changed = true;
            }
            let level = self.voice.input_level().clamp(0.0, 1.0);
            let threshold_response = ui.add(
                egui::Slider::new(&mut self.config.voice_threshold, 0.0..=1.0)
                    .text("Mic threshold"),
            );
            let slider_rect = threshold_response.rect;
            let marker_x = egui::lerp(slider_rect.left()..=slider_rect.right(), level);
            ui.painter().vline(
                marker_x,
                slider_rect.y_range(),
                egui::Stroke::new(2.0, egui::Color32::from_rgb(59, 165, 93)),
            );
            ui.painter().text(
                slider_rect.center(),
                egui::Align2::CENTER_CENTER,
                format!("input {:.3}", level),
                egui::FontId::proportional(10.0),
                self.theme_text_color(),
            );
            if threshold_response.changed() {
                voice_settings_changed = true;
            }
            if ui.checkbox(&mut self.config.voice_use_ptt, "Push to talk").changed() {
                voice_settings_changed = true;
            }
            if self.config.voice_use_ptt {
                ui.horizontal(|ui| {
                    ui.label("PTT key:");
                    let btn_label = if self.capturing_ptt_key {
                        "Press a key..."
                    } else {
                        &self.config.ptt_key.to_uppercase()
                    };
                    if ui.button(btn_label).clicked() {
                        self.capturing_ptt_key = !self.capturing_ptt_key;
                    }
                });
            }
            if ui.add(egui::Slider::new(&mut self.config.webcam_fps, 1.0..=30.0).text("Webcam FPS")).changed() {
                save_config = true;
            }
            if ui.add(egui::Slider::new(&mut self.config.webcam_quality, 1..=100).text("Webcam JPEG quality")).changed() {
                save_config = true;
            }
            ui.add_space(4.0);
            // --- Audio / webcam device selection ---
            ui.label(egui::RichText::new("Audio Devices").strong());
            {
                let names = VoiceEngine::input_device_names();
                let label = if self.config.audio_input_device.is_empty() {
                    "Default".to_string()
                } else {
                    self.config.audio_input_device.clone()
                };
                egui::ComboBox::from_id_salt("audio_input")
                    .selected_text(&label)
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(self.config.audio_input_device.is_empty(), "Default").clicked() {
                            if !self.config.audio_input_device.is_empty() {
                                self.config.audio_input_device.clear();
                                voice_devices_changed = true;
                            }
                        }
                        for name in &names {
                            if ui.selectable_label(self.config.audio_input_device == *name, name).clicked() {
                                if self.config.audio_input_device != *name {
                                    self.config.audio_input_device = name.clone();
                                    voice_devices_changed = true;
                                }
                            }
                        }
                    });
            }
            {
                let names = VoiceEngine::output_device_names();
                let label = if self.config.audio_output_device.is_empty() {
                    "Default".to_string()
                } else {
                    self.config.audio_output_device.clone()
                };
                egui::ComboBox::from_id_salt("audio_output")
                    .selected_text(&label)
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(self.config.audio_output_device.is_empty(), "Default").clicked() {
                            if !self.config.audio_output_device.is_empty() {
                                self.config.audio_output_device.clear();
                                voice_devices_changed = true;
                            }
                        }
                        for name in &names {
                            if ui.selectable_label(self.config.audio_output_device == *name, name).clicked() {
                                if self.config.audio_output_device != *name {
                                    self.config.audio_output_device = name.clone();
                                    voice_devices_changed = true;
                                }
                            }
                        }
                    });
            }
            ui.label(egui::RichText::new("Webcam").strong());
            {
                let names = webcam_device_names();
                let label = if self.config.webcam_device.is_empty() {
                    "Default".to_string()
                } else {
                    self.config.webcam_device.clone()
                };
                egui::ComboBox::from_id_salt("webcam_dev")
                    .selected_text(&label)
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(self.config.webcam_device.is_empty(), "Default").clicked() {
                            if !self.config.webcam_device.is_empty() {
                                self.config.webcam_device.clear();
                                save_config = true;
                            }
                        }
                        for name in &names {
                            if ui.selectable_label(self.config.webcam_device == *name, name).clicked() {
                                if self.config.webcam_device != *name {
                                    self.config.webcam_device = name.clone();
                                    save_config = true;
                                }
                            }
                        }
                    });
            }
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(format!(
                    "{}\n{}\nlevel {:.3}, queued {}, sent {}, received {}, playback {}",
                    self.voice.input_status(),
                    self.voice.output_status(),
                    self.voice.input_level(),
                    self.voice.queued_outgoing_packets(),
                    self.voice_packets_sent,
                    self.voice_packets_received,
                    self.voice.playback_queued_samples(),
                ))
                .size(10.0)
                .color(Colors::TEXT_SECONDARY),
            );
            if ui.button("Save options").clicked() {
                voice_settings_changed = true;
                save_config = true;
                crate::config::sync_startup_registry(self.config.start_on_startup);
            }
        });

        if voice_devices_changed {
            self.rebuild_voice_engine();
            save_config = true;
        } else if voice_settings_changed {
            self.apply_voice_settings();
            save_config = true;
        }
        if save_config {
            self.config.save();
        }
    }

    fn render_chat_area(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.vertical(|ui| {
            self.render_peer_profile(ui, ctx);

            // Reserve space for the input bar so the scroll area doesn't eat it.
            let input_bar_height = 40.0;
            let available_height = ui.available_height().max(input_bar_height + 4.0);
            let scroll_height = available_height - input_bar_height - 4.0;
            egui::ScrollArea::vertical()
                .max_height(scroll_height)
                .auto_shrink([false; 2])
                .stick_to_bottom(self.chat.scroll_to_bottom)
                .show(ui, |ui| {
                    if let Some(conv) = &self.viewing_history_chat {
                        let text_color = self.theme_text_color();
                        let self_name_color = rgb_to_color32(&self.name_color);
                        let peer_name_color = rgb_to_color32(&conv.peer.name_color);
                        ui.label(egui::RichText::new(format!("History with {}", conv.peer.name)).strong());
                        for msg in &conv.messages {
                            if msg.is_image {
                                if let Ok(img) = image::load_from_memory(&msg.image_data) {
                                    let rgba = img.to_rgba8();
                                    let size = [rgba.width() as usize, rgba.height() as usize];
                                    let pixels = rgba.into_raw();
                                    let tex = ctx.load_texture(
                                        format!("history_img_{}", msg.timestamp.timestamp_nanos_opt().unwrap_or_default()),
                                        egui::ColorImage::from_rgba_unmultiplied(size, &pixels),
                                        egui::TextureOptions::LINEAR,
                                    );
                                    let time_str = msg.timestamp.format("%H:%M").to_string();
                                    let who = if msg.is_self { "You" } else { &conv.peer.name };
                                    let name_color = if msg.is_self { self_name_color } else { peer_name_color };
                                    Self::label_image_message(ui, &time_str, who, name_color, text_color);
                                    ui.image((tex.id(), egui::vec2(240.0, 160.0)));
                                } else {
                                    ui.label(egui::RichText::new("[image]").color(text_color));
                                }
                            } else {
                                let time_str = msg.timestamp.format("%H:%M").to_string();
                                let who = if msg.is_self { "You" } else { &conv.peer.name };
                                let name_color = if msg.is_self { self_name_color } else { peer_name_color };
                                Self::label_text_message(ui, &time_str, who, &msg.content, name_color, text_color);
                            }
                        }
                    } else {
                        let text_color = self.theme_text_color();
                        let peer_color_by_name = self
                            .chat
                            .peer_names
                            .iter()
                            .filter_map(|(peer_id, name)| {
                                self.chat.peer_colors.get(peer_id).copied().map(|color| (name.clone(), color))
                            })
                            .collect::<std::collections::HashMap<_, _>>();
                        let self_name_color = rgb_to_color32(&self.name_color);
                        for entry in &mut self.chat.entries {
                            match entry {
                                ChatEntry::Text { sender, content, is_self, timestamp } => {
                                    let name_color = if *is_self {
                                        self_name_color
                                    } else {
                                        peer_color_by_name.get(sender).copied().unwrap_or(Colors::TEXT_NAME_OTHER)
                                    };
                                    let time_str = format_time(timestamp);
                                    Self::label_text_message(ui, &time_str, sender, content, name_color, text_color);
                                }
                                ChatEntry::Image { sender, png_data, texture, is_self, timestamp } => {
                                    let name_color = if *is_self {
                                        self_name_color
                                    } else {
                                        peer_color_by_name.get(sender).copied().unwrap_or(Colors::TEXT_NAME_OTHER)
                                    };
                                    let time_str = format_time(timestamp);
                                    Self::label_image_message(ui, &time_str, sender, name_color, text_color);
                                    if texture.is_none() {
                                        if let Ok(img) = image::load_from_memory(png_data) {
                                            let rgba = img.to_rgba8();
                                            let size = [rgba.width() as usize, rgba.height() as usize];
                                            let pixels = rgba.into_raw();
                                            *texture = Some(ctx.load_texture(
                                                format!("chat_img_{:?}", timestamp),
                                                egui::ColorImage::from_rgba_unmultiplied(size, &pixels),
                                                egui::TextureOptions::LINEAR,
                                            ));
                                        }
                                    }
                                    if let Some(tex) = texture {
                                        let w = tex.size()[0] as f32;
                                        let h = tex.size()[1] as f32;
                                        let max_w = ui.available_width().min(360.0);
                                        let scale = (max_w / w).min(1.0);
                                        ui.image((tex.id(), egui::vec2(w * scale, h * scale)));
                                    }
                                }
                                ChatEntry::System { msg, timestamp } => {
                                    let time_str = format_time(timestamp);
                                    ui.label(egui::RichText::new(format!("[{time_str}] {msg}")).color(Colors::TEXT_SYSTEM));
                                }
                            }
                        }
                    }
                });
            self.chat.scroll_to_bottom = false;

            self.themed_separator(ui);
            if self.viewing_history_chat.is_none() && self.joined {
                self.render_input_bar(ui);
            }
        });
    }

    fn render_peer_profile(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(peer_id) = &self.selected_peer_profile.clone() else {
            return;
        };
        if !self.chat.peer_names.contains_key(peer_id) && !self.chat.peer_bios.contains_key(peer_id) {
            return;
        }
        let name = self.chat.peer_display_name(peer_id);
        let color = self.chat.peer_colors.get(peer_id).copied().unwrap_or(egui::Color32::WHITE);
        let bio = self.chat.peer_bios.get(peer_id).cloned().unwrap_or_default();

        egui::Frame::dark_canvas(ui.style())
            .fill(self.theme_text_panel_color())
            .stroke(egui::Stroke::new(1.0, self.theme_separator_color()))
            .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Avatar
                let (av_rect, _) = ui.allocate_exact_size(egui::vec2(48.0, 48.0), egui::Sense::hover());
                if let Some(tex) = self.chat.avatar_texture(peer_id, ctx) {
                    ui.painter().image(
                        tex.id(),
                        av_rect,
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                } else {
                    paint_letter_avatar(ui, av_rect, &name, color);
                }
                if self.voice.is_peer_talking(peer_id) {
                    ui.painter().rect_stroke(
                        av_rect,
                        4.0,
                        egui::Stroke::new(3.0, color),
                        egui::StrokeKind::Inside,
                    );
                }
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(&name).color(color).strong());
                    if !bio.is_empty() {
                        ui.label(egui::RichText::new(&bio).color(Colors::TEXT_SECONDARY).italics());
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("✕").clicked() {
                        self.selected_peer_profile = None;
                    }
                });
            });
        });
        self.themed_separator(ui);
    }

    fn render_input_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let right_padding = 8.0;
            let response = ui.add_sized(
                [ui.available_width().max(260.0) - 152.0 - right_padding, 32.0],
                egui::TextEdit::singleline(&mut self.input_text).hint_text("Type a message..."),
            );
            let send_clicked = Self::icon_button(ui, true, ButtonIcon::Send, "Send").clicked();
            let image_clicked = Self::icon_button(
                ui,
                self.config.images_enabled,
                ButtonIcon::Image,
                if self.config.images_enabled { "Choose an image" } else { "Images are disabled" },
            )
            .clicked();
            let mic_on = self.config.voice_enabled && self.voice.is_enabled();
            if Self::icon_button(
                ui,
                self.config.voice_enabled,
                if mic_on { ButtonIcon::Mic } else { ButtonIcon::MicMuted },
                if self.config.voice_enabled { "Mute/unmute microphone" } else { "Voice packets disabled" },
            )
            .clicked()
            {
                self.voice.set_enabled(!self.voice.is_enabled());
            }
            let webcam_streaming = self.webcam_streaming.load(std::sync::atomic::Ordering::Relaxed);
            if Self::icon_button(
                ui,
                self.config.webcam_enabled,
                if webcam_streaming { ButtonIcon::Stop } else { ButtonIcon::Webcam },
                if self.config.webcam_enabled { "Start/stop webcam stream" } else { "Webcam packets disabled" },
            )
            .clicked()
            {
                self.toggle_webcam_stream();
            }
            ui.add_space(right_padding);
            if let Some(err) = &self.webcam_error {
                ui.label(egui::RichText::new(err).color(egui::Color32::from_rgb(240, 100, 100)));
            }

            if send_clicked || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                self.send_text();
                response.request_focus();
            }
            if image_clicked {
                if let Some(img) = SlpChatApp::pick_avatar() {
                    self.pending_image = Some(img);
                }
            }
        });
    }

    pub(crate) fn webcam_layout_key(&self, peer_id: &str) -> String {
        if peer_id == crate::app::LOCAL_WEBCAM_PEER_ID {
            format!("self:{}", self.player_uuid)
        } else if let Some(uuid) = self.chat.peer_uuids.get(peer_id) {
            format!("peer:{uuid}")
        } else {
            format!("node:{peer_id}")
        }
    }

    pub(crate) fn remember_webcam_box_layout(&mut self) {
        for (peer_id, pos) in &self.webcam_box_positions {
            let key = self.webcam_layout_key(peer_id);
            if let Some(size) = self.webcam_box_sizes.get(peer_id) {
                self.config.webcam_boxes.insert(
                    key,
                    crate::profile::WebcamBoxConfig {
                        x: pos.x,
                        y: pos.y,
                        w: size.x,
                        h: size.y,
                    },
                );
            }
        }
    }

    pub(crate) fn render_webcam_boxes(&mut self, ctx: &egui::Context) {
        let peer_ids: Vec<String> = self.chat.peer_video_frames.keys().cloned().collect();
        for peer_id in &peer_ids {
            let Some(jpeg_data) = self.chat.peer_video_frames.get(peer_id).cloned() else {
                continue;
            };

            // Build or fetch texture
            if !self.chat.peer_video_textures.contains_key(peer_id) {
                if let Ok(img) = image::load_from_memory(&jpeg_data) {
                    let rgba = img.to_rgba8();
                    let tex = ctx.load_texture(
                        format!("wbox_{peer_id}"),
                        egui::ColorImage::from_rgba_unmultiplied(
                            [rgba.width() as usize, rgba.height() as usize],
                            rgba.as_raw(),
                        ),
                        egui::TextureOptions::LINEAR,
                    );
                    self.chat.peer_video_textures.insert(peer_id.clone(), tex);
                } else {
                    continue;
                }
            }
            let Some(tex) = self.chat.peer_video_textures.get(peer_id) else {
                continue;
            };
            let tex_size = tex.size();
            let aspect = tex_size[0] as f32 / tex_size[1].max(1) as f32;

            // Default size
            let default_w = 200.0;
            let default_h = default_w / aspect;

            let pos = self
                .webcam_box_positions
                .entry(peer_id.clone())
                .or_insert_with(|| {
                    // Simple stack: first at (10, 80), each below by 170px
                    let idx = peer_ids.iter().position(|p| p == peer_id).unwrap_or(0);
                    egui::pos2(10.0, 80.0 + idx as f32 * (default_h + 10.0))
                });
            let size = self
                .webcam_box_sizes
                .entry(peer_id.clone())
                .or_insert(egui::vec2(default_w, default_h));

            // Clamp size: minimum 80px wide
            if size.x < 80.0 {
                size.x = 80.0;
                size.y = size.x / aspect;
            }

            let label = if *peer_id == crate::app::LOCAL_WEBCAM_PEER_ID {
                self.username.clone()
            } else {
                self.chat.peer_display_name(peer_id)
            };
            let name_color = if *peer_id == crate::app::LOCAL_WEBCAM_PEER_ID {
                rgb_to_color32(&self.name_color)
            } else {
                self.chat.peer_colors.get(peer_id).copied().unwrap_or(Colors::TEXT_NAME_OTHER)
            };
            let peer_talking = if *peer_id == crate::app::LOCAL_WEBCAM_PEER_ID {
                self.voice.is_self_talking()
            } else {
                self.voice.is_peer_talking(peer_id)
            };

            let area_id = egui::Id::new(format!("webcam_box_{peer_id}"));
            let area = egui::Area::new(area_id)
                .fixed_pos(*pos)
                .movable(false)
                .interactable(true);

            area.show(ctx, |ui| {
                let (rect, response) = ui.allocate_exact_size(*size, egui::Sense::click_and_drag());

                // Invisible resize hit-zone in the bottom-right corner. Keep the
                // webcam window resizable, but don't draw a little attached box.
                let handle_size = 14.0;
                let handle_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.right() - handle_size, rect.bottom() - handle_size),
                    egui::vec2(handle_size, handle_size),
                );
                let handle_resp = ui.interact(handle_rect, area_id.with("resize"), egui::Sense::drag());

                if handle_resp.dragged() {
                    let delta = handle_resp.drag_delta();
                    let new_w = (size.x + delta.x).max(80.0);
                    size.x = new_w;
                    size.y = new_w / aspect;
                } else if response.dragged() {
                    let delta = response.drag_delta();
                    pos.x += delta.x;
                    pos.y += delta.y;
                }

                ui.painter().image(
                    tex.id(),
                    rect,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );

                if peer_talking {
                    ui.painter().rect_stroke(
                        rect,
                        0.0,
                        egui::Stroke::new(3.0, name_color),
                        egui::StrokeKind::Inside,
                    );
                }

                // Name label overlay at top-right with peer's name color.
                let label_font = egui::FontId::proportional(13.0);
                let galley = ui.painter().layout_no_wrap(label.clone(), label_font, name_color);
                let label_w = galley.rect.width();
                let label_h = galley.rect.height();
                let lx = rect.right() - 8.0 - label_w;
                let ly = rect.top() + 4.0;
                let label_bg = egui::Rect::from_min_size(
                    egui::pos2(lx - 4.0, ly - 2.0),
                    egui::vec2(label_w + 8.0, label_h + 4.0),
                );
                ui.painter().rect_filled(label_bg, 3.0, egui::Color32::from_black_alpha(140));
                ui.painter().galley(egui::pos2(lx, ly), galley, egui::Color32::WHITE);
            });
        }
    }

    pub(crate) fn render_options_window(&mut self, ctx: &egui::Context) {
        if !self.show_options {
            return;
        }
        let mut open = self.show_options;
        egui::Window::new("Options")
            .open(&mut open)
            .resizable(true)
            .show(ctx, |ui| {
                self.render_options_contents(ui);
            });
        self.show_options = open;
    }

    pub(crate) fn render_editing_theme(&mut self, ctx: &egui::Context) {
        if !self.show_theme_editor {
            return;
        }
        let mut open = self.show_theme_editor;
        egui::Window::new("Edit Theme")
            .open(&mut open)
            .show(ctx, |ui| {
                theme_color_row(ui, "Text panel", &mut self.edit_theme.text_panel_color, &mut self.edit_theme_hex.text_panel_color);
                theme_color_row(ui, "Side panel", &mut self.edit_theme.side_panel_color, &mut self.edit_theme_hex.side_panel_color);
                theme_color_row(ui, "Separator", &mut self.edit_theme.separator_color, &mut self.edit_theme_hex.separator_color);
                theme_color_row(ui, "Button", &mut self.edit_theme.button_color, &mut self.edit_theme_hex.button_color);
                ui.add(egui::Slider::new(&mut self.edit_theme.button_radius, 0.0..=24.0).text("Button radius"));
                theme_color_row(ui, "Text", &mut self.edit_theme.text_color, &mut self.edit_theme_hex.text_color);
                ui.add(egui::Slider::new(&mut self.edit_theme.overlay_text_size, 8..=48).text("Text size"));

                self.themed_separator(ui);
                ui.label(egui::RichText::new("Overlay").strong());
                egui::ComboBox::from_id_salt("overlay_position")
                    .selected_text(match self.edit_theme.overlay_position {
                        0 => "Top left",
                        1 => "Top right",
                        2 => "Bottom left",
                        3 => "Bottom right",
                        _ => "Top left",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.edit_theme.overlay_position, 0, "Top left");
                        ui.selectable_value(&mut self.edit_theme.overlay_position, 1, "Top right");
                        ui.selectable_value(&mut self.edit_theme.overlay_position, 2, "Bottom left");
                        ui.selectable_value(&mut self.edit_theme.overlay_position, 3, "Bottom right");
                    });
                ui.add(egui::Slider::new(&mut self.edit_theme.overlay_distance_x, 0..=2000).text("Overlay X offset"));
                ui.add(egui::Slider::new(&mut self.edit_theme.overlay_distance_y, 0..=2000).text("Overlay Y offset"));
                ui.add(egui::Slider::new(&mut self.edit_theme.overlay_window_w, 160..=1600).text("Overlay textbox width"));
                ui.add(egui::Slider::new(&mut self.edit_theme.overlay_window_h, 80..=1000).text("Overlay textbox height"));
                theme_color_row(ui, "Overlay text outline", &mut self.edit_theme.overlay_outline_color, &mut self.edit_theme_hex.overlay_outline_color);
                ui.add(egui::Slider::new(&mut self.edit_theme.overlay_outline_thickness, 0..=8).text("Overlay outline thickness"));

                if ui.button("Apply").clicked() {
                    self.theme = self.edit_theme.clone();
                    save_theme(&self.theme);
                }
            });
        self.show_theme_editor = open;
    }
}
