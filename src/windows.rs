//! Native Win32 UI and input boundary. The camera never injects input itself.
#![allow(unsafe_op_in_unsafe_fn)]

mod camera_options;
mod capture;
mod click_feedback;
mod desktop_test;
mod drawing;
mod messages;
mod pointer;
mod runtime;
mod sketchup;
mod startup;

use crate::updates::{self, UpdateResult};
use crate::{
    cameras::{self, CameraDevice, CameraPreference},
    engine::*,
    settings::{Profile, ROBO_PROFILE_NAME, Settings},
    vision::*,
};
use std::{
    cell::Cell,
    collections::VecDeque,
    ffi::c_void,
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::Receiver,
    },
    time::Instant,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::{Dwm::*, Gdi::*},
    System::{LibraryLoader::*, SystemInformation::*, Threading::*},
    UI::{Controls::*, HiDpi::*, Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*},
};

const APP_CLASS: &str = "robocam.window";
const TRAY_MESSAGE: u32 = WM_APP + 1;
const DEFERRED_DPI: u32 = WM_APP + 2;
const DEFERRED_REDRAW: u32 = WM_APP + 5;
const APPBAR_CALLBACK: u32 = WM_APP + 3;
const APPBAR_REPOSITION: u32 = WM_APP + 4;
const ENABLE: u16 = 101;
const OPTIONS: u16 = 102;
const KEYBOARD: u16 = 103;
const WIZARD: u16 = 104;
const BAR: u16 = 105;
const HELP: u16 = 106;
const QUIT: u16 = 107;
const PREVIEW: u16 = 110;
const STATUS: u16 = 111;
const SAVE: u16 = 310;
const CANCEL: u16 = 311;
const PAGE: u16 = 312;
const PROFILE: u16 = 313;
const ADD_PROFILE: u16 = 314;
const DELETE_PROFILE: u16 = 315;
const NEXT: u16 = 320;
const CAMERA_SETTINGS: u16 = 321;
const RESET: u16 = 322;
const UPDATES: u16 = 323;
const REFRESH_CAMERAS: u16 = 324;
const FORGET_CAMERA: u16 = 325;
const INPUT_TAG: usize = 0x524F424F;
const SS_OWNERDRAW: u32 = 13;
const SS_NOTIFY: u32 = 0x100;
const SS_LEFT: u32 = 0;
static PHYSICAL_INPUT: AtomicU64 = AtomicU64::new(0);
thread_local! {
    static IN_CALLBACK: Cell<bool> = const { Cell::new(false) };
    static MAIN_WINDOW: Cell<HWND> = const { Cell::new(null_mut()) };
    static APPBAR_WINDOW: Cell<HWND> = const { Cell::new(null_mut()) };
    static BAR_WINDOW: Cell<HWND> = const { Cell::new(null_mut()) };
    static APPBAR_POSITIONING: Cell<bool> = const { Cell::new(false) };
    static APPBAR_PENDING: Cell<bool> = const { Cell::new(false) };
    static APPBAR_RECT: Cell<Option<(u32,[i32;4])>> = const { Cell::new(None) };
    static TASKBAR_CREATED: Cell<u32> = const { Cell::new(0) };
}
struct CallbackGuard;
impl CallbackGuard {
    fn enter() -> Option<Self> {
        IN_CALLBACK.with(|busy| if busy.replace(true) { None } else { Some(Self) })
    }
}
impl Drop for CallbackGuard {
    fn drop(&mut self) {
        IN_CALLBACK.with(|busy| busy.set(false));
    }
}

const BAR_ACTIONS: [(u16, &str, ClickAction); 9] = [
    (201, "No click", ClickAction::NoClick),
    (202, "Left click", ClickAction::Left),
    (203, "Middle click", ClickAction::Middle),
    (204, "Right click", ClickAction::Right),
    (205, "Double click", ClickAction::Double),
    (
        206,
        "SketchUp: select connected (triple click)",
        ClickAction::SketchUpConnected,
    ),
    (207, "Drag", ClickAction::Drag),
    (208, "Scroll", ClickAction::Scroll),
    (
        211,
        "SketchUp: edit group/component",
        ClickAction::SketchUpEdit,
    ),
];

const BAR_BUTTONS: [(u16, &str); 11] = [
    (210, "Enable / pause"),
    (202, "Left click"),
    (205, "Double click"),
    (211, "SketchUp: edit group/component"),
    (206, "SketchUp: select connected (triple click)"),
    (204, "Right click"),
    (207, "Drag"),
    (203, "Middle click"),
    (208, "Scroll"),
    (209, "Show main window"),
    (212, "Pause / resume movement and clicks"),
];

fn wide(s: &str) -> Vec<u16> {
    crate::i18n::translate(s)
        .encode_utf16()
        .chain(Some(0))
        .collect()
}
fn wide_raw(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn rgb(r: u8, g: u8, b: u8) -> u32 {
    r as u32 | ((g as u32) << 8) | ((b as u32) << 16)
}
fn now() -> u64 {
    unsafe { GetTickCount64() }
}

fn click_sequence(
    start: u64,
    button: MouseButton,
    count: u8,
    press_ms: u64,
    gap_ms: u64,
) -> impl Iterator<Item = (u64, MouseButton, bool)> {
    (0..u64::from(count)).flat_map(move |index| {
        let down = start + index * (press_ms + gap_ms);
        [(down, button, true), (down + press_ms, button, false)]
    })
}

#[cfg(test)]
mod click_timing_tests {
    use super::*;

    #[test]
    fn long_presses_never_overlap_in_multi_clicks() {
        for button in [MouseButton::Left, MouseButton::Middle, MouseButton::Right] {
            for count in 1..=3 {
                let sequence: Vec<_> = click_sequence(100, button, count, 120, 25).collect();
                assert_eq!(sequence.len(), usize::from(count) * 2);
                for pair in sequence.as_chunks::<2>().0 {
                    assert_eq!((pair[0].1, pair[0].2), (button, true));
                    assert_eq!((pair[1].1, pair[1].2), (button, false));
                    assert_eq!(pair[1].0 - pair[0].0, 120);
                }
                for pair in sequence[1..].as_chunks::<2>().0 {
                    assert_eq!(pair[1].0 - pair[0].0, 25);
                }
            }
        }
    }

    #[test]
    fn zero_duration_keeps_immediate_clicks() {
        assert!(click_sequence(100, MouseButton::Left, 3, 0, 0).all(|event| event.0 == 100));
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct BarVisualState {
    selected: ClickAction,
    locked: ClickAction,
    drag: bool,
    scroll: bool,
    enabled: bool,
    full_paused: bool,
    held: Option<MouseButton>,
}

struct App {
    hwnd: HWND,
    bar: HWND,
    options: HWND,
    wizard: HWND,
    font: HFONT,
    icon: HICON,
    /// Per-DPI frames of the embedded icon, owned by the app: big, small, tray.
    window_icons: [HICON; 2],
    tray_icon: HICON,
    mouse_hook: HHOOK,
    config: Settings,
    motion: MotionEngine,
    click: ClickEngine,
    camera: Option<VisionWorker>,
    camera_devices: Vec<CameraDevice>,
    camera_choices: Vec<Option<CameraPreference>>,
    camera_discovery: Option<Receiver<Result<Vec<CameraDevice>, String>>>,
    active_camera: Option<CameraDevice>,
    frame: Option<VisionFrame>,
    last_frame: u64,
    enabled: bool,
    full_paused: bool,
    zoom_query: Option<Receiver<(CameraPreference, Result<cameras::DriverZoom, String>)>>,
    zoom_capabilities: Option<(CameraPreference, Result<cameras::DriverZoom, String>)>,
    driver_zoom_state: Option<Result<cameras::DriverZoom, String>>,
    status: String,
    last_status: String,
    controls: Vec<(HWND, u8)>,
    options_page: u8,
    hover_id: u16,
    hover_since: u64,
    hover_done: bool,
    manual_start: Option<(i32, i32)>,
    wizard_step: u8,
    calibration_started: u64,
    calibration_x: f32,
    calibration_y: f32,
    calibration_position: (f32, f32),
    calibration_min: (f32, f32),
    calibration_max: (f32, f32),
    injected: [bool; 3],
    click_cursor: click_feedback::ClickCursor,
    dry_run: bool,
    smoke_started: Instant,
    was_controlling: bool,
    pending_input: VecDeque<(u64, MouseButton, bool)>,
    update_receiver: Option<Receiver<UpdateResult>>,
    manual_update: bool,
    available_release: Option<(String, String)>,
    tooltip_text: Vec<Vec<u16>>,
    auto_close: bool,
    desktop_test_stage: Option<(u8, u128)>,
    camera_fitted: bool,
    window_dpi: Vec<(HWND, u32)>,
    dpi_fonts: Vec<(u32, HFONT)>,
    logical_children: Vec<(HWND, HWND, [i32; 4])>,
    bar_size: [i32; 2],
    bar_visual_state: Option<BarVisualState>,
    last_foreground: HWND,
    diagnostic_path: Option<std::path::PathBuf>,
    diagnostic_written_ms: u64,
    frames_received: u64,
    input_sent: Cell<u64>,
    input_failed: Cell<u64>,
    exit_after_ms: Option<u64>,
    click_actions: u64,
    last_click_dispatch_delay_ms: u64,
    pending_target: Option<HWND>,
}

pub fn run() {
    unsafe {
        let args: Vec<String> = std::env::args().collect();
        let argument = |name: &str| {
            args.iter()
                .position(|arg| arg == name)
                .and_then(|index| args.get(index + 1))
        };
        if let Some(path) = argument("--export-default-settings") {
            match serde_json::to_vec_pretty(&Settings::default())
                .map_err(|error| error.to_string())
                .and_then(|bytes| std::fs::write(path, bytes).map_err(|error| error.to_string()))
            {
                Ok(()) => return,
                Err(error) => {
                    eprintln!("Could not export settings: {error}");
                    std::process::exit(1);
                }
            }
        }
        if let Some(path) = argument("--list-cameras") {
            let result = cameras::enumerate()
                .and_then(|devices| serde_json::to_vec_pretty(&devices).map_err(|e| e.to_string()))
                .and_then(|bytes| std::fs::write(path, bytes).map_err(|e| e.to_string()));
            if let Err(error) = result {
                eprintln!("{error}");
                std::process::exit(1);
            }
            return;
        }
        if let Some(path) = argument("--camera-zoom-report") {
            let result = (|| -> Result<serde_json::Value, String> {
                let devices = cameras::enumerate()?;
                let selected = if let Some(name) = argument("--camera-name") {
                    let matches: Vec<_> = devices.iter().filter(|d| d.name == *name).collect();
                    if matches.len() != 1 {
                        return Err("Camera name must identify exactly one device.".into());
                    }
                    matches[0].clone()
                } else {
                    let (settings, _) = Settings::load();
                    cameras::resolve(&devices, settings.profile().camera.as_ref())?.clone()
                };
                let increments = argument("--camera-zoom-increments")
                    .map(|v| v.parse::<u32>())
                    .transpose()
                    .map_err(|e| e.to_string())?;
                let zoom = cameras::driver_zoom(&selected.preference(), increments)?;
                Ok(serde_json::json!({"camera":selected.preference(),"zoom":zoom}))
            })();
            let report = match &result {
                Ok(report) => report.clone(),
                Err(error) => serde_json::json!({"error":error}),
            };
            let written = std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap());
            if result.is_err() || written.is_err() {
                std::process::exit(1);
            }
            return;
        }
        if args.iter().any(|s| s == "--request-exit") {
            let running = FindWindowW(wide_raw(APP_CLASS).as_ptr(), wide_raw("robocam").as_ptr());
            if !running.is_null() {
                PostMessageW(running, WM_COMMAND, usize::from(QUIT), 0);
            }
            return;
        }
        if args.iter().any(|s| s == "--version") {
            return;
        }
        let desktop_test = args.iter().any(|s| s == "--desktop-test");
        let auto_close = desktop_test || args.iter().any(|s| s == "--smoke-test");
        let dry_run = auto_close
            || argument("--capture-ui").is_some()
            || args.iter().any(|s| s == "--ui-test");
        if let Some(mode) = argument("--configure-startup") {
            if !matches!(mode.as_str(), "enable" | "disable") {
                std::process::exit(2);
            }
            if let Err(error) = startup::configure(mode == "enable") {
                eprintln!("{error}");
                std::process::exit(1);
            }
            return;
        }
        if !dry_run && !runtime::elevated() {
            if let Err(error) = runtime::relaunch_elevated(&args) {
                eprintln!("{error}");
            }
            return;
        }
        let single = CreateMutexW(null(), 0, wide("Local\\robocam.SingleInstance").as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS && !dry_run {
            let previous = FindWindowW(wide(APP_CLASS).as_ptr(), wide("robocam").as_ptr());
            if !previous.is_null() {
                ShowWindow(previous, SW_RESTORE);
                SetForegroundWindow(previous);
            }
            CloseHandle(single);
            return;
        }
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        TASKBAR_CREATED.with(|message| {
            message.set(RegisterWindowMessageW(wide_raw("TaskbarCreated").as_ptr()))
        });
        InitCommonControls();
        let instance = GetModuleHandleW(null());
        // MAKEINTRESOURCEW(1): this value is a resource identifier, not memory.
        #[allow(clippy::manual_dangling_ptr)]
        let icon = LoadIconW(instance, 1usize as *const u16);
        let icon = if icon.is_null() {
            LoadIconW(null_mut(), IDI_APPLICATION)
        } else {
            icon
        };
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS,
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hIcon: icon,
            hbrBackground: (COLOR_BTNFACE + 1) as HBRUSH,
            ..zeroed()
        };
        // Keep the class name alive for RegisterClassW.
        let class_name = wide(APP_CLASS);
        let class = WNDCLASSW {
            lpszClassName: class_name.as_ptr(),
            ..class
        };
        if RegisterClassW(&class) == 0 {
            message(null_mut(), "Could not register the robocam window.");
            return;
        }
        let contrast_class_name = wide_raw("robocam.high-contrast-bar");
        let contrast_class = WNDCLASSW {
            lpszClassName: contrast_class_name.as_ptr(),
            hbrBackground: GetStockObject(BLACK_BRUSH) as HBRUSH,
            ..class
        };
        if RegisterClassW(&contrast_class) == 0 {
            message(
                null_mut(),
                "Could not register the high-contrast click bar.",
            );
            return;
        }
        let (config, warning) = Settings::load();
        crate::i18n::set_language(&config.language);
        let font = CreateFontW(
            -14,
            0,
            0,
            0,
            400,
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
        let mut app = Box::new(App {
            hwnd: null_mut(),
            bar: null_mut(),
            options: null_mut(),
            wizard: null_mut(),
            font,
            icon,
            window_icons: [null_mut(); 2],
            tray_icon: load_icon(GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForSystem())),
            mouse_hook: null_mut(),
            config,
            motion: MotionEngine::default(),
            click: ClickEngine::default(),
            camera: None,
            camera_devices: Vec::new(),
            camera_choices: Vec::new(),
            camera_discovery: None,
            active_camera: None,
            frame: None,
            last_frame: 0,
            enabled: false,
            full_paused: args.iter().any(|arg| arg == "--fully-paused"),
            zoom_query: None,
            zoom_capabilities: None,
            driver_zoom_state: None,
            status: "Starting camera…".into(),
            last_status: String::new(),
            controls: Vec::new(),
            options_page: 0,
            hover_id: 0,
            hover_since: 0,
            hover_done: false,
            manual_start: None,
            wizard_step: 0,
            calibration_started: 0,
            calibration_x: 0.0,
            calibration_y: 0.0,
            calibration_position: (0.0, 0.0),
            calibration_min: (0.0, 0.0),
            calibration_max: (0.0, 0.0),
            injected: [false; 3],
            dry_run,
            smoke_started: Instant::now(),
            was_controlling: false,
            pending_input: VecDeque::new(),
            click_cursor: click_feedback::ClickCursor::default(),
            update_receiver: None,
            manual_update: false,
            available_release: None,
            tooltip_text: Vec::new(),
            auto_close,
            desktop_test_stage: desktop_test.then_some((0, 0)),
            camera_fitted: false,
            window_dpi: Vec::new(),
            dpi_fonts: vec![(96, font)],
            logical_children: Vec::new(),
            bar_size: [561, 52],
            bar_visual_state: None,
            last_foreground: null_mut(),
            diagnostic_path: argument("--diagnostic-report").map(std::path::PathBuf::from),
            diagnostic_written_ms: 0,
            frames_received: 0,
            input_sent: Cell::new(0),
            input_failed: Cell::new(0),
            exit_after_ms: argument("--exit-after-ms").and_then(|value| value.parse().ok()),
            click_actions: 0,
            last_click_dispatch_delay_ms: 0,
            pending_target: None,
        });
        let pointer = &mut *app as *mut App as *const c_void;
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            wide("robocam").as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            scale(300, GetDpiForSystem()),
            scale(300, GetDpiForSystem()),
            null_mut(),
            null_mut(),
            instance,
            pointer,
        );
        if hwnd.is_null() {
            message(null_mut(), "Could not create the robocam window.");
            return;
        }
        app.hwnd = hwnd;
        MAIN_WINDOW.with(|window| window.set(hwnd));
        let setup_guard = CallbackGuard::enter().unwrap();
        app.prepare_window(hwnd, 300, 300);
        app.update_window_icons(app.dpi_for(hwnd));
        app.create_main();
        app.create_bar();
        app.register_hotkeys();
        if !dry_run {
            app.mouse_hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), instance, 0);
        }
        app.tray(NIM_ADD);
        if !dry_run && let Err(error) = runtime::ensure_realtime() {
            message(hwnd, &error);
        }
        if !dry_run {
            let _ = opencv::core::set_num_threads(1);
        }
        app.enabled = argument("--capture-ui").is_some()
            || (app.config.startup_enabled()
                && !dry_run
                && !args.iter().any(|arg| arg == "--paused"));
        if !dry_run {
            app.start_camera();
        }
        if app.config.check_updates && !dry_run {
            app.update_receiver = Some(updates::check_for_updates(env!("CARGO_PKG_VERSION")));
        }
        let minimized = args.iter().any(|s| s == "--minimized") || app.config.minimized_at_startup;
        ShowWindow(
            hwnd,
            if minimized && !dry_run {
                SW_HIDE
            } else {
                SW_SHOW
            },
        );
        SetTimer(hwnd, 1, 15, None);
        if dry_run {
            app.open_options();
        }
        if auto_close {
            app.verify_bar_lifecycle();
        }
        if dry_run && !auto_close {
            // Override a launcher's inherited hidden STARTUPINFO after the
            // first ShowWindow call has consumed it.
            ShowWindow(hwnd, SW_RESTORE);
            ShowWindow(app.options, SW_RESTORE);
            SetForegroundWindow(app.options);
        }
        if !app.config.wizard_completed && !minimized && !dry_run {
            app.open_wizard();
        }
        if let Some(warning) = warning {
            message(hwnd, &warning);
        }
        drop(setup_guard);
        if let Some(directory) = argument("--capture-ui") {
            capture::start(
                directory.into(),
                app.hwnd,
                app.bar,
                app.options,
                scale(app.bar_size[0], app.dpi_for(app.bar)),
            );
        }
        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            if messages::route(&msg) {
                continue;
            }
            if (!app.options.is_null() && IsDialogMessageW(app.options, &msg) != 0)
                || (!app.wizard.is_null() && IsDialogMessageW(app.wizard, &msg) != 0)
            {
                continue;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _cleanup_guard = CallbackGuard::enter();
        remove_appbar(app.bar);
        app.release_all();
        app.write_diagnostics(false);
        if let Some(camera) = app.camera.take() {
            camera.stop();
        }
        app.tray(NIM_DELETE);
        if !app.mouse_hook.is_null() {
            UnhookWindowsHookEx(app.mouse_hook);
        }
        for id in 1..=8 {
            UnregisterHotKey(hwnd, id);
        }
        for (_, font) in app.dpi_fonts.drain(..) {
            DeleteObject(font);
        }
        for icon in app.window_icons.into_iter().chain([app.tray_icon]) {
            if !icon.is_null() {
                DestroyIcon(icon);
            }
        }
        CloseHandle(single);
        if dry_run
            && let Some(arg) = args.iter().position(|s| s == "--smoke-report")
            && let Some(path) = args.get(arg + 1)
        {
            let _ = std::fs::write(
                path,
                if desktop_test {
                    "robocam: Show Desktop, toolbar hit testing and work area, minimize to tray, restore, clean shutdown OK\n"
                } else {
                    "robocam: native windows, options, click bar, timer, clean shutdown OK\n"
                },
            );
        }
    }
}

unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let data = &*(lparam as *const MSLLHOOKSTRUCT);
        if data.flags & LLMHF_INJECTED == 0 && data.dwExtraInfo != INPUT_TAG {
            PHYSICAL_INPUT.store(now(), Ordering::Relaxed);
        }
    }
    CallNextHookEx(null_mut(), code, wparam, lparam)
}

unsafe fn message(hwnd: HWND, text: &str) {
    messages::show(hwnd, text);
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == DEFERRED_REDRAW {
        let child = wp as HWND;
        if IsWindow(child) != 0 {
            InvalidateRect(child, null(), 0);
        }
        return 0;
    }
    if msg == WM_NCCREATE {
        let create = &*(lp as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
    }
    // These notifications can arrive synchronously during Shell operations.
    if MAIN_WINDOW.with(|main| main.get() == hwnd)
        && ((msg == WM_SYSCOMMAND && wp & 0xfff0 == SC_MINIMIZE as usize)
            || (msg == WM_SIZE && wp == SIZE_MINIMIZED as usize))
    {
        ShowWindow(hwnd, SW_HIDE);
        return 0;
    }
    if BAR_WINDOW.with(|bar| bar.get() == hwnd) {
        if msg == WM_SYSCOMMAND && wp & 0xfff0 == SC_MINIMIZE as usize {
            return 0;
        }
        if msg == WM_WINDOWPOSCHANGING {
            let position = &mut *(lp as *mut WINDOWPOS);
            position.flags &= !SWP_HIDEWINDOW;
            position.flags |= SWP_NOACTIVATE;
            if position.flags & SWP_NOZORDER == 0 {
                position.hwndInsertAfter = HWND_TOPMOST;
            }
        }
    }
    // Native parent painting synchronously sends WM_DRAWITEM for owner-drawn
    // children. Do not hold an App borrow/guard while DefWindowProc performs
    // that work: otherwise every child draw is deferred and briefly blank.
    if msg == WM_PAINT {
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    // Shell notifications may arrive synchronously during registration and
    // resize. These helpers use only thread-local handles, never borrow App.
    if matches!(msg, WM_DESTROY | WM_NCDESTROY) {
        remove_appbar(hwnd);
    }
    if TASKBAR_CREATED.with(|message| message.get() != 0 && message.get() == msg)
        && MAIN_WINDOW.with(|window| window.get() == hwnd)
    {
        APPBAR_WINDOW.with(|window| window.set(null_mut()));
        APPBAR_RECT.with(|rectangle| rectangle.set(None));
        queue_appbar_position();
        return 0;
    }
    if matches!(msg, WM_DISPLAYCHANGE | WM_SETTINGCHANGE)
        && MAIN_WINDOW.with(|window| window.get() == hwnd)
    {
        queue_appbar_position();
    }
    if APPBAR_WINDOW.with(|window| window.get() == hwnd) {
        if msg == APPBAR_CALLBACK && matches!(wp as u32, ABN_POSCHANGED | ABN_STATECHANGE) {
            queue_appbar_position();
            return 0;
        }
        if msg == WM_WINDOWPOSCHANGED || msg == WM_ACTIVATE {
            let mut data = appbar_data(hwnd);
            SHAppBarMessage(
                if msg == WM_ACTIVATE {
                    ABM_ACTIVATE
                } else {
                    ABM_WINDOWPOSCHANGED
                },
                &mut data,
            );
        }
    }
    // Default processing may run a nested native menu loop. It must run without
    // a borrowed App so timer-driven head control continues inside those menus.
    if msg == TRAY_MESSAGE && lp as u32 == WM_RBUTTONUP {
        App::tray_menu(hwnd);
        return 0;
    }
    if !matches!(
        msg,
        WM_COMMAND
            | WM_TIMER
            | WM_HOTKEY
            | WM_DRAWITEM
            | WM_PAINT
            | WM_SIZE
            | WM_GETMINMAXINFO
            | WM_DPICHANGED
            | DEFERRED_DPI
            | APPBAR_CALLBACK
            | APPBAR_REPOSITION
            | WM_NCDESTROY
            | WM_LBUTTONDOWN
            | WM_LBUTTONUP
            | WM_NCHITTEST
            | WM_MOUSEACTIVATE
            | WM_CLOSE
            | WM_QUERYENDSESSION
            | WM_ENDSESSION
            | WM_DESTROY
            | TRAY_MESSAGE
    ) {
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    // Win32 synchronously sends notifications during resize, paint and destroy.
    // Never create an aliasing &mut App in such a nested callback.
    let Some(_guard) = CallbackGuard::enter() else {
        match msg {
            WM_DPICHANGED => {
                let rect = Box::new(*(lp as *const RECT));
                let pointer = Box::into_raw(rect);
                if PostMessageW(hwnd, DEFERRED_DPI, wp, pointer as isize) == 0 {
                    drop(Box::from_raw(pointer));
                }
                return 0;
            }
            WM_SIZE | WM_COMMAND | WM_HOTKEY | WM_CLOSE | DEFERRED_DPI | APPBAR_CALLBACK
            | APPBAR_REPOSITION => {
                PostMessageW(hwnd, msg, wp, lp);
                return 0;
            }
            WM_DRAWITEM => {
                // Invalidation inside an in-progress WM_PAINT can be consumed
                // by EndPaint. Schedule it after the synchronous paint returns.
                PostMessageW(
                    hwnd,
                    DEFERRED_REDRAW,
                    (*(lp as *const DRAWITEMSTRUCT)).hwndItem as usize,
                    0,
                );
                return 0;
            }
            WM_DESTROY if MAIN_WINDOW.with(|main| main.get() == hwnd) => {
                PostQuitMessage(0);
                return 0;
            }
            WM_TIMER => return 0,
            _ => return DefWindowProcW(hwnd, msg, wp, lp),
        }
    };
    let app = &mut *ptr;
    match msg {
        APPBAR_REPOSITION => {
            APPBAR_PENDING.with(|pending| pending.set(false));
            app.position_bar();
            0
        }
        APPBAR_CALLBACK if hwnd == app.bar => {
            match wp as u32 {
                ABN_FULLSCREENAPP | ABN_WINDOWARRANGE => {
                    SetWindowPos(
                        hwnd,
                        HWND_TOPMOST,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    );
                }
                _ => {}
            }
            0
        }
        WM_COMMAND => {
            app.command(hwnd, (wp & 0xffff) as u16, ((wp >> 16) & 0xffff) as u16);
            0
        }
        WM_TIMER if hwnd == app.hwnd => {
            app.tick();
            0
        }
        WM_HOTKEY => {
            match wp {
                1 => app.toggle(),
                8 => app.toggle_full_pause(),
                2 => {
                    app.release_all();
                    app.move_cursor(screen_center());
                    app.motion.reset();
                }
                3 => {
                    app.config.profile_mut().motion.restricted_workspace =
                        !app.config.profile().motion.restricted_workspace;
                    app.save();
                }
                4..=7 => {
                    let m = &mut app.config.profile_mut().motion;
                    match wp {
                        4 => m.speed_x = m.speed_x.saturating_sub(1),
                        5 => m.speed_x = (m.speed_x + 1).min(30),
                        6 => m.speed_y = (m.speed_y + 1).min(30),
                        _ => m.speed_y = m.speed_y.saturating_sub(1),
                    }
                    app.save();
                }
                _ => {}
            }
            0
        }
        WM_DRAWITEM => {
            app.draw_item(&*(lp as *const DRAWITEMSTRUCT));
            1
        }
        WM_SIZE if hwnd == app.hwnd => {
            app.layout_main();
            0
        }
        WM_GETMINMAXINFO if hwnd == app.hwnd => {
            let limits = &mut *(lp as *mut MINMAXINFO);
            let dpi = app.dpi_for(hwnd);
            limits.ptMinTrackSize = POINT {
                x: scale(300, dpi),
                y: scale(300, dpi),
            };
            0
        }
        WM_DPICHANGED => {
            app.change_dpi(hwnd, (wp & 0xffff) as u32, &*(lp as *const RECT));
            0
        }
        DEFERRED_DPI => {
            let rectangle = Box::from_raw(lp as *mut RECT);
            app.change_dpi(hwnd, (wp & 0xffff) as u32, &rectangle);
            0
        }
        WM_NCDESTROY => {
            app.window_dpi.retain(|(window, _)| *window != hwnd);
            app.logical_children
                .retain(|(_, parent, _)| *parent != hwnd);
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_LBUTTONDOWN if hwnd == app.hwnd => {
            let p = point_from_lp(lp);
            app.manual_start = Some(p);
            SetCapture(hwnd);
            0
        }
        WM_LBUTTONUP if hwnd == app.hwnd => {
            app.finish_manual_roi(point_from_lp(lp));
            ReleaseCapture();
            0
        }
        WM_MOUSEACTIVATE if hwnd == app.bar => MA_NOACTIVATE as isize,
        WM_NCHITTEST if hwnd == app.bar && app.config.profile().bar_position == 4 => {
            let hit = DefWindowProcW(hwnd, msg, wp, lp);
            if hit == HTCLIENT as isize {
                HTCAPTION as isize
            } else {
                hit
            }
        }
        WM_CLOSE => {
            if hwnd == app.hwnd {
                ShowWindow(hwnd, SW_HIDE);
            } else if hwnd == app.options {
                app.destroy_owned(hwnd);
                app.options = null_mut();
                app.controls.clear();
            } else if hwnd == app.wizard {
                app.calibration_started = 0;
                app.destroy_owned(hwnd);
                app.wizard = null_mut();
            } else if hwnd == app.bar {
                app.keep_bar_visible();
            }
            0
        }
        WM_QUERYENDSESSION => {
            app.release_all();
            1
        }
        WM_ENDSESSION if wp != 0 => {
            remove_appbar(app.bar);
            app.release_all();
            PostQuitMessage(0);
            0
        }
        WM_DESTROY if hwnd == app.hwnd => {
            remove_appbar(app.bar);
            app.release_all();
            PostQuitMessage(0);
            0
        }
        TRAY_MESSAGE => {
            if lp as u32 == WM_LBUTTONUP || lp as u32 == WM_LBUTTONDBLCLK {
                app.show_main();
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn point_from_lp(lp: LPARAM) -> (i32, i32) {
    ((lp as u16 as i16) as i32, ((lp >> 16) as u16 as i16) as i32)
}
unsafe fn cursor() -> (i32, i32) {
    let mut p: POINT = zeroed();
    GetCursorPos(&mut p);
    (p.x, p.y)
}
unsafe fn screen() -> ScreenRect {
    ScreenRect {
        left: GetSystemMetrics(SM_XVIRTUALSCREEN),
        top: GetSystemMetrics(SM_YVIRTUALSCREEN),
        width: GetSystemMetrics(SM_CXVIRTUALSCREEN),
        height: GetSystemMetrics(SM_CYVIRTUALSCREEN),
    }
}
unsafe fn screen_center() -> (i32, i32) {
    let s = screen();
    (s.left + s.width / 2, s.top + s.height / 2)
}
unsafe fn text(hwnd: HWND) -> String {
    let mut buffer = vec![0u16; GetWindowTextLengthW(hwnd) as usize + 1];
    let n = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
    String::from_utf16_lossy(&buffer[..n as usize])
}
unsafe fn set_text(hwnd: HWND, value: &str) {
    SetWindowTextW(hwnd, wide(value).as_ptr());
}
unsafe fn set_text_raw(hwnd: HWND, value: &str) {
    SetWindowTextW(hwnd, wide_raw(value).as_ptr());
}
/// Loads the embedded icon's frame for `size` pixels; the caller destroys it.
unsafe fn load_icon(size: i32) -> HICON {
    // MAKEINTRESOURCEW(1): this value is a resource identifier, not memory.
    #[allow(clippy::manual_dangling_ptr)]
    let resource = 1usize as *const u16;
    LoadImageW(
        GetModuleHandleW(null()),
        resource,
        IMAGE_ICON,
        size,
        size,
        LR_DEFAULTCOLOR,
    ) as HICON
}

fn scale(value: i32, dpi: u32) -> i32 {
    ((i64::from(value) * i64::from(dpi.max(1)) + 48) / 96) as i32
}

unsafe fn appbar_data(hwnd: HWND) -> APPBARDATA {
    APPBARDATA {
        cbSize: size_of::<APPBARDATA>() as u32,
        hWnd: hwnd,
        ..zeroed()
    }
}

unsafe fn queue_appbar_position() {
    if APPBAR_POSITIONING.with(Cell::get) || APPBAR_PENDING.with(|pending| pending.replace(true)) {
        return;
    }
    let main = MAIN_WINDOW.with(Cell::get);
    if main.is_null() || PostMessageW(main, APPBAR_REPOSITION, 0, 0) == 0 {
        APPBAR_PENDING.with(|pending| pending.set(false));
    }
}

unsafe fn remove_appbar(hwnd: HWND) {
    if hwnd.is_null() || !APPBAR_WINDOW.with(|window| window.get() == hwnd) {
        return;
    }
    APPBAR_WINDOW.with(|window| window.set(null_mut()));
    APPBAR_RECT.with(|rectangle| rectangle.set(None));
    let previous = APPBAR_POSITIONING.with(|busy| busy.replace(true));
    let mut data = appbar_data(hwnd);
    SHAppBarMessage(ABM_REMOVE, &mut data);
    APPBAR_POSITIONING.with(|busy| busy.set(previous));
}

struct AppbarPositionGuard(bool);
impl AppbarPositionGuard {
    fn enter() -> Self {
        Self(APPBAR_POSITIONING.with(|busy| busy.replace(true)))
    }
}
impl Drop for AppbarPositionGuard {
    fn drop(&mut self) {
        APPBAR_POSITIONING.with(|busy| busy.set(self.0));
    }
}

impl App {
    unsafe fn destroy_owned(&mut self, hwnd: HWND) {
        // The bar has no owner so minimizing the preview cannot hide it.
        // Explicitly destroy it before tearing down the main window.
        if hwnd == self.hwnd && !self.bar.is_null() {
            let bar = self.bar;
            self.bar = null_mut();
            self.destroy_owned(bar);
        }
        BAR_WINDOW.with(|bar| {
            if bar.get() == hwnd {
                bar.set(null_mut());
            }
        });
        remove_appbar(if hwnd == self.hwnd { self.bar } else { hwnd });
        self.window_dpi.retain(|(window, _)| *window != hwnd);
        self.logical_children
            .retain(|(_, parent, _)| *parent != hwnd);
        DestroyWindow(hwnd);
    }
    unsafe fn dpi_for(&self, hwnd: HWND) -> u32 {
        let root = GetAncestor(hwnd, GA_ROOT);
        self.window_dpi
            .iter()
            .find(|(window, _)| *window == root)
            .map(|(_, dpi)| *dpi)
            .unwrap_or_else(|| GetDpiForWindow(hwnd).max(96))
    }

    unsafe fn font_for(&self, hwnd: HWND) -> HFONT {
        let dpi = self.dpi_for(hwnd);
        self.dpi_fonts
            .iter()
            .find(|(value, _)| *value == dpi)
            .map(|(_, font)| *font)
            .unwrap_or(self.font)
    }

    unsafe fn remember_dpi(&mut self, hwnd: HWND, dpi: u32) {
        let dpi = dpi.max(96);
        if let Some((_, value)) = self
            .window_dpi
            .iter_mut()
            .find(|(window, _)| *window == hwnd)
        {
            *value = dpi;
        } else {
            self.window_dpi.push((hwnd, dpi));
        }
        if !self.dpi_fonts.iter().any(|(value, _)| *value == dpi) {
            let font = CreateFontW(
                -scale(14, dpi),
                0,
                0,
                0,
                400,
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
            if !font.is_null() {
                self.dpi_fonts.push((dpi, font));
            }
        }
    }

    unsafe fn prepare_window(&mut self, hwnd: HWND, width: i32, height: i32) {
        if hwnd.is_null() {
            return;
        }
        let dpi = GetDpiForWindow(hwnd).max(96);
        self.remember_dpi(hwnd, dpi);
        SetWindowPos(
            hwnd,
            null_mut(),
            0,
            0,
            scale(width, dpi),
            scale(height, dpi),
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }

    unsafe fn fit_camera_window(&mut self, width: i32, height: i32) {
        if self.camera_fitted {
            return;
        }
        self.camera_fitted = true;
        let dpi = self.dpi_for(self.hwnd);
        let mut bounds = RECT {
            left: 0,
            top: 0,
            right: scale(width.max(300), dpi),
            bottom: scale(height + 78, dpi),
        };
        AdjustWindowRectExForDpi(
            &mut bounds,
            GetWindowLongW(self.hwnd, GWL_STYLE) as u32,
            1,
            GetWindowLongW(self.hwnd, GWL_EXSTYLE) as u32,
            dpi,
        );
        SetWindowPos(
            self.hwnd,
            null_mut(),
            0,
            0,
            bounds.right - bounds.left,
            bounds.bottom - bounds.top,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }

    /// Title bar, taskbar and Alt+Tab use the icon frame drawn for this DPI.
    unsafe fn update_window_icons(&mut self, dpi: u32) {
        for (index, (kind, metric)) in [(ICON_BIG, SM_CXICON), (ICON_SMALL, SM_CXSMICON)]
            .into_iter()
            .enumerate()
        {
            let icon = load_icon(GetSystemMetricsForDpi(metric, dpi));
            if icon.is_null() {
                continue;
            }
            SendMessageW(self.hwnd, WM_SETICON, kind as usize, icon as isize);
            let previous = std::mem::replace(&mut self.window_icons[index], icon);
            if !previous.is_null() {
                DestroyIcon(previous);
            }
        }
    }

    unsafe fn change_dpi(&mut self, hwnd: HWND, dpi: u32, suggested: &RECT) {
        let dpi = dpi.max(96);
        self.remember_dpi(hwnd, dpi);
        let font = self.font_for(hwnd);
        // Store original logical bounds rather than repeatedly scaling rounded
        // pixels. Combo boxes retain their intended dropdown heights as well.
        for &(child, parent, [x, y, w, h]) in &self.logical_children {
            if parent == hwnd && IsWindow(child) != 0 {
                SendMessageW(child, WM_SETFONT, font as usize, 0);
                SetWindowPos(
                    child,
                    null_mut(),
                    scale(x, dpi),
                    scale(y, dpi),
                    scale(w, dpi),
                    scale(h, dpi),
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }
        SetWindowPos(
            hwnd,
            null_mut(),
            suggested.left,
            suggested.top,
            suggested.right - suggested.left,
            suggested.bottom - suggested.top,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        if hwnd == self.hwnd {
            self.update_window_icons(dpi);
            self.layout_main();
        }
        if hwnd == self.bar && self.config.profile().bar_position != 4 {
            self.position_bar();
        }
        self.manual_start = None;
        RedrawWindow(
            hwnd,
            null(),
            null_mut(),
            RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN,
        );
    }

    unsafe fn child(
        &mut self,
        parent: HWND,
        class: &str,
        title: &str,
        id: u16,
        style: u32,
        rect: [i32; 4],
    ) -> HWND {
        let dpi = self.dpi_for(parent);
        let [x, y, w, h] = rect.map(|value| scale(value, dpi));
        let title = if class == "EDIT" || class == "COMBOBOX" {
            wide_raw(title)
        } else {
            wide(title)
        };
        let hwnd = CreateWindowExW(
            0,
            wide_raw(class).as_ptr(),
            title.as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS | style,
            x,
            y,
            w,
            h,
            parent,
            id as usize as HMENU,
            GetModuleHandleW(null()),
            null(),
        );
        self.logical_children.push((hwnd, parent, rect));
        SendMessageW(hwnd, WM_SETFONT, self.font_for(parent) as usize, 1);
        hwnd
    }

    unsafe fn create_main(&mut self) {
        let menu = CreateMenu();
        let file = CreatePopupMenu();
        let tools = CreatePopupMenu();
        for (id, label) in [
            (ENABLE, "Enable / pause\tF11"),
            (BAR, "Show / hide click bar"),
            (KEYBOARD, "On-screen keyboard"),
            (QUIT, "Exit"),
        ] {
            AppendMenuW(file, MF_STRING, id as usize, wide(label).as_ptr());
        }
        for (id, label) in [
            (OPTIONS, "Configuration…"),
            (WIZARD, "Setup and calibration…"),
            (HELP, "Help and about…"),
            (UPDATES, "Check for updates…"),
        ] {
            AppendMenuW(tools, MF_STRING, id as usize, wide(label).as_ptr());
        }
        AppendMenuW(menu, MF_POPUP, file as usize, wide("&File").as_ptr());
        AppendMenuW(menu, MF_POPUP, tools as usize, wide("&Tools").as_ptr());
        SetMenu(self.hwnd, menu);
        for (i, (id, label)) in [
            (ENABLE, "Enable / pause"),
            (OPTIONS, "Configuration"),
            (KEYBOARD, "Keyboard"),
            (WIZARD, "Setup"),
            (BAR, "Click bar"),
            (HELP, "Help"),
        ]
        .iter()
        .enumerate()
        {
            let button = self.child(
                self.hwnd,
                "BUTTON",
                label,
                *id,
                BS_OWNERDRAW as u32 | WS_TABSTOP,
                [4 + i as i32 * 38, 3, 36, 36],
            );
            self.tooltip(self.hwnd, button, label);
        }
        // SS_NOTIFY redirects a preview click through WM_COMMAND; manual ROI uses a subclass below.
        let preview = self.child(
            self.hwnd,
            "STATIC",
            "Camera preview",
            PREVIEW,
            SS_OWNERDRAW | SS_NOTIFY,
            [0, 42, 280, 175],
        );
        SetWindowSubclass(preview, Some(preview_proc), 1, self as *mut Self as usize);
        self.child(
            self.hwnd,
            "STATIC",
            "Starting camera…",
            STATUS,
            SS_LEFT,
            [5, 220, 275, 35],
        );
        self.layout_main();
    }

    unsafe fn layout_main(&self) {
        if self.hwnd.is_null() {
            return;
        }
        let dpi = self.dpi_for(self.hwnd);
        let mut r: RECT = zeroed();
        GetClientRect(self.hwnd, &mut r);
        MoveWindow(
            GetDlgItem(self.hwnd, PREVIEW as i32),
            0,
            scale(42, dpi),
            r.right,
            (r.bottom - scale(78, dpi)).max(scale(40, dpi)),
            1,
        );
        MoveWindow(
            GetDlgItem(self.hwnd, STATUS as i32),
            scale(5, dpi),
            (r.bottom - scale(34, dpi)).max(scale(83, dpi)),
            r.right - scale(10, dpi),
            scale(32, dpi),
            1,
        );
    }

    unsafe fn tooltip(&mut self, parent: HWND, target: HWND, label: &str) {
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST,
            wide("tooltips_class32").as_ptr(),
            null(),
            WS_POPUP | TTS_ALWAYSTIP,
            0,
            0,
            0,
            0,
            parent,
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        self.tooltip_text.push(wide(label));
        let mut info: TTTOOLINFOW = zeroed();
        info.cbSize = size_of::<TTTOOLINFOW>() as u32;
        info.uFlags = TTF_IDISHWND | TTF_SUBCLASS;
        info.hwnd = parent;
        info.uId = target as usize;
        info.lpszText = self.tooltip_text.last_mut().unwrap().as_mut_ptr();
        SendMessageW(hwnd, TTM_ADDTOOLW, 0, &info as *const _ as isize);
    }

    unsafe fn create_bar(&mut self) {
        self.bar_visual_state = None;
        if !self.bar.is_null() {
            self.destroy_owned(self.bar);
        }
        let mut main_rect: RECT = zeroed();
        GetWindowRect(self.hwnd, &mut main_rect);
        self.bar = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            wide_raw(if self.config.profile().high_contrast_bar {
                "robocam.high-contrast-bar"
            } else {
                APP_CLASS
            })
            .as_ptr(),
            wide("robocam click bar").as_ptr(),
            WS_POPUP | WS_CLIPCHILDREN,
            main_rect.left,
            main_rect.top,
            scale(561, self.dpi_for(self.hwnd)),
            scale(52, self.dpi_for(self.hwnd)),
            null_mut(),
            null_mut(),
            GetModuleHandleW(null()),
            self as *mut Self as *const c_void,
        );
        BAR_WINDOW.with(|bar| bar.set(self.bar));
        let exclude_from_peek: i32 = 1;
        DwmSetWindowAttribute(
            self.bar,
            DWMWA_EXCLUDED_FROM_PEEK as u32,
            (&exclude_from_peek as *const i32).cast(),
            size_of::<i32>() as u32,
        );
        self.prepare_window(self.bar, 561, 52);
        let thin = self.config.profile().thin_bar;
        let vertical = matches!(self.config.profile().bar_position, 2 | 3);
        let unit = if thin { 30 } else { 52 };
        let button = if thin { 76 } else { 48 };
        let gap = if thin { 9 } else { 15 };
        let mut offset = 0;
        for (id, label) in BAR_BUTTONS {
            if matches!(id, 202 | 209) {
                offset += gap;
            }
            let title = if thin && id == 211 {
                "SketchUp 2"
            } else if thin && id == 206 {
                "SketchUp 3"
            } else {
                label
            };
            let width = if thin {
                if vertical {
                    160
                } else {
                    (crate::i18n::translate(title).chars().count() as i32 * 7 + 18).max(76)
                }
            } else {
                button
            };
            let rect = if vertical {
                [2, offset + 2, width, unit - 4]
            } else {
                [offset + 2, 2, width, unit - 4]
            };
            let control = self.child(
                self.bar,
                "BUTTON",
                title,
                id,
                if thin && !self.config.profile().high_contrast_bar {
                    BS_PUSHBUTTON as u32
                } else {
                    BS_OWNERDRAW as u32
                },
                rect,
            );
            self.tooltip(self.bar, control, label);
            offset += if vertical { unit } else { width + 4 };
        }
        let (w, h) = if vertical {
            (if thin { 164 } else { button + 4 }, offset)
        } else {
            (offset, unit)
        };
        self.bar_size = [w, h];
        self.position_bar();
        ShowWindow(self.bar, SW_SHOWNOACTIVATE);
        self.refresh_bar_visuals();
    }

    unsafe fn refresh_bar_visuals(&mut self) {
        let state = BarVisualState {
            selected: self.click.selected(),
            locked: self.click.locked(),
            drag: self.click.drag_enabled(),
            scroll: self.click.scrolling(),
            enabled: self.enabled,
            full_paused: self.full_paused,
            held: self.click.held_button(),
        };
        if self.bar_visual_state == Some(state) {
            return;
        }
        self.bar_visual_state = Some(state);
        if !self.bar.is_null() {
            for id in 201..=212 {
                let control = GetDlgItem(self.bar, id);
                if !control.is_null() {
                    InvalidateRect(control, null(), 0);
                }
            }
        }
        for id in [ENABLE, BAR] {
            let control = GetDlgItem(self.hwnd, id as i32);
            if !control.is_null() {
                InvalidateRect(control, null(), 0);
            }
        }
    }

    unsafe fn position_bar(&mut self) {
        if self.bar.is_null() || IsWindow(self.bar) == 0 {
            return;
        }
        let _positioning = AppbarPositionGuard::enter();
        let dpi = self.dpi_for(self.bar);
        let [w, h] = self.bar_size.map(|value| scale(value, dpi));
        let mut monitor: MONITORINFO = zeroed();
        monitor.cbSize = size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(
            MonitorFromWindow(self.bar, MONITOR_DEFAULTTONEAREST),
            &mut monitor,
        );
        if self.config.profile().bar_position == 4 {
            remove_appbar(self.bar);
            let area = monitor.rcWork;
            SetWindowPos(
                self.bar,
                HWND_TOPMOST,
                area.left + (area.right - area.left - w) / 2,
                area.top + scale(70, dpi),
                w,
                h,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            return;
        }
        let edge = match self.config.profile().bar_position {
            1 => ABE_BOTTOM,
            2 => ABE_LEFT,
            3 => ABE_RIGHT,
            _ => ABE_TOP,
        };
        let mut data = appbar_data(self.bar);
        let registered = APPBAR_WINDOW.with(|window| window.get() == self.bar);
        if !registered {
            data.uCallbackMessage = APPBAR_CALLBACK;
            if SHAppBarMessage(ABM_NEW, &mut data) == 0 {
                // Keep controls reachable even while Explorer is unavailable.
                SetWindowPos(
                    self.bar,
                    HWND_TOPMOST,
                    monitor.rcWork.left,
                    monitor.rcWork.top,
                    w,
                    h,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
                self.status = "Windows could not reserve space for the click bar.".into();
                return;
            }
            APPBAR_WINDOW.with(|window| window.set(self.bar));
            APPBAR_RECT.with(|rectangle| rectangle.set(None));
        }
        data.uEdge = edge;
        // Start from monitor bounds, not rcWork, which already excludes this
        // bar. Shell negotiation accounts for taskbars and other appbars.
        data.rc = monitor.rcMonitor;
        match edge {
            ABE_BOTTOM => data.rc.top = data.rc.bottom - h,
            ABE_LEFT => data.rc.right = data.rc.left + w,
            ABE_RIGHT => data.rc.left = data.rc.right - w,
            _ => data.rc.bottom = data.rc.top + h,
        }
        SHAppBarMessage(ABM_QUERYPOS, &mut data);
        match edge {
            ABE_BOTTOM => data.rc.top = data.rc.bottom - h,
            ABE_LEFT => data.rc.right = data.rc.left + w,
            ABE_RIGHT => data.rc.left = data.rc.right - w,
            _ => data.rc.bottom = data.rc.top + h,
        }
        let proposed = [data.rc.left, data.rc.top, data.rc.right, data.rc.bottom];
        // ABM_SETPOS notifies all appbars. Skip unchanged reservations to avoid
        // feedback loops between independently registered desktop toolbars.
        if APPBAR_RECT.with(|rectangle| rectangle.get() != Some((edge, proposed))) {
            SHAppBarMessage(ABM_SETPOS, &mut data);
            APPBAR_RECT.with(|rectangle| {
                rectangle.set(Some((
                    edge,
                    [data.rc.left, data.rc.top, data.rc.right, data.rc.bottom],
                )))
            });
        }
        let mut current: RECT = zeroed();
        GetWindowRect(self.bar, &mut current);
        if !registered
            || IsWindowVisible(self.bar) == 0
            || current.left != data.rc.left
            || current.top != data.rc.top
            || current.right != data.rc.right
            || current.bottom != data.rc.bottom
        {
            SetWindowPos(
                self.bar,
                HWND_TOPMOST,
                data.rc.left,
                data.rc.top,
                data.rc.right - data.rc.left,
                data.rc.bottom - data.rc.top,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }

    unsafe fn start_camera(&mut self) {
        if self.dry_run {
            return;
        }
        self.release_all();
        self.motion.reset();
        if let Some(camera) = self.camera.take() {
            camera.stop();
        }
        self.frame = None;
        self.last_frame = 0;
        let p = self.config.profile();
        self.camera = Some(VisionWorker::start(VisionConfig {
            camera_index: p.camera_index,
            camera: p.camera.clone(),
            camera_zoom_steps: p.camera_zoom_steps,
            track_face: p.track_face,
            stop_on_face_loss: p.stop_on_face_loss,
            face_timeout_ms: p.face_timeout_ms,
            detection_interval_ms: p.detection_interval_ms,
            manual_area: p.manual_area,
        }));
        self.status = "Opening camera…".into();
    }

    unsafe fn keep_bar_visible(&self) {
        if !self.bar.is_null() && IsWindow(self.bar) != 0 {
            if IsIconic(self.bar) != 0 {
                ShowWindow(self.bar, SW_SHOWNOACTIVATE);
            }
            SetWindowPos(
                self.bar,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }

    unsafe fn tick(&mut self) {
        let time = now();
        if (self.auto_close
            && self.smoke_started.elapsed().as_secs_f32() > 2.0
            && self.desktop_test_stage.is_none_or(|(stage, _)| stage == 2))
            || self
                .exit_after_ms
                .is_some_and(|limit| self.smoke_started.elapsed().as_millis() >= u128::from(limit))
        {
            self.destroy_owned(self.hwnd);
            return;
        }
        // Show Desktop can raise the desktop without changing our visibility bit.
        let foreground = GetForegroundWindow();
        if foreground != self.last_foreground
            || IsWindowVisible(self.bar) == 0
            || IsIconic(self.bar) != 0
        {
            self.last_foreground = foreground;
            self.keep_bar_visible();
        }
        if self.desktop_test_stage.is_some() {
            self.verify_desktop_transition();
        }
        self.poll_updates();
        self.poll_camera_discovery();
        self.poll_zoom_range();
        let event = self.camera.as_ref().and_then(|camera| camera.try_recv());
        let mut displacement = None;
        if let Some(event) = event {
            match event {
                VisionEvent::Cameras { devices, selected } => {
                    self.set_camera_devices(devices);
                    self.active_camera = selected.clone();
                    if selected.is_none() && !self.dry_run {
                        self.options_page = 2;
                        self.open_options();
                        SendMessageW(GetDlgItem(self.options, PAGE as i32), CB_SETCURSEL, 2, 0);
                        self.show_options_page();
                    }
                }
                VisionEvent::Frame(frame) => {
                    self.frames_received += 1;
                    if self.frame.is_none() {
                        self.fit_camera_window(frame.width as i32, frame.height as i32);
                    }
                    if frame.captured_at.elapsed().as_millis() < 250 {
                        displacement = Some((frame.dx, frame.dy));
                        self.last_frame = time;
                    }
                    self.status = if frame.tracking_valid {
                        format!(
                            "{} · {:.0} fps · {} points",
                            if self.full_paused {
                                "Movement and clicks paused — use the hotkey or last button"
                            } else if self.enabled {
                                "Tracking"
                            } else {
                                "Clicks paused — dwell on Play to resume"
                            },
                            frame.fps,
                            frame.tracked_points
                        )
                    } else {
                        "No face — center your face in the camera".into()
                    };
                    self.frame = Some(frame);
                    InvalidateRect(GetDlgItem(self.hwnd, PREVIEW as i32), null(), 0);
                }
                VisionEvent::Zoom(zoom) => self.driver_zoom_state = Some(zoom),
                VisionEvent::Error(error) => {
                    self.status = error;
                    self.release_all();
                    self.last_frame = 0;
                    InvalidateRect(GetDlgItem(self.hwnd, PREVIEW as i32), null(), 0);
                }
                VisionEvent::Stopped => {
                    self.release_all();
                    if self.last_frame != 0 {
                        self.status = "Camera stopped. Open Configuration to restart it.".into();
                    }
                    self.last_frame = 0;
                    InvalidateRect(GetDlgItem(self.hwnd, PREVIEW as i32), null(), 0);
                }
            }
        }
        let physical = PHYSICAL_INPUT.load(Ordering::Relaxed);
        let paused_by_mouse =
            physical > 0 && time.saturating_sub(physical) < self.config.profile().pause_on_mouse_ms;
        let tracking = self.tracking_ready();
        if self.enabled && !self.full_paused && tracking && !paused_by_mouse {
            self.flush_pending_input(time);
        } else if (!tracking || paused_by_mouse)
            && (self.was_controlling || !self.pending_input.is_empty())
        {
            self.release_all();
        }
        let mut position = cursor();
        let calibrating = self.calibration_started > 0;
        if calibrating {
            let (dx, dy) = displacement.unwrap_or((0.0, 0.0));
            self.calibrate_frame(dx, dy, tracking);
        }
        if let Some((dx, dy)) = displacement {
            if tracking && !self.full_paused && !paused_by_mouse && !calibrating {
                self.was_controlling = true;
                let movement =
                    self.motion
                        .process(dx, dy, position, screen(), &self.config.profile().motion);
                let requested = if !self.click.scrolling() && self.pending_input.is_empty() {
                    self.move_cursor(movement.position);
                    position = cursor();
                    Some(movement.position)
                } else {
                    None
                };
                self.pointer_actions(time, movement, position, requested);
            } else {
                self.motion.reset();
                self.hover_since = time;
                self.hover_done = false;
                if self.was_controlling {
                    self.release_all();
                }
            }
        } else if !tracking || paused_by_mouse {
            if self.was_controlling {
                self.release_all();
            }
            self.motion.reset();
            self.hover_since = time;
            self.hover_done = false;
        }
        if paused_by_mouse && self.enabled && tracking {
            self.status = "Physical mouse in use — head control paused".into();
        }
        if self.status != self.last_status {
            set_text(GetDlgItem(self.hwnd, STATUS as i32), &self.status);
            self.last_status = self.status.clone();
        }
        self.refresh_wizard_tracking();
        self.refresh_bar_visuals();
        if time.saturating_sub(self.diagnostic_written_ms) >= 1000 {
            if !self.dry_run {
                let _ = runtime::ensure_realtime();
            }
            self.diagnostic_written_ms = time;
            self.write_diagnostics(true);
        }
    }

    unsafe fn dispatch(&mut self, events: Vec<InputEvent>) {
        for event in events {
            if self.full_paused && !matches!(event, InputEvent::ButtonUp(_)) {
                continue;
            }
            match event {
                InputEvent::SketchUpEdit => self.sketchup_click(2),
                InputEvent::SketchUpConnected => self.sketchup_click(3),
                InputEvent::MoveTo(x, y) => self.move_cursor((x, y)),
                InputEvent::Click(button, count) => {
                    self.click_actions += 1;
                    let spacing = self.config.profile().event_delay_ms;
                    let press = self.config.profile().click.press_ms.unwrap_or(spacing);
                    let time = now();
                    self.pending_input
                        .extend(click_sequence(time, button, count, press, spacing));
                    self.update_click_cursor();
                    self.play_click_sound(false);
                    // A zero-delay click should reach Windows in this frame,
                    // without waiting for another WM_TIMER delivery.
                    self.flush_pending_input(now());
                }
                InputEvent::ButtonDown(button) => {
                    self.button(button, true);
                    self.play_click_sound(true);
                }
                InputEvent::ButtonUp(button) => {
                    self.button(button, false);
                    self.play_click_sound(false);
                }
                InputEvent::Scroll(x, y) => {
                    if y != 0 {
                        self.inject(MOUSEEVENTF_WHEEL, 0, 0, y as u32);
                    }
                    if x != 0 {
                        self.inject(MOUSEEVENTF_HWHEEL, 0, 0, x as u32);
                    }
                }
            }
        }
    }

    unsafe fn inject(&self, flags: u32, dx: i32, dy: i32, data: u32) {
        if self.dry_run
            || (self.full_paused
                && flags
                    & (MOUSEEVENTF_MOVE
                        | MOUSEEVENTF_LEFTDOWN
                        | MOUSEEVENTF_RIGHTDOWN
                        | MOUSEEVENTF_MIDDLEDOWN
                        | MOUSEEVENTF_WHEEL
                        | MOUSEEVENTF_HWHEEL)
                    != 0)
        {
            return;
        }
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx,
                    dy,
                    mouseData: data,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: INPUT_TAG,
                },
            },
        };
        if SendInput(1, &input, size_of::<INPUT>() as i32) == 1 {
            self.input_sent.set(self.input_sent.get() + 1);
        } else {
            self.input_failed.set(self.input_failed.get() + 1);
        }
    }

    unsafe fn flush_pending_input(&mut self, time: u64) {
        if self
            .pending_target
            .is_some_and(|target| GetForegroundWindow() != target)
        {
            self.release_all();
            return;
        }
        while self
            .pending_input
            .front()
            .is_some_and(|(due, _, _)| *due <= time)
        {
            let (due, button, down) = self.pending_input.pop_front().unwrap();
            let delay = now().saturating_sub(due);
            self.last_click_dispatch_delay_ms = delay;
            self.button(button, down);
            // Preserve press duration and inter-click gaps when a timer is late.
            // Otherwise an overdue down/up pair could collapse into one frame.
            for (next, _, _) in &mut self.pending_input {
                *next += delay;
            }
        }
        if self.pending_input.is_empty() {
            self.pending_target = None;
        }
    }

    fn tracking_ready(&self) -> bool {
        self.last_frame != 0 && self.frame.as_ref().is_some_and(VisionFrame::tracking_ready)
    }

    unsafe fn move_cursor(&self, position: (i32, i32)) {
        let s = screen();
        let current = cursor();
        if current == position {
            return;
        }
        let (x, y) = pointer::absolute_position(position, s);
        self.inject(
            MOUSEEVENTF_ABSOLUTE
                | MOUSEEVENTF_VIRTUALDESK
                | MOUSEEVENTF_MOVE
                | MOUSEEVENTF_MOVE_NOCOALESCE,
            x,
            y,
            0,
        );
    }

    unsafe fn write_diagnostics(&self, running: bool) {
        let Some(path) = &self.diagnostic_path else {
            return;
        };
        let mut bar: RECT = zeroed();
        GetWindowRect(self.bar, &mut bar);
        let mut monitor: MONITORINFO = zeroed();
        monitor.cbSize = size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(
            MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTONEAREST),
            &mut monitor,
        );
        let rectangle = |rect: RECT| [rect.left, rect.top, rect.right, rect.bottom];
        let frame = self.frame.as_ref();
        let report = serde_json::json!({
            "version":env!("CARGO_PKG_VERSION"),
            "bar_button_ids":BAR_BUTTONS.map(|(id,_)|id),
            "process_id":GetCurrentProcessId(), "running":running,
            "elevated":runtime::elevated(), "priority_class":GetPriorityClass(GetCurrentProcess()),
            "uptime_ms":self.smoke_started.elapsed().as_millis() as u64,
            "profile":self.config.active_profile, "enabled":self.enabled,
            "full_paused":self.full_paused,
            "camera_zoom_steps":self.config.profile().camera_zoom_steps,
            "driver_zoom":self.driver_zoom_state.as_ref().and_then(|result| result.as_ref().ok()),
            "driver_zoom_error":self.driver_zoom_state.as_ref().and_then(|result| result.as_ref().err()),
            "camera":self.active_camera, "saved_camera":self.config.profile().camera,
            "cameras_detected":self.camera_devices.len(),
            "status":self.status, "frames_received":self.frames_received,
            "fps":frame.map(|frame|frame.fps),
            "face_detected":frame.is_some_and(|frame|frame.face_detected),
            "tracking_valid":frame.is_some_and(|frame|frame.tracking_valid),
            "tracked_points":frame.map(|frame|frame.tracked_points),
            "last_frame_age_ms":frame.map(|frame|frame.captured_at.elapsed().as_millis() as u64),
            "input_events_sent":self.input_sent.get(), "input_failures":self.input_failed.get(),
            "configured_dwell_ms":self.config.profile().click.dwell_ms,
            "last_dwell_duration_ms":self.click.last_dwell_duration_ms(),
            "last_click_dispatch_delay_ms":self.last_click_dispatch_delay_ms,
            "click_actions":self.click_actions,
            "click_cursor_active":self.click_cursor.active(),
            "physical_mouse_pause_ms":self.config.profile().pause_on_mouse_ms,
            "calibration_ready":self.tracking_ready(),
            "bar_bounds":rectangle(bar), "monitor_bounds":rectangle(monitor.rcMonitor),
            "work_area":rectangle(monitor.rcWork),
            "bar_registered":APPBAR_WINDOW.with(|window|window.get()==self.bar),
            "bar_visible":IsWindowVisible(self.bar)!=0,
            "dpi":self.dpi_for(self.hwnd),
        });
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(bytes) = serde_json::to_vec_pretty(&report)
            && let Err(error) = std::fs::write(path, bytes)
        {
            eprintln!("Could not write diagnostic report: {error}");
        }
    }

    unsafe fn button(&mut self, button: MouseButton, down: bool) {
        let (index, flag) = match (button, down) {
            (MouseButton::Left, true) => (0, MOUSEEVENTF_LEFTDOWN),
            (MouseButton::Left, false) => (0, MOUSEEVENTF_LEFTUP),
            (MouseButton::Middle, true) => (1, MOUSEEVENTF_MIDDLEDOWN),
            (MouseButton::Middle, false) => (1, MOUSEEVENTF_MIDDLEUP),
            (MouseButton::Right, true) => (2, MOUSEEVENTF_RIGHTDOWN),
            (MouseButton::Right, false) => (2, MOUSEEVENTF_RIGHTUP),
        };
        self.inject(flag, 0, 0, 0);
        self.injected[index] = down;
        self.update_click_cursor();
    }

    unsafe fn release_all(&mut self) {
        self.pending_target = None;
        self.was_controlling = false;
        self.pending_input.clear();
        let events = self.click.reset();
        self.dispatch(events);
        for (i, b) in [MouseButton::Left, MouseButton::Middle, MouseButton::Right]
            .into_iter()
            .enumerate()
        {
            if self.injected[i] {
                self.button(b, false);
            }
        }
        self.update_click_cursor();
    }

    unsafe fn toggle_full_pause(&mut self) {
        self.full_paused = !self.full_paused;
        self.release_all();
        self.motion.reset();
        self.hover_id = 0;
        self.hover_since = now();
        self.hover_done = false;
        self.refresh_bar_visuals();
    }

    unsafe fn toggle(&mut self) {
        self.enabled = !self.enabled;
        self.release_all();
        self.motion.reset();
        InvalidateRect(GetDlgItem(self.hwnd, ENABLE as i32), null(), 0);
    }
    unsafe fn save(&self) {
        if self.dry_run {
            return;
        }
        if let Err(error) = self.config.save() {
            message(self.hwnd, &format!("Could not save settings: {error}"));
        }
    }
    unsafe fn register_hotkeys(&self) {
        if self.dry_run {
            return;
        }
        for id in 1..=8 {
            UnregisterHotKey(self.hwnd, id);
        }
        for (id, key) in [
            (1, self.config.profile().toggle_key),
            (2, self.config.profile().center_key),
            (3, self.config.profile().workspace_key),
            (8, self.config.profile().full_pause_key),
        ] {
            UnregisterHotKey(self.hwnd, id);
            if key != 0 && RegisterHotKey(self.hwnd, id, MOD_NOREPEAT, key) == 0 {
                message(
                    self.hwnd,
                    &format!(
                        "Hotkey {} is already in use. Choose another key in Configuration.",
                        key
                    ),
                );
            }
        }
        if self.config.profile().arrow_sensitivity {
            for (id, key) in [(4, VK_LEFT), (5, VK_RIGHT), (6, VK_UP), (7, VK_DOWN)] {
                RegisterHotKey(self.hwnd, id, MOD_NOREPEAT, key as u32);
            }
        }
    }
    unsafe fn tray(&self, operation: u32) {
        let mut icon: NOTIFYICONDATAW = zeroed();
        icon.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        icon.hWnd = self.hwnd;
        icon.uID = 1;
        icon.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        icon.uCallbackMessage = TRAY_MESSAGE;
        icon.hIcon = if self.tray_icon.is_null() {
            self.icon
        } else {
            self.tray_icon
        };
        let tip = wide("robocam — F11 enable / pause");
        icon.szTip[..tip.len()].copy_from_slice(&tip);
        Shell_NotifyIconW(operation, &icon);
    }
    unsafe fn tray_menu(hwnd: HWND) {
        let menu = CreatePopupMenu();
        for (id, label) in [
            (209, "Open robocam"),
            (ENABLE, "Enable / pause"),
            (OPTIONS, "Configuration…"),
            (QUIT, "Exit"),
        ] {
            AppendMenuW(menu, MF_STRING, id as usize, wide(label).as_ptr());
        }
        let p = cursor();
        SetForegroundWindow(hwnd);
        TrackPopupMenu(menu, TPM_RIGHTBUTTON, p.0, p.1, 0, hwnd, null());
        DestroyMenu(menu);
    }
    unsafe fn inside_window(&self, hwnd: HWND, p: (i32, i32)) -> bool {
        if hwnd.is_null() || IsWindowVisible(hwnd) == 0 {
            return false;
        }
        let mut r: RECT = zeroed();
        GetWindowRect(hwnd, &mut r);
        p.0 >= r.left && p.0 < r.right && p.1 >= r.top && p.1 < r.bottom
    }
    unsafe fn bar_control_at(&self, p: (i32, i32)) -> u16 {
        if !self.inside_window(self.bar, p) {
            return 0;
        }
        let mut point = POINT { x: p.0, y: p.1 };
        // A popup or another window can cover the bar. Do not activate a
        // button merely because its rectangle is underneath the pointer.
        if GetAncestor(WindowFromPoint(point), GA_ROOT) != self.bar {
            return 0;
        }
        ScreenToClient(self.bar, &mut point);
        let child = ChildWindowFromPointEx(self.bar, point, CWP_SKIPINVISIBLE | CWP_SKIPDISABLED);
        if child == self.bar {
            0
        } else {
            GetDlgCtrlID(child) as u16
        }
    }
    unsafe fn show_main(&self) {
        ShowWindow(self.hwnd, SW_RESTORE);
        BringWindowToTop(self.hwnd);
        SetForegroundWindow(self.hwnd);
    }

    unsafe fn verify_bar_lifecycle(&mut self) {
        assert!(GetWindow(self.bar, GW_OWNER).is_null());
        ShowWindow(self.hwnd, SW_MINIMIZE);
        assert_eq!(
            IsWindowVisible(self.hwnd),
            0,
            "minimize must hide the taskbar window"
        );
        assert_ne!(IsWindowVisible(self.bar), 0, "bar hidden by minimize");
        ShowWindow(self.hwnd, SW_HIDE);
        assert_ne!(IsWindowVisible(self.bar), 0, "bar hidden with preview");
        self.command(self.bar, 209, 0);
        assert_eq!(IsIconic(self.hwnd), 0);
        assert_ne!(IsWindowVisible(self.hwnd), 0);
        // A second activation must never hide an already-visible preview.
        self.command(self.bar, 209, 0);
        assert_ne!(IsWindowVisible(self.hwnd), 0);
        for id in [201, KEYBOARD, OPTIONS, WIZARD, HELP] {
            assert!(GetDlgItem(self.bar, i32::from(id)).is_null());
        }
        assert_eq!(GetDlgCtrlID(GetWindow(self.bar, GW_CHILD)), 210);
        let enabled = self.enabled;
        self.command(self.bar, 210, 0);
        assert_eq!(self.enabled, !enabled);
        self.command(self.bar, 210, 0);
        assert_eq!(self.enabled, enabled);

        // Simulate a stalled timer: a timed press must not collapse into an
        // immediate down/up pair, and pausing must still release it at once.
        let time = now();
        self.pending_input
            .extend(click_sequence(time - 500, MouseButton::Left, 2, 120, 25));
        self.flush_pending_input(time);
        assert!(self.injected[0]);
        assert_eq!(self.pending_input.len(), 3);
        assert!(self.pending_input.front().unwrap().0 >= time + 120);
        self.release_all();
        assert!(!self.injected.iter().any(|down| *down));
        assert!(self.pending_input.is_empty());

        self.verify_click_feedback();
        self.verify_bar_pointer_geometry();
        let enabled = self.enabled;
        self.full_paused = false;
        self.button(MouseButton::Left, true);
        self.pending_input
            .extend(click_sequence(now() + 500, MouseButton::Left, 3, 10, 50));
        self.command(self.bar, 212, 0);
        assert!(self.full_paused);
        assert!(self.pending_input.is_empty());
        assert!(!self.injected.iter().any(|pressed| *pressed));
        let click_actions = self.click_actions;
        self.dispatch(vec![
            InputEvent::Click(MouseButton::Left, 3),
            InputEvent::Scroll(0, 120),
        ]);
        assert_eq!(self.click_actions, click_actions);
        self.pointer_actions(now() + 5000, MotionOutput::default(), cursor(), None);
        assert!(
            self.full_paused,
            "dwell must not automatically resume full pause"
        );
        self.command(self.bar, 212, 0);
        assert!(!self.full_paused);
        assert_eq!(self.enabled, enabled);
        ShowWindow(self.bar, SW_HIDE);
        self.keep_bar_visible();
        assert_ne!(
            IsWindowVisible(self.bar),
            0,
            "bar stayed hidden after desktop arrangement"
        );
        SetWindowPos(
            self.bar,
            HWND_BOTTOM,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
        assert_ne!(
            GetWindowLongW(self.bar, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST,
            0
        );
    }

    unsafe fn verify_click_feedback(&mut self) {
        let previous = self.config.profile().black_cursor_on_click;
        self.config.profile_mut().black_cursor_on_click = true;
        // Keep feedback across multi-click gaps; dry-run never hides the cursor.
        for count in 1..=3 {
            self.pending_input
                .extend(click_sequence(now(), MouseButton::Left, count, 100, 25));
            while let Some((_, button, down)) = self.pending_input.pop_front() {
                self.button(button, down);
                assert_eq!(
                    self.click_cursor.active(),
                    down || !self.pending_input.is_empty()
                );
            }
            assert!(!self.click_cursor.active());
        }
        for button in [MouseButton::Left, MouseButton::Middle, MouseButton::Right] {
            self.dispatch(vec![InputEvent::ButtonDown(button)]);
            assert!(self.click_cursor.active());
            self.release_all();
            assert!(!self.click_cursor.active());
        }
        // Cancellation before a delayed SketchUp press restores feedback too.
        self.pending_input
            .extend(click_sequence(now() + 40, MouseButton::Left, 2, 100, 25));
        self.update_click_cursor();
        assert!(self.click_cursor.active());
        self.release_all();
        assert!(!self.click_cursor.active());
        self.config.profile_mut().black_cursor_on_click = false;
        self.button(MouseButton::Left, true);
        assert!(!self.click_cursor.active());
        self.release_all();
        self.config.profile_mut().black_cursor_on_click = previous;
    }
}

unsafe extern "system" fn preview_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    if !matches!(msg, WM_LBUTTONDOWN | WM_LBUTTONUP) {
        return DefSubclassProc(hwnd, msg, wp, lp);
    }
    let Some(_guard) = CallbackGuard::enter() else {
        return DefSubclassProc(hwnd, msg, wp, lp);
    };
    let app = &mut *(data as *mut App);
    if msg == WM_LBUTTONDOWN {
        app.manual_start = Some(point_from_lp(lp));
        SetCapture(hwnd);
        return 0;
    }
    if msg == WM_LBUTTONUP {
        app.finish_manual_roi(point_from_lp(lp));
        ReleaseCapture();
        return 0;
    }
    DefSubclassProc(hwnd, msg, wp, lp)
}

impl App {
    unsafe fn command(&mut self, source: HWND, id: u16, notification: u16) {
        if notification != BN_CLICKED as u16 && !matches!(id, PAGE | PROFILE | 440) {
            return;
        }
        match id {
            ENABLE | 210 | 201 => self.toggle(),
            212 => self.toggle_full_pause(),
            OPTIONS => self.open_options(),
            KEYBOARD => {
                if self.dry_run {
                    return;
                }
                let command = &self.config.profile().keyboard_command;
                // ShellExecute handles App Paths and Windows' accessibility keyboard correctly.
                let result = ShellExecuteW(
                    self.hwnd,
                    wide_raw("open").as_ptr(),
                    wide_raw(command).as_ptr(),
                    null(),
                    null(),
                    SW_SHOWNORMAL,
                );
                if result as usize <= 32 {
                    message(
                        self.hwnd,
                        "Could not open the keyboard. Configure its executable path in General.",
                    );
                }
            }
            WIZARD => self.open_wizard(),
            BAR => {
                self.keep_bar_visible();
            }
            HELP => message(
                self.hwnd,
                concat!(
                    "robocam ",
                    env!("CARGO_PKG_VERSION"),
                    "\nHead-controlled mouse for Windows 11\n\n",
                    "F11: enable / pause    F10: center pointer    F9: workspace limits\n",
                    "Center your face in the preview, enable, then move your head. Stop over a target to click. Select an action on the click bar; select it again to lock it.\n\n",
                    "Drag modifies the selected single-click button: stop to hold, move, then stop again to release. Scroll: stop to enter, move your head to scroll, stop again to leave. No click disables desktop clicks; the click bar remains accessible.\n\n",
                    "To track a manual region, turn off automatic face location and drag a rectangle in the preview.\n\n",
                    "Includes GPL-3.0-or-later contributions by Cesar Mauri Loba and contributors. Camera processing stays on this computer.\n",
                    "See README and THIRD_PARTY_NOTICES in the installation directory."
                ),
            ),
            UPDATES => {
                if self.dry_run {
                    return;
                }
                if let Some((version, url)) = &self.available_release {
                    messages::show_releases(
                        self.hwnd,
                        &format!("robocam {version} is available.\n\n{url}"),
                        url,
                    );
                } else {
                    // An explicit request also subscribes to an in-flight
                    // startup check instead of silently doing nothing.
                    self.manual_update = true;
                    if self.update_receiver.is_none() {
                        self.update_receiver =
                            Some(updates::check_for_updates(env!("CARGO_PKG_VERSION")));
                    }
                }
            }
            QUIT => {
                self.release_all();
                self.destroy_owned(self.hwnd);
            }
            209 => self.show_main(),
            202..=208 | 211 => {
                if source == self.bar
                    && now().saturating_sub(PHYSICAL_INPUT.load(Ordering::Relaxed)) < 100
                    && self.was_controlling
                {
                    self.release_all();
                }
                let action = BAR_ACTIONS.iter().find(|(n, _, _)| *n == id).unwrap().2;
                let events = self.click.select(action, self.config.profile().fast_bar);
                self.dispatch(events);
                self.refresh_bar_visuals();
            }
            PAGE if notification == CBN_SELCHANGE as u16 => {
                self.options_page =
                    SendMessageW(GetDlgItem(source, PAGE as i32), CB_GETCURSEL, 0, 0) as u8;
                self.show_options_page();
            }
            PROFILE if notification == CBN_SELCHANGE as u16 => {
                let name = text(GetDlgItem(source, PROFILE as i32));
                if self.config.profiles.contains_key(&name) {
                    self.release_all();
                    self.config.active_profile = name;
                    self.save();
                    self.register_hotkeys();
                    self.start_camera();
                    self.create_bar();
                    self.destroy_owned(self.options);
                    self.options = null_mut();
                    self.open_options();
                }
            }
            SAVE => {
                if self.read_options() {
                    self.save();
                    self.register_hotkeys();
                    self.create_bar();
                    self.start_camera();
                    self.destroy_owned(self.options);
                    self.options = null_mut();
                    self.controls.clear();
                }
            }
            CANCEL => {
                self.destroy_owned(self.options);
                self.options = null_mut();
                self.controls.clear();
            }
            ADD_PROFILE => {
                let name = text(GetDlgItem(self.options, PROFILE as i32))
                    .trim()
                    .to_owned();
                if name.is_empty() || name.len() > 64 {
                    message(
                        self.options,
                        "Enter a profile name of 1–64 characters in the profile box.",
                    );
                    return;
                }
                if self.config.profiles.contains_key(&name) {
                    message(
                        self.options,
                        "That profile already exists. Enter a new name, then choose Save as.",
                    );
                    return;
                }
                let original = self.config.profile().clone();
                if !self.read_options() {
                    return;
                }
                let new_profile = self.config.profile().clone();
                *self.config.profile_mut() = original;
                self.config.profiles.insert(name.clone(), new_profile);
                self.config.active_profile = name;
                self.save();
                self.register_hotkeys();
                self.create_bar();
                self.start_camera();
                self.destroy_owned(self.options);
                self.options = null_mut();
                self.open_options();
            }
            DELETE_PROFILE => {
                if self.config.active_profile == ROBO_PROFILE_NAME {
                    message(
                        self.options,
                        "Robo is a built-in profile and cannot be deleted.",
                    );
                    return;
                }
                if self.config.profiles.len() == 1 {
                    message(self.options, "Keep at least one profile.");
                    return;
                }
                let name = self.config.active_profile.clone();
                self.config.profiles.remove(&name);
                self.config.active_profile = self.config.profiles.keys().next().unwrap().clone();
                self.save();
                self.release_all();
                self.start_camera();
                self.create_bar();
                self.register_hotkeys();
                self.destroy_owned(self.options);
                self.options = null_mut();
                self.open_options();
            }
            CAMERA_SETTINGS => {
                if let Some(camera) = &self.camera {
                    camera.request_camera_settings();
                }
            }
            REFRESH_CAMERAS => {
                self.camera_discovery = Some(cameras::discover());
            }
            FORGET_CAMERA => {
                SendMessageW(GetDlgItem(self.options, 440), CB_SETCURSEL, 0, 0);
            }
            440 if notification == CBN_SELCHANGE as u16 => self.query_zoom_range(),
            447 => self.update_zoom_options(),
            441 | 444 => self.update_camera_options(),
            RESET => {
                self.release_all();
                self.start_camera();
            }
            NEXT => self.wizard_next(),
            _ => {}
        }
    }

    unsafe fn open_options(&mut self) {
        if !self.options.is_null() {
            ShowWindow(self.options, SW_RESTORE);
            SetForegroundWindow(self.options);
            return;
        }
        self.options = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            wide(APP_CLASS).as_ptr(),
            wide("robocam — Configuration").as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            scale(550, self.dpi_for(self.hwnd)),
            scale(535, self.dpi_for(self.hwnd)),
            self.hwnd,
            null_mut(),
            GetModuleHandleW(null()),
            self as *mut Self as *const c_void,
        );
        self.prepare_window(self.options, 550, 589);
        self.controls.clear();
        let profile = self.child(
            self.options,
            "COMBOBOX",
            "",
            PROFILE,
            WS_TABSTOP | CBS_DROPDOWN as u32 | WS_VSCROLL,
            [18, 14, 276, 240],
        );
        for name in self.config.profiles.keys() {
            SendMessageW(profile, CB_ADDSTRING, 0, wide_raw(name).as_ptr() as isize);
        }
        set_text_raw(profile, &self.config.active_profile);
        self.child(
            self.options,
            "BUTTON",
            "Save as…",
            ADD_PROFILE,
            WS_TABSTOP,
            [308, 13, 98, 27],
        );
        self.child(
            self.options,
            "BUTTON",
            "Delete",
            DELETE_PROFILE,
            WS_TABSTOP,
            [414, 13, 98, 27],
        );
        let pages = self.child(
            self.options,
            "COMBOBOX",
            "",
            PAGE,
            WS_TABSTOP | CBS_DROPDOWNLIST as u32,
            [18, 51, 494, 180],
        );
        for name in [
            "Pointer motion",
            "Clicking",
            "Camera",
            "Click bar",
            "General",
        ] {
            SendMessageW(pages, CB_ADDSTRING, 0, wide(name).as_ptr() as isize);
        }
        SendMessageW(pages, CB_SETCURSEL, self.options_page as usize, 0);
        let p = self.config.profile().clone();
        self.edit(0, 0, 400, "Horizontal speed (0–30)", p.motion.speed_x);
        self.edit(0, 1, 401, "Vertical speed (0–30)", p.motion.speed_y);
        self.edit(0, 2, 402, "Acceleration (0–5)", p.motion.acceleration);
        self.edit(0, 3, 403, "Smoothing (0–8)", p.motion.smoothing);
        self.edit(0, 4, 404, "Easy stop (0–10)", p.motion.easy_stop);
        self.check(
            0,
            5,
            405,
            "Reverse horizontal direction",
            p.motion.reverse_x,
        );
        self.check(0, 6, 406, "Reverse vertical direction", p.motion.reverse_y);
        self.check(
            0,
            7,
            407,
            "Restrict workspace",
            p.motion.restricted_workspace,
        );
        for (i, label) in [
            "Left margin (%)",
            "Top margin (%)",
            "Right margin (%)",
            "Bottom margin (%)",
        ]
        .into_iter()
        .enumerate()
        {
            self.edit(0, 8 + i, 408 + i as u16, label, p.motion.margins_percent[i]);
        }
        self.check(
            0,
            12,
            412,
            "Wrap pointer at workspace edges",
            p.motion.wrap_pointer,
        );

        self.choice(
            1,
            0,
            420,
            "Click method",
            &["Dwell", "Gesture", "Disabled"],
            match p.click.mode {
                ClickMode::Dwell => 0,
                ClickMode::Gesture => 1,
                ClickMode::None => 2,
            },
        );
        self.edit(1, 1, 421, "Dwell time (200–5000 ms)", p.click.dwell_ms);
        self.edit(
            1,
            2,
            422,
            "Movement tolerance (1–8 px)",
            p.click.tolerance_px,
        );
        self.check(1, 3, 423, "Allow consecutive clicks", p.click.consecutive);
        self.check(
            1,
            4,
            424,
            "Fast gesture (fixed second dwell)",
            p.click.fast_gesture,
        );
        self.check(1, 5, 425, "Play a sound on click", p.click_sound);
        self.check(
            1,
            6,
            426,
            "Black cursor with gray outline during click",
            p.black_cursor_on_click,
        );
        self.edit(
            1,
            7,
            427,
            "Pause after physical mouse (ms)",
            p.pause_on_mouse_ms,
        );
        self.edit(
            1,
            8,
            428,
            "Mouse press duration (0-1000 ms)",
            p.click.press_ms.unwrap_or(p.event_delay_ms),
        );
        let actions: Vec<&str> = ClickAction::ALL.iter().map(|a| a.label()).collect();
        for (i, label) in [
            "Gesture: move left",
            "Gesture: move right",
            "Gesture: move up",
            "Gesture: move down",
        ]
        .into_iter()
        .enumerate()
        {
            let selected = ClickAction::ALL
                .iter()
                .position(|a| *a == p.click.gestures[i])
                .unwrap_or(0);
            self.choice(1, 9 + i, 429 + i as u16, label, &actions, selected);
        }
        self.note(1, 13, "Gesture: dwell, move in a direction, dwell again.");

        self.camera_choices.clear();
        let label = self.child(self.options, "STATIC", "Camera", 0, 0, [20, 94, 125, 23]);
        self.controls.push((label, 2));
        let chooser = self.child(
            self.options,
            "COMBOBOX",
            "",
            440,
            WS_TABSTOP | CBS_DROPDOWNLIST as u32 | WS_VSCROLL,
            [150, 90, 360, 260],
        );
        SendMessageW(chooser, CB_SETDROPPEDWIDTH, 480, 0);
        self.controls.push((chooser, 2));
        self.fill_camera_choices(p.camera.clone());
        self.camera_discovery = Some(cameras::discover());
        self.check(2, 1, 441, "Locate face automatically", p.track_face);
        self.edit(
            2,
            2,
            442,
            "Face-loss timeout (100–59000 ms)",
            p.face_timeout_ms,
        );
        self.edit(
            2,
            3,
            443,
            "Face detection interval (50–2000 ms)",
            p.detection_interval_ms,
        );
        self.check(
            2,
            4,
            444,
            "Stop pointer when face is not detected",
            p.stop_on_face_loss,
        );
        self.update_camera_options();
        self.note(
            2,
            5,
            "For manual tracking, disable automatic face location,",
        );
        self.note(
            2,
            6,
            "save, then drag a rectangle over your face in the preview.",
        );
        self.check(
            2,
            8,
            447,
            "Fixed zoom via camera driver",
            p.camera_zoom_steps.is_some(),
        );
        self.edit(
            2,
            9,
            448,
            "Native zoom increments",
            p.camera_zoom_steps.unwrap_or(0),
        );
        let zoom_note = self.child(
            self.options,
            "STATIC",
            "Reading driver zoom range…",
            449,
            0,
            [20, 364, 490, 48],
        );
        self.controls.push((zoom_note, 2));
        for (id, label, x) in [
            (REFRESH_CAMERAS, "Refresh cameras", 20),
            (FORGET_CAMERA, "Forget saved camera", 268),
        ] {
            let button = self.child(
                self.options,
                "BUTTON",
                label,
                id,
                WS_TABSTOP,
                [x, 279, 242, 25],
            );
            self.controls.push((button, 2));
        }
        for (id, label, x) in [
            (CAMERA_SETTINGS, "Camera driver settings…", 20),
            (RESET, "Reconnect camera", 268),
        ] {
            let button = self.child(
                self.options,
                "BUTTON",
                label,
                id,
                WS_TABSTOP,
                [x, 414, 242, 25],
            );
            self.controls.push((button, 2));
        }
        self.query_zoom_range();

        self.choice(
            3,
            0,
            450,
            "Selection behavior",
            &["Normal (dwell to select)", "Fast (hover to select)"],
            p.fast_bar as usize,
        );
        self.choice(
            3,
            1,
            451,
            "Button style",
            &["48 px icons", "Compact text"],
            p.thin_bar as usize,
        );
        self.choice(
            3,
            2,
            452,
            "Position",
            &["Top", "Bottom", "Left", "Right", "Floating"],
            p.bar_position as usize,
        );
        self.note(3, 4, "The click bar stays visible until robocam exits.");
        self.check(
            3,
            5,
            466,
            "High-contrast bar (black background)",
            p.high_contrast_bar,
        );
        self.note(
            3,
            6,
            "Normal: select once for the next click; again to lock.",
        );
        self.note(3, 7, "Fast: hover for the next click; click to lock.");
        self.note(
            3,
            9,
            "Drag works with left, middle, or right single clicks.",
        );
        self.note(
            3,
            10,
            "A second dwell releases the held button or ends scrolling.",
        );

        self.edit(
            4,
            0,
            455,
            "On-screen keyboard executable",
            &p.keyboard_command,
        );
        let keys: Vec<String> = std::iter::once("Disabled".into())
            .chain((1..=11).map(|i| format!("F{i}")))
            .collect();
        let key_refs: Vec<&str> = keys.iter().map(String::as_str).collect();
        for (row, id, label, key) in [
            (1, 456, "Enable / pause hotkey", p.toggle_key),
            (2, 457, "Center pointer hotkey", p.center_key),
            (3, 458, "Workspace toggle hotkey", p.workspace_key),
            (
                13,
                467,
                "Pause movement and clicks hotkey",
                p.full_pause_key,
            ),
        ] {
            self.choice(
                4,
                row,
                id,
                label,
                &key_refs,
                if (0x70..=0x7A).contains(&key) {
                    (key - 0x6F) as usize
                } else {
                    0
                },
            );
        }
        self.check(
            4,
            4,
            459,
            "Enable head control at startup",
            self.config.startup_enabled(),
        );
        self.check(
            4,
            5,
            460,
            "Start minimized in the notification area",
            self.config.minimized_at_startup,
        );
        self.check(
            4,
            6,
            461,
            "Start robocam when I sign in to Windows",
            login_enabled(),
        );
        self.check(
            4,
            7,
            462,
            "Arrow hotkeys adjust pointer speed",
            p.arrow_sensitivity,
        );
        self.check(
            4,
            8,
            463,
            "Check for updates at startup",
            self.config.check_updates,
        );
        self.edit(
            4,
            9,
            464,
            "Delay between button events (0–50 ms)",
            p.event_delay_ms,
        );
        self.choice(
            4,
            10,
            465,
            "Language (restart required)",
            &["English", "Italiano"],
            (self.config.language == "it") as usize,
        );
        self.note(4, 11, "Closing the main window keeps robocam in the tray.");
        self.note(
            4,
            12,
            "Use File > Exit or the tray menu to stop it completely.",
        );
        self.child(
            self.options,
            "BUTTON",
            "Save",
            SAVE,
            WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
            [308, 516, 98, 28],
        );
        self.child(
            self.options,
            "BUTTON",
            "Cancel",
            CANCEL,
            WS_TABSTOP,
            [414, 516, 98, 28],
        );
        self.show_options_page();
        ShowWindow(self.options, SW_SHOW);
        SetForegroundWindow(self.options);
    }

    unsafe fn edit(
        &mut self,
        page: u8,
        row: usize,
        id: u16,
        label: &str,
        value: impl std::fmt::Display,
    ) {
        let y = 90 + row as i32 * 27;
        let label = self.child(self.options, "STATIC", label, 0, 0, [20, y + 4, 306, 23]);
        self.controls.push((label, page));
        let field = self.child(
            self.options,
            "EDIT",
            &value.to_string(),
            id,
            WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL as u32,
            [332, y, 178, 24],
        );
        self.controls.push((field, page));
    }
    unsafe fn check(&mut self, page: u8, row: usize, id: u16, label: &str, value: bool) {
        let field = self.child(
            self.options,
            "BUTTON",
            label,
            id,
            WS_TABSTOP | BS_AUTOCHECKBOX as u32,
            [20, 90 + row as i32 * 27, 490, 24],
        );
        SendMessageW(
            field,
            BM_SETCHECK,
            if value { BST_CHECKED } else { BST_UNCHECKED } as usize,
            0,
        );
        self.controls.push((field, page));
    }
    unsafe fn choice(
        &mut self,
        page: u8,
        row: usize,
        id: u16,
        label: &str,
        items: &[&str],
        selected: usize,
    ) {
        let y = 90 + row as i32 * 27;
        let label = self.child(self.options, "STATIC", label, 0, 0, [20, y + 4, 260, 23]);
        self.controls.push((label, page));
        let field = self.child(
            self.options,
            "COMBOBOX",
            "",
            id,
            WS_TABSTOP | CBS_DROPDOWNLIST as u32 | WS_VSCROLL,
            [286, y, 224, 240],
        );
        for item in items {
            SendMessageW(field, CB_ADDSTRING, 0, wide(item).as_ptr() as isize);
        }
        SendMessageW(field, CB_SETCURSEL, selected, 0);
        self.controls.push((field, page));
    }
    unsafe fn note(&mut self, page: u8, row: usize, label: &str) {
        let field = self.child(
            self.options,
            "STATIC",
            label,
            0,
            0,
            [20, 90 + row as i32 * 27, 495, 24],
        );
        self.controls.push((field, page));
    }
    unsafe fn show_options_page(&self) {
        for (hwnd, page) in &self.controls {
            ShowWindow(
                *hwnd,
                if *page == self.options_page {
                    SW_SHOW
                } else {
                    SW_HIDE
                },
            );
        }
    }
    unsafe fn number(&self, id: u16, min: u64, max: u64) -> Result<u64, String> {
        let value = text(GetDlgItem(self.options, id as i32));
        match value.trim().parse::<u64>() {
            Ok(n) if (min..=max).contains(&n) => Ok(n),
            _ => Err(format!(
                "Enter a number between {min} and {max} (field contains {value:?})."
            )),
        }
    }
    unsafe fn checked(&self, id: u16) -> bool {
        SendMessageW(GetDlgItem(self.options, id as i32), BM_GETCHECK, 0, 0) == BST_CHECKED as isize
    }
    unsafe fn update_camera_options(&self) {
        let automatic = self.checked(441);
        EnableWindow(GetDlgItem(self.options, 444), i32::from(automatic));
        EnableWindow(
            GetDlgItem(self.options, 442),
            i32::from(automatic && self.checked(444)),
        );
        EnableWindow(GetDlgItem(self.options, 443), i32::from(automatic));
    }
    unsafe fn selected(&self, id: u16) -> usize {
        SendMessageW(GetDlgItem(self.options, id as i32), CB_GETCURSEL, 0, 0).max(0) as usize
    }

    unsafe fn read_options(&mut self) -> bool {
        let result = (|| -> Result<Profile, String> {
            let mut p = self.config.profile().clone();
            p.motion.speed_x = self.number(400, 0, 30)? as u8;
            p.motion.speed_y = self.number(401, 0, 30)? as u8;
            p.motion.acceleration = self.number(402, 0, 5)? as u8;
            p.motion.smoothing = self.number(403, 0, 8)? as u8;
            p.motion.easy_stop = self.number(404, 0, 10)? as u8;
            p.motion.reverse_x = self.checked(405);
            p.motion.reverse_y = self.checked(406);
            p.motion.restricted_workspace = self.checked(407);
            for i in 0..4 {
                p.motion.margins_percent[i] = self.number(408 + i as u16, 0, 50)? as u8;
            }
            p.motion.wrap_pointer = self.checked(412);
            p.click.mode = match self.selected(420) {
                1 => ClickMode::Gesture,
                2 => ClickMode::None,
                _ => ClickMode::Dwell,
            };
            p.click.dwell_ms = self.number(421, 200, 5000)?;
            p.click.tolerance_px = self.number(422, 1, 8)? as u8;
            p.click.consecutive = self.checked(423);
            p.click.fast_gesture = self.checked(424);
            p.click_sound = self.checked(425);
            p.black_cursor_on_click = self.checked(426);
            p.pause_on_mouse_ms = self.number(427, 0, 10000)?;
            p.click.press_ms = Some(self.number(428, 0, 1000)?);
            for i in 0..4 {
                p.click.gestures[i] = ClickAction::ALL[self
                    .selected(429 + i as u16)
                    .min(ClickAction::ALL.len() - 1)];
            }
            p.camera = self
                .camera_choices
                .get(self.selected(440))
                .cloned()
                .flatten();
            p.camera_zoom_steps = if self.checked(447) {
                let maximum = self
                    .zoom_capabilities
                    .as_ref()
                    .and_then(|(_, range)| range.as_ref().ok())
                    .ok_or("The selected camera has no verified native zoom range.")?
                    .maximum_increments()?;
                Some(self.number(448, 0, u64::from(maximum))? as u32)
            } else {
                None
            };
            p.track_face = self.checked(441);
            p.stop_on_face_loss = self.checked(444);
            p.face_timeout_ms = self.number(442, 100, 59_000)?;
            p.detection_interval_ms = self.number(443, 50, 2000)?;
            p.fast_bar = self.selected(450) == 1;
            p.thin_bar = self.selected(451) == 1;
            p.bar_position = self.selected(452) as u8;
            p.show_bar = true;
            p.high_contrast_bar = self.checked(466);
            p.keyboard_command = text(GetDlgItem(self.options, 455));
            p.enabled_at_startup = Some(self.checked(459));
            let key = |id| {
                if self.selected(id) == 0 {
                    0
                } else {
                    self.selected(id) as u32 + 0x6F
                }
            };
            p.toggle_key = key(456);
            p.center_key = key(457);
            p.workspace_key = key(458);
            p.full_pause_key = key(467);
            p.arrow_sensitivity = self.checked(462);
            p.event_delay_ms = self.number(464, 0, 50)?;
            let hotkeys = [
                p.toggle_key,
                p.center_key,
                p.workspace_key,
                p.full_pause_key,
            ];
            for i in 0..hotkeys.len() {
                for j in i + 1..hotkeys.len() {
                    if hotkeys[i] != 0 && hotkeys[i] == hotkeys[j] {
                        return Err("Choose a different hotkey for each action.".into());
                    }
                }
            }
            if p.keyboard_command.trim().is_empty() {
                return Err("Enter an on-screen keyboard executable.".into());
            }
            Ok(p)
        })();
        match result {
            Ok(profile) => {
                if !self.dry_run
                    && let Err(e) = set_login(self.checked(461))
                {
                    message(
                        self.options,
                        &format!("Could not change startup setting: {e}"),
                    );
                    return false;
                }
                self.release_all();
                *self.config.profile_mut() = profile;
                self.config.minimized_at_startup = self.checked(460);
                self.config.check_updates = self.checked(463);
                self.config.language = if self.selected(465) == 1 { "it" } else { "en" }.into();
                true
            }
            Err(error) => {
                message(self.options, &error);
                false
            }
        }
    }

    unsafe fn finish_manual_roi(&mut self, end: (i32, i32)) {
        let Some(start) = self.manual_start.take() else {
            return;
        };
        if self.config.profile().track_face {
            return;
        }
        let mut r: RECT = zeroed();
        GetClientRect(GetDlgItem(self.hwnd, PREVIEW as i32), &mut r);
        let left = start.0.min(end.0).max(0);
        let top = start.1.min(end.1).max(0);
        let width = (start.0 - end.0).abs().min(r.right - left);
        let height = (start.1 - end.1).abs().min(r.bottom - top);
        if width < 20 || height < 20 || r.right <= 0 || r.bottom <= 0 {
            return;
        }
        self.config.profile_mut().manual_area = [
            left as f32 / r.right as f32,
            top as f32 / r.bottom as f32,
            width as f32 / r.right as f32,
            height as f32 / r.bottom as f32,
        ];
        self.save();
        self.start_camera();
    }
}

unsafe fn login_enabled() -> bool {
    startup::enabled()
}
unsafe fn set_login(enabled: bool) -> Result<(), String> {
    startup::configure(enabled)
}

impl App {
    unsafe fn open_wizard(&mut self) {
        if !self.wizard.is_null() {
            SetForegroundWindow(self.wizard);
            return;
        }
        self.release_all();
        self.wizard_step = 0;
        self.calibration_started = 0;
        self.wizard = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            wide(APP_CLASS).as_ptr(),
            wide("robocam — Setup").as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            scale(400, self.dpi_for(self.hwnd)),
            scale(300, self.dpi_for(self.hwnd)),
            self.hwnd,
            null_mut(),
            GetModuleHandleW(null()),
            self as *mut Self as *const c_void,
        );
        self.prepare_window(self.wizard, 400, 300);
        self.child(self.wizard, "STATIC", "", 330, 0, [20, 18, 350, 172]);
        self.child(self.wizard, "STATIC", "", 331, 0, [20, 192, 350, 22]);
        self.child(
            self.wizard,
            "BUTTON",
            "Next",
            NEXT,
            WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
            [254, 218, 112, 28],
        );
        self.wizard_text();
        ShowWindow(self.hwnd, SW_RESTORE);
        ShowWindow(self.wizard, SW_SHOW);
    }
    unsafe fn wizard_text(&self) {
        let description = match self.wizard_step {
            0 => {
                "Welcome to robocam\n\nPosition your camera in front of you with good, even lighting. Your face should occupy roughly one third of the preview width.\n\nThis setup measures your comfortable head movement to adjust pointer speed. Head control is paused during calibration."
            }
            1 => {
                "Frame your face\n\nLook at the live camera preview. Center your face until a tracking rectangle appears.\n\nNext, you will have 8 seconds to move your head slowly LEFT and RIGHT over a comfortable range. Keep your shoulders still."
            }
            2 => {
                "Horizontal calibration\n\nMove your head slowly LEFT and RIGHT over a comfortable range. Do not force the movement.\n\nWait until the measurement completes. Next you will move UP and DOWN for 8 seconds."
            }
            3 => {
                "Vertical calibration\n\nMove your head slowly UP and DOWN over a comfortable range.\n\nWait until the measurement completes, then choose Next to review your settings."
            }
            _ => {
                "Ready\n\nYour pointer speeds have been adjusted.\n\nUse the enable button to start or pause head control. Move your head to move the pointer; hold still to click.\n\nYou can tune speed, smoothing and dwell time in Configuration at any time."
            }
        };
        set_text(GetDlgItem(self.wizard, 330), description);
        set_text(GetDlgItem(self.wizard, 331), "");
        set_text(
            GetDlgItem(self.wizard, NEXT as i32),
            if self.wizard_step == 4 {
                "Finish"
            } else {
                "Next"
            },
        );
    }
    unsafe fn start_measurement(&mut self) {
        self.release_all();
        self.motion.reset();
        self.calibration_started = now();
        self.calibration_position = (0.0, 0.0);
        self.calibration_min = (0.0, 0.0);
        self.calibration_max = (0.0, 0.0);
        EnableWindow(GetDlgItem(self.wizard, NEXT as i32), 0);
        self.wizard_text();
    }
    unsafe fn wizard_next(&mut self) {
        match self.wizard_step {
            0 => {
                self.wizard_step = 1;
                self.wizard_text();
            }
            1 => {
                if !self.tracking_ready() {
                    message(
                        self.wizard,
                        "Wait until head tracking is ready in the preview before starting.",
                    );
                    return;
                }
                self.wizard_step = 2;
                self.start_measurement();
            }
            2 => {
                if self.calibration_x < 4.0 {
                    self.start_measurement();
                } else {
                    self.wizard_step = 3;
                    self.start_measurement();
                }
            }
            3 => {
                if self.calibration_y < 4.0 {
                    self.start_measurement();
                    return;
                }
                let desktop = screen();
                let speed = |distance: f32, extent: i32| {
                    (6.0 * (extent as f32 / (distance * 1.6)).ln())
                        .round()
                        .clamp(1.0, 30.0) as u8
                };
                let sx = speed(self.calibration_x, desktop.width);
                let sy = speed(self.calibration_y, desktop.height);
                self.config.profile_mut().motion.speed_x = sx;
                self.config.profile_mut().motion.speed_y = sy;
                self.wizard_step = 4;
                self.wizard_text();
            }
            _ => {
                self.config.wizard_completed = true;
                self.save();
                self.destroy_owned(self.wizard);
                self.wizard = null_mut();
            }
        }
    }
    unsafe fn calibrate_frame(&mut self, dx: f32, dy: f32, tracking: bool) {
        if tracking {
            self.calibration_position.0 += dx;
            self.calibration_position.1 += dy;
            self.calibration_min.0 = self.calibration_min.0.min(self.calibration_position.0);
            self.calibration_min.1 = self.calibration_min.1.min(self.calibration_position.1);
            self.calibration_max.0 = self.calibration_max.0.max(self.calibration_position.0);
            self.calibration_max.1 = self.calibration_max.1.max(self.calibration_position.1);
        }
        if now().saturating_sub(self.calibration_started) < 8000 {
            return;
        }
        self.calibration_started = 0;
        let range = if self.wizard_step == 2 {
            self.calibration_max.0 - self.calibration_min.0
        } else {
            self.calibration_max.1 - self.calibration_min.1
        };
        if self.wizard_step == 2 {
            self.calibration_x = range;
        } else {
            self.calibration_y = range;
        }
        set_text(
            GetDlgItem(self.wizard, 330),
            if range < 4.0 {
                "Not enough movement was measured.\n\nCheck that your face is visible and well lit. Choose Retry and move your head slowly over a comfortable range."
            } else {
                "Measurement complete.\n\nChoose Next to continue."
            },
        );
        set_text(
            GetDlgItem(self.wizard, NEXT as i32),
            if range < 4.0 { "Retry" } else { "Next" },
        );
        EnableWindow(GetDlgItem(self.wizard, NEXT as i32), 1);
    }

    unsafe fn refresh_wizard_tracking(&self) {
        if self.wizard.is_null() || self.wizard_step != 1 {
            return;
        }
        let ready = self.tracking_ready();
        let label = if ready {
            "Head tracking ready"
        } else {
            "Waiting for reliable head tracking…"
        };
        let status = GetDlgItem(self.wizard, 331);
        if text(status) != crate::i18n::translate(label) {
            set_text(status, label);
        }
        EnableWindow(GetDlgItem(self.wizard, NEXT as i32), i32::from(ready));
    }

    unsafe fn poll_updates(&mut self) {
        let Some(result) = self
            .update_receiver
            .as_ref()
            .and_then(|receiver| receiver.try_recv().ok())
        else {
            return;
        };
        self.update_receiver = None;
        let description = match result {
            UpdateResult::NewRelease { version, url } => {
                let description = format!("robocam {version} is available.\n\n{url}");
                set_text(self.hwnd, &format!("robocam — update {version} available"));
                self.available_release = Some((version, url));
                description
            }
            UpdateResult::Current => "You are running the latest published version.".into(),
            UpdateResult::NotPublished => "No robocam release has been published yet.".into(),
            UpdateResult::Failed(error) => format!("Could not check for updates: {error}"),
        };
        if self.manual_update {
            self.manual_update = false;
            let url = self
                .available_release
                .as_ref()
                .map_or(updates::RELEASES_URL, |(_, url)| url.as_str());
            messages::show_releases(self.hwnd, &description, url);
        }
    }
}
