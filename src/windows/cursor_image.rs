//! A premultiplied-alpha image of the user's standard Windows arrow.
//! Upload the complete surface explicitly; WS_EX_TRANSPARENT/WM_PAINT can defer
//! painting until after a short click has already ended.
use super::*;

pub(super) const ACTIVE: isize = 1 << 31;
pub(super) const NATIVE_VISIBLE: isize = 1 << 30;

pub(super) unsafe fn set_active(hwnd: HWND, active: bool) {
    let value = GetWindowLongPtrW(hwnd, GWLP_USERDATA) & !ACTIVE;
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, value | if active { ACTIVE } else { 0 });
}

pub(super) struct Surface {
    pub dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    pub pixels: *mut u32,
    pub width: i32,
    pub height: i32,
}

impl Surface {
    pub unsafe fn new(width: i32, height: i32) -> Option<Self> {
        if !(1..=2048).contains(&width) || !(1..=2048).contains(&height) {
            return None;
        }
        let dc = CreateCompatibleDC(null_mut());
        if dc.is_null() {
            return None;
        }
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
        let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut pixels, null_mut(), 0);
        if bitmap.is_null() || pixels.is_null() {
            if !bitmap.is_null() {
                DeleteObject(bitmap);
            }
            DeleteDC(dc);
            return None;
        }
        let previous = SelectObject(dc, bitmap);
        Some(Self {
            dc,
            bitmap,
            previous,
            pixels: pixels.cast(),
            width,
            height,
        })
    }
    pub unsafe fn data(&self) -> &[u32] {
        std::slice::from_raw_parts(self.pixels, (self.width * self.height) as usize)
    }
    unsafe fn data_mut(&mut self) -> &mut [u32] {
        std::slice::from_raw_parts_mut(self.pixels, (self.width * self.height) as usize)
    }
    pub unsafe fn save(&self, path: &std::path::Path) -> Result<(), String> {
        GdiFlush();
        let mut bmp = vec![0u8; 54];
        bmp[0..2].copy_from_slice(b"BM");
        bmp[2..6].copy_from_slice(&(54 + self.width as u32 * self.height as u32 * 4).to_le_bytes());
        bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
        bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
        bmp[18..22].copy_from_slice(&self.width.to_le_bytes());
        bmp[22..26].copy_from_slice(&(-self.height).to_le_bytes());
        bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
        bmp[28..30].copy_from_slice(&32u16.to_le_bytes());
        for pixel in self.data() {
            bmp.extend_from_slice(&pixel.to_le_bytes());
        }
        std::fs::write(path, bmp).map_err(|e| e.to_string())
    }
}
impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.previous);
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
        }
    }
}

fn recolor(alpha: &[u8], width: usize, height: usize, border: usize) -> Vec<u32> {
    alpha
        .iter()
        .enumerate()
        .map(|(i, a)| {
            if *a == 0 {
                return 0;
            }
            let x = i % width;
            let y = i / width;
            let edge = x < border
                || y < border
                || x + border >= width
                || y + border >= height
                || (y.saturating_sub(border)..=(y + border).min(height - 1)).any(|ny| {
                    (x.saturating_sub(border)..=(x + border).min(width - 1))
                        .any(|nx| alpha[ny * width + nx] < 128)
                });
            let shade = if edge { 150 * u32::from(*a) / 255 } else { 0 };
            (u32::from(*a) << 24) | (shade << 16) | (shade << 8) | shade
        })
        .collect()
}

pub(super) unsafe fn arrow(dpi: u32) -> Option<(Surface, POINT)> {
    let cursor = LoadCursorW(null_mut(), IDC_ARROW);
    let mut info: ICONINFO = zeroed();
    if cursor.is_null() || GetIconInfo(cursor, &mut info) == 0 {
        return None;
    }
    let mut bitmap: BITMAP = zeroed();
    let has_color = !info.hbmColor.is_null();
    let found = GetObjectW(
        if has_color {
            info.hbmColor
        } else {
            info.hbmMask
        },
        size_of::<BITMAP>() as i32,
        (&mut bitmap as *mut BITMAP).cast(),
    ) != 0;
    if !info.hbmColor.is_null() {
        DeleteObject(info.hbmColor);
    }
    if !info.hbmMask.is_null() {
        DeleteObject(info.hbmMask);
    }
    if !found || bitmap.bmWidth <= 0 || bitmap.bmHeight <= 0 {
        return None;
    }
    let original_height = if has_color {
        bitmap.bmHeight
    } else {
        bitmap.bmHeight / 2
    };
    if original_height == 0 {
        return None;
    }
    let width = bitmap.bmWidth.max(GetSystemMetricsForDpi(SM_CXCURSOR, dpi));
    let height = original_height.max(GetSystemMetricsForDpi(SM_CYCURSOR, dpi));
    let mut black = Surface::new(width, height)?;
    let mut white = Surface::new(width, height)?;
    black.data_mut().fill(0xff000000);
    white.data_mut().fill(0xffffffff);
    if DrawIconEx(
        black.dc,
        0,
        0,
        cursor,
        width,
        height,
        0,
        null_mut(),
        DI_NORMAL,
    ) == 0
        || DrawIconEx(
            white.dc,
            0,
            0,
            cursor,
            width,
            height,
            0,
            null_mut(),
            DI_NORMAL,
        ) == 0
    {
        return None;
    }
    GdiFlush();
    // Two backgrounds recover coverage even for legacy monochrome cursors,
    // whose DrawIconEx output does not have a usable alpha channel.
    let alpha: Vec<u8> = black
        .data()
        .iter()
        .zip(white.data())
        .map(|(black, white)| {
            let difference = [0, 8, 16]
                .into_iter()
                .map(|shift| ((white >> shift) & 255).saturating_sub((black >> shift) & 255))
                .max()
                .unwrap_or(255);
            (255 - difference) as u8
        })
        .collect();
    if !alpha.iter().any(|a| *a > 0) {
        return None;
    }
    let pixels = recolor(
        &alpha,
        width as usize,
        height as usize,
        scale(1, dpi).max(1) as usize,
    );
    black.data_mut().copy_from_slice(&pixels);
    Some((
        black,
        POINT {
            x: (i64::from(info.xHotspot) * i64::from(width) / i64::from(bitmap.bmWidth)) as i32,
            y: (i64::from(info.yHotspot) * i64::from(height) / i64::from(original_height)) as i32,
        },
    ))
}

pub(super) unsafe fn show(hwnd: HWND) -> bool {
    let Some((image, hotspot)) = arrow(GetDpiForWindow(hwnd).max(96)) else {
        return false;
    };
    let mut position: POINT = zeroed();
    if GetCursorPos(&mut position) == 0 {
        return false;
    }
    position.x -= hotspot.x;
    position.y -= hotspot.y;
    let size = SIZE {
        cx: image.width,
        cy: image.height,
    };
    let origin = POINT { x: 0, y: 0 };
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    if UpdateLayeredWindow(
        hwnd,
        null_mut(),
        &position,
        &size,
        image.dc,
        &origin,
        0,
        &blend,
        ULW_ALPHA,
    ) == 0
    {
        return false;
    }
    SetWindowLongPtrW(
        hwnd,
        GWLP_USERDATA,
        (GetWindowLongPtrW(hwnd, GWLP_USERDATA) & (ACTIVE | NATIVE_VISIBLE))
            | hotspot.x as isize
            | ((hotspot.y as isize) << 16),
    );
    SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        0,
        0,
        0,
        0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
    );
    // The replacement must have reached the compositor before hiding the real cursor.
    DwmFlush() >= 0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recoloring_preserves_transparency_and_keeps_a_black_body_and_gray_outline() {
        let mut alpha = vec![0u8; 49];
        for y in 1..6 {
            for x in 1..6 {
                alpha[y * 7 + x] = 255;
            }
        }
        let pixels = recolor(&alpha, 7, 7, 1);
        assert_eq!(pixels[0], 0);
        assert_eq!(pixels[3 * 7 + 3], 0xff000000);
        assert_eq!(pixels[7 + 1], 0xff969696);
        for (a, pixel) in alpha.iter().zip(pixels) {
            assert_eq!(u32::from(*a), pixel >> 24);
        }
    }
}
