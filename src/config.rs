use crate::profile::{PersistentConfig, WebcamBoxConfig};
use std::collections::HashMap;

#[derive(Clone)]
pub struct AppConfig {
    pub voice_enabled: bool,
    pub images_enabled: bool,
    pub webcam_enabled: bool,
    pub avatars_enabled: bool,
    pub file_transmission_enabled: bool,
    pub store_messages: bool,
    pub overlay_enabled: bool,
    pub start_on_startup: bool,
    pub voice_volume: f32,
    pub voice_threshold: f32,
    pub voice_use_ptt: bool,
    pub ptt_key: String,
    pub webcam_fps: f32,
    pub webcam_quality: u8,
    pub webcam_boxes: HashMap<String, WebcamBoxConfig>,
    pub audio_input_device: String,
    pub audio_output_device: String,
    pub webcam_device: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            voice_enabled: true,
            images_enabled: true,
            webcam_enabled: true,
            avatars_enabled: true,
            file_transmission_enabled: false,
            store_messages: true,
            overlay_enabled: true,
            start_on_startup: false,
            voice_volume: 1.0,
            voice_threshold: 0.01,
            voice_use_ptt: true,
            ptt_key: "V".to_string(),
            webcam_fps: 6.0,
            webcam_quality: 60,
            webcam_boxes: HashMap::new(),
            audio_input_device: String::new(),
            audio_output_device: String::new(),
            webcam_device: String::new(),
        }
    }
}

impl From<PersistentConfig> for AppConfig {
    fn from(value: PersistentConfig) -> Self {
        Self {
            voice_enabled: value.voice_enabled,
            images_enabled: value.images_enabled,
            webcam_enabled: value.webcam_enabled,
            avatars_enabled: value.avatars_enabled,
            file_transmission_enabled: value.file_transmission_enabled,
            store_messages: value.store_messages,
            overlay_enabled: value.overlay_enabled,
            start_on_startup: value.start_on_startup,
            voice_volume: value.voice_volume,
            voice_threshold: value.voice_threshold,
            voice_use_ptt: value.voice_use_ptt,
            ptt_key: value.ptt_key.clone(),
            webcam_fps: value.webcam_fps,
            webcam_quality: value.webcam_quality,
            webcam_boxes: value.webcam_boxes,
            audio_input_device: value.audio_input_device,
            audio_output_device: value.audio_output_device,
            webcam_device: value.webcam_device,
        }
    }
}

impl AppConfig {
    pub fn to_persistent(&self) -> PersistentConfig {
        PersistentConfig {
            voice_enabled: self.voice_enabled,
            images_enabled: self.images_enabled,
            webcam_enabled: self.webcam_enabled,
            avatars_enabled: self.avatars_enabled,
            file_transmission_enabled: self.file_transmission_enabled,
            store_messages: self.store_messages,
            overlay_enabled: self.overlay_enabled,
            start_on_startup: self.start_on_startup,
            voice_volume: self.voice_volume,
            voice_threshold: self.voice_threshold,
            voice_use_ptt: self.voice_use_ptt,
            ptt_key: self.ptt_key.clone(),
            webcam_fps: self.webcam_fps,
            webcam_quality: self.webcam_quality,
            webcam_boxes: self.webcam_boxes.clone(),
            audio_input_device: self.audio_input_device.clone(),
            audio_output_device: self.audio_output_device.clone(),
            webcam_device: self.webcam_device.clone(),
        }
    }

    pub fn save(&self) {
        crate::profile::save_config(&self.to_persistent());
    }
}

pub fn sync_startup_registry(enabled: bool) {
    let _ = sync_startup_registry_impl(enabled);
}

#[cfg(target_os = "windows")]
fn sync_startup_registry_impl(enabled: bool) -> std::io::Result<()> {
    use std::process::{Command, Stdio};

    const TASK: &str = "SLP Chat";
    if enabled {
        let exe = std::env::current_exe()?;
        let action = format!("\"{}\" --startup", exe.display());
        Command::new("schtasks")
            .args(["/Create", "/TN", TASK, "/TR", &action, "/SC", "ONLOGON", "/RL", "LIMITED", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
    } else {
        Command::new("schtasks")
            .args(["/Delete", "/TN", TASK, "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn sync_startup_registry_impl(enabled: bool) -> std::io::Result<()> {
    let dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("Library/LaunchAgents");
    let plist = dir.join("com.slpauth.app.plist");
    if enabled {
        std::fs::create_dir_all(&dir)?;
        let exe = std::env::current_exe()?;
        let content = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>com.slpauth.app</string>
  <key>ProgramArguments</key>
  <array><string>{}</string><string>--startup</string></array>
  <key>RunAtLoad</key><true/>
</dict>
</plist>
"#,
            exe.display()
        );
        std::fs::write(plist, content)?;
    } else if plist.exists() {
        std::fs::remove_file(plist)?;
    }
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn sync_startup_registry_impl(enabled: bool) -> std::io::Result<()> {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("autostart");
    let desktop = dir.join("slpauth_app.desktop");
    if enabled {
        std::fs::create_dir_all(&dir)?;
        let exe = std::env::current_exe()?;
        let exec = format!("\"{}\" --startup", exe.display());
        let content = format!(
            "[Desktop Entry]\nType=Application\nName=SLP Chat\nExec={}\nX-GNOME-Autostart-enabled=true\nTerminal=false\n",
            exec
        );
        std::fs::write(desktop, content)?;
    } else if desktop.exists() {
        std::fs::remove_file(desktop)?;
    }
    Ok(())
}

#[cfg(not(any(unix, target_os = "windows")))]
fn sync_startup_registry_impl(_enabled: bool) -> std::io::Result<()> {
    Ok(())
}
