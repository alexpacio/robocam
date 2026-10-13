//! Original outline artwork and buffered native GDI rendering.

use super::*;

const fn color(r: u8, g: u8, b: u8) -> u32 {
    r as u32 | ((g as u32) << 8) | ((b as u32) << 16)
}

const PAPER: u32 = color(246, 248, 249);
const INK: u32 = color(45, 60, 65);
const TEAL: u32 = color(0, 113, 105);
const SELECTED: u32 = color(216, 241, 235);
const PRESSED: u32 = color(189, 225, 216);
const PAUSED: u32 = color(252, 233, 196);
const PAUSED_MARK: u32 = color(166, 95, 0);
const MUTED: u32 = color(124, 137, 143);
const RED: u32 = color(215, 35, 42);
const WHITE: u32 = color(255, 255, 255);
const BLACK: u32 = color(0, 0, 0);
const YELLOW: u32 = color(255, 230, 0);
const CONTRAST_RED: u32 = color(255, 90, 90);
const CONTRAST_MUTED: u32 = color(143, 143, 143);
const CONTRAST_PRESSED: u32 = color(42, 42, 42);

/// GDI draws aliased curves; buttons render at this factor and are averaged down.
const SUPERSAMPLE: i32 = 4;

/// Everything about a toolbar button's state that its artwork depends on.
struct ButtonLook {
    id: u16,
    power: bool,
    active: bool,
    pressed: bool,
    on_bar: bool,
    high_contrast: bool,
    compact: bool,
    locked: bool,
    face: u32,
    color: u32,
    accent: u32,
}

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

/// Fills a rounded rectangle without an outline.
unsafe fn fill_round(dc: HDC, rectangle: RECT, radius: i32, color: u32) {
    let brush = CreateSolidBrush(color);
    if brush.is_null() {
        return;
    }
    let previous_brush = SelectObject(dc, brush);
    let previous_pen = SelectObject(dc, GetStockObject(NULL_PEN));
    // A null pen excludes the right and bottom edges from the interior.
    RoundRect(
        dc,
        rectangle.left,
        rectangle.top,
        rectangle.right + 1,
        rectangle.bottom + 1,
        radius * 2,
        radius * 2,
    );
    SelectObject(dc, previous_pen);
    SelectObject(dc, previous_brush);
    DeleteObject(brush);
}

unsafe fn outline_round(dc: HDC, rectangle: RECT, radius: i32, color: u32, width: i32) {
    let pen = CreatePen(PS_INSIDEFRAME, width, color);
    let previous_pen = SelectObject(dc, pen);
    let previous_brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
    RoundRect(
        dc,
        rectangle.left,
        rectangle.top,
        rectangle.right,
        rectangle.bottom,
        radius * 2,
        radius * 2,
    );
    SelectObject(dc, previous_brush);
    SelectObject(dc, previous_pen);
    DeleteObject(pen);
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
            self.draw_button(item, dc, width, height, dpi);
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

    unsafe fn draw_button(
        &self,
        item: &DRAWITEMSTRUCT,
        dc: HDC,
        width: i32,
        height: i32,
        dpi: u32,
    ) {
        let id = item.CtlID as u16;
        let action = BAR_ACTIONS
            .iter()
            .find(|(control, _, _)| *control == id)
            .map(|(_, _, action)| *action);
        let power = matches!(id, ENABLE | 210 | 212);
        // Pause is the state that matters most, so it is the active one.
        let paused = !self.enabled || self.full_paused;
        let active = match id {
            ENABLE | 210 | 201 => paused,
            212 => self.full_paused,
            BAR => true,
            207 => self.click.drag_enabled(),
            208 => self.click.scrolling() || self.click.selected() == ClickAction::Scroll,
            _ => action.is_some_and(|action| self.click.selected() == action),
        };
        let pressed = item.itemState & ODS_SELECTED != 0;
        let disabled =
            item.itemState & ODS_DISABLED != 0 || (paused && matches!(id, 202..=208 | 211));
        let on_bar = GetParent(item.hwndItem) == self.bar;
        let high_contrast = on_bar && self.config.profile().high_contrast_bar;
        // State lives in the button face; the drawing keeps its ink colour.
        let face = if high_contrast {
            if pressed { CONTRAST_PRESSED } else { BLACK }
        } else if pressed {
            PRESSED
        } else if active && power {
            PAUSED
        } else if active {
            SELECTED
        } else {
            PAPER
        };
        let color = if high_contrast {
            if disabled {
                CONTRAST_MUTED
            } else if active || pressed {
                YELLOW
            } else {
                WHITE
            }
        } else if disabled {
            MUTED
        } else {
            INK
        };
        let look = ButtonLook {
            id,
            power,
            active,
            pressed,
            on_bar,
            high_contrast,
            compact: high_contrast && self.config.profile().thin_bar,
            locked: action.is_some_and(|action| self.click.locked() == action),
            face,
            color,
            accent: if disabled {
                color
            } else if high_contrast {
                CONTRAST_RED
            } else {
                RED
            },
        };
        // GDI does not antialias, so the artwork is drawn larger and averaged down.
        let (large_width, large_height) = (width * SUPERSAMPLE, height * SUPERSAMPLE);
        match BufferedDc::new(item.hDC, large_width, large_height) {
            Some(large) => {
                let large_dpi = dpi * SUPERSAMPLE as u32;
                self.paint_button(large.dc, &look, large_width, large_height, large_dpi);
                SetStretchBltMode(dc, HALFTONE);
                SetBrushOrgEx(dc, 0, 0, null_mut());
                StretchBlt(
                    dc,
                    0,
                    0,
                    width,
                    height,
                    large.dc,
                    0,
                    0,
                    large_width,
                    large_height,
                    SRCCOPY,
                );
            }
            None => self.paint_button(dc, &look, width, height, dpi),
        }
        // Text and keyboard focus stay pixel-exact at the real size.
        if look.compact {
            let rectangle = RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            };
            label(
                dc,
                &text(item.hwndItem),
                rectangle,
                color,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
            );
            if look.locked {
                outline_rectangle(
                    dc,
                    RECT {
                        left: scale(4, dpi),
                        top: scale(4, dpi),
                        right: width - scale(4, dpi),
                        bottom: height - scale(4, dpi),
                    },
                    color,
                    scale(1, dpi),
                );
            }
        }
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

    unsafe fn paint_button(&self, dc: HDC, look: &ButtonLook, width: i32, height: i32, dpi: u32) {
        let rectangle = RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        };
        fill(
            dc,
            rectangle,
            if look.high_contrast {
                BLACK
            } else {
                GetSysColor(COLOR_BTNFACE)
            },
        );
        let radius = scale(if look.on_bar { 10 } else { 8 }, dpi);
        fill_round(dc, rectangle, radius, look.face);
        if look.active {
            let thickness = scale(if look.on_bar { 3 } else { 2 }, dpi);
            let inset = width * 29 / 100;
            fill_round(
                dc,
                RECT {
                    left: inset,
                    top: height - 2 * thickness,
                    right: width - inset,
                    bottom: height - thickness,
                },
                thickness / 2,
                if look.high_contrast {
                    YELLOW
                } else if look.power {
                    PAUSED_MARK
                } else {
                    TEAL
                },
            );
        }
        if look.high_contrast && (look.active || look.pressed) {
            outline_round(dc, rectangle, radius, YELLOW, scale(2, dpi));
        }
        if look.compact {
            return;
        }
        let icon_size = scale(if look.id >= 200 { 40 } else { 28 }, dpi)
            .min(width)
            .min(height);
        let offset = scale(i32::from(look.pressed), dpi);
        // Smaller icons get relatively heavier strokes to stay legible.
        let units = if icon_size >= scale(36, dpi) {
            2.5
        } else {
            3.0
        };
        let stroke = ((units * icon_size as f32 / 48.0).round() as i32).max(1);
        let icon = Outline::new(
            dc,
            [
                (width - icon_size) / 2 + offset,
                (height - icon_size) / 2 + offset,
                icon_size,
            ],
            [look.color, look.accent, look.face],
            stroke,
        );
        match look.id {
            ENABLE | 210 => icon.power(self.enabled),
            OPTIONS => icon.sliders(),
            KEYBOARD => icon.keyboard(),
            WIZARD => icon.wand(),
            BAR => icon.bar(),
            HELP => icon.help(),
            209 => icon.robot(),
            212 => {
                icon.power(!self.full_paused);
                icon.badge("M");
            }
            201 => {
                icon.mouse(None, false);
                icon.line((7.0, 41.0), (41.0, 7.0));
            }
            // The single click is the base case and carries no badge.
            202 => icon.mouse(Some(MouseButton::Left), false),
            203 => icon.mouse(Some(MouseButton::Middle), false),
            204 => icon.mouse(Some(MouseButton::Right), false),
            205 => {
                icon.mouse(Some(MouseButton::Left), false);
                icon.badge("2");
            }
            206 => {
                icon.component();
                icon.badge("3");
            }
            207 => {
                icon.mouse(Some(self.click.drag_button()), false);
                icon.drag();
            }
            208 => {
                icon.mouse(None, true);
                icon.scroll();
            }
            211 => {
                icon.component();
                icon.badge("2");
            }
            _ => {}
        }
        if look.locked {
            // Bottom left keeps the lock off the highlighted button,
            // the bottom-right modifiers and the active marker.
            let size = scale(16, dpi);
            let margin = scale(3, dpi);
            let top = height - scale(24, dpi);
            let ring = scale(2, dpi);
            fill_round(
                dc,
                RECT {
                    left: margin - ring,
                    top: top - ring,
                    right: margin + size + ring,
                    bottom: top + size + ring,
                },
                scale(5, dpi) + ring,
                look.face,
            );
            let (chip, ink) = if look.high_contrast {
                (YELLOW, BLACK)
            } else {
                (TEAL, WHITE)
            };
            let stroke = ((1.8 * size as f32 / 16.0).round() as i32).max(1);
            Outline::new(dc, [margin, top, size], [ink, ink, chip], stroke).lock();
        }
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

type Point = (f32, f32);

/// Strokes artwork drawn on a 48-unit grid with round caps and joins.
struct Outline {
    dc: HDC,
    x: i32,
    y: i32,
    size: i32,
    color: u32,
    accent: u32,
    paper: u32,
    pen: HPEN,
    previous_pen: HGDIOBJ,
    previous_brush: HGDIOBJ,
}

impl Outline {
    /// `colors` are the ink, the red for the involved part and the face behind the icon.
    unsafe fn new(dc: HDC, [x, y, size]: [i32; 3], colors: [u32; 3], stroke: i32) -> Self {
        let [color, accent, paper] = colors;
        let brush = LOGBRUSH {
            lbStyle: BS_SOLID,
            lbColor: color,
            lbHatch: 0,
        };
        let pen = ExtCreatePen(
            (PS_GEOMETRIC | PS_SOLID | PS_ENDCAP_ROUND | PS_JOIN_ROUND) as u32,
            stroke.max(1) as u32,
            &brush,
            0,
            null(),
        );
        let previous_pen = SelectObject(dc, pen);
        let previous_brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
        Self {
            dc,
            x,
            y,
            size,
            color,
            accent,
            paper,
            pen,
            previous_pen,
            previous_brush,
        }
    }

    fn unit(&self, value: f32) -> i32 {
        (value * self.size as f32 / 48.0).round() as i32
    }

    fn point(&self, (x, y): Point) -> POINT {
        POINT {
            x: self.x + self.unit(x),
            y: self.y + self.unit(y),
        }
    }

    /// Samples a circular arc; angles grow clockwise on screen, in degrees.
    fn arc(center: Point, radius: f32, from: f32, to: f32) -> Vec<Point> {
        const STEPS: usize = 16;
        (0..=STEPS)
            .map(|step| {
                let angle = (from + (to - from) * step as f32 / STEPS as f32).to_radians();
                (
                    center.0 + radius * angle.cos(),
                    center.1 + radius * angle.sin(),
                )
            })
            .collect()
    }

    unsafe fn polyline(&self, points: &[Point]) {
        let points: Vec<POINT> = points.iter().map(|point| self.point(*point)).collect();
        Polyline(self.dc, points.as_ptr(), points.len() as i32);
    }

    unsafe fn polygon(&self, points: &[Point]) {
        let points: Vec<POINT> = points.iter().map(|point| self.point(*point)).collect();
        Polygon(self.dc, points.as_ptr(), points.len() as i32);
    }

    unsafe fn line(&self, from: Point, to: Point) {
        self.polyline(&[from, to]);
    }

    unsafe fn round(&self, (x, y): Point, (w, h): Point, radius: f32) {
        let start = self.point((x, y));
        let end = self.point((x + w, y + h));
        let diameter = self.unit(radius * 2.0);
        RoundRect(self.dc, start.x, start.y, end.x, end.y, diameter, diameter);
    }

    unsafe fn circle(&self, (x, y): Point, radius: f32) {
        let start = self.point((x - radius, y - radius));
        let end = self.point((x + radius, y + radius));
        Ellipse(self.dc, start.x, start.y, end.x, end.y);
    }

    /// Runs `draw` with a solid brush and, unless `outlined`, without the pen.
    unsafe fn filled(&self, color: u32, outlined: bool, draw: impl FnOnce()) {
        let brush = CreateSolidBrush(color);
        if brush.is_null() {
            return;
        }
        let previous_brush = SelectObject(self.dc, brush);
        let previous_pen = if outlined {
            null_mut()
        } else {
            SelectObject(self.dc, GetStockObject(NULL_PEN))
        };
        draw();
        if !outlined {
            SelectObject(self.dc, previous_pen);
        }
        SelectObject(self.dc, previous_brush);
        DeleteObject(brush);
    }

    unsafe fn dot(&self, center: Point, radius: f32) {
        self.filled(self.color, false, || self.circle(center, radius));
    }

    /// Clears the face behind a modifier so it reads in front of the drawing.
    unsafe fn knockout(&self, center: Point, radius: f32) {
        self.filled(self.paper, false, || self.circle(center, radius));
    }

    /// Three buttons and a separate wheel; the involved part is red.
    unsafe fn mouse(&self, button: Option<MouseButton>, wheel: bool) {
        // Button edges sit where x = 15 and x = 25 meet the 24 × 38 body's top arc.
        const TOP: Point = (20.0, 15.0);
        let (from, to, near, far) = match button {
            Some(MouseButton::Left) => (180.0, 245.4, 8.0, 15.0),
            Some(MouseButton::Middle) => (245.4, 294.6, 15.0, 25.0),
            Some(MouseButton::Right) => (294.6, 360.0, 25.0, 32.0),
            None => (0.0, 0.0, 0.0, 0.0),
        };
        if button.is_some() {
            let mut region = vec![(near, 20.0)];
            region.extend(Self::arc(TOP, 12.0, from, to));
            region.push((far, 20.0));
            self.filled(self.accent, false, || self.polygon(&region));
        }
        self.round((8.0, 3.0), (24.0, 38.0), 12.0);
        self.line((8.0, 20.0), (32.0, 20.0));
        self.line((15.0, 4.1), (15.0, 20.0));
        self.line((25.0, 4.1), (25.0, 20.0));
        let wheel = if wheel { self.accent } else { self.paper };
        self.filled(wheel, true, || self.round((17.5, 8.0), (5.0, 8.0), 2.5));
    }

    /// Repeat counts sit bottom right, like every other modifier.
    unsafe fn badge(&self, text: &str) {
        self.knockout((36.0, 36.0), 11.0);
        self.dot((36.0, 36.0), 9.5);
        let font = CreateFontW(
            -self.unit(13.0).max(6),
            0,
            0,
            0,
            700,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            DEFAULT_PITCH as u32,
            wide("Segoe UI").as_ptr(),
        );
        let previous = if font.is_null() {
            null_mut()
        } else {
            SelectObject(self.dc, font)
        };
        let start = self.point((26.5, 26.5));
        let end = self.point((45.5, 45.5));
        label(
            self.dc,
            text,
            RECT {
                left: start.x,
                top: start.y,
                right: end.x,
                bottom: end.y,
            },
            self.paper,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
        if !font.is_null() {
            SelectObject(self.dc, previous);
            DeleteObject(font);
        }
    }

    unsafe fn drag(&self) {
        self.knockout((36.0, 36.0), 11.5);
        self.line((28.0, 36.0), (44.0, 36.0));
        self.line((36.0, 28.0), (36.0, 44.0));
        self.polyline(&[(41.0, 33.0), (44.0, 36.0), (41.0, 39.0)]);
        self.polyline(&[(31.0, 33.0), (28.0, 36.0), (31.0, 39.0)]);
        self.polyline(&[(33.0, 31.0), (36.0, 28.0), (39.0, 31.0)]);
        self.polyline(&[(33.0, 41.0), (36.0, 44.0), (39.0, 41.0)]);
    }

    unsafe fn scroll(&self) {
        self.line((41.0, 9.0), (41.0, 37.0));
        self.polyline(&[(37.5, 12.5), (41.0, 9.0), (44.5, 12.5)]);
        self.polyline(&[(37.5, 33.5), (41.0, 37.0), (44.5, 33.5)]);
    }

    /// A SketchUp group or component, entered through its red top face.
    unsafe fn component(&self) {
        self.filled(self.accent, true, || {
            self.polygon(&[(10.0, 12.5), (24.0, 20.0), (38.0, 12.5), (24.0, 5.0)])
        });
        self.polyline(&[
            (10.0, 12.5),
            (10.0, 29.5),
            (24.0, 37.0),
            (38.0, 29.5),
            (38.0, 12.5),
        ]);
        self.line((24.0, 20.0), (24.0, 37.0));
    }

    /// Fills the whole box with the face colour and draws a padlock in ink.
    unsafe fn lock(&self) {
        self.filled(self.paper, false, || {
            self.round((0.0, 0.0), (48.0, 48.0), 15.0)
        });
        self.filled(self.color, false, || {
            self.round((10.5, 22.5), (27.0, 19.5), 4.5)
        });
        let mut shackle = vec![(16.5, 22.5)];
        shackle.extend(Self::arc((24.0, 16.5), 7.5, 180.0, 360.0));
        shackle.push((31.5, 22.5));
        self.polyline(&shackle);
    }

    unsafe fn power(&self, enabled: bool) {
        self.circle((24.0, 24.0), 19.0);
        if enabled {
            self.line((20.0, 17.0), (20.0, 31.0));
            self.line((28.0, 17.0), (28.0, 31.0));
        } else {
            self.polyline(&[(20.5, 16.5), (32.0, 24.0), (20.5, 31.5), (20.5, 16.5)]);
        }
    }

    unsafe fn sliders(&self) {
        for (x, knob) in [(12.0, 16.0), (24.0, 30.0), (36.0, 21.0)] {
            self.line((x, 7.0), (x, knob - 4.5));
            self.line((x, knob + 4.5), (x, 41.0));
            self.circle((x, knob), 4.5);
        }
    }

    unsafe fn keyboard(&self) {
        self.round((4.5, 11.0), (39.0, 27.0), 5.0);
        for row in [18.5, 24.5] {
            for column in 0..5 {
                self.dot((12.0 + column as f32 * 6.0, row), 1.8);
            }
        }
        self.line((16.0, 31.5), (32.0, 31.5));
    }

    unsafe fn wand(&self) {
        self.line((9.0, 39.0), (27.0, 21.0));
        self.line((34.0, 6.0), (34.0, 20.0));
        self.line((27.0, 13.0), (41.0, 13.0));
        self.line((17.0, 7.0), (17.0, 12.0));
        self.line((14.5, 9.5), (19.5, 9.5));
        self.line((40.0, 27.0), (40.0, 32.0));
        self.line((37.5, 29.5), (42.5, 29.5));
    }

    unsafe fn bar(&self) {
        self.round((4.0, 14.0), (40.0, 20.0), 6.0);
        self.filled(self.color, true, || {
            self.round((10.0, 19.5), (7.0, 9.0), 2.0)
        });
        self.round((20.5, 19.5), (7.0, 9.0), 2.0);
        self.round((31.0, 19.5), (7.0, 9.0), 2.0);
    }

    unsafe fn help(&self) {
        self.circle((24.0, 24.0), 19.0);
        let mut hook = Self::arc((24.0, 19.0), 5.5, 180.0, 421.8);
        hook.extend([(25.3, 24.6), (24.4, 25.6), (24.05, 26.6), (24.0, 28.5)]);
        self.polyline(&hook);
        self.dot((24.0, 34.5), 1.9);
    }

    unsafe fn robot(&self) {
        self.round((9.0, 14.0), (30.0, 24.0), 8.0);
        self.line((24.0, 14.0), (24.0, 9.5));
        self.line((20.0, 31.5), (28.0, 31.5));
        self.line((5.0, 22.0), (5.0, 30.0));
        self.line((43.0, 22.0), (43.0, 30.0));
        self.dot((24.0, 7.0), 2.5);
        self.dot((18.5, 25.0), 2.6);
        self.dot((29.5, 25.0), 2.6);
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
