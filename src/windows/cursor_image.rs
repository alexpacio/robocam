//! Premultiplied overlay bitmap, retaining the user's standard arrow shape and hotspot.
use super::*;

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

    /// Supply the complete premultiplied-alpha surface before showing it.
    pub unsafe fn present(&self, hwnd: HWND, hotspot: POINT) -> bool {
        let mut position: POINT = zeroed();
        if GetCursorPos(&mut position) == 0 {
            return false;
        }
        position.x -= hotspot.x;
        position.y -= hotspot.y;
        let size = SIZE {
            cx: self.width,
            cy: self.height,
        };
        let origin = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        UpdateLayeredWindow(
            hwnd,
            null_mut(),
            &position,
            &size,
            self.dc,
            &origin,
            0,
            &blend,
            ULW_ALPHA,
        ) != 0
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
            let [r, g, b] = drawing::CONTRAST_YELLOW.map(|c| {
                if edge {
                    u32::from(c) * u32::from(*a) / 255
                } else {
                    0
                }
            });
            (u32::from(*a) << 24) | (r << 16) | (g << 8) | b
        })
        .collect()
}

pub(super) unsafe fn native_image(cursor: HCURSOR, background: u32) -> Option<(Surface, POINT)> {
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
    let width = bitmap.bmWidth;
    let height = original_height;
    let mut image = Surface::new(width, height)?;
    image.data_mut().fill(background);
    if DrawIconEx(
        image.dc,
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
    Some((
        image,
        POINT {
            x: info.xHotspot as i32,
            y: info.yHotspot as i32,
        },
    ))
}

pub(super) unsafe fn arrow() -> Option<(Surface, POINT)> {
    let cursor = LoadCursorW(null_mut(), IDC_ARROW);
    let (mut black, hotspot) = native_image(cursor, 0xff000000)?;
    let (white, _) = native_image(cursor, 0xffffffff)?;
    let (width, height) = (black.width, black.height);
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
        (width / 32).max(1) as usize,
    );
    black.data_mut().copy_from_slice(&pixels);
    Some((black, hotspot))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recoloring_preserves_transparency_and_uses_the_toolbar_yellow() {
        let mut alpha = vec![0u8; 49];
        for y in 1..6 {
            for x in 1..6 {
                alpha[y * 7 + x] = 255;
            }
        }
        let pixels = recolor(&alpha, 7, 7, 1);
        assert_eq!(pixels[0], 0);
        assert_eq!(pixels[3 * 7 + 3], 0xff000000);
        assert_eq!(pixels[7 + 1], 0xffffe600);
        for (a, pixel) in alpha.iter().zip(pixels) {
            assert_eq!(u32::from(*a), pixel >> 24);
        }
    }
}
