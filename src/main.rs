mod app;
mod chat;
mod config;
mod connect;
mod overlay;
mod profile;
mod protocol;
mod stealth;
mod theme;
mod ui;
mod voice;

#[cfg(target_os = "windows")]
fn try_acquire_single_instance() -> bool {
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::System::Threading::CreateMutexA;

    let name = b"Global\\SLPChat_SingleInstance\0";
    let handle = unsafe {
        CreateMutexA(
            std::ptr::null() as *const windows_sys::Win32::Security::SECURITY_ATTRIBUTES,
            1i32,
            name.as_ptr(),
        )
    };
    if handle == 0 as _ {
        return false;
    }
    if unsafe { GetLastError() } == 183 {
        false
    } else {
        std::mem::forget(handle);
        true
    }
}

#[cfg(target_os = "macos")]
fn try_acquire_single_instance() -> bool {
    let pid_path = profile::instance_pid_path();
    if let Ok(content) = std::fs::read_to_string(&pid_path) {
        if let Ok(pid) = content.trim().parse::<u32>() {
            // Check whether that process is still alive via `ps -p <pid>`.
            let alive = std::process::Command::new("ps")
                .args(["-p", &pid.to_string()])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if alive && pid != std::process::id() {
                return false;
            }
        }
    }
    if let Some(parent) = pid_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&pid_path, std::process::id().to_string());
    true
}

#[cfg(target_os = "windows")]
fn signal_show_window() {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    let signal_path = profile::show_signal_path();
    let _ = std::fs::write(&signal_path, "show");

    let title: Vec<u16> = "SLP Chat".encode_utf16().chain(std::iter::once(0)).collect();
    let hwnd = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
    if hwnd != 0 as _ {
        unsafe {
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (ex & !(WS_EX_TOOLWINDOW as isize)) | WS_EX_APPWINDOW as isize);
            let sw = GetSystemMetrics(SM_CXSCREEN);
            let sh = GetSystemMetrics(SM_CYSCREEN);
            let ww = 480i32.min(sw);
            let wh = 640i32.min(sh);
            SetWindowPos(
                hwnd, std::ptr::null_mut(),
                (sw - ww) / 2, (sh - wh) / 2, ww, wh,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
            for _ in 0..5 {
                PostMessageW(hwnd, 0, 0, 0);
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn signal_show_window() {
    if let Ok(content) = std::fs::read_to_string(profile::instance_pid_path()) {
        if let Ok(pid) = content.trim().parse::<u32>() {
            let script = format!(
                concat!(
                    "tell application \"System Events\"\n",
                    "  set theProcess to first process whose unix id is {}\n",
                    "  set visible of theProcess to true\n",
                    "  set frontmost of theProcess to true\n",
                    "end tell"
                ),
                pid
            );
            let _ = std::process::Command::new("osascript")
                .args(["-e", &script])
                .output();
        }
    }
}

fn main() {
    let launched_at_startup = std::env::args().any(|a| a == "--startup");

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    if std::env::var_os("SLPAUTH_ALLOW_MULTI_INSTANCE").is_none()
        && !try_acquire_single_instance()
    {
        if !launched_at_startup {
            signal_show_window();
        }
        return;
    }

    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size([800.0, 600.0])
        .with_min_inner_size([500.0, 350.0])
        .with_app_id("slpauth_app")
        .with_title("SLP Chat");

    // In overlay-startup mode, start hidden until explicitly signaled.
    // On Windows we cannot use with_visible(false) — it freezes the event
    // loop.  Instead, start the window off-screen; the frame-2 stealth-hide
    // in update() applies WS_EX_TOOLWINDOW + COM ITaskbarList::DeleteTab to
    // fully remove it from the taskbar (eframe/winit's with_taskbar(false)
    // is unreliable because winit's first-frame set_visible overwrites the
    // extended style).
    #[cfg(target_os = "windows")]
    if launched_at_startup {
        viewport = viewport.with_position(eframe::egui::pos2(-32000.0, -32000.0));
    }
    #[cfg(not(target_os = "windows"))]
    if launched_at_startup {
        viewport = viewport.with_visible(false);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "SLP Chat",
        native_options,
        Box::new(move |_cc| Ok(Box::new(app::SlpChatApp::new(launched_at_startup)))),
    )
    .expect("Failed to run eframe");
}

