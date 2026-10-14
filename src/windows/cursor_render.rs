//! The overlay and its visibility state belong to one dedicated thread. Camera
//! work, modal UI, and reentrant main-window messages cannot stop its movement or
//! run a stale timer which hides the native pointer after button-up.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize},
    mpsc,
};
use std::thread::JoinHandle;
use std::time::Duration;
use windows_sys::Win32::UI::Magnification::*;

struct Command {
    pressed: bool,
    reply: mpsc::SyncSender<()>,
}

pub(super) struct Worker {
    commands: Option<mpsc::Sender<Command>>,
    thread: Option<JoinHandle<()>>,
    active: Arc<AtomicBool>,
    window: Arc<AtomicUsize>,
}
impl Worker {
    pub fn start() -> Option<Self> {
        let (tx, rx) = mpsc::channel::<Command>();
        let active = Arc::new(AtomicBool::new(false));
        let window = Arc::new(AtomicUsize::new(0));
        let state = active.clone();
        let handle = window.clone();
        let thread = std::thread::Builder::new()
            .name("click-cursor".into())
            .spawn(move || unsafe {
                let Some(mut renderer) = Renderer::new() else {
                    return;
                };
                handle.store(renderer.hwnd as usize, Ordering::Release);
                let mut pressed = false;
                loop {
                    let mut message: MSG = zeroed();
                    while PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
                        TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                    match rx.recv_timeout(Duration::from_millis(8)) {
                        Ok(command) => {
                            pressed = command.pressed;
                            if command.pressed {
                                renderer.show();
                            } else {
                                renderer.hide();
                            }
                            state.store(renderer.hidden_native, Ordering::Release);
                            let _ = command.reply.send(());
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            if renderer.hidden_native && (!pressed || !renderer.follow()) {
                                pressed = false;
                                renderer.hide();
                            }
                            state.store(renderer.hidden_native, Ordering::Release);
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
                drop(renderer);
                state.store(false, Ordering::Release);
                handle.store(0, Ordering::Release);
            })
            .ok()?;
        Some(Self {
            commands: Some(tx),
            thread: Some(thread),
            active,
            window,
        })
    }
    pub fn active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }
    pub fn window(&self) -> HWND {
        self.window.load(Ordering::Acquire) as HWND
    }
    pub fn set(&self, pressed: bool) {
        let (reply, done) = mpsc::sync_channel(1);
        if self
            .commands
            .as_ref()
            .is_some_and(|tx| tx.send(Command { pressed, reply }).is_ok())
        {
            // Release completes before the caller continues. The render thread
            // does not call into any of the main thread's windows.
            if done.recv().is_err() {
                self.active.store(false, Ordering::Release);
            }
        } else {
            self.active.store(false, Ordering::Release);
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.commands.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Renderer {
    hwnd: HWND,
    hidden_native: bool,
    image: Option<(cursor_image::Surface, POINT)>,
    guard: Option<cursor_process::CursorProcess>,
}
impl Renderer {
    unsafe fn new() -> Option<Self> {
        if MagInitialize() == 0 {
            return None;
        }
        let class = wide_raw("robocam.click-cursor");
        let instance = GetModuleHandleW(null());
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..zeroed()
        });
        // No owner: minimizing the main window cannot hide the pressed cursor.
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class.as_ptr(),
            wide_raw("").as_ptr(),
            WS_POPUP,
            0,
            0,
            1,
            1,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            MagUninitialize();
            return None;
        }
        let excluded = 1i32;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_EXCLUDED_FROM_PEEK as u32,
            (&excluded as *const i32).cast(),
            size_of::<i32>() as u32,
        );
        Some(Self {
            hwnd,
            hidden_native: false,
            image: None,
            guard: None,
        })
    }

    unsafe fn show(&mut self) {
        if self.hidden_native {
            return;
        }
        // Re-read only on press so a changed Windows arrow shape is respected.
        self.image = cursor_image::arrow();
        let Some((image, hotspot)) = &self.image else {
            return;
        };
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
        if !self.guard.as_mut().is_some_and(|guard| guard.signal(true)) {
            return;
        }
        if !image.present(self.hwnd, *hotspot) || !self.follow() || DwmFlush() < 0 {
            self.hide();
            return;
        }
        // Only suppress native drawing once our opaque arrow is composed.
        // Its image, shape, role selection, and saved scheme stay untouched.
        self.hidden_native = MagShowSystemCursor(0) != 0;
        if !self.hidden_native {
            self.hide();
        }
    }

    unsafe fn follow(&self) -> bool {
        let Some((_, hotspot)) = &self.image else {
            return false;
        };
        let mut position: POINT = zeroed();
        if GetCursorPos(&mut position) == 0 || IsWindow(self.hwnd) == 0 {
            return false;
        }
        if self
            .guard
            .as_ref()
            .is_none_or(|guard| guard.exit_code().is_some())
        {
            return false;
        }
        if SetWindowPos(
            self.hwnd,
            HWND_TOPMOST,
            position.x - hotspot.x,
            position.y - hotspot.y,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        ) == 0
        {
            return false;
        }
        // Trusted system UI can be above the normal topmost band. Fall back to
        // the standard pointer instead of hiding it behind an occluded overlay.
        let target = GetAncestor(WindowFromPoint(position), GA_ROOT);
        let mut above = GetWindow(self.hwnd, GW_HWNDPREV);
        while !above.is_null() {
            if above == target {
                return false;
            }
            above = GetWindow(above, GW_HWNDPREV);
        }
        true
    }

    unsafe fn hide(&mut self) {
        // One thread owns this transition; there is no timer callback capable
        // of suppressing the pointer again while we restore it.
        if self.hidden_native && MagShowSystemCursor(1) == 0 {
            // Keep the drawn arrow until the next restoration attempt. Never
            // remove the only visible pointer just because restoration failed.
            return;
        }
        self.hidden_native = false;
        if let Some(guard) = &mut self.guard {
            guard.signal(false);
        }
        ShowWindow(self.hwnd, SW_HIDE);
        self.image = None;
    }
}
impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            self.hide();
            // Closing an armed pipe also restores visibility after API failure.
            drop(self.guard.take());
            DestroyWindow(self.hwnd);
            MagUninitialize();
        }
    }
}

unsafe extern "system" fn procedure(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST => HTTRANSPARENT as isize,
        WM_MOUSEACTIVATE => MA_NOACTIVATE as isize,
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
