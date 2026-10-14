//! Opt-in acceptance of click cursor feedback in the current session. It replaces
//! and restores the system cursor roles without injecting input, so the pointer
//! briefly changes. Role images are read back through GDI; the shape handed to
//! the display (including a hardware pointer) is read with Desktop Duplication.
//! It saves the normal and highlighted arrow, and the displayed shapes, beside
//! the report.
use super::*;
use click_cursor::{ClickCursor, Highlight, ROLES, RoleImage};
use display_pointer::{Duplication, PointerShape};
use std::{path::Path, time::Duration};

unsafe fn roles() -> Result<Vec<RoleImage>, String> {
    ROLES
        .iter()
        .map(|role| RoleImage::read(*role).ok_or(format!("Could not read cursor role {role}.")))
        .collect()
}

unsafe fn showing() -> bool {
    let mut info = CURSORINFO {
        cbSize: size_of::<CURSORINFO>() as u32,
        ..zeroed()
    };
    GetCursorInfo(&mut info) != 0 && info.flags & CURSOR_SHOWING != 0
}

/// Images side by side, each `(width, height, pixels)` over white then black.
fn save_bmp(images: &[(usize, usize, Vec<u32>, Vec<u32>)], path: &Path) -> Result<(), String> {
    let height = images.iter().map(|image| image.1).max().unwrap_or(0);
    let width: usize = images.iter().map(|image| image.0 * 2).sum();
    let mut bmp = vec![0u8; 54];
    bmp[0..2].copy_from_slice(b"BM");
    bmp[2..6].copy_from_slice(&(54 + width as u32 * height as u32 * 4).to_le_bytes());
    bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
    bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
    bmp[18..22].copy_from_slice(&(width as i32).to_le_bytes());
    bmp[22..26].copy_from_slice(&(-(height as i32)).to_le_bytes());
    bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
    bmp[28..30].copy_from_slice(&32u16.to_le_bytes());
    for y in 0..height {
        for (w, h, over_white, over_black) in images {
            for pixels in [over_white, over_black] {
                for x in 0..*w {
                    let pixel = if y < *h { pixels[y * w + x] } else { 0x808080 };
                    bmp.extend_from_slice(&(pixel | 0xff00_0000).to_le_bytes());
                }
            }
        }
    }
    std::fs::write(path, bmp).map_err(|e| e.to_string())
}

fn role_bmp(image: &RoleImage) -> (usize, usize, Vec<u32>, Vec<u32>) {
    let (w, h) = (image.width as usize, image.height as usize);
    (w, h, image.over_white.clone(), image.over_black.clone())
}

/// A displayed colour shape composited over white and over black.
fn shape_bmp(shape: &PointerShape) -> (usize, usize, Vec<u32>, Vec<u32>) {
    let over = |background: u32| {
        shape
            .pixels
            .iter()
            .map(|pixel| {
                let alpha = pixel >> 24;
                [16, 8, 0].into_iter().fold(0, |out, shift| {
                    let channel = (pixel >> shift) & 255;
                    let base = (background >> shift) & 255;
                    out | ((channel * alpha + base * (255 - alpha)) / 255) << shift
                })
            })
            .collect()
    };
    let (w, h) = (shape.width as usize, shape.height as usize);
    (w, h, over(0xffffff), over(0))
}

unsafe fn pump() {
    let mut message: MSG = zeroed();
    while PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
        TranslateMessage(&message);
        DispatchMessageW(&message);
    }
}

fn wait(mut done: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        unsafe { pump() };
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    false
}

/// A small white topmost window under the pointer with the standard arrow as
/// its class cursor, so the displayed pointer is a system role even over an
/// application that draws its own cursor.
unsafe fn arrow_window() -> HWND {
    let class = wide("robocam.cursor-test");
    let instance = GetModuleHandleW(null());
    let window_class = WNDCLASSW {
        lpfnWndProc: Some(DefWindowProcW),
        hInstance: instance,
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        hbrBackground: GetStockObject(WHITE_BRUSH),
        lpszClassName: class.as_ptr(),
        ..zeroed()
    };
    RegisterClassW(&window_class);
    let (x, y) = cursor();
    let window = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        class.as_ptr(),
        null(),
        WS_POPUP | WS_VISIBLE,
        x - 150,
        y - 150,
        300,
        300,
        null_mut(),
        null_mut(),
        instance,
        null(),
    );
    // A move makes Windows ask the window under the pointer for its cursor.
    SetCursorPos(x + 1, y);
    SetCursorPos(x, y);
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(200) {
        pump();
        std::thread::sleep(Duration::from_millis(5));
    }
    window
}

pub(super) unsafe fn run(report: &Path) -> Result<(), String> {
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    let window = arrow_window();
    let original = roles()?;
    let showing_before = showing();
    let source = click_cursor::arrow_source();
    let mut preview = Highlight::default();
    let (file, size) = preview
        .highlight_file()
        .ok_or("Could not build the highlight pointer file.")?;
    let expected = RoleImage::from_file(&file, size).ok_or("Could not load the pointer file.")?;
    drop(preview);
    let mut display = Duplication::new();
    let displayed_before = match &mut display {
        Ok(display) => {
            display.poll(Duration::from_millis(500));
            display.latest.clone()
        }
        Err(_) => None,
    };
    let mut feedback = ClickCursor::default();

    // A click with a 0 ms press: released before the worker even sees it.
    let started = Instant::now();
    feedback.update(true, true, false);
    feedback.update(true, false, false);
    let applied = wait(|| feedback.applied());
    let apply_ms = started.elapsed().as_millis();
    let pressed = roles()?;
    let highlighted = applied && pressed.iter().all(|image| image.is_highlight_of(&expected));
    let showing_pressed = showing();
    let displayed_pressed = match &mut display {
        Ok(display) => {
            display.wait_for_change(displayed_before.as_ref(), Duration::from_millis(400));
            display.latest.clone()
        }
        Err(_) => None,
    };
    // Windows must not put its own pointer back while the highlight is shown.
    let late = click_cursor::MINIMUM_VISIBLE * 4 / 5;
    std::thread::sleep(late.saturating_sub(started.elapsed()));
    let persisted = feedback.applied() && roles()? == pressed;
    let released = wait(|| !feedback.applied());
    let visible_ms = started.elapsed().as_millis();
    let minimum_kept = visible_ms >= click_cursor::MINIMUM_VISIBLE.as_millis();
    let restored = released && roles()? == original;
    let displayed_after = match &mut display {
        Ok(display) => {
            display.wait_for_change(displayed_pressed.as_ref(), Duration::from_millis(1000));
            display.latest.clone()
        }
        Err(_) => None,
    };

    let mut images = vec![role_bmp(&original[0]), role_bmp(&pressed[0])];
    images.extend(
        [&displayed_before, &displayed_pressed, &displayed_after]
            .into_iter()
            .flatten()
            .filter(|shape| !shape.pixels.is_empty())
            .map(shape_bmp),
    );
    save_bmp(&images, &report.with_extension("bmp"))?;

    // Rapid presses end with the saved scheme, whichever request arrives last.
    for cycle in 0..20 {
        feedback.update(true, true, false);
        if cycle % 2 == 0 {
            std::thread::sleep(Duration::from_millis(15));
        }
        feedback.update(true, false, false);
    }
    feedback.update(false, false, false);
    let repeated = !feedback.applied() && roles()? == original;

    // Closing an armed guard's pipe is what Robocam terminating looks like.
    let mut crashed = Highlight::default();
    crashed.apply();
    let crash_applied = crashed.applied();
    let started = Instant::now();
    crashed.abandon();
    let guard_ms = started.elapsed().as_millis();
    let recovered = crash_applied && roles()? == original;

    // The display received a black and yellow pointer as large as the user's
    // own (both include any shadow margin Windows adds), then the user's again.
    let display_verified = match (&displayed_before, &displayed_pressed, &displayed_after) {
        (Some(before), Some(pressed), Some(after)) => {
            let (black, yellow) = pressed.black_and_yellow();
            pressed.kind == 2
                && pressed.width.abs_diff(before.width) <= 2
                && pressed.height.abs_diff(before.height) <= 2
                && black > 0
                && yellow > 0
                && pressed != before
                && after.width == before.width
                && after.kind == before.kind
        }
        _ => false,
    };
    let showing_after = showing();
    let success = highlighted
        && persisted
        && minimum_kept
        && showing_pressed == showing_before
        && showing_after == showing_before
        && restored
        && repeated
        && recovered
        && (display_verified || display.is_err());
    let shape = |shape: &Option<PointerShape>| shape.as_ref().map(PointerShape::report);
    let result = serde_json::json!({
        "source":format!("{source:?}"),"pointer_size":size,"highlight_file":file,
        "highlight":{"width":expected.width,"height":expected.height,
            "hotspot":[expected.hotspot.0,expected.hotspot.1]},
        "roles":ROLES.len(),"highlighted_all_roles":highlighted,
        "apply_ms":apply_ms,"visible_ms":visible_ms,
        "minimum_visible_ms":click_cursor::MINIMUM_VISIBLE.as_millis(),
        "highlight_persisted":persisted,"guard_restore_ms":guard_ms,
        "cursor_showing":{"before":showing_before,"pressed":showing_pressed,
            "after":showing_after},
        "restored_on_release":restored,"restored_after_repeated_presses":repeated,
        "restored_by_guard":recovered,
        "display_pointer":{"available":display.is_ok(),
            "error":display.as_ref().err(),
            "before":shape(&displayed_before),"pressed":shape(&displayed_pressed),
            "after":shape(&displayed_after)},
        "physical_display_verified":display_verified,"success":success});
    if !window.is_null() {
        DestroyWindow(window);
    }
    std::fs::write(report, serde_json::to_vec_pretty(&result).unwrap())
        .map_err(|e| e.to_string())?;
    if !success {
        return Err("Click cursor acceptance failed; see report".into());
    }
    Ok(())
}
