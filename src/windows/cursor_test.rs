//! Opt-in compositor test. Place this test's own white window under the pointer
//! and check real black/gray pixels without moving the user's mouse.
use super::*;
use std::{path::Path, time::Duration};

struct Fixture {
    hwnd: HWND,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.hwnd);
        }
    }
}

pub(super) unsafe fn run(report: &Path) -> Result<(), String> {
    let mut original: POINT = zeroed();
    if GetCursorPos(&mut original) == 0 {
        return Err("No interactive desktop".into());
    }
    let foreground = GetForegroundWindow();
    let class = wide_raw("robocam.cursor-test");
    let instance = GetModuleHandleW(null());
    RegisterClassW(&WNDCLASSW {
        lpfnWndProc: Some(DefWindowProcW),
        hInstance: instance,
        lpszClassName: class.as_ptr(),
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        hbrBackground: GetStockObject(WHITE_BRUSH) as HBRUSH,
        ..zeroed()
    });
    let mut monitor = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..zeroed()
    };
    GetMonitorInfoW(
        MonitorFromPoint(original, MONITOR_DEFAULTTONEAREST),
        &mut monitor,
    );
    let x = (original.x - 70).clamp(
        monitor.rcWork.left,
        (monitor.rcWork.right - 256).max(monitor.rcWork.left),
    );
    let y = (original.y - 70).clamp(
        monitor.rcWork.top,
        (monitor.rcWork.bottom - 256).max(monitor.rcWork.top),
    );
    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
        class.as_ptr(),
        wide_raw("robocam cursor acceptance test").as_ptr(),
        WS_POPUP,
        x,
        y,
        256,
        256,
        null_mut(),
        null_mut(),
        instance,
        null(),
    );
    if hwnd.is_null() {
        return Err("Could not create cursor test window".into());
    }
    let _fixture = Fixture { hwnd };
    SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        x,
        y,
        256,
        256,
        SWP_NOACTIVATE | SWP_SHOWWINDOW,
    );
    UpdateWindow(hwnd);
    let mut feedback = click_feedback::ClickCursor::default();
    feedback.update(true, false);
    if !feedback.active() {
        return Err("Cursor replacement could not be displayed".into());
    }
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(250) {
        let mut message: MSG = zeroed();
        while PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    DwmFlush();
    let mut cursor_position: POINT = zeroed();
    GetCursorPos(&mut cursor_position);
    let mut feedback_bounds: RECT = zeroed();
    GetWindowRect(feedback.window(), &mut feedback_bounds);
    let mut cloaked: u32 = 0;
    DwmGetWindowAttribute(
        feedback.window(),
        DWMWA_CLOAKED as u32,
        (&mut cloaked as *mut u32).cast(),
        size_of::<u32>() as u32,
    );
    let (source, hotspot) = cursor_image::arrow(GetDpiForWindow(feedback.window()))
        .ok_or("Native arrow unavailable")?;
    source.save(&report.with_extension("source.bmp"))?;
    let source_opaque = source.data().iter().filter(|p| **p >> 24 == 255).count();
    let image = cursor_image::Surface::new(256, 256).ok_or("Could not capture test window")?;
    let screen = GetDC(null_mut());
    let copied = BitBlt(image.dc, 0, 0, 256, 256, screen, x, y, SRCCOPY | CAPTUREBLT);
    ReleaseDC(null_mut(), screen);
    if copied == 0 {
        return Err("Could not read compositor pixels".into());
    }
    GdiFlush();
    let black = image
        .data()
        .iter()
        .filter(|p| **p & 0x00ffffff == 0)
        .count();
    let gray = image
        .data()
        .iter()
        .filter(|p| {
            let channels = [**p & 255, (**p >> 8) & 255, (**p >> 16) & 255];
            channels.iter().all(|c| (100..=190).contains(c))
                && channels.iter().max().unwrap() - channels.iter().min().unwrap() < 12
        })
        .count();
    image.save(&report.with_extension("bmp"))?;
    let hit = WindowFromPoint(cursor_position);
    let native_fallback = feedback.native_visible();
    feedback.update(false, false);
    let result = serde_json::json!({"black_pixels":black, "gray_pixels":gray,
        "source_opaque":source_opaque, "hotspot":[hotspot.x, hotspot.y],
        "pointer":[cursor_position.x,cursor_position.y],
        "test_window":[x,y,x+256,y+256],
        "feedback_bounds":[feedback_bounds.left,feedback_bounds.top,feedback_bounds.right,feedback_bounds.bottom],
        "cloaked":cloaked,
        "click_through":hit != feedback.window(), "foreground_unchanged":GetForegroundWindow() == foreground,
        "native_fallback":native_fallback, "fixture_uncovered":hit == hwnd,
        "cursor_restored":!feedback.active(),
        "success":!feedback.active() && if hit == hwnd { black > 5 && gray > 2 && !native_fallback } else { native_fallback }});
    std::fs::write(report, serde_json::to_vec_pretty(&result).unwrap())
        .map_err(|e| e.to_string())?;
    if result["success"] != true {
        return Err("Cursor pixels were missing from the real desktop".into());
    }
    Ok(())
}
