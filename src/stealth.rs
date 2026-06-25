//! Platform stealth-window helpers.

#[cfg(target_os = "windows")]
pub(crate) fn find_main_hwnd() -> *mut core::ffi::c_void {
    std::ptr::null_mut()
}

#[cfg(target_os = "windows")]
pub(crate) fn stealth_hide_window(_hwnd: *mut core::ffi::c_void) {}

#[cfg(target_os = "windows")]
pub(crate) fn stealth_show_window(_hwnd: *mut core::ffi::c_void) {}
