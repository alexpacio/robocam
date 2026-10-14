//! Click feedback draws a temporary arrow; it never replaces Windows cursor roles.
use super::*;
use std::io::Read;
use windows_sys::Win32::{Media::Audio::*, UI::Magnification::*};

const CLICK_SOUND: &[u8] = include_bytes!("../../assets/sounds/eviacam-click.wav");
const DRAG_SOUND: &[u8] = include_bytes!("../../assets/sounds/eviacam-drag.wav");

pub(super) unsafe fn play_sound(drag_start: bool) {
    let sound = if drag_start { DRAG_SOUND } else { CLICK_SOUND };
    PlaySoundW(
        sound.as_ptr().cast(),
        null_mut(),
        SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
    );
}

/// Only restore visibility if our renderer dies during a press. This process
/// never loads/replaces cursor roles, resets the scheme, or injects mouse events.
pub(super) unsafe fn guardian() {
    let initialized = MagInitialize() != 0;
    let mut armed = false;
    let mut byte = [0u8];
    while std::io::stdin().read_exact(&mut byte).is_ok() {
        armed = byte[0] == 1;
    }
    if armed && initialized {
        MagShowSystemCursor(1);
    }
    if initialized {
        MagUninitialize();
    }
}

#[derive(Default)]
pub(super) struct ClickCursor {
    requested: bool,
    dry_run: bool,
    renderer: Option<cursor_render::Worker>,
}
impl ClickCursor {
    pub(super) fn active(&self) -> bool {
        if self.dry_run {
            self.requested
        } else {
            self.renderer.as_ref().is_some_and(|worker| worker.active())
        }
    }
    pub(super) fn window(&self) -> HWND {
        self.renderer
            .as_ref()
            .map_or(null_mut(), |worker| worker.window())
    }
    pub(super) unsafe fn update(&mut self, active: bool, dry_run: bool) {
        if self.requested == active && self.dry_run == dry_run {
            return;
        }
        self.requested = active;
        self.dry_run = dry_run;
        if dry_run {
            return;
        }
        if active && self.renderer.is_none() {
            self.renderer = cursor_render::Worker::start();
        }
        if let Some(worker) = &self.renderer {
            worker.set(active);
        }
    }
}

impl App {
    pub(super) unsafe fn update_click_cursor(&mut self) {
        // Pending clicks and the gaps between presses keep the normal pointer.
        let active =
            self.config.profile().black_cursor_on_click && self.injected.iter().any(|down| *down);
        self.click_cursor.update(active, self.dry_run);
    }
    pub(super) unsafe fn play_click_sound(&self, drag_start: bool) {
        if self.config.profile().click_sound && !self.dry_run {
            play_sound(drag_start);
        }
    }
}
