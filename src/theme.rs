//! Theme and color utilities for the SLP Chat UI.

use eframe::egui;

use crate::profile::{self, ThemeConfig};

pub(crate) struct Colors;
impl Colors {
    pub const BG_INPUT: egui::Color32 = egui::Color32::from_rgb(20, 20, 23);
    pub const BG_HOVER: egui::Color32 = egui::Color32::from_rgb(35, 36, 40);
    pub const TEXT_SECONDARY: egui::Color32 = egui::Color32::from_rgb(142, 146, 153);
    pub const TEXT_NAME_OTHER: egui::Color32 = egui::Color32::from_rgb(180, 184, 190);
    pub const TEXT_SYSTEM: egui::Color32 = egui::Color32::from_rgb(100, 104, 112);
    pub const DEFAULT_NAME_COLOR: [u8; 3] = [87, 203, 222];
}

#[derive(Clone)]
pub(crate) struct ThemeHexFields {
    pub text_panel_color: String,
    pub side_panel_color: String,
    pub separator_color: String,
    pub button_color: String,
    pub text_color: String,
    pub overlay_outline_color: String,
}

impl ThemeHexFields {
    pub fn from_theme(theme: &ThemeConfig) -> Self {
        Self {
            text_panel_color: rgb_to_hex(&theme.text_panel_color),
            side_panel_color: rgb_to_hex(&theme.side_panel_color),
            separator_color: rgb_to_hex(&theme.separator_color),
            button_color: rgb_to_hex(&theme.button_color),
            text_color: rgb_to_hex(&theme.text_color),
            overlay_outline_color: rgb_to_hex(&theme.overlay_outline_color),
        }
    }
}

pub(crate) fn rgb_to_color32(rgb: &[u8; 3]) -> egui::Color32 {
    egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

pub(crate) fn rgb_to_hex(rgb: &[u8; 3]) -> String {
    format!("{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

pub(crate) fn parse_hex_color(hex: &str) -> Option<[u8; 3]> {
    let hex = hex.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some([r, g, b])
}

pub(crate) fn theme_color_row(
    ui: &mut egui::Ui,
    label: &str,
    rgb: &mut [u8; 3],
    hex: &mut String,
) {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut color = rgb_to_color32(rgb);
        if ui.color_edit_button_srgba(&mut color).changed() {
            *rgb = [color.r(), color.g(), color.b()];
            *hex = rgb_to_hex(rgb);
        }
        if ui.text_edit_singleline(hex).changed() {
            if let Some(parsed) = parse_hex_color(hex) {
                *rgb = parsed;
            }
        }
    });
}

pub(crate) fn paint_letter_avatar(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    name: &str,
    color: egui::Color32,
) {
    let painter = ui.painter();
    painter.circle_filled(rect.center(), rect.width().min(rect.height()) / 2.0, color);
    let letter = name.chars().next().unwrap_or('?').to_uppercase().to_string();
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        letter,
        egui::FontId::proportional(rect.height() * 0.45),
        egui::Color32::WHITE,
    );
}

pub(crate) fn save_theme(theme: &ThemeConfig) {
    profile::save_theme(theme);
}
