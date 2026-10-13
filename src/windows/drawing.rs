//! Original outline artwork and buffered native GDI rendering.

use super::*;

const fn color(r: u8, g: u8, b: u8) -> u32 {
    r as u32 | ((g as u32) << 8) | ((b as u32) << 16)
}

const PAPER: u32 = color(246, 248, 249);
const INK: u32 = color(45, 60, 65);
const TEAL: u32 = color(0, 113, 105);
const SELECTED: u32 = color(216, 241, 235);
const MUTED: u32 = color(124, 137, 143);
const WHITE: u32 = color(255, 255, 255);

/// A compatible surface avoids partially drawn icons and camera frames.
struct BufferedDc {
    dc: HDC,
    bitmap: HBITMAP,
    previous_bitmap: HGDIOBJ,
}

impl BufferedDc {
    unsafe fn new(target: HDC, width: i32, height: i32) -> Option<Self> {
        let dc = CreateCompatibleDC(target);
        if dc.is_null() {
            return None;
        }
        let bitmap = CreateCompatibleBitmap(target, width.max(1), height.max(1));
        if bitmap.is_null() {
            DeleteDC(dc);
            return None;
        }
        let previous_bitmap = SelectObject(dc, bitmap);
        Some(Self {
            dc,
            bitmap,
            previous_bitmap,
        })
    }
}

impl Drop for BufferedDc {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.previous_bitmap);
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
        }
    }
}

unsafe fn fill(dc: HDC, rectangle: RECT, color: u32) {
    let brush = CreateSolidBrush(color);
    if !brush.is_null() {
        FillRect(dc, &rectangle, brush);
        DeleteObject(brush);
    }
}

unsafe fn label(dc: HDC, text: &str, rectangle: RECT, color: u32, flags: u32) {
    let text = wide(text);
    let mut rectangle = rectangle;
    SetBkMode(dc, TRANSPARENT as i32);
    SetTextColor(dc, color);
    DrawTextW(
        dc,
        text.as_ptr(),
        (text.len() - 1) as i32,
        &mut rectangle,
        flags,
    );
}

impl App {
    pub(super) unsafe fn draw_item(&self, item: &DRAWITEMSTRUCT) {
        let width = item.rcItem.right - item.rcItem.left;
        let height = item.rcItem.bottom - item.rcItem.top;
        if width <= 0 || height <= 0 {
            return;
        }
        let Some(surface) = BufferedDc::new(item.hDC, width, height) else {
            return;
        };
        let dc = surface.dc;
        let saved = SaveDC(dc);
        let dpi = self.dpi_for(item.hwndItem);
        SelectObject(dc, self.font_for(item.hwndItem));
        let rectangle = RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        };
        if item.CtlID == u32::from(PREVIEW) {
            self.draw_preview(dc, rectangle, item.hwndItem);
        } else {
            let id = item.CtlID as u16;
            let action = BAR_ACTIONS
                .iter()
                .find(|(control, _, _)| *control == id)
                .map(|(_, _, action)| *action);
            let active = match id {
                ENABLE | 210 => self.enabled,
                201 => !self.enabled,
                BAR => self.config.profile().show_bar,
                207 => self.click.drag_enabled(),
                208 => self.click.scrolling() || self.click.selected() == ClickAction::Scroll,
                _ => action.is_some_and(|action| self.click.selected() == action),
            };
            let pressed = item.itemState & ODS_SELECTED != 0;
            let disabled = item.itemState & ODS_DISABLED != 0;
            fill(
                dc,
                rectangle,
                if pressed {
                    rgb(189, 225, 216)
                } else if active {
                    SELECTED
                } else {
                    PAPER
                },
            );
            if active {
                fill(
                    dc,
                    RECT {
                        left: 0,
                        top: height - scale(3, dpi),
                        right: width,
                        bottom: height,
                    },
                    TEAL,
                );
            }
            let icon_size = scale(if id >= 200 { 48 } else { 32 }, dpi)
                .min(width)
                .min(height);
            let offset = scale(i32::from(pressed), dpi);
            let color = if disabled {
                MUTED
            } else if active {
                TEAL
            } else {
                INK
            };
            let mut icon = Outline::new(
                dc,
                (width - icon_size) / 2 + offset,
                (height - icon_size) / 2 + offset,
                icon_size,
                color,
                dpi,
            );
            match id {
                ENABLE | 210 => icon.power(self.enabled),
                OPTIONS => icon.sliders(),
                KEYBOARD => icon.keyboard(),
                WIZARD => icon.wand(),
                BAR => icon.bar(),
                HELP => icon.help(),
                209 => icon.robot(),
                201 => {
                    icon.mouse(0);
                    icon.line(7, 40, 41, 7);
                }
                202 => {
                    icon.mouse(1);
                    icon.number("1");
                }
                203 => icon.mouse(2),
                204 => icon.mouse(3),
                205 => {
                    icon.mouse(1);
                    icon.number("2");
                }
                206 => {
                    icon.mouse(1);
                    icon.number("3");
                }
                207 => {
                    icon.mouse(match self.click.drag_button() {
                        MouseButton::Left => 1,
                        MouseButton::Middle => 2,
                        MouseButton::Right => 3,
                    });
                    icon.arrow(35, 33, 26, 33);
                    icon.arrow(35, 33, 44, 33);
                    icon.arrow(35, 33, 35, 24);
                    icon.arrow(35, 33, 35, 42);
                }
                208 => {
                    icon.mouse(4);
                    icon.arrow(38, 23, 38, 7);
                    icon.arrow(38, 27, 38, 43);
                }
                211 => {
                    icon.mouse(1);
                    icon.number("SU");
                }
                _ => {}
            }
            if action.is_some_and(|action| self.click.locked() == action) {
                icon.lock();
            }
            drop(icon);
            if item.itemState & ODS_FOCUS != 0 {
                let margin = scale(2, dpi);
                let focus = RECT {
                    left: margin,
                    top: margin,
                    right: width - margin,
                    bottom: height - margin,
                };
                DrawFocusRect(dc, &focus);
            }
        }
        RestoreDC(dc, saved);
        BitBlt(
            item.hDC,
            item.rcItem.left,
            item.rcItem.top,
            width,
            height,
            dc,
            0,
            0,
            SRCCOPY,
        );
    }

    unsafe fn draw_preview(&self, dc: HDC, rectangle: RECT, hwnd: HWND) {
        let dpi = self.dpi_for(hwnd);
        fill(dc, rectangle, rgb(23, 32, 36));
        let width = rectangle.right - rectangle.left;
        let height = rectangle.bottom - rectangle.top;
        let Some(frame) = self.frame.as_ref() else {
            let unavailable = self.camera.as_ref().is_some_and(VisionWorker::is_finished);
            label(
                dc,
                if unavailable {
                    "Camera unavailable\nChoose a camera in Configuration."
                } else {
                    "Camera preview\nWaiting for camera…"
                },
                RECT {
                    left: scale(12, dpi),
                    top: (height / 2 - scale(22, dpi)).max(scale(4, dpi)),
                    right: width - scale(12, dpi),
                    bottom: height - scale(4, dpi),
                },
                rgb(211, 222, 224),
                DT_CENTER | DT_WORDBREAK | DT_NOPREFIX,
            );
            return;
        };
        let Some(pixel_count) = frame.width.checked_mul(frame.height) else {
            return;
        };
        if frame.width == 0 || frame.height == 0 || frame.rgb.len() / 3 != pixel_count {
            return;
        }
        // GDI expects BGR, while the vision API intentionally exposes RGB.
        // A 32-bit DIB has no per-row padding ambiguity at arbitrary widths.
        let mut pixels = Vec::with_capacity(pixel_count * 4);
        for pixel in frame.rgb.as_chunks::<3>().0 {
            pixels.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 0]);
        }
        let bitmap = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: frame.width as i32,
                biHeight: -(frame.height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..zeroed()
            },
            ..zeroed()
        };
        SetStretchBltMode(dc, HALFTONE);
        SetBrushOrgEx(dc, 0, 0, null_mut());
        StretchDIBits(
            dc,
            0,
            0,
            width,
            height,
            0,
            0,
            frame.width as i32,
            frame.height as i32,
            pixels.as_ptr() as *const c_void,
            &bitmap,
            DIB_RGB_COLORS,
            SRCCOPY,
        );

        if let Some([x, y, w, h]) = frame.face_rect {
            let map_x =
                |value: i32| (value as f64 * f64::from(width) / frame.width as f64).round() as i32;
            let map_y = |value: i32| {
                (value as f64 * f64::from(height) / frame.height as f64).round() as i32
            };
            let area = RECT {
                left: map_x(x).clamp(0, width),
                top: map_y(y).clamp(0, height),
                right: map_x(x + w).clamp(0, width),
                bottom: map_y(y + h).clamp(0, height),
            };
            let tracked = frame.tracking_ready();
            outline_rectangle(
                dc,
                area,
                if tracked {
                    rgb(58, 235, 185)
                } else {
                    rgb(246, 187, 65)
                },
                scale(2, dpi),
            );
        }
        // Manual ROI uses the full preview client rectangle, matching the
        // coordinate mapping in finish_manual_roi (no letterbox offset).
        if let Some(start) = self.manual_start {
            let mut current = POINT { x: 0, y: 0 };
            GetCursorPos(&mut current);
            ScreenToClient(hwnd, &mut current);
            let region = RECT {
                left: start.0.min(current.x).clamp(0, width),
                top: start.1.min(current.y).clamp(0, height),
                right: start.0.max(current.x).clamp(0, width),
                bottom: start.1.max(current.y).clamp(0, height),
            };
            outline_rectangle(dc, region, WHITE, scale(1, dpi));
        }
        if self.last_frame == 0 || !frame.tracking_ready() {
            let banner = RECT {
                left: 0,
                top: (height - scale(24, dpi)).max(0),
                right: width,
                bottom: height,
            };
            fill(dc, banner, rgb(23, 32, 36));
            label(
                dc,
                if self.last_frame == 0 {
                    "Camera unavailable"
                } else {
                    "Looking for your face"
                },
                banner,
                rgb(242, 208, 128),
                DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
            );
        }
    }
}

unsafe fn outline_rectangle(dc: HDC, rectangle: RECT, color: u32, width: i32) {
    let pen = CreatePen(PS_SOLID, width, color);
    let previous_pen = SelectObject(dc, pen);
    let previous_brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
    Rectangle(
        dc,
        rectangle.left,
        rectangle.top,
        rectangle.right,
        rectangle.bottom,
    );
    SelectObject(dc, previous_brush);
    SelectObject(dc, previous_pen);
    DeleteObject(pen);
}

struct Outline {
    dc: HDC,
    x: i32,
    y: i32,
    size: i32,
    color: u32,
    pen: HPEN,
    previous_pen: HGDIOBJ,
    previous_brush: HGDIOBJ,
}

impl Outline {
    unsafe fn new(dc: HDC, x: i32, y: i32, size: i32, color: u32, dpi: u32) -> Self {
        let pen = CreatePen(
            PS_SOLID,
            scale(if size >= scale(28, dpi) { 2 } else { 1 }, dpi),
            color,
        );
        let previous_pen = SelectObject(dc, pen);
        let previous_brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
        Self {
            dc,
            x,
            y,
            size,
            color,
            pen,
            previous_pen,
            previous_brush,
        }
    }

    fn px(&self, value: i32) -> i32 {
        self.x + value * self.size / 48
    }
    fn py(&self, value: i32) -> i32 {
        self.y + value * self.size / 48
    }

    unsafe fn line(&self, x1: i32, y1: i32, x2: i32, y2: i32) {
        MoveToEx(self.dc, self.px(x1), self.py(y1), null_mut());
        LineTo(self.dc, self.px(x2), self.py(y2));
    }

    unsafe fn round(&self, x: i32, y: i32, w: i32, h: i32, radius: i32) {
        RoundRect(
            self.dc,
            self.px(x),
            self.py(y),
            self.px(x + w),
            self.py(y + h),
            radius * self.size / 48,
            radius * self.size / 48,
        );
    }

    unsafe fn circle(&self, x: i32, y: i32, diameter: i32, solid: bool) {
        let brush = if solid {
            CreateSolidBrush(self.color)
        } else {
            null_mut()
        };
        let previous = if solid {
            SelectObject(self.dc, brush)
        } else {
            null_mut()
        };
        Ellipse(
            self.dc,
            self.px(x),
            self.py(y),
            self.px(x + diameter),
            self.py(y + diameter),
        );
        if solid {
            SelectObject(self.dc, previous);
            DeleteObject(brush);
        }
    }

    unsafe fn arrow(&self, x1: i32, y1: i32, x2: i32, y2: i32) {
        self.line(x1, y1, x2, y2);
        if x1 == x2 {
            let tail = if y1 < y2 { y2 - 5 } else { y2 + 5 };
            self.line(x2 - 4, tail, x2, y2);
            self.line(x2 + 4, tail, x2, y2);
        } else {
            let tail = if x1 < x2 { x2 - 5 } else { x2 + 5 };
            self.line(tail, y2 - 4, x2, y2);
            self.line(tail, y2 + 4, x2, y2);
        }
    }

    unsafe fn mouse(&self, button: u8) {
        self.round(9, 4, 29, 40, 16);
        self.line(19, 6, 19, 23);
        self.line(28, 6, 28, 23);
        self.line(10, 23, 37, 23);
        let red = rgb(215, 35, 42);
        let region = match button {
            1 => Some((11, 8, 7, 13)),
            2 => Some((21, 6, 6, 17)),
            3 => Some((29, 8, 7, 13)),
            _ => None,
        };
        if let Some((x, y, w, h)) = region {
            let brush = CreateSolidBrush(red);
            let old = SelectObject(self.dc, brush);
            self.round(x, y, w, h, 3);
            SelectObject(self.dc, old);
            DeleteObject(brush);
        }
        // The central button and wheel are distinct parts of the drawing.
        let brush = CreateSolidBrush(if button == 4 { red } else { rgb(242, 242, 242) });
        let old = SelectObject(self.dc, brush);
        self.round(22, 10, 4, 9, 3);
        SelectObject(self.dc, old);
        DeleteObject(brush);
        self.line(22, 13, 26, 13);
        self.line(22, 16, 26, 16);
    }

    unsafe fn number(&self, number: &str) {
        self.circle(29, 28, 16, true);
        label(
            self.dc,
            number,
            RECT {
                left: self.px(29),
                top: self.py(28),
                right: self.px(45),
                bottom: self.py(44),
            },
            WHITE,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
    }

    unsafe fn lock(&self) {
        self.round(3, 34, 7, 7, 2);
        self.round(4, 29, 5, 7, 4);
    }

    unsafe fn power(&self, enabled: bool) {
        self.circle(5, 5, 38, false);
        if enabled {
            self.line(19, 15, 19, 33);
            self.line(29, 15, 29, 33);
        } else {
            self.line(19, 14, 34, 24);
            self.line(34, 24, 19, 34);
            self.line(19, 34, 19, 14);
        }
    }

    unsafe fn sliders(&self) {
        for (x, y) in [(12, 15), (24, 29), (36, 19)] {
            self.line(x, 7, x, y - 4);
            self.line(x, y + 4, x, 41);
            self.circle(x - 4, y - 4, 8, false);
        }
    }

    unsafe fn keyboard(&self) {
        self.round(4, 10, 40, 29, 5);
        for row in 0..2 {
            for column in 0..5 {
                self.line(10 + column * 6, 17 + row * 7, 12 + column * 6, 17 + row * 7);
            }
        }
        self.line(15, 32, 33, 32);
    }

    unsafe fn wand(&self) {
        self.line(9, 39, 31, 17);
        self.line(13, 41, 35, 19);
        self.line(9, 39, 13, 41);
        self.line(31, 17, 35, 19);
        self.line(29, 5, 29, 11);
        self.line(39, 10, 43, 6);
        self.line(39, 20, 45, 20);
        self.line(17, 12, 21, 16);
    }

    unsafe fn bar(&self) {
        self.round(3, 13, 42, 23, 4);
        for x in [10, 21, 32] {
            self.round(x, 19, 6, 10, 2);
        }
    }

    unsafe fn help(&self) {
        self.circle(5, 5, 38, false);
        self.line(17, 15, 21, 12);
        self.line(21, 12, 28, 12);
        self.line(28, 12, 32, 17);
        self.line(32, 17, 30, 22);
        self.line(30, 22, 24, 26);
        self.line(24, 26, 24, 29);
        self.circle(22, 34, 4, true);
    }

    unsafe fn robot(&mut self) {
        self.round(7, 13, 34, 27, 9);
        self.line(24, 13, 24, 7);
        self.circle(21, 3, 6, true);
        self.circle(14, 21, 6, true);
        self.circle(28, 21, 6, true);
        self.line(18, 33, 30, 33);
        self.line(3, 23, 7, 23);
        self.line(41, 23, 45, 23);
    }
}

impl Drop for Outline {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.previous_brush);
            SelectObject(self.dc, self.previous_pen);
            DeleteObject(self.pen);
        }
    }
}
