//! Optional audio feedback for clicks and the start of a drag.
use super::*;
use windows_sys::Win32::Media::Audio::*;

const CLICK_SOUND: &[u8] = include_bytes!("../../assets/sounds/robocam-click.wav");
const DRAG_SOUND: &[u8] = include_bytes!("../../assets/sounds/robocam-drag.wav");

impl App {
    pub(super) unsafe fn play_click_sound(&self, drag_start: bool) {
        if self.config.profile().click_sound && !self.dry_run {
            let sound = if drag_start { DRAG_SOUND } else { CLICK_SOUND };
            PlaySoundW(
                sound.as_ptr().cast(),
                null_mut(),
                SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
            );
        }
    }
}
