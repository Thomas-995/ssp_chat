#[path = "../app.rs"] mod app;
#[path = "../chat.rs"] mod chat;
#[path = "../config.rs"] mod config;
#[path = "../connect.rs"] mod connect;
#[path = "../overlay.rs"] mod overlay;
#[path = "../profile.rs"] mod profile;
#[path = "../protocol.rs"] mod protocol;
#[path = "../stealth.rs"] mod stealth;
#[path = "../theme.rs"] mod theme;
#[path = "../ui.rs"] mod ui;
#[path = "../voice.rs"] mod voice;

fn main() {
    std::env::set_var("SLPAUTH_PROFILE", "tight");
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_min_inner_size([500.0, 350.0])
            .with_title("SLP Chat (tight)"),
        ..Default::default()
    };
    eframe::run_native(
        "SLP Chat",
        native_options,
        Box::new(|_cc| Ok(Box::new(app::SlpChatApp::new(false)))),
    )
    .expect("Failed to run eframe");
}
