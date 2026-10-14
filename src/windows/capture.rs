//! Reproducible documentation captures of this application's own native windows.
//! --capture-ui disables camera capture, input injection and settings writes.
use super::*;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use windows_sys::Win32::Storage::Xps::PrintWindow;

pub(super) fn start(directory: PathBuf, main: HWND, bar: HWND, options: HWND, bar_width: i32) {
    let handles = [main as usize, bar as usize, options as usize];
    std::thread::spawn(move || unsafe {
        let [main, bar, options] = handles.map(|h| h as HWND);
        let result = (|| -> Result<(), String> {
            std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
            std::thread::sleep(Duration::from_millis(800));
            window(main, &directory.join("main-window.bmp"), None)?;
            window(bar, &directory.join("topbar.bmp"), Some(bar_width))?;
            let chooser = GetDlgItem(options, PAGE as i32);
            for (page, filename) in [
                (2, "camera-settings.bmp"),
                (1, "click-settings.bmp"),
                (3, "click-bar-settings.bmp"),
                (4, "general-settings.bmp"),
            ] {
                SendMessageW(chooser, CB_SETCURSEL, page, 0);
                SendMessageW(
                    options,
                    WM_COMMAND,
                    usize::from(PAGE) | ((CBN_SELCHANGE as usize) << 16),
                    chooser as isize,
                );
                // Allow asynchronous driver capability discovery to finish.
                for _ in 0..40 {
                    let label = text(GetDlgItem(options, 449));
                    if !label.contains("Reading") && !label.contains("Lettura") {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                window(options, &directory.join(filename), None)?;
            }
            SendMessageW(main, WM_COMMAND, 212, 0);
            let confirmation = GetLastActivePopup(main);
            window(
                confirmation,
                &directory.join("pause-confirmation.bmp"),
                None,
            )?;
            SendMessageW(confirmation, WM_COMMAND, 1, 0);
            std::thread::sleep(Duration::from_millis(150));
            window(main, &directory.join("main-paused.bmp"), None)?;
            window(bar, &directory.join("topbar-paused.bmp"), Some(bar_width))?;
            Ok(())
        })();
        let message = match result {
            Ok(()) => "OK".into(),
            Err(e) => e,
        };
        let _ = std::fs::write(directory.join("capture-result.txt"), message);
        PostMessageW(main, WM_COMMAND, usize::from(QUIT), 0);
    });
}

unsafe fn window(hwnd: HWND, path: &Path, crop_width: Option<i32>) -> Result<(), String> {
    let mut rect: RECT = zeroed();
    if GetWindowRect(hwnd, &mut rect) == 0 {
        return Err("Capture window is unavailable".into());
    }
    let width = crop_width
        .unwrap_or(rect.right - rect.left)
        .min(rect.right - rect.left);
    let height = rect.bottom - rect.top;
    if width <= 0 || height <= 0 {
        return Err("Invalid capture dimensions".into());
    }
    let bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or("Capture is too large")?;
    let source = GetDC(null_mut());
    let dc = CreateCompatibleDC(source);
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..zeroed()
        },
        ..zeroed()
    };
    let mut pixels = null_mut();
    let bitmap = CreateDIBSection(source, &info, DIB_RGB_COLORS, &mut pixels, null_mut(), 0);
    if dc.is_null() || bitmap.is_null() || pixels.is_null() {
        if !bitmap.is_null() {
            DeleteObject(bitmap);
        }
        if !dc.is_null() {
            DeleteDC(dc);
        }
        ReleaseDC(null_mut(), source);
        return Err("Could not allocate screenshot surface".into());
    }
    let previous = SelectObject(dc, bitmap);
    RedrawWindow(
        hwnd,
        null(),
        null_mut(),
        RDW_INVALIDATE | RDW_UPDATENOW | RDW_ALLCHILDREN,
    );
    let result = if PrintWindow(hwnd, dc, 2) == 0 {
        Err("PrintWindow failed".into())
    } else {
        let mut bmp = vec![0u8; 54];
        bmp[0..2].copy_from_slice(b"BM");
        bmp[2..6].copy_from_slice(&((bytes + 54) as u32).to_le_bytes());
        bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
        bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
        bmp[18..22].copy_from_slice(&width.to_le_bytes());
        bmp[22..26].copy_from_slice(&(-height).to_le_bytes());
        bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
        bmp[28..30].copy_from_slice(&32u16.to_le_bytes());
        bmp.extend_from_slice(std::slice::from_raw_parts(pixels.cast::<u8>(), bytes));
        std::fs::write(path, bmp).map_err(|e| e.to_string())
    };
    SelectObject(dc, previous);
    DeleteObject(bitmap);
    DeleteDC(dc);
    ReleaseDC(null_mut(), source);
    result
}
