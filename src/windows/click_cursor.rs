//! Click feedback swaps the images of the standard Windows cursor roles while an
//! injected button is held. Windows keeps drawing its own pointer, so feedback
//! can never hide it; release reloads the user's saved cursor scheme.
//!
//! The highlight is built from the pointer file Windows actually draws, at the
//! size it draws it, and applied as a pointer file. A hardware (GPU) pointer only
//! shows what Windows hands to the display: on a host with the accessibility
//! pointer style, a 32 px image built from `LoadCursorW` never appeared there.
use super::*;
use std::{
    io::Read,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::AtomicBool,
        mpsc::{self, RecvTimeoutError},
    },
    thread::JoinHandle,
    time::Duration,
};
use windows_sys::Win32::System::Registry::*;

/// Every standard role, so links, text and resize targets are highlighted too.
/// Applications' own cursors (for example SketchUp tools) are not system roles.
pub(super) const ROLES: [SYSTEM_CURSOR_ID; 14] = [
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
    OCR_HELP,
];
const OVER_BLACK: u32 = 0xff00_0000;
const OVER_WHITE: u32 = 0xffff_ffff;
/// Feedback stays at least this long after each press, so clicks with a 0 ms
/// press duration (a common setting) are visible too.
pub(super) const MINIMUM_VISIBLE: Duration = Duration::from_millis(500);

/// Reload the saved scheme, including the pointer size and colour chosen in
/// Windows settings. SetSystemCursor changes are never written to the registry.
unsafe fn reload_scheme() -> bool {
    SystemParametersInfoW(SPI_SETCURSORS, 0, null_mut(), 0) != 0
}

/// `--cursor-guardian`: reload the scheme if Robocam exits while the highlight
/// is applied. This process never replaces cursors or injects input.
pub(super) unsafe fn guardian() {
    let mut armed = false;
    let mut byte = [0u8];
    while std::io::stdin().read_exact(&mut byte).is_ok() {
        armed = byte[0] == 1;
    }
    if armed {
        reload_scheme();
    }
}

/// A top-down 32bpp DIB, freed on drop.
struct Dib {
    bitmap: HBITMAP,
    pixels: *mut u32,
    len: usize,
}
impl Dib {
    unsafe fn new(width: i32, height: i32) -> Option<Self> {
        if !(1..=1024).contains(&width) || !(1..=1024).contains(&height) {
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
        let bitmap = CreateDIBSection(
            null_mut(),
            &info,
            DIB_RGB_COLORS,
            &mut pixels,
            null_mut(),
            0,
        );
        if bitmap.is_null() || pixels.is_null() {
            if !bitmap.is_null() {
                DeleteObject(bitmap);
            }
            return None;
        }
        Some(Self {
            bitmap,
            pixels: pixels.cast(),
            len: (width * height) as usize,
        })
    }
    unsafe fn data(&mut self) -> &mut [u32] {
        std::slice::from_raw_parts_mut(self.pixels, self.len)
    }
}
impl Drop for Dib {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.bitmap);
        }
    }
}

/// Size and hotspot of a cursor as Windows currently draws it.
unsafe fn metrics(cursor: HCURSOR) -> Option<(i32, i32, (i32, i32))> {
    let mut info: ICONINFO = zeroed();
    if cursor.is_null() || GetIconInfo(cursor, &mut info) == 0 {
        return None;
    }
    let color = !info.hbmColor.is_null();
    let mut bitmap: BITMAP = zeroed();
    let found = GetObjectW(
        if color { info.hbmColor } else { info.hbmMask },
        size_of::<BITMAP>() as i32,
        (&mut bitmap as *mut BITMAP).cast(),
    ) != 0;
    for handle in [info.hbmColor, info.hbmMask] {
        if !handle.is_null() {
            DeleteObject(handle);
        }
    }
    // Monochrome cursors stack their AND and XOR masks vertically.
    let height = if color {
        bitmap.bmHeight
    } else {
        bitmap.bmHeight / 2
    };
    let hotspot = (info.xHotspot as i32, info.yHotspot as i32);
    (found && bitmap.bmWidth > 0 && height > 0).then_some((bitmap.bmWidth, height, hotspot))
}

unsafe fn render(cursor: HCURSOR, width: i32, height: i32, background: u32) -> Option<Vec<u32>> {
    let mut dib = Dib::new(width, height)?;
    dib.data().fill(background);
    let dc = CreateCompatibleDC(null_mut());
    if dc.is_null() {
        return None;
    }
    let previous = SelectObject(dc, dib.bitmap);
    let drawn = DrawIconEx(dc, 0, 0, cursor, width, height, 0, null_mut(), DI_NORMAL) != 0;
    GdiFlush();
    SelectObject(dc, previous);
    DeleteDC(dc);
    drawn.then(|| dib.data().to_vec())
}

/// Two backgrounds recover coverage even for monochrome and inverting cursors,
/// whose DrawIconEx output has no usable alpha channel.
fn coverage(over_black: &[u32], over_white: &[u32]) -> Vec<u8> {
    over_black
        .iter()
        .zip(over_white)
        .map(|(black, white)| {
            let difference = [0, 8, 16]
                .into_iter()
                .map(|shift| ((white >> shift) & 255).saturating_sub((black >> shift) & 255))
                .max()
                .unwrap_or(255);
            (255 - difference) as u8
        })
        .collect()
}

/// Map the arrow's outline to toolbar yellow and its fill to black, keeping the
/// outline width and edge antialiasing of the user's scheme. The outline is the
/// part that meets transparency, light or dark. Arrows drawn in one shade still
/// get a yellow edge. Cursor colour bitmaps use straight (not premultiplied) alpha.
fn recolor(over_black: &[u32], alpha: &[u8], width: usize) -> Vec<u32> {
    let height = alpha.len() / width;
    let light: Vec<bool> = (over_black.iter().zip(alpha))
        .map(|(pixel, a)| {
            // Drawn over black, every channel is premultiplied by coverage.
            let channel =
                |shift: u32| (((pixel >> shift) & 255) * 255 / u32::from((*a).max(1))).min(255);
            (2 * channel(16) + 5 * channel(8) + channel(0)) / 8 >= 128
        })
        .collect();
    let transparent = |x: usize, y: usize, dx: isize, dy: isize| {
        let (Some(x), Some(y)) = (x.checked_add_signed(dx), y.checked_add_signed(dy)) else {
            return true;
        };
        x >= width || y >= height || alpha[y * width + x] < 128
    };
    let edge: Vec<bool> = (0..alpha.len())
        .map(|i| {
            [(-1, 0), (1, 0), (0, -1), (0, 1)]
                .into_iter()
                .any(|(dx, dy)| transparent(i % width, i / width, dx, dy))
        })
        .collect();
    let solid = |i: &usize| alpha[*i] >= 128;
    let edges = (0..alpha.len()).filter(|i| edge[*i] && solid(i));
    let (count, lit) = edges.fold((0, 0), |(n, l), i| (n + 1, l + usize::from(light[i])));
    let outline_light = lit * 2 > count;
    let contrast = (0..alpha.len()).any(|i| !edge[i] && solid(&i) && light[i] != outline_light);
    (0..alpha.len())
        .map(|i| {
            if alpha[i] == 0 {
                return 0;
            }
            let yellow = edge[i] || (contrast && light[i] == outline_light);
            let [r, g, b] = if yellow {
                drawing::CONTRAST_YELLOW
            } else {
                [0; 3]
            };
            (u32::from(alpha[i]) << 24) | (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
        })
        .collect()
}

unsafe fn user_dword(key: &str, value: &str) -> Option<u32> {
    let mut data = 0u32;
    let mut size = size_of::<u32>() as u32;
    (RegGetValueW(
        HKEY_CURRENT_USER,
        wide_raw(key).as_ptr(),
        wide_raw(value).as_ptr(),
        RRF_RT_REG_DWORD,
        null_mut(),
        (&mut data as *mut u32).cast(),
        &mut size,
    ) == ERROR_SUCCESS)
        .then_some(data)
}

/// A string value with environment variables expanded, if present and not empty.
unsafe fn user_string(key: &str, value: &str) -> Option<String> {
    let mut buffer = vec![0u16; 2048];
    let mut size = (buffer.len() * 2) as u32;
    if RegGetValueW(
        HKEY_CURRENT_USER,
        wide_raw(key).as_ptr(),
        wide_raw(value).as_ptr(),
        // REG_EXPAND_SZ values are expanded and then match REG_SZ.
        RRF_RT_REG_SZ,
        null_mut(),
        buffer.as_mut_ptr().cast(),
        &mut size,
    ) != ERROR_SUCCESS
    {
        return None;
    }
    let text = String::from_utf16_lossy(&buffer[..size as usize / 2]);
    let text = text.trim_end_matches('\0');
    (!text.is_empty()).then(|| text.to_owned())
}

fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

/// Where Windows takes the arrow it draws from.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Source {
    File(PathBuf),
    /// The scheme names no file: the system's built-in arrow.
    System,
}

/// With the accessibility "custom colour" pointer style (CursorType 6) Windows
/// draws per-user copies in `%LOCALAPPDATA%\Microsoft\Windows\Cursors`, not the
/// files named by the scheme. Otherwise the scheme's Arrow file is drawn.
pub(super) unsafe fn arrow_source() -> Source {
    if user_dword("Software\\Microsoft\\Accessibility", "CursorType") == Some(6)
        && let Some(local) = std::env::var_os("LOCALAPPDATA")
    {
        let path = PathBuf::from(local).join("Microsoft\\Windows\\Cursors\\arrow_eoa.cur");
        if path.is_file() {
            return Source::File(path);
        }
    }
    match user_string("Control Panel\\Cursors", "Arrow") {
        Some(path) if Path::new(&path).is_file() => Source::File(path.into()),
        _ => Source::System,
    }
}

/// Windows draws pointers at CursorBaseSize scaled to the monitor's DPI, while
/// SM_CXCURSOR keeps reporting the 32 px base.
pub(super) fn pointer_size(base: u32, dpi: u32) -> i32 {
    scale(base.clamp(32, 256) as i32, dpi).clamp(32, 256)
}

/// The size of the pointer on the monitor it is currently on.
pub(super) unsafe fn current_pointer_size() -> i32 {
    let base = user_dword("Control Panel\\Cursors", "CursorBaseSize").unwrap_or(32);
    let mut point: POINT = zeroed();
    GetCursorPos(&mut point);
    let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
    let (mut x, mut y) = (0, 0);
    if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y) != 0 {
        x = GetDpiForSystem();
    }
    pointer_size(base, x)
}

unsafe fn load_source(source: &Source, size: i32) -> Option<HCURSOR> {
    let cursor = match source {
        Source::File(path) => LoadImageW(
            null_mut(),
            wide_path(path).as_ptr(),
            IMAGE_CURSOR,
            size,
            size,
            LR_LOADFROMFILE,
        ),
        // Reload the shared arrow from its resource at this size, if possible.
        Source::System => CopyImage(
            LoadCursorW(null_mut(), IDC_ARROW),
            IMAGE_CURSOR,
            size,
            size,
            LR_COPYFROMRESOURCE,
        ),
    };
    (!cursor.is_null()).then_some(cursor)
}

/// Square straight-alpha ARGB pixels, top-down, with a hotspot.
pub(super) struct Image {
    pub size: i32,
    pub hotspot: (i32, i32),
    pub pixels: Vec<u32>,
}

/// The user's arrow at `size`, same shape and hotspot, in black and yellow.
unsafe fn highlight_image(source: &Source, size: i32) -> Option<Image> {
    let cursor = load_source(source, size)?;
    let image = (|| {
        let (width, height, hotspot) = metrics(cursor)?;
        let over_black = render(cursor, size, size, OVER_BLACK)?;
        let alpha = coverage(&over_black, &render(cursor, size, size, OVER_WHITE)?);
        alpha.iter().any(|a| *a > 0).then(|| Image {
            size,
            // A loader that ignored the requested size is stretched by DrawIconEx.
            hotspot: (hotspot.0 * size / width, hotspot.1 * size / height),
            pixels: recolor(&over_black, &alpha, size as usize),
        })
    })();
    DestroyCursor(cursor);
    image
}

/// A single-image 32bpp .cur file, loaded by Windows like a scheme's pointer.
pub(super) fn cur_file(image: &Image) -> Vec<u8> {
    let size = image.size as usize;
    // AND mask rows are padded to 32 bits; set bits are transparent.
    let stride = size.div_ceil(32) * 4;
    let image_len = size * size * 4 + stride * size;
    let mut file = Vec::with_capacity(22 + 40 + image_len);
    for word in [0u16, 2, 1] {
        file.extend(word.to_le_bytes());
    }
    // A 256 px entry is stored as 0; the bitmap header carries the real size.
    let dimension = if size >= 256 { 0 } else { size as u8 };
    file.extend([dimension, dimension, 0, 0]);
    file.extend((image.hotspot.0 as u16).to_le_bytes());
    file.extend((image.hotspot.1 as u16).to_le_bytes());
    file.extend(((40 + image_len) as u32).to_le_bytes());
    file.extend(22u32.to_le_bytes());
    // BITMAPINFOHEADER: the height covers the colour image and the AND mask.
    file.extend(40u32.to_le_bytes());
    file.extend((size as i32).to_le_bytes());
    file.extend((size as i32 * 2).to_le_bytes());
    file.extend(1u16.to_le_bytes());
    file.extend(32u16.to_le_bytes());
    file.extend(0u32.to_le_bytes());
    file.extend((image_len as u32).to_le_bytes());
    file.extend([0u8; 16]);
    // Both bitmaps are stored bottom-up.
    for row in image.pixels.chunks(size).rev() {
        for pixel in row {
            file.extend(pixel.to_le_bytes());
        }
    }
    for row in image.pixels.chunks(size).rev() {
        let mut bits = vec![0u8; stride];
        for (x, pixel) in row.iter().enumerate() {
            if pixel >> 24 == 0 {
                bits[x / 8] |= 0x80 >> (x % 8);
            }
        }
        file.extend(bits);
    }
    file
}

/// One role's current image, for checks that a press changed it and that
/// release restored it exactly.
#[derive(PartialEq)]
pub(super) struct RoleImage {
    pub width: i32,
    pub height: i32,
    pub hotspot: (i32, i32),
    pub over_black: Vec<u32>,
    pub over_white: Vec<u32>,
}
impl RoleImage {
    pub unsafe fn read(role: SYSTEM_CURSOR_ID) -> Option<Self> {
        Self::from_cursor(LoadCursorW(null_mut(), role as usize as *const u16))
    }
    /// A pointer file as Windows loads it at `size`.
    pub unsafe fn from_file(path: &Path, size: i32) -> Option<Self> {
        let cursor = LoadImageW(
            null_mut(),
            wide_path(path).as_ptr(),
            IMAGE_CURSOR,
            size,
            size,
            LR_LOADFROMFILE,
        );
        if cursor.is_null() {
            return None;
        }
        let image = Self::from_cursor(cursor);
        DestroyCursor(cursor);
        image
    }
    unsafe fn from_cursor(cursor: HCURSOR) -> Option<Self> {
        let (width, height, hotspot) = metrics(cursor)?;
        Some(Self {
            width,
            height,
            hotspot,
            over_black: render(cursor, width, height, OVER_BLACK)?,
            over_white: render(cursor, width, height, OVER_WHITE)?,
        })
    }
    /// Same size, hotspot and shape as `expected`, drawn only in black, toolbar
    /// yellow and the shades between them, with both colours present.
    pub fn is_highlight_of(&self, expected: &Self) -> bool {
        let alpha = coverage(&self.over_black, &self.over_white);
        let shape = coverage(&expected.over_black, &expected.over_white);
        let [yellow_r, yellow_g, _] = drawing::CONTRAST_YELLOW.map(u32::from);
        let opaque: Vec<[u32; 3]> = (self.over_white.iter().zip(&alpha))
            .filter(|(_, a)| **a == 255)
            .map(|(pixel, _)| [16, 8, 0].map(|shift| (pixel >> shift) & 255))
            .collect();
        let on_line = |[r, g, b]: [u32; 3]| b <= 2 && g.abs_diff(r * yellow_g / yellow_r) <= 3;
        self.width == expected.width
            && self.height == expected.height
            && self.hotspot == expected.hotspot
            && alpha.iter().zip(&shape).all(|(a, b)| a.abs_diff(*b) <= 3)
            && opaque.iter().all(|pixel| on_line(*pixel))
            && opaque.iter().any(|[r, _, _]| *r <= 2)
            && opaque.iter().any(|[r, _, _]| *r >= yellow_r - 2)
    }
}

/// Replaced roles plus the guard able to restore them. Owned by one worker
/// thread so a scheme reload never blocks camera or UI processing.
#[derive(Default)]
pub(super) struct Highlight {
    applied: bool,
    guard: Option<cursor_process::CursorProcess>,
    /// The pointer file last written, with what it was built from.
    file: Option<(String, PathBuf, i32)>,
}
impl Highlight {
    pub fn applied(&self) -> bool {
        self.applied
    }
    /// Start the guard ahead of the first press; it takes a process launch.
    pub unsafe fn prepare(&mut self) -> bool {
        if self
            .guard
            .as_ref()
            .is_some_and(|guard| guard.exit_code().is_some())
        {
            self.guard = None;
        }
        if self.guard.is_none() {
            self.guard = cursor_process::CursorProcess::start("--cursor-guardian");
        }
        // Build the pointer file now, so the first press only loads it.
        self.highlight_file();
        self.guard.is_some()
    }
    /// The highlight as a pointer file at the size Windows currently draws the
    /// pointer, rebuilt only when the user's arrow, its size or the DPI changes.
    pub unsafe fn highlight_file(&mut self) -> Option<(PathBuf, i32)> {
        let source = arrow_source();
        let size = current_pointer_size();
        let modified = match &source {
            Source::File(path) => std::fs::metadata(path).and_then(|m| m.modified()).ok(),
            Source::System => None,
        };
        let key = format!("{source:?}|{modified:?}|{size}");
        if let Some((built, path, size)) = &self.file
            && *built == key
            && path.is_file()
        {
            return Some((path.clone(), *size));
        }
        let image = highlight_image(&source, size)?;
        let directory = PathBuf::from(std::env::var_os("LOCALAPPDATA")?)
            .join("robocam")
            .join("cursor");
        std::fs::create_dir_all(&directory).ok()?;
        let path = directory.join(format!("highlight-{size}.cur"));
        std::fs::write(&path, cur_file(&image)).ok()?;
        self.file = Some((key, path.clone(), size));
        Some((path, size))
    }
    pub unsafe fn apply(&mut self) {
        if self.applied || !self.prepare() {
            return;
        }
        let Some((path, size)) = self.highlight_file() else {
            return;
        };
        // Never change the session's cursors without a process able to restore them.
        if !self.guard.as_mut().is_some_and(|guard| guard.signal(true)) {
            return;
        }
        self.applied = true;
        // Load the file like a scheme pointer, once per role: SetSystemCursor
        // destroys the handle it receives.
        let path = wide_path(&path);
        let replaced = ROLES.iter().all(|role| {
            let cursor = LoadImageW(
                null_mut(),
                path.as_ptr(),
                IMAGE_CURSOR,
                size,
                size,
                LR_LOADFROMFILE,
            );
            !cursor.is_null() && SetSystemCursor(cursor, *role) != 0
        });
        if !replaced {
            self.restore();
        }
    }
    pub unsafe fn restore(&mut self) {
        // On failure stay armed: the next release, shutdown or the guard retries.
        if self.applied && reload_scheme() {
            self.applied = false;
            self.disarm();
        }
    }
    fn disarm(&mut self) {
        if let Some(guard) = &mut self.guard {
            guard.signal(false);
        }
    }
    /// Close the guard's pipe while armed, exactly as a crash would, and wait
    /// for the guard to restore the scheme. Used by the cursor acceptance test.
    pub fn abandon(mut self) {
        self.applied = false;
        drop(self.guard.take());
    }
}
impl Drop for Highlight {
    fn drop(&mut self) {
        unsafe {
            self.restore();
        }
        // An armed guard also reloads the scheme when its pipe closes.
        drop(self.guard.take());
    }
}

#[derive(Default)]
pub(super) struct ClickCursor {
    requested: bool,
    applied: Arc<AtomicBool>,
    commands: Option<mpsc::Sender<bool>>,
    thread: Option<JoinHandle<()>>,
}
impl ClickCursor {
    /// Whether Robocam wants the highlight: a press is held, or a multi-click continues.
    pub(super) fn active(&self) -> bool {
        self.requested
    }
    /// Whether the session's cursor roles are currently replaced.
    pub(super) fn applied(&self) -> bool {
        self.applied.load(Ordering::Acquire)
    }
    pub(super) fn update(&mut self, enabled: bool, active: bool, dry_run: bool) {
        let active = enabled && active;
        if enabled && !dry_run && self.commands.is_none() {
            self.start();
        }
        if self.requested != active {
            self.requested = active;
            if let Some(commands) = &self.commands {
                let _ = commands.send(active);
            }
        }
        if !enabled {
            self.stop();
        }
    }
    fn start(&mut self) {
        let (commands, requests) = mpsc::channel::<bool>();
        let applied = self.applied.clone();
        let thread = std::thread::Builder::new()
            .name("click-cursor".into())
            .spawn(move || unsafe {
                let mut highlight = Highlight::default();
                highlight.prepare();
                let mut pressed = false;
                let mut visible_until = Instant::now();
                loop {
                    let request = if pressed || !highlight.applied() {
                        requests.recv().map_err(|_| RecvTimeoutError::Disconnected)
                    } else {
                        requests
                            .recv_timeout(visible_until.saturating_duration_since(Instant::now()))
                    };
                    let mut next = match request {
                        Ok(press) => Some(press),
                        Err(RecvTimeoutError::Timeout) => None,
                        Err(RecvTimeoutError::Disconnected) => break,
                    };
                    // Every press is shown, even one already released by the
                    // time this thread sees it, and restarts the minimum time.
                    while let Some(press) = next {
                        pressed = press;
                        if press {
                            highlight.apply();
                            visible_until = Instant::now() + MINIMUM_VISIBLE;
                        }
                        next = requests.try_recv().ok();
                    }
                    if !pressed && Instant::now() >= visible_until {
                        highlight.restore();
                        // A failed reload is retried each second, not in a busy loop.
                        visible_until = Instant::now() + Duration::from_secs(1);
                    }
                    applied.store(highlight.applied(), Ordering::Release);
                }
                drop(highlight);
                applied.store(false, Ordering::Release);
            });
        if let Ok(thread) = thread {
            self.commands = Some(commands);
            self.thread = Some(thread);
        }
    }
    /// Restore the scheme at once and wait for the guard to exit, so an
    /// installer can replace the executable after shutdown.
    pub(super) fn stop(&mut self) {
        self.requested = false;
        self.commands.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
impl Drop for ClickCursor {
    fn drop(&mut self) {
        self.stop();
    }
}

impl App {
    pub(super) unsafe fn update_click_cursor(&mut self) {
        // A press starts the highlight. It lasts through the gaps of the same
        // double or triple click, so the scheme is reloaded once per click.
        let held = self.injected.iter().any(|down| *down);
        let continuing = self.click_cursor.active() && !self.pending_input.is_empty();
        self.click_cursor.update(
            self.config.profile().black_cursor_on_click,
            held || continuing,
            self.dry_run,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 9x9 arrow stand-in: transparent rim, two-pixel outline, 3x3 fill.
    fn arrow(fill: u32, outline: u32) -> (Vec<u32>, Vec<u8>) {
        let mut over_black = vec![0xff000000; 81];
        let mut alpha = vec![0u8; 81];
        for y in 1..8 {
            for x in 1..8 {
                let inner = (3..6).contains(&x) && (3..6).contains(&y);
                over_black[y * 9 + x] = if inner { fill } else { outline };
                alpha[y * 9 + x] = 255;
            }
        }
        (over_black, alpha)
    }

    #[test]
    fn outline_keeps_its_width_in_yellow_and_fill_turns_black_in_both_schemes() {
        for (fill, outline) in [(0xffffffff, 0xff000000), (0xff000000, 0xffffffff)] {
            let (over_black, alpha) = arrow(fill, outline);
            let pixels = recolor(&over_black, &alpha, 9);
            assert_eq!(pixels[0], 0);
            assert_eq!(pixels[9 + 1], 0xffffe600);
            assert_eq!(pixels[2 * 9 + 2], 0xffffe600);
            assert_eq!(pixels[4 * 9 + 4], 0xff000000);
            for (a, pixel) in alpha.iter().zip(pixels) {
                assert_eq!(u32::from(*a), pixel >> 24);
            }
        }
    }

    #[test]
    fn antialiased_edges_keep_full_colour_and_one_shade_arrows_get_an_edge() {
        // Half-covered outline: straight alpha keeps the full yellow.
        let (mut over_black, mut alpha) = arrow(0xffffffff, 0xff000000);
        alpha[4 * 9 + 1] = 128;
        assert_eq!(recolor(&over_black, &alpha, 9)[4 * 9 + 1], 0x80ffe600);
        // Without a contrasting outline only pixels meeting transparency are yellow.
        over_black.fill(0xff000000);
        alpha[4 * 9 + 1] = 255;
        let pixels = recolor(&over_black, &alpha, 9);
        assert_eq!(pixels[9 + 4], 0xffffe600);
        assert_eq!(pixels[2 * 9 + 4], 0xff000000);
        assert_eq!(pixels[4 * 9 + 4], 0xff000000);
    }

    #[test]
    fn pointer_size_follows_base_size_and_monitor_dpi() {
        // The deployment host: base 48 at 130 DPI is drawn at 65 px.
        assert_eq!(pointer_size(48, 130), 65);
        assert_eq!(pointer_size(32, 96), 32);
        assert_eq!(pointer_size(32, 144), 48);
        assert_eq!(pointer_size(0, 96), 32);
        assert_eq!(pointer_size(256, 192), 256);
    }

    #[test]
    fn pointer_file_loads_at_its_size_with_its_hotspot_and_shape() {
        let (over_black, alpha) = arrow(0xffffffff, 0xff000000);
        let small = recolor(&over_black, &alpha, 9);
        // Scale the 9 px stand-in to an odd size, like the host's 65 px.
        let size = 65;
        let pixels = (0..size * size)
            .map(|i| small[(i / size) * 9 / size * 9 + (i % size) * 9 / size])
            .collect();
        let image = Image {
            size: size as i32,
            hotspot: (7, 9),
            pixels,
        };
        let path = std::env::temp_dir().join(format!("robocam-test-{}.cur", std::process::id()));
        std::fs::write(&path, cur_file(&image)).unwrap();
        let loaded = unsafe { RoleImage::from_file(&path, size as i32) };
        std::fs::remove_file(&path).unwrap();
        let loaded = loaded.expect("Windows could not load the pointer file");
        assert_eq!((loaded.width, loaded.height), (65, 65));
        assert_eq!(loaded.hotspot, (7, 9));
        let shape = coverage(&loaded.over_black, &loaded.over_white);
        for (a, pixel) in shape.iter().zip(&image.pixels) {
            assert_eq!(u32::from(*a), pixel >> 24);
        }
        assert!(loaded.is_highlight_of(&loaded));
    }

    #[test]
    fn coverage_is_recovered_from_black_and_white_backgrounds() {
        let over_black = [0xff000000, 0xffffffff, 0xff000000, 0xff404040];
        let over_white = [0xffffffff, 0xffffffff, 0xff000000, 0xffc0c0c0];
        assert_eq!(coverage(&over_black, &over_white), [0, 255, 255, 127]);
    }
}
