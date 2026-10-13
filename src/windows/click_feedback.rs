//! Temporary click feedback, without changing the user's Windows cursor scheme.
use super::*;
use windows_sys::Win32::{Media::Audio::*, UI::Magnification::*};

const CLICK_SOUND: &[u8] = include_bytes!("../../assets/sounds/eviacam-click.wav");
const DRAG_SOUND: &[u8] = include_bytes!("../../assets/sounds/eviacam-drag.wav");
const CURSOR_CLASS: &str = "robocam.click-cursor";

pub(super) unsafe fn play_sound(drag_start: bool) {
    // Static storage remains valid until asynchronous playback finishes.
    let sound = if drag_start { DRAG_SOUND } else { CLICK_SOUND };
    PlaySoundW(
        sound.as_ptr().cast(),
        null_mut(),
        SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
    );
}

#[derive(Default)]
pub(super) struct ClickCursor {
    window: HWND,
    initialized: bool,
    active: bool,
}

impl ClickCursor {
    pub(super) fn active(&self) -> bool {
        self.active
    }
    pub(super) fn window(&self) -> HWND {
        self.window
    }
    pub(super) unsafe fn native_visible(&self) -> bool {
        GetWindowLongPtrW(self.window, GWLP_USERDATA) & cursor_image::NATIVE_VISIBLE != 0
    }

    pub(super) unsafe fn update(&mut self, active: bool, dry_run: bool) {
        if active == self.active {
            return;
        }
        if dry_run {
            self.active = active;
            return;
        }
        if !active {
            self.hide();
            return;
        }
        if !self.prepare() {
            return;
        }
        // Show a non-activating, click-through replacement before hiding the
        // real cursor. This also works over applications with custom cursors.
        if !position_window(self.window) {
            return;
        }
        if !cursor_image::show(self.window) {
            ShowWindow(self.window, SW_HIDE);
            return;
        }
        if !refresh_visibility(self.window) {
            ShowWindow(self.window, SW_HIDE);
            return;
        }
        self.active = true;
        cursor_image::set_active(self.window, true);
        if SetTimer(self.window, 1, 10, None) == 0 {
            self.hide();
        }
    }

    unsafe fn prepare(&mut self) -> bool {
        if !self.initialized {
            self.initialized = MagInitialize() != 0;
            if !self.initialized {
                return false;
            }
        }
        if self.window.is_null() {
            let instance = GetModuleHandleW(null());
            let class = wide_raw(CURSOR_CLASS);
            let definition = WNDCLASSW {
                lpfnWndProc: Some(cursor_proc),
                hInstance: instance,
                lpszClassName: class.as_ptr(),
                ..zeroed()
            };
            RegisterClassW(&definition);
            self.window = CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_NOACTIVATE
                    | WS_EX_TOOLWINDOW
                    | WS_EX_TOPMOST,
                class.as_ptr(),
                wide_raw("").as_ptr(),
                WS_POPUP,
                0,
                0,
                32,
                32,
                null_mut(),
                null_mut(),
                instance,
                null(),
            );
            if self.window.is_null() {
                return false;
            }
            SetWindowLongPtrW(self.window, GWLP_USERDATA, cursor_image::NATIVE_VISIBLE);
            let exclude_from_peek: i32 = 1;
            DwmSetWindowAttribute(
                self.window,
                DWMWA_EXCLUDED_FROM_PEEK as u32,
                (&exclude_from_peek as *const i32).cast(),
                size_of::<i32>() as u32,
            );
        }
        true
    }

    unsafe fn hide(&mut self) {
        if self.initialized && self.active {
            MagShowSystemCursor(1);
            SetWindowLongPtrW(
                self.window,
                GWLP_USERDATA,
                GetWindowLongPtrW(self.window, GWLP_USERDATA) | cursor_image::NATIVE_VISIBLE,
            );
            DwmFlush();
        }
        if !self.window.is_null() {
            cursor_image::set_active(self.window, false);
            KillTimer(self.window, 1);
            ShowWindow(self.window, SW_HIDE);
        }
        self.active = false;
    }
}

impl Drop for ClickCursor {
    fn drop(&mut self) {
        unsafe {
            self.hide();
            if !self.window.is_null() {
                DestroyWindow(self.window);
            }
            if self.initialized {
                MagUninitialize();
            }
        }
    }
}

unsafe fn position_window(hwnd: HWND) -> bool {
    let mut point: POINT = zeroed();
    if GetCursorPos(&mut point) == 0 {
        return false;
    }
    let hotspot = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
    SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        point.x - (hotspot & 0xffff) as i32,
        point.y - ((hotspot >> 16) & 0x3fff) as i32,
        0,
        0,
        SWP_NOSIZE
            | SWP_NOACTIVATE
            | SWP_NOOWNERZORDER
            | if hotspot & cursor_image::ACTIVE != 0 {
                SWP_SHOWWINDOW
            } else {
                0
            },
    ) != 0
}

/// Trusted system UI (for example the on-screen keyboard) can live above the
/// application's topmost band. Never hide the native cursor behind that UI.
unsafe fn refresh_visibility(hwnd: HWND) -> bool {
    let mut point: POINT = zeroed();
    if GetCursorPos(&mut point) == 0 {
        return false;
    }
    let target = GetAncestor(WindowFromPoint(point), GA_ROOT);
    let mut above = GetWindow(hwnd, GW_HWNDPREV);
    let mut covered = false;
    while !above.is_null() {
        if above == target {
            covered = true;
            break;
        }
        above = GetWindow(above, GW_HWNDPREV);
    }
    let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
    if covered != (state & cursor_image::NATIVE_VISIBLE != 0) {
        if MagShowSystemCursor(i32::from(covered)) == 0 {
            return false;
        }
        SetWindowLongPtrW(
            hwnd,
            GWLP_USERDATA,
            (state & !cursor_image::NATIVE_VISIBLE)
                | if covered {
                    cursor_image::NATIVE_VISIBLE
                } else {
                    0
                },
        );
    }
    true
}

unsafe extern "system" fn cursor_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST => HTTRANSPARENT as isize,
        WM_MOUSEACTIVATE => MA_NOACTIVATE as isize,
        WM_TIMER => {
            if GetWindowLongPtrW(hwnd, GWLP_USERDATA) & cursor_image::ACTIVE != 0 {
                position_window(hwnd);
                refresh_visibility(hwnd);
            }
            0
        }
        WM_DPICHANGED => {
            PostMessageW(hwnd, WM_APP, 0, 0);
            0
        }
        WM_APP => {
            if GetWindowLongPtrW(hwnd, GWLP_USERDATA) & cursor_image::ACTIVE != 0
                && !cursor_image::show(hwnd)
            {
                MagShowSystemCursor(1);
                SetWindowLongPtrW(
                    hwnd,
                    GWLP_USERDATA,
                    GetWindowLongPtrW(hwnd, GWLP_USERDATA) | cursor_image::NATIVE_VISIBLE,
                );
                cursor_image::set_active(hwnd, false);
                ShowWindow(hwnd, SW_HIDE);
            }
            0
        }
        WM_WINDOWPOSCHANGING
            if GetWindowLongPtrW(hwnd, GWLP_USERDATA) & cursor_image::ACTIVE != 0 =>
        {
            let position = &mut *(lp as *mut WINDOWPOS);
            position.flags &= !SWP_HIDEWINDOW;
            if position.flags & SWP_NOZORDER == 0 {
                position.hwndInsertAfter = HWND_TOPMOST;
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

impl App {
    pub(super) unsafe fn update_click_cursor(&mut self) {
        let active = self.config.profile().black_cursor_on_click
            && (self.injected.iter().any(|down| *down) || !self.pending_input.is_empty());
        self.click_cursor.update(active, self.dry_run);
    }

    pub(super) unsafe fn play_click_sound(&self, drag_start: bool) {
        if self.config.profile().click_sound && !self.dry_run {
            play_sound(drag_start);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires an interactive Windows desktop"]
    fn native_cursor_lifecycle() {
        unsafe {
            let mut feedback = ClickCursor::default();
            let foreground = GetForegroundWindow();
            feedback.update(true, false);
            assert!(
                feedback.active(),
                "could not display native cursor feedback"
            );
            assert_ne!(IsWindowVisible(feedback.window), 0);
            assert_eq!(GetForegroundWindow(), foreground, "feedback stole focus");
            let mut point: POINT = zeroed();
            GetCursorPos(&mut point);
            assert_ne!(
                WindowFromPoint(point),
                feedback.window,
                "feedback intercepted input"
            );
            feedback.update(false, false);
            assert!(!feedback.active());
            assert_eq!(IsWindowVisible(feedback.window), 0);
            // Reusing and then dropping an active instance must restore too.
            feedback.update(true, false);
            assert!(feedback.active());
        }
    }
}
