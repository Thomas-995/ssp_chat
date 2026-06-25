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

#[cfg(target_os = "windows")]
const OVERLAY_COLOR_KEY: u32 = 0x00FF00FF;
#[cfg(target_os = "windows")]
const OVERLAY_INPUT_BG: u32 = 0x001E1E1E;
#[cfg(target_os = "windows")]
static OVERLAY_CLASS_REGISTERED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(target_os = "windows")]
static OVERLAY_HWND: std::sync::atomic::AtomicPtr<core::ffi::c_void> = std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
#[cfg(target_os = "windows")]
static OVERLAY_EDIT_HWND: std::sync::atomic::AtomicPtr<core::ffi::c_void> = std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

#[cfg(target_os = "windows")]
struct NativeKeyState {
    prev: [bool; 256],
    hold: [u16; 256],
}

#[cfg(target_os = "windows")]
static NATIVE_KEY_STATE: std::sync::OnceLock<std::sync::Mutex<NativeKeyState>> = std::sync::OnceLock::new();

#[cfg(target_os = "windows")]
fn native_key_state() -> &'static std::sync::Mutex<NativeKeyState> {
    NATIVE_KEY_STATE.get_or_init(|| std::sync::Mutex::new(NativeKeyState { prev: [false; 256], hold: [0; 256] }))
}

#[cfg(target_os = "windows")]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(target_os = "windows")]
fn colorref(color: egui::Color32) -> u32 {
    color.r() as u32 | ((color.g() as u32) << 8) | ((color.b() as u32) << 16)
}

#[cfg(target_os = "windows")]
fn read_edit_text(hwnd: windows_sys::Win32::Foundation::HWND) -> String {
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; len as usize + 1];
        GetWindowTextW(hwnd, buf.as_mut_ptr(), len + 1);
        String::from_utf16_lossy(&buf[..len as usize])
    }
}

#[cfg(target_os = "windows")]
fn read_edit_sel(hwnd: windows_sys::Win32::Foundation::HWND) -> (usize, usize) {
    let mut start: u32 = 0;
    let mut end: u32 = 0;
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW(
            hwnd,
            0x00B0,
            &mut start as *mut u32 as usize,
            &mut end as *mut u32 as isize,
        );
    }
    (start as usize, end as usize)
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn overlay_wnd_proc(
    hwnd: windows_sys::Win32::Foundation::HWND,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    match msg {
        0x0084 => 1, // WM_NCHITTEST -> HTCLIENT; keep textbox interactive.
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

#[cfg(target_os = "windows")]
unsafe fn create_native_overlay_window() -> windows_sys::Win32::Foundation::HWND {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    if !OVERLAY_CLASS_REGISTERED.load(std::sync::atomic::Ordering::SeqCst) {
        let class_name = wide("SLPChatNativeOverlay");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(overlay_wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: std::ptr::null_mut(),
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        RegisterClassExW(&wc);
        OVERLAY_CLASS_REGISTERED.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    let class_name = wide("SLPChatNativeOverlay");
    let title = wide("SLP Overlay");
    let hwnd = CreateWindowExW(
        WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
        class_name.as_ptr(),
        title.as_ptr(),
        WS_POPUP,
        0,
        0,
        1,
        1,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        std::ptr::null(),
    );
    if !hwnd.is_null() {
        SetLayeredWindowAttributes(hwnd, OVERLAY_COLOR_KEY, 0, LWA_COLORKEY);
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
    hwnd
}

#[cfg(target_os = "windows")]
fn decode_bgra(data: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    let img = image::load_from_memory(data).ok()?.resize_exact(w, h, image::imageops::FilterType::Triangle);
    let rgba = img.to_rgba8();
    let mut bgra = Vec::with_capacity((w * h * 4) as usize);
    for p in rgba.chunks(4) {
        bgra.push(p[2]);
        bgra.push(p[1]);
        bgra.push(p[0]);
        bgra.push(p[3]);
    }
    Some(bgra)
}

#[cfg(target_os = "windows")]
struct NativeParticipant {
    name: String,
    colorref: u32,
    talking: bool,
    bgra: Option<Vec<u8>>,
}

#[cfg(target_os = "windows")]
struct NativeMessage {
    time: String,
    sender: Option<String>,
    content: String,
    colorref: u32,
    text_colorref: u32,
}

impl SlpChatApp {
    pub(crate) fn render_dolphin_overlay(&mut self, ctx: &egui::Context) {
        #[cfg(target_os = "windows")]
        {
            self.render_windows_native_overlay();
            let _ = ctx;
            return;
        }

        #[cfg(not(target_os = "windows"))]
        {
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
    }

    #[cfg(target_os = "windows")]
    fn render_windows_native_overlay(&mut self) {
        use std::sync::atomic::Ordering;
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
        use windows_sys::Win32::UI::WindowsAndMessaging::*;

        if !self.config.overlay_enabled || !self.overlay_visible {
            let hwnd = OVERLAY_HWND.load(Ordering::SeqCst) as windows_sys::Win32::Foundation::HWND;
            if !hwnd.is_null() {
                unsafe { ShowWindow(hwnd, SW_HIDE) };
            }
            return;
        }

        let Some((dolphin_hwnd_raw, dx, dy, dw, dh)) = find_dolphin_rect() else {
            let hwnd = OVERLAY_HWND.load(Ordering::SeqCst) as windows_sys::Win32::Foundation::HWND;
            if !hwnd.is_null() {
                unsafe { ShowWindow(hwnd, SW_HIDE) };
            }
            return;
        };

        let mut hwnd = OVERLAY_HWND.load(Ordering::SeqCst) as windows_sys::Win32::Foundation::HWND;
        if hwnd.is_null() {
            hwnd = unsafe { create_native_overlay_window() };
            OVERLAY_HWND.store(hwnd as *mut core::ffi::c_void, Ordering::SeqCst);
            if hwnd.is_null() {
                return;
            }
        }

        let mut edit = OVERLAY_EDIT_HWND.load(Ordering::SeqCst) as windows_sys::Win32::Foundation::HWND;
        if edit.is_null() {
            let edit_class = wide("EDIT");
            edit = unsafe {
                CreateWindowExW(
                    0,
                    edit_class.as_ptr(),
                    std::ptr::null(),
                    0x40000080,
                    0,
                    0,
                    self.theme.overlay_window_w.max(100),
                    INPUT_H as i32,
                    hwnd,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            };
            OVERLAY_EDIT_HWND.store(edit as *mut core::ffi::c_void, Ordering::SeqCst);
        }

        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            while PeekMessageW(&mut msg, hwnd, 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        self.poll_native_overlay_keyboard(edit, dolphin_hwnd_raw as windows_sys::Win32::Foundation::HWND, hwnd);

        let viewport_size = egui::vec2(dw.max(1) as f32, dh.max(1) as f32);
        let content = self.overlay_content_rect(viewport_size);
        let input_text = if edit.is_null() { String::new() } else { read_edit_text(edit) };
        let (sel_start, sel_end) = if edit.is_null() { (0, 0) } else { read_edit_sel(edit) };
        let messages = self.native_overlay_messages();
        let participants = self.native_overlay_participants();
        let media_on_right = matches!(self.theme.overlay_position, 0 | 2);

        unsafe {
            SetWindowPos(hwnd, HWND_TOPMOST, dx, dy, dw, dh, SWP_NOACTIVATE | SWP_SHOWWINDOW);
            paint_native_overlay_window(
                hwnd,
                dw,
                dh,
                content,
                &messages,
                &participants,
                &input_text,
                sel_start,
                sel_end,
                self.theme.overlay_text_size.max(8),
                colorref(rgb_to_color32(&self.theme.overlay_outline_color)),
                self.theme.overlay_outline_thickness.max(0),
                media_on_right,
            );
        }
    }

    #[cfg(target_os = "windows")]
    fn poll_native_overlay_keyboard(
        &mut self,
        edit: windows_sys::Win32::Foundation::HWND,
        dolphin_hwnd: windows_sys::Win32::Foundation::HWND,
        overlay_hwnd: windows_sys::Win32::Foundation::HWND,
    ) {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        if edit.is_null() {
            return;
        }
        let fg = unsafe { GetForegroundWindow() };
        if fg != dolphin_hwnd && fg != overlay_hwnd {
            if let Ok(mut state) = native_key_state().lock() {
                state.prev = [false; 256];
                state.hold = [0; 256];
            }
            return;
        }

        let mut cur = [false; 256];
        for vk in 0u16..=0xFEu16 {
            cur[vk as usize] = unsafe { GetAsyncKeyState(vk as i32) } as u16 & 0x8000 != 0;
        }
        unsafe {
            let mut ks = [0u8; 256];
            GetKeyboardState(ks.as_mut_ptr());
            for vk in 0u16..=0xFEu16 {
                if cur[vk as usize] { ks[vk as usize] |= 0x80; } else { ks[vk as usize] &= !0x80; }
            }
            SetKeyboardState(ks.as_ptr());
        }
        let ctrl = cur[0x11];
        let mut state = match native_key_state().lock() {
            Ok(s) => s,
            Err(_) => return,
        };
        const REPEAT_DELAY: u16 = 30;
        const REPEAT_RATE: u16 = 3;
        for vk in 0x08u16..=0xFEu16 {
            if matches!(vk, 0x10 | 0x11 | 0x12 | 0x5B | 0x5C | 0x14 | 0x90 | 0x91) {
                continue;
            }
            let down = cur[vk as usize];
            let was_down = state.prev[vk as usize];
            state.prev[vk as usize] = down;
            let trigger = if down && !was_down {
                state.hold[vk as usize] = 0;
                true
            } else if down && was_down {
                state.hold[vk as usize] = state.hold[vk as usize].saturating_add(1);
                let h = state.hold[vk as usize];
                h >= REPEAT_DELAY && (h - REPEAT_DELAY) % REPEAT_RATE == 0
            } else {
                state.hold[vk as usize] = 0;
                false
            };
            if !trigger {
                continue;
            }
            if vk == 0x0D {
                drop(state);
                let text = read_edit_text(edit);
                unsafe { SetWindowTextW(edit, wide("").as_ptr()) };
                if !text.trim().is_empty() {
                    self.input_text = text;
                    self.send_text();
                }
                return;
            }
            let scan = unsafe { MapVirtualKeyW(vk as u32, 0) };
            let extended = matches!(vk, 0x25 | 0x26 | 0x27 | 0x28 | 0x24 | 0x23 | 0x2D | 0x2E | 0x21 | 0x22);
            let lparam = ((scan & 0xFF) << 16) as isize | 1 | if extended { 1 << 24 } else { 0 };
            unsafe { SendMessageW(edit, 0x0100, vk as usize, lparam) };
            if !ctrl {
                let mut char_buf = [0u16; 4];
                let mut ks = [0u8; 256];
                unsafe { GetKeyboardState(ks.as_mut_ptr()) };
                let n = unsafe { ToUnicode(vk as u32, scan, ks.as_ptr(), char_buf.as_mut_ptr(), 4, 0) };
                if n > 0 {
                    for ch in char_buf.iter().take(n as usize) {
                        if *ch >= 0x20 || *ch == 0x08 {
                            unsafe { SendMessageW(edit, 0x0102, *ch as usize, lparam) };
                        }
                    }
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn native_overlay_messages(&self) -> Vec<NativeMessage> {
        self.overlay_messages()
            .into_iter()
            .map(|m| NativeMessage {
                time: m.time,
                sender: m.sender,
                content: m.content,
                colorref: colorref(m.color),
                text_colorref: colorref(m.text_color),
            })
            .collect()
    }

    #[cfg(target_os = "windows")]
    fn native_overlay_participants(&self) -> Vec<NativeParticipant> {
        let mut out = Vec::new();
        let self_bgra = self
            .chat
            .peer_video_frames
            .get(LOCAL_WEBCAM_PEER_ID)
            .and_then(|d| decode_bgra(d, 48, 48))
            .or_else(|| self.avatar_png.as_deref().and_then(|d| decode_bgra(d, 48, 48)));
        out.push(NativeParticipant {
            name: self.username.clone(),
            colorref: colorref(rgb_to_color32(&self.name_color)),
            talking: self.voice.is_self_talking(),
            bgra: self_bgra,
        });
        for (peer_id, name) in &self.chat.peer_names {
            let color = self.chat.peer_colors.get(peer_id).copied().unwrap_or(Colors::TEXT_NAME_OTHER);
            let bgra = self
                .chat
                .peer_video_frames
                .get(peer_id)
                .and_then(|d| decode_bgra(d, 48, 48))
                .or_else(|| self.chat.peer_avatars.get(peer_id).and_then(|d| decode_bgra(d, 48, 48)));
            out.push(NativeParticipant {
                name: name.clone(),
                colorref: colorref(color),
                talking: self.voice.is_peer_talking(peer_id),
                bgra,
            });
        }
        out
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

#[cfg(target_os = "windows")]
unsafe fn paint_native_overlay_window(
    hwnd: windows_sys::Win32::Foundation::HWND,
    w: i32,
    h: i32,
    content: egui::Rect,
    messages: &[NativeMessage],
    participants: &[NativeParticipant],
    input_text: &str,
    _sel_start: usize,
    sel_end: usize,
    text_size: i32,
    outline_colorref: u32,
    outline_thickness: i32,
    media_on_right: bool,
) {
    use windows_sys::Win32::Foundation::{RECT, SIZE};
    use windows_sys::Win32::Graphics::Gdi::*;

    const DT_LEFT: u32 = 0x0000;
    const DT_WORDBREAK: u32 = 0x0010;
    const DT_CALCRECT: u32 = 0x0400;
    const DT_NOPREFIX: u32 = 0x0800;
    const TRANSPARENT_BK: i32 = 1;
    const NULL_BRUSH_ID: i32 = 5;

    let hdc = GetDC(hwnd);
    if hdc.is_null() {
        return;
    }
    let mem_dc = CreateCompatibleDC(hdc);
    let bmp = CreateCompatibleBitmap(hdc, w, h);
    let old_bmp = SelectObject(mem_dc, bmp);

    let full_rc = RECT { left: 0, top: 0, right: w, bottom: h };
    let bg = CreateSolidBrush(OVERLAY_COLOR_KEY);
    FillRect(mem_dc, &full_rc, bg);
    DeleteObject(bg);

    let font_name = wide("Segoe UI");
    let font = CreateFontW(
        -text_size, 0, 0, 0, 700, 0, 0, 0, 1, 0, 0, 3, 0, font_name.as_ptr(),
    );
    let old_font = SelectObject(mem_dc, font);
    SetBkMode(mem_dc, TRANSPARENT_BK);

    let cx = content.min.x.round() as i32;
    let cy = content.min.y.round() as i32;
    let cw = content.width().round() as i32;
    let ch = content.height().round() as i32;
    let input_h = INPUT_H as i32;
    let pad = PAD as i32;
    let msg_bottom = cy + ch - input_h;
    let sidebar_w = if participants.is_empty() { 0 } else { MEDIA_W as i32 };
    let sidebar_x = if media_on_right { cx + cw - sidebar_w } else { cx };
    let text_left = if media_on_right { cx + pad } else { cx + sidebar_w + GAP as i32 + pad };
    let text_right = if media_on_right { cx + cw - sidebar_w - GAP as i32 - pad } else { cx + cw - pad };

    if sidebar_w > 0 {
        let small_font = CreateFontW(
            -(text_size - 2).max(8), 0, 0, 0, 700, 0, 0, 0, 1, 0, 0, 3, 0, font_name.as_ptr(),
        );
        let prev_font = SelectObject(mem_dc, small_font);
        let avatar_size = 48;
        let mut py = cy + pad;
        for p in participants {
            if py + avatar_size > cy + ch - pad {
                break;
            }
            let ax = sidebar_x + pad;
            if let Some(bgra) = &p.bgra {
                let bmi_header = BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: 48,
                    biHeight: -48,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: 0,
                    biSizeImage: 0,
                    biXPelsPerMeter: 0,
                    biYPelsPerMeter: 0,
                    biClrUsed: 0,
                    biClrImportant: 0,
                };
                #[repr(C)]
                struct BmpInfo { header: BITMAPINFOHEADER, colors: [u8; 4] }
                let bmi = BmpInfo { header: bmi_header, colors: [0; 4] };
                StretchDIBits(
                    mem_dc,
                    ax,
                    py,
                    avatar_size,
                    avatar_size,
                    0,
                    0,
                    48,
                    48,
                    bgra.as_ptr() as *const _,
                    &bmi as *const _ as *const BITMAPINFO,
                    0,
                    SRCCOPY,
                );
            } else {
                let sq = RECT { left: ax, top: py, right: ax + avatar_size, bottom: py + avatar_size };
                let brush = CreateSolidBrush(p.colorref);
                FillRect(mem_dc, &sq, brush);
                DeleteObject(brush);
                if let Some(initial) = p.name.chars().next() {
                    let s = wide(&initial.to_uppercase().to_string());
                    SetTextColor(mem_dc, 0x00FFFFFF);
                    TextOutW(mem_dc, ax + avatar_size / 2 - text_size / 3, py + avatar_size / 2 - text_size / 2, s.as_ptr(), 1);
                }
            }
            if p.talking {
                let brush = CreateSolidBrush(p.colorref);
                for i in 0..TALK_STROKE as i32 {
                    let rc = RECT { left: ax + i, top: py + i, right: ax + avatar_size - i, bottom: py + avatar_size - i };
                    FrameRect(mem_dc, &rc, brush);
                }
                DeleteObject(brush);
            }

            let name = short_name(&p.name);
            let name_w = wide(&name);
            let name_len = (name_w.len() - 1) as i32;
            let nx = ax + avatar_size + 5;
            let ny = py + (avatar_size - text_size) / 2;
            if outline_thickness > 0 {
                SetTextColor(mem_dc, outline_colorref);
                for ox in -outline_thickness..=outline_thickness {
                    for oy in -outline_thickness..=outline_thickness {
                        if ox != 0 || oy != 0 {
                            TextOutW(mem_dc, nx + ox, ny + oy, name_w.as_ptr(), name_len);
                        }
                    }
                }
            }
            SetTextColor(mem_dc, p.colorref);
            TextOutW(mem_dc, nx, ny, name_w.as_ptr(), name_len);
            py += avatar_size + pad;
        }
        SelectObject(mem_dc, prev_font);
        DeleteObject(small_font);
    }
    SelectObject(mem_dc, font);

    let mut heights = Vec::with_capacity(messages.len());
    for m in messages {
        let text = if let Some(sender) = &m.sender {
            format!("[{}] {}: {}", m.time, sender, m.content)
        } else {
            format!("[{}] {}", m.time, m.content)
        };
        let wt = wide(&text);
        let mut rc = RECT { left: text_left, top: 0, right: text_right, bottom: msg_bottom };
        let th = DrawTextW(mem_dc, wt.as_ptr(), (wt.len() - 1) as i32, &mut rc, DT_LEFT | DT_WORDBREAK | DT_NOPREFIX | DT_CALCRECT);
        heights.push(th.max(text_size + 4));
    }
    let total_h: i32 = heights.iter().sum();
    let avail_h = msg_bottom - cy - pad;
    let mut y = if total_h <= avail_h { msg_bottom - total_h } else { cy + pad };
    let start_idx = if total_h > avail_h {
        let mut skip = total_h - avail_h;
        let mut idx = 0;
        while idx < heights.len() && skip > 0 {
            skip -= heights[idx];
            idx += 1;
        }
        idx
    } else { 0 };
    if start_idx > 0 {
        y = msg_bottom;
        for i in (start_idx..heights.len()).rev() { y -= heights[i]; }
    }

    for (i, m) in messages.iter().enumerate().skip(start_idx) {
        if y >= msg_bottom { break; }
        let is_system = m.sender.is_none();
        let full = if let Some(sender) = &m.sender {
            format!("[{}] {}: {}", m.time, sender, m.content)
        } else {
            format!("[{}] {}", m.time, m.content)
        };
        let full_w = wide(&full);
        let full_len = (full_w.len() - 1) as i32;
        if outline_thickness > 0 {
            SetTextColor(mem_dc, outline_colorref);
            for ox in -outline_thickness..=outline_thickness {
                for oy in -outline_thickness..=outline_thickness {
                    if ox != 0 || oy != 0 {
                        let mut rc = RECT { left: text_left + ox, top: y + oy, right: text_right + ox, bottom: msg_bottom };
                        DrawTextW(mem_dc, full_w.as_ptr(), full_len, &mut rc, DT_LEFT | DT_WORDBREAK | DT_NOPREFIX);
                    }
                }
            }
        }
        SetTextColor(mem_dc, if is_system { m.colorref } else { m.text_colorref });
        let mut rc = RECT { left: text_left, top: y, right: text_right, bottom: msg_bottom };
        DrawTextW(mem_dc, full_w.as_ptr(), full_len, &mut rc, DT_LEFT | DT_WORDBREAK | DT_NOPREFIX);
        if let Some(sender) = &m.sender {
            let time_prefix = format!("[{}] ", m.time);
            let time_w = wide(&time_prefix);
            let time_len = (time_w.len() - 1) as i32;
            let mut time_size = SIZE { cx: 0, cy: 0 };
            GetTextExtentPoint32W(mem_dc, time_w.as_ptr(), time_len, &mut time_size);
            let name_prefix = format!("{}: ", sender);
            let name_w = wide(&name_prefix);
            SetTextColor(mem_dc, m.colorref);
            TextOutW(mem_dc, text_left + time_size.cx, y, name_w.as_ptr(), (name_w.len() - 1) as i32);
        }
        y += heights[i];
    }

    let bar_top = cy + ch - input_h;
    let bar_rc = RECT { left: cx, top: bar_top, right: cx + cw, bottom: cy + ch };
    let bar = CreateSolidBrush(OVERLAY_INPUT_BG);
    FillRect(mem_dc, &bar_rc, bar);
    DeleteObject(bar);
    let line = CreateSolidBrush(0x003A3A3A);
    FillRect(mem_dc, &RECT { left: cx, top: bar_top, right: cx + cw, bottom: bar_top + 1 }, line);
    DeleteObject(line);

    let tx = cx + pad + 4;
    let ty = bar_top + (input_h - text_size) / 2;
    if input_text.is_empty() {
        let ph = wide("Type a message...");
        SetTextColor(mem_dc, 0x00888888);
        TextOutW(mem_dc, tx, ty, ph.as_ptr(), (ph.len() - 1) as i32);
    } else {
        let txt = wide(input_text);
        SetTextColor(mem_dc, 0x00FFFFFF);
        TextOutW(mem_dc, tx, ty, txt.as_ptr(), (txt.len() - 1) as i32);
    }
    let caret_byte = char_to_byte(input_text, sel_end);
    let before = wide(&input_text[..caret_byte]);
    let mut caret_size = SIZE { cx: 0, cy: 0 };
    GetTextExtentPoint32W(mem_dc, before.as_ptr(), (before.len() - 1) as i32, &mut caret_size);
    let caret = CreateSolidBrush(0x00FFFFFF);
    FillRect(mem_dc, &RECT { left: tx + caret_size.cx, top: bar_top + 3, right: tx + caret_size.cx + 2, bottom: cy + ch - 3 }, caret);
    DeleteObject(caret);

    BitBlt(hdc, 0, 0, w, h, mem_dc, 0, 0, SRCCOPY);

    SelectObject(mem_dc, old_font);
    SelectObject(mem_dc, old_bmp);
    DeleteObject(font);
    DeleteObject(bmp);
    DeleteDC(mem_dc);
    ReleaseDC(hwnd, hdc);
}

#[cfg(target_os = "windows")]
fn char_to_byte(s: &str, char_pos: usize) -> usize {
    s.char_indices().nth(char_pos).map(|(i, _)| i).unwrap_or(s.len())
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
