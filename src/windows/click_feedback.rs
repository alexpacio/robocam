//! Temporary click feedback, without changing the user's Windows cursor scheme.
use super::*;
use windows_sys::Win32::{Media::Audio::*, UI::Magnification::*};

const CLICK_SOUND: &[u8] = include_bytes!("../../assets/sounds/eviacam-click.wav");
const DRAG_SOUND: &[u8] = include_bytes!("../../assets/sounds/eviacam-drag.wav");
const CURSOR_CLASS: &str = "robocam.click-cursor";
const TRANSPARENT_COLOR: u32 = 0x00ff00ff;

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
        ShowWindow(self.window, SW_SHOWNOACTIVATE);
        UpdateWindow(self.window);
        if MagShowSystemCursor(0) == 0 {
            ShowWindow(self.window, SW_HIDE);
            return;
        }
        self.active = true;
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
            if SetLayeredWindowAttributes(self.window, TRANSPARENT_COLOR, 255, LWA_COLORKEY) == 0 {
                DestroyWindow(self.window);
                self.window = null_mut();
                return false;
            }
        }
        true
    }

    unsafe fn hide(&mut self) {
        if self.initialized && self.active {
            MagShowSystemCursor(1);
        }
        if !self.window.is_null() {
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
    let dpi = GetDpiForWindow(hwnd).max(96);
    let size = GetSystemMetricsForDpi(SM_CXCURSOR, dpi).max(scale(32, dpi));
    // The tip is at (1, 1) in a 32-unit drawing; keep the real click hotspot.
    let inset = (size / 32).max(1);
    SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        point.x - inset,
        point.y - inset,
        size,
        size,
        SWP_NOACTIVATE | SWP_NOOWNERZORDER,
    ) != 0
}

unsafe extern "system" fn cursor_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST => HTTRANSPARENT as isize,
        WM_MOUSEACTIVATE => MA_NOACTIVATE as isize,
        WM_TIMER => {
            position_window(hwnd);
            0
        }
        WM_DPICHANGED => {
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_PAINT => {
            let mut paint: PAINTSTRUCT = zeroed();
            let dc = BeginPaint(hwnd, &mut paint);
            let mut rect: RECT = zeroed();
            GetClientRect(hwnd, &mut rect);
            let background = CreateSolidBrush(TRANSPARENT_COLOR);
            FillRect(dc, &rect, background);
            DeleteObject(background);
            let size = rect.right.max(32);
            let points = [
                (1, 1),
                (1, 24),
                (7, 18),
                (12, 29),
                (17, 27),
                (12, 16),
                (22, 16),
            ]
            .map(|(x, y)| POINT {
                x: x * size / 32,
                y: y * size / 32,
            });
            let pen = CreatePen(PS_SOLID, (size / 32).max(1), rgb(150, 150, 150));
            let old_pen = SelectObject(dc, pen);
            let old_brush = SelectObject(dc, GetStockObject(BLACK_BRUSH));
            Polygon(dc, points.as_ptr(), points.len() as i32);
            SelectObject(dc, old_brush);
            SelectObject(dc, old_pen);
            DeleteObject(pen);
            EndPaint(hwnd, &paint);
            0
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
