//! The desktop guard keeps other applications' windows out of the space reserved
//! for the docked click bar, and releases a pointer confinement (ClipCursor) left
//! by another application while head control is active. It runs on its own
//! thread: window events are checked shortly after they happen, and everything
//! is swept every 2 seconds for windows the events missed.
use super::*;
use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{Arc, Mutex, mpsc},
    thread::JoinHandle,
    time::Duration,
};
use windows_sys::Win32::UI::Accessibility::*;

const SWEEP: Duration = Duration::from_secs(2);
/// Let a window finish appearing or animating before correcting it.
const SETTLE_MS: u32 = 150;
/// A window moved back this often within `REPEAT_WINDOW` is left alone for a while.
const REPEATS: usize = 3;
const REPEAT_WINDOW: Duration = Duration::from_secs(10);
const SUSPEND: Duration = Duration::from_secs(60);
const RECENT: usize = 20;

/// Shell surfaces and transient popups that legitimately sit at screen edges
/// or vanish on their own.
const EXCLUDED_CLASSES: [&str; 16] = [
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "#32768",
    "tooltips_class32",
    "SysShadow",
    "ComboLBox",
    "Ghost",
    "Windows.UI.Core.CoreWindow",
    "XamlExplorerHostIslandWindow",
    "ForegroundStaging",
    "MultitaskingViewFrame",
    "TaskListThumbnailWnd",
    "IME",
    "MSCTFIME UI",
];

#[derive(Default)]
struct Shared {
    /// The docked bar's rectangle; None while it floats or is hidden.
    bar: Option<RECT>,
    /// Head control is moving the pointer.
    active: bool,
    windows_moved: u64,
    recent_moves: VecDeque<serde_json::Value>,
    clip_releases: u64,
    last_clip_release: Option<serde_json::Value>,
}

pub(super) struct DesktopGuard {
    shared: Arc<Mutex<Shared>>,
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}
impl DesktopGuard {
    pub fn start() -> Option<Self> {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let (started, thread_id) = mpsc::channel();
        let state = shared.clone();
        let thread = std::thread::Builder::new()
            .name("desktop-guard".into())
            .spawn(move || unsafe { run(state, started) })
            .ok()?;
        let Ok(thread_id) = thread_id.recv() else {
            let _ = thread.join();
            return None;
        };
        Some(Self {
            shared,
            thread_id,
            thread: Some(thread),
        })
    }
    pub fn update(&self, bar: Option<RECT>, active: bool) {
        let mut shared = self.shared.lock().unwrap_or_else(|p| p.into_inner());
        shared.bar = bar;
        shared.active = active;
    }
    pub fn report(&self) -> serde_json::Value {
        let shared = self.shared.lock().unwrap_or_else(|p| p.into_inner());
        serde_json::json!({
            "windows_moved_from_bar":shared.windows_moved,
            "recent_moves":shared.recent_moves,
            "clip_releases":shared.clip_releases,
            "last_clip_release":shared.last_clip_release,
        })
    }
    pub fn stop(&mut self) {
        if let Some(thread) = self.thread.take() {
            unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0) };
            let _ = thread.join();
        }
    }
}
impl Drop for DesktopGuard {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Window events are delivered on the guard thread, through its message loop.
#[derive(Default)]
struct Events {
    dirty: Vec<HWND>,
    dragging: Vec<HWND>,
    settle_timer: usize,
}
thread_local! {
    static EVENTS: RefCell<Events> = RefCell::new(Events::default());
}

unsafe extern "system" fn on_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    object: i32,
    child: i32,
    _thread: u32,
    _time: u32,
) {
    if hwnd.is_null()
        || object != OBJID_WINDOW
        || child != CHILDID_SELF as i32
        || GetAncestor(hwnd, GA_ROOT) != hwnd
    {
        return;
    }
    EVENTS.with(|events| {
        let mut events = events.borrow_mut();
        match event {
            EVENT_SYSTEM_MOVESIZESTART => {
                events.dragging.push(hwnd);
                return;
            }
            EVENT_SYSTEM_MOVESIZEEND => events.dragging.retain(|window| *window != hwnd),
            _ => {}
        }
        if !events.dirty.contains(&hwnd) {
            events.dirty.push(hwnd);
        }
        if events.settle_timer == 0 {
            events.settle_timer = SetTimer(null_mut(), 0, SETTLE_MS, None);
        }
    });
}

unsafe fn run(shared: Arc<Mutex<Shared>>, started: mpsc::Sender<u32>) {
    // Create this thread's message queue before anyone posts to it.
    let mut message: MSG = zeroed();
    PeekMessageW(&mut message, null_mut(), WM_USER, WM_USER, PM_NOREMOVE);
    let hooks: Vec<HWINEVENTHOOK> = [
        (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
        (EVENT_SYSTEM_MOVESIZESTART, EVENT_SYSTEM_MOVESIZEEND),
        (EVENT_OBJECT_SHOW, EVENT_OBJECT_SHOW),
    ]
    .into_iter()
    .map(|(first, last)| {
        SetWinEventHook(
            first,
            last,
            null_mut(),
            Some(on_event),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        )
    })
    .collect();
    let sweep = SetTimer(null_mut(), 0, SWEEP.as_millis() as u32, None);
    let _ = started.send(GetCurrentThreadId());
    let mut guard = Guard {
        shared,
        own_process: GetCurrentProcessId(),
        history: HashMap::new(),
    };
    while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
        if message.message != WM_TIMER || !message.hwnd.is_null() {
            TranslateMessage(&message);
            DispatchMessageW(&message);
            continue;
        }
        if message.wParam == sweep {
            guard.sweep();
        } else {
            KillTimer(null_mut(), message.wParam);
            let (dirty, dragging) = EVENTS.with(|events| {
                let mut events = events.borrow_mut();
                events.settle_timer = 0;
                (std::mem::take(&mut events.dirty), events.dragging.clone())
            });
            for window in dirty.into_iter().filter(|w| !dragging.contains(w)) {
                guard.check(window);
            }
        }
    }
    KillTimer(null_mut(), sweep);
    for hook in hooks.into_iter().filter(|hook| !hook.is_null()) {
        UnhookWinEvent(hook);
    }
}

struct Guard {
    shared: Arc<Mutex<Shared>>,
    own_process: u32,
    /// Recent corrections per window, and until when it is left alone.
    history: HashMap<usize, (Vec<Instant>, Option<Instant>)>,
}
impl Guard {
    fn state(&self) -> (Option<RECT>, bool) {
        let shared = self.shared.lock().unwrap_or_else(|p| p.into_inner());
        (shared.bar, shared.active)
    }

    unsafe fn sweep(&mut self) {
        self.history
            .retain(|window, _| IsWindow(*window as HWND) != 0);
        self.release_clip();
        unsafe extern "system" fn collect(hwnd: HWND, windows: LPARAM) -> windows_sys::core::BOOL {
            (*(windows as *mut Vec<HWND>)).push(hwnd);
            1
        }
        let mut windows: Vec<HWND> = Vec::new();
        EnumWindows(Some(collect), &mut windows as *mut Vec<HWND> as LPARAM);
        let dragging = EVENTS.with(|events| events.borrow().dragging.clone());
        for window in windows.into_iter().filter(|w| !dragging.contains(w)) {
            self.check(window);
        }
    }

    /// Another application confining the pointer (for example while it hangs
    /// during a drag) must not trap head control inside part of the screen.
    unsafe fn release_clip(&mut self) {
        if !self.state().1 {
            return;
        }
        let mut clip: RECT = zeroed();
        let desktop = screen();
        if GetClipCursor(&mut clip) == 0
            || (clip.left <= desktop.left
                && clip.top <= desktop.top
                && clip.right >= desktop.left + desktop.width
                && clip.bottom >= desktop.top + desktop.height)
        {
            return;
        }
        if ClipCursor(null()) != 0 {
            let mut shared = self.shared.lock().unwrap_or_else(|p| p.into_inner());
            shared.clip_releases += 1;
            shared.last_clip_release = Some(serde_json::json!({
                "time_ms":now(),"clip":[clip.left,clip.top,clip.right,clip.bottom],
                "foreground":process_name(GetForegroundWindow()),
            }));
        }
    }

    unsafe fn check(&mut self, window: HWND) {
        let Some(bar) = self.state().0 else {
            return;
        };
        if !self.eligible(window) {
            return;
        }
        let mut frame: RECT = zeroed();
        if DwmGetWindowAttribute(
            window,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            (&mut frame as *mut RECT).cast(),
            size_of::<RECT>() as u32,
        ) != 0
            && GetWindowRect(window, &mut frame) == 0
        {
            return;
        }
        let mut overlap: RECT = zeroed();
        if IntersectRect(&mut overlap, &frame, &bar) == 0 {
            return;
        }
        let mut monitor = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..zeroed()
        };
        if GetMonitorInfoW(
            MonitorFromRect(&frame, MONITOR_DEFAULTTONEAREST),
            &mut monitor,
        ) == 0
            || frame_covers(frame, monitor.rcMonitor)
        {
            return;
        }
        let target = placement::avoid_bar(frame, monitor.rcWork, Some(bar));
        let rectangle = |r: RECT| [r.left, r.top, r.right, r.bottom];
        if rectangle(target) == rectangle(frame) || !self.allow(window) {
            return;
        }
        let mut outer: RECT = zeroed();
        GetWindowRect(window, &mut outer);
        let resizable = GetWindowLongPtrW(window, GWL_STYLE) as u32 & WS_THICKFRAME != 0;
        let (grow_x, grow_y) = (
            (target.right - target.left) - (frame.right - frame.left),
            (target.bottom - target.top) - (frame.bottom - frame.top),
        );
        let resize = resizable && (grow_x, grow_y) != (0, 0);
        // Asynchronous: a window whose thread stops responding never blocks the guard.
        SetWindowPos(
            window,
            null_mut(),
            outer.left + target.left - frame.left,
            outer.top + target.top - frame.top,
            outer.right - outer.left + grow_x,
            outer.bottom - outer.top + grow_y,
            SWP_NOZORDER
                | SWP_NOACTIVATE
                | SWP_NOOWNERZORDER
                | SWP_ASYNCWINDOWPOS
                | if resize { 0 } else { SWP_NOSIZE },
        );
        let mut class = [0u16; 128];
        let length = GetClassNameW(window, class.as_mut_ptr(), class.len() as i32);
        let mut shared = self.shared.lock().unwrap_or_else(|p| p.into_inner());
        shared.windows_moved += 1;
        if shared.recent_moves.len() == RECENT {
            shared.recent_moves.pop_front();
        }
        shared.recent_moves.push_back(serde_json::json!({
            "time_ms":now(),
            "class":String::from_utf16_lossy(&class[..length.max(0) as usize]),
            "process":process_name(window),
            "from":rectangle(frame),"to":rectangle(target),
        }));
    }

    unsafe fn eligible(&self, window: HWND) -> bool {
        let style = GetWindowLongPtrW(window, GWL_STYLE) as u32;
        let extended = GetWindowLongPtrW(window, GWL_EXSTYLE) as u32;
        let mut process = 0;
        GetWindowThreadProcessId(window, &mut process);
        let mut cloaked = 0u32;
        DwmGetWindowAttribute(
            window,
            DWMWA_CLOAKED as u32,
            (&mut cloaked as *mut u32).cast(),
            size_of::<u32>() as u32,
        );
        let mut class = [0u16; 128];
        let length = GetClassNameW(window, class.as_mut_ptr(), class.len() as i32);
        let class = String::from_utf16_lossy(&class[..length.max(0) as usize]);
        IsWindowVisible(window) != 0
            && IsIconic(window) == 0
            && IsZoomed(window) == 0
            && style & WS_CHILD == 0
            // Click-through overlays do not cover anything the user can reach.
            && extended & WS_EX_TRANSPARENT == 0
            && cloaked == 0
            && process != self.own_process
            && !EXCLUDED_CLASSES.contains(&class.as_str())
            && IsHungAppWindow(window) == 0
    }

    /// Stop fighting a window that keeps moving itself back.
    fn allow(&mut self, window: HWND) -> bool {
        let now = Instant::now();
        let (moves, suspended) = self.history.entry(window as usize).or_default();
        if suspended.is_some_and(|until| now < until) {
            return false;
        }
        moves.retain(|time| now.duration_since(*time) < REPEAT_WINDOW);
        if moves.len() >= REPEATS {
            *suspended = Some(now + SUSPEND);
            moves.clear();
            return false;
        }
        moves.push(now);
        true
    }
}

/// Fullscreen windows (games, presentations, video) keep the whole monitor.
fn frame_covers(frame: RECT, monitor: RECT) -> bool {
    frame.left <= monitor.left
        && frame.top <= monitor.top
        && frame.right >= monitor.right
        && frame.bottom >= monitor.bottom
}

unsafe fn process_name(window: HWND) -> String {
    let mut id = 0;
    GetWindowThreadProcessId(window, &mut id);
    let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, id);
    if process.is_null() {
        return String::new();
    }
    let mut path = vec![0u16; 1024];
    let mut length = path.len() as u32;
    let found = QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut length) != 0;
    CloseHandle(process);
    if !found {
        return String::new();
    }
    String::from_utf16_lossy(&path[..length as usize])
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_moved_back_repeatedly_is_left_alone() {
        let mut guard = Guard {
            shared: Arc::default(),
            own_process: 0,
            history: HashMap::new(),
        };
        let window = 0x1234 as HWND;
        for _ in 0..REPEATS {
            assert!(guard.allow(window));
        }
        assert!(!guard.allow(window));
        assert!(!guard.allow(window));
        assert!(guard.allow(0x5678 as HWND));
    }

    #[test]
    fn fullscreen_windows_are_recognized() {
        let monitor = RECT {
            left: 0,
            top: 0,
            right: 3440,
            bottom: 1440,
        };
        assert!(frame_covers(monitor, monitor));
        assert!(!frame_covers(RECT { top: 70, ..monitor }, monitor));
    }
}
