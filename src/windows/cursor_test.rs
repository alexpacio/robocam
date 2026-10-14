//! Opt-in compositor acceptance in an owned white fixture. GDI captures verify
//! the overlay pixels, not the physical hardware cursor; human QA is still needed.
use super::*;
use std::{path::Path, time::Duration};

struct Fixture {
    hwnd: HWND,
    original: POINT,
    pressed: bool,
}
impl Fixture {
    unsafe fn press(&mut self, down: bool) -> Result<(), String> {
        if down {
            let p = cursor();
            if WindowFromPoint(POINT { x: p.0, y: p.1 }) != self.hwnd {
                return Err("Test window is covered; no click was injected.".into());
            }
        }
        if !mouse(
            if down {
                MOUSEEVENTF_LEFTDOWN
            } else {
                MOUSEEVENTF_LEFTUP
            },
            0,
            0,
        ) {
            return Err("Test input was rejected.".into());
        }
        self.pressed = down;
        Ok(())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        unsafe {
            if self.pressed {
                mouse(MOUSEEVENTF_LEFTUP, 0, 0);
            }
            if GetCapture() == self.hwnd {
                ReleaseCapture();
            }
            DestroyWindow(self.hwnd);
            SetCursorPos(self.original.x, self.original.y);
        }
    }
}
unsafe fn mouse(flags: u32, x: i32, y: i32) -> bool {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: x,
                dy: y,
                dwFlags: flags,
                dwExtraInfo: INPUT_TAG,
                ..zeroed()
            },
        },
    };
    SendInput(1, &input, size_of::<INPUT>() as i32) == 1
}
unsafe extern "system" fn procedure(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_LBUTTONDOWN => {
            SetCapture(hwnd);
            0
        }
        WM_LBUTTONUP => {
            ReleaseCapture();
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
unsafe fn pump(ms: u64) {
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(ms) {
        let mut msg: MSG = zeroed();
        while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}
unsafe fn image(cursor: HCURSOR) -> Result<(cursor_image::Surface, POINT), String> {
    cursor_image::native_image(cursor, 0xffffffff)
        .ok_or("Could not read the native cursor image.".into())
}
unsafe fn role_image(role: u32) -> Result<Vec<u32>, String> {
    Ok(image(LoadCursorW(null_mut(), role as usize as *const u16))?
        .0
        .data()
        .to_vec())
}
const ROLES: [u32; 13] = [
    OCR_NORMAL,
    OCR_IBEAM,
    OCR_WAIT,
    OCR_CROSS,
    OCR_UP,
    OCR_SIZENWSE,
    OCR_SIZENESW,
    OCR_SIZEWE,
    OCR_SIZENS,
    OCR_SIZEALL,
    OCR_NO,
    OCR_HAND,
    OCR_APPSTARTING,
];

unsafe fn capture(hwnd: HWND) -> Result<cursor_image::Surface, String> {
    let mut bounds: RECT = zeroed();
    GetWindowRect(hwnd, &mut bounds);
    let image = cursor_image::Surface::new(300, 300).ok_or("Could not allocate capture")?;
    let dc = GetDC(null_mut());
    let copied = BitBlt(
        image.dc,
        0,
        0,
        300,
        300,
        dc,
        bounds.left,
        bounds.top,
        SRCCOPY | CAPTUREBLT,
    );
    ReleaseDC(null_mut(), dc);
    if copied == 0 {
        return Err("Could not capture compositor pixels".into());
    }
    GdiFlush();
    Ok(image)
}

unsafe fn sample(
    stage: &str,
    feedback: &click_feedback::ClickCursor,
    fixture: HWND,
    expected: bool,
) -> Result<serde_json::Value, String> {
    DwmFlush();
    let image = capture(fixture)?;
    let black = image.data().iter().filter(|p| **p & 0xffffff == 0).count();
    let yellow = image
        .data()
        .iter()
        .filter(|p| {
            let (r, g, b) = ((**p >> 16) & 255, (**p >> 8) & 255, **p & 255);
            r >= 180 && g >= 140 && b <= 64 && r >= g + 8
        })
        .count();
    let mut position: POINT = zeroed();
    GetCursorPos(&mut position);
    let mut bounds: RECT = zeroed();
    GetWindowRect(feedback.window(), &mut bounds);
    let (_, hotspot) = cursor_image::arrow().ok_or("Could not load arrow")?;
    let at_pointer = bounds.left == position.x - hotspot.x && bounds.top == position.y - hotspot.y;
    let click_through = WindowFromPoint(position) == fixture;
    let visible = IsWindowVisible(feedback.window()) != 0;
    let success = click_through
        && if expected {
            feedback.active() && visible && at_pointer && black > 5 && yellow > 2
        } else {
            !feedback.active() && !visible && yellow == 0
        };
    Ok(serde_json::json!({"stage":stage, "overlay_visible":visible,
        "black_pixels":black,"yellow_pixels":yellow,"at_pointer":at_pointer,
        "click_through":click_through,"success":success}))
}

pub(super) unsafe fn run(report: &Path) -> Result<(), String> {
    if [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON]
        .iter()
        .any(|key| GetAsyncKeyState(i32::from(*key)) < 0)
    {
        return Err("Release physical mouse buttons before running this test.".into());
    }
    let mut original: POINT = zeroed();
    if GetCursorPos(&mut original) == 0 {
        return Err("No interactive desktop.".into());
    }
    let mut monitor = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..zeroed()
    };
    GetMonitorInfoW(
        MonitorFromPoint(original, MONITOR_DEFAULTTONEAREST),
        &mut monitor,
    );
    let x = (monitor.rcWork.left + 128).min(monitor.rcWork.right - 300);
    let y = (monitor.rcWork.top + 128).min(monitor.rcWork.bottom - 300);
    let class = wide_raw("robocam.cursor-test");
    let instance = GetModuleHandleW(null());
    RegisterClassW(&WNDCLASSW {
        lpfnWndProc: Some(procedure),
        hInstance: instance,
        lpszClassName: class.as_ptr(),
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        hbrBackground: GetStockObject(WHITE_BRUSH) as HBRUSH,
        ..zeroed()
    });
    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
        class.as_ptr(),
        wide_raw("robocam cursor test").as_ptr(),
        WS_POPUP,
        x,
        y,
        300,
        300,
        null_mut(),
        null_mut(),
        instance,
        null(),
    );
    if hwnd.is_null() {
        return Err("Could not create the test target.".into());
    }
    let mut fixture = Fixture {
        hwnd,
        original,
        pressed: false,
    };
    SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        x,
        y,
        300,
        300,
        SWP_SHOWWINDOW | SWP_NOACTIVATE,
    );
    UpdateWindow(hwnd);
    SetCursorPos(x + 80, y + 80);
    pump(60);
    SetCursor(LoadCursorW(null_mut(), IDC_ARROW));
    let original_roles: Vec<_> = ROLES
        .into_iter()
        .map(|role| Ok((role, role_image(role)?)))
        .collect::<Result<_, String>>()?;
    let mut feedback = click_feedback::ClickCursor::default();
    let mut samples = Vec::new();
    fixture.press(true)?;
    pump(20);
    // The real press can activate the fixture; measure the overlay only after it.
    let foreground = GetForegroundWindow();
    feedback.update(true, false);
    if !feedback.active() {
        return Err("Cursor overlay could not start".into());
    }
    for i in 0..40 {
        if i >= 10 {
            let (dx, dy) =
                pointer::absolute_position((x + 80 + (i - 10) * 2, y + 80 + i - 10), screen());
            mouse(
                MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                dx,
                dy,
            );
        }
        // Deliberately do NOT pump the caller's UI: movement/rendering must
        // continue even while the webcam or a modal window blocks that thread.
        std::thread::sleep(Duration::from_millis(50));
        samples.push(sample(
            "held-with-main-thread-blocked",
            &feedback,
            hwnd,
            true,
        )?);
    }
    capture(hwnd)?.save(&report.with_extension("bmp"))?;
    fixture.press(false)?;
    feedback.update(false, false);
    pump(30);
    samples.push(sample("released", &feedback, hwnd, false)?);
    for role in [OCR_NORMAL, OCR_HAND, OCR_IBEAM, OCR_CROSS, OCR_SIZEALL] {
        fixture.press(true)?;
        feedback.update(true, false);
        SetCursor(LoadCursorW(null_mut(), role as usize as *const u16));
        pump(20);
        samples.push(sample("target-changed-cursor", &feedback, hwnd, true)?);
        fixture.press(false)?;
        feedback.update(false, false);
    }
    SetCursor(LoadCursorW(null_mut(), IDC_ARROW));
    for cycle in 0..30 {
        fixture.press(true)?;
        feedback.update(true, false);
        if cycle % 3 != 0 {
            pump(15);
        }
        samples.push(sample("repeated-down", &feedback, hwnd, true)?);
        fixture.press(false)?;
        feedback.update(false, false);
        pump(10);
        samples.push(sample("repeated-up", &feedback, hwnd, false)?);
    }
    let foreground_unchanged = GetForegroundWindow() == foreground;
    feedback.update(true, false);
    let overlay = feedback.window();
    drop(feedback);
    let dropped_window = IsWindow(overlay) == 0;
    let roles_unchanged = original_roles
        .iter()
        .all(|(role, pixels)| role_image(*role).is_ok_and(|now| now == *pixels));
    let success = roles_unchanged
        && dropped_window
        && foreground_unchanged
        && samples.iter().all(|s| s["success"] == true);
    let result = serde_json::json!({"samples":samples,"roles_unchanged":roles_unchanged,
        "drop_destroyed_overlay":dropped_window,"foreground_unchanged":foreground_unchanged,
        "physical_cursor_visibility_verified":false,"success":success});
    std::fs::write(report, serde_json::to_vec_pretty(&result).unwrap())
        .map_err(|e| e.to_string())?;
    if !success {
        return Err("Composited cursor acceptance failed; see report".into());
    }
    Ok(())
}
