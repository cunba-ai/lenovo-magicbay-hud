//! Find the HUD monitor (1424x280) and position the egui window on it via raw
//! Win32 SetWindowPos (physical pixels — no DPI/coordinate ambiguity).
#[cfg(windows)]
mod imp {
    use std::mem::{size_of, zeroed};
    use windows::core::{w, BOOL, PCWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE};
    use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO};
    use windows::Win32::System::Console::GetConsoleWindow;
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GetForegroundWindow, GetWindowLongPtrW, GWL_EXSTYLE, SetWindowLongPtrW,
        SetWindowPos, ShowWindow, SW_HIDE, SWP_FRAMECHANGED, SWP_NOZORDER, SWP_SHOWWINDOW,
        WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
    };

    const HUD_W: i32 = 1424;
    const HUD_H: i32 = 280;

    #[derive(Default)]
    struct Collector {
        hud: Option<RECT>,
    }

    unsafe extern "system" fn monitor_cb(
        monitor: HMONITOR,
        _dc: HDC,
        _rect: *mut RECT,
        data: LPARAM,
    ) -> BOOL {
        unsafe {
            let c = &mut *(data.0 as *mut Collector);
            let mut mi: MONITORINFO = zeroed();
            mi.cbSize = size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(monitor, &mut mi).as_bool() {
                let r = mi.rcMonitor;
                if r.right - r.left == HUD_W && r.bottom - r.top == HUD_H {
                    c.hud = Some(r);
                }
            }
            BOOL::from(true)
        }
    }

    fn find() -> Option<RECT> {
        unsafe {
            let mut c = Collector::default();
            let _ = EnumDisplayMonitors(None, None, Some(monitor_cb), LPARAM(&mut c as *mut _ as isize));
            c.hud
        }
    }

    pub fn has_hud() -> bool {
        find().is_some()
    }

    /// Physical (x, y, w, h) of the HUD monitor.
    pub fn hud_physical() -> Option<(i32, i32, i32, i32)> {
        let r = find()?;
        Some((r.left, r.top, r.right - r.left, r.bottom - r.top))
    }

    /// Move + size OUR egui window to the physical rect (by window title — GetForegroundWindow
    /// races with other apps and grabs the wrong window), via raw SetWindowPos. The window stays
    /// square/opaque; the screen's own rounded glass clips the corners (so no desktop shows).
    pub fn place_window(x: i32, y: i32, w: i32, h: i32) {
        unsafe {
            let hwnd = FindWindowW(PCWSTR::null(), w!("MagicBay HUD")).unwrap_or(GetForegroundWindow());
            if hwnd.0.is_null() {
                return;
            }
            // Hide from the taskbar / Alt+Tab: add WS_EX_TOOLWINDOW and clear
            // WS_EX_APPWINDOW. Hide first, then re-show (SWP_SHOWWINDOW) so the
            // taskbar re-evaluates and drops the button.
            let _ = ShowWindow(hwnd, SW_HIDE);
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let new_ex = (ex | (WS_EX_TOOLWINDOW.0 as isize)) & !(WS_EX_APPWINDOW.0 as isize);
            let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_ex);
            let _ = SetWindowPos(hwnd, None, x, y, w, h, SWP_FRAMECHANGED | SWP_SHOWWINDOW | SWP_NOZORDER);
            // explicitly square (DWMWCP_DONOTROUND) so corners aren't transparent
            let pref: u32 = 1; // DWMWCP_DONOTROUND
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &pref as *const u32 as *const _,
                size_of::<u32>() as u32,
            );
        }
    }

    pub fn hide_console() {
        unsafe {
            let hwnd = GetConsoleWindow();
            if !hwnd.0.is_null() {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn has_hud() -> bool {
        false
    }
    pub fn hud_physical() -> Option<(i32, i32, i32, i32)> {
        None
    }
    pub fn place_window(_x: i32, _y: i32, _w: i32, _h: i32) {}
    pub fn hide_console() {}
}

pub fn has_hud() -> bool {
    imp::has_hud()
}
pub fn hud_physical() -> Option<(i32, i32, i32, i32)> {
    imp::hud_physical()
}
pub fn place_window(x: i32, y: i32, w: i32, h: i32) {
    imp::place_window(x, y, w, h)
}
pub fn hide_console() {
    imp::hide_console()
}
