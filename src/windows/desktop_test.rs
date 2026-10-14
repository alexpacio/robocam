//! Opt-in interactive Shell acceptance check; ordinary CI smoke tests do not
//! rearrange the desktop. Run with --desktop-test --smoke-report <path>.
use super::*;
use ::windows::Win32::{
    System::Com::{
        CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
    },
    UI::Shell::{IShellDispatch4, Shell},
};

unsafe fn toggle_desktop() -> ::windows::core::Result<()> {
    CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
    let result = (|| {
        let shell: IShellDispatch4 = CoCreateInstance(&Shell, None, CLSCTX_ALL)?;
        shell.ToggleDesktop()
    })();
    CoUninitialize();
    result
}

impl App {
    pub(super) unsafe fn verify_desktop_transition(&mut self) {
        let elapsed = self.smoke_started.elapsed().as_millis();
        match self.desktop_test_stage {
            Some((0, _)) if elapsed >= 300 => {
                toggle_desktop().expect("Shell Show Desktop failed");
                self.desktop_test_stage = Some((1, elapsed));
            }
            Some((1, started)) if elapsed >= started + 900 => {
                let mut bar: RECT = zeroed();
                GetWindowRect(self.bar, &mut bar);
                let top = GetAncestor(
                    WindowFromPoint(POINT {
                        x: bar.left + scale(20, self.dpi_for(self.bar)),
                        y: bar.top + scale(20, self.dpi_for(self.bar)),
                    }),
                    GA_ROOT,
                );
                let visible = IsWindowVisible(self.bar) != 0 && IsIconic(self.bar) == 0;
                let main_hidden = IsWindowVisible(self.hwnd) == 0;
                // Restore the user's desktop before reporting an assertion.
                toggle_desktop().expect("Shell restore desktop failed");
                self.desktop_test_stage = Some((2, elapsed));
                self.show_main();
                assert!(visible, "Show Desktop hid the toolbar");
                assert_eq!(top, self.bar, "the desktop covers the toolbar");
                assert!(main_hidden, "the main window did not minimize to tray");
                assert_ne!(IsWindowVisible(self.hwnd), 0, "toolbar restore failed");
            }
            _ => {}
        }
    }
}
