//! Modeless updater UI: the GUI keeps processing head movement during network work.
use super::*;
use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};
use updates::{DownloadEvent, ReleaseInstaller, UpdateResult};

const CLASS: &str = "robocam.updater";

/// The update handoff records the live window state. It takes precedence over
/// the normal login preference, including when upgrading from the 0.7.0 helper.
fn restart_minimized(report: &std::path::Path) -> Option<bool> {
    let bytes = std::fs::read(report.parent()?.join("restart-state.json")).ok()?;
    let state: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    state["minimized"].as_bool()
}

pub(super) fn start_minimized(
    explicit: bool,
    preference: bool,
    report: Option<&std::path::Path>,
) -> bool {
    explicit || report.and_then(restart_minimized).unwrap_or(preference)
}
thread_local! {
    static WINDOW: std::cell::Cell<HWND> = const { std::cell::Cell::new(null_mut()) };
    static COMMAND: std::cell::Cell<u16> = const { std::cell::Cell::new(0) };
}

#[derive(Default)]
pub(super) struct Updater {
    checking: Option<Receiver<UpdateResult>>,
    downloading: Option<Receiver<DownloadEvent>>,
    cancel: Option<Arc<AtomicBool>>,
    release: Option<ReleaseInstaller>,
    handoff: Option<(PathBuf, Instant)>,
    handed_off: bool,
    interactive: bool,
    pub result: Option<PathBuf>,
}

impl Drop for Updater {
    fn drop(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        if !self.handed_off
            && let Some((directory, _)) = &self.handoff
        {
            let _ = std::fs::write(directory.join("cancel"), b"");
        }
    }
}

impl Updater {
    pub unsafe fn check(&mut self, owner: HWND, manual: bool) {
        self.interactive |= manual;
        if self.downloading.is_some() || self.handoff.is_some() {
            if manual {
                show_existing();
            }
            return;
        }
        if self.checking.is_none() {
            self.release = None;
            self.checking = Some(updates::check_for_updates(env!("CARGO_PKG_VERSION")));
        }
        if manual {
            panel(owner, "Checking for updates…", "Check again", false, 0);
        }
    }

    fn cancel(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        if let Some((directory, _)) = self.handoff.take() {
            let _ = std::fs::write(directory.join("cancel"), b"");
        }
        self.downloading = None;
        self.checking = None;
        self.interactive = false;
    }

    pub unsafe fn error(&mut self, owner: HWND, error: &str) {
        self.handoff = None;
        self.interactive = true;
        panel(
            owner,
            &format!(
                "{}\n\n{error}",
                crate::i18n::translate("Update failed. Robocam remains available.")
            ),
            "Retry",
            true,
            0,
        );
    }

    /// Return a verified file only after the user chose Download and install.
    pub unsafe fn poll(&mut self, owner: HWND) -> Option<updates::VerifiedInstaller> {
        let command = COMMAND.with(|value| value.replace(0));
        if command == 2 {
            self.cancel();
        }
        if command == 1 && self.downloading.is_none() && self.handoff.is_none() {
            if let Some(release) = self.release.clone() {
                match update_install::staging_directory(&release.version) {
                    Ok(directory) => {
                        let cancel = Arc::new(AtomicBool::new(false));
                        self.downloading = Some(updates::download_installer(
                            release,
                            directory,
                            cancel.clone(),
                        ));
                        self.cancel = Some(cancel);
                        panel(
                            owner,
                            "Downloading and verifying the update…",
                            "Download and install",
                            false,
                            0,
                        );
                    }
                    Err(error) => self.error(owner, &error),
                }
            } else {
                self.check(owner, true);
            }
        }
        if let Some(result) = self.checking.as_ref().and_then(|r| r.try_recv().ok()) {
            self.checking = None;
            match result {
                UpdateResult::NewRelease(release) => {
                    set_text(
                        owner,
                        &format!("robocam — update {} available", release.version),
                    );
                    // A startup check announces availability in the title. Only
                    // an explicit check opens controls beneath the head pointer.
                    let text = format!(
                        "robocam {}\n\n{}",
                        release.version,
                        crate::i18n::translate(
                            "An update is available. Download and install it now?\n\nRobocam will restart automatically. Profiles and saved settings will be retained."
                        )
                    );
                    if self.interactive {
                        panel(owner, &text, "Download and install", true, 0);
                    }
                    self.release = Some(release);
                }
                UpdateResult::Current if self.interactive => panel(
                    owner,
                    "You are running the latest published version.",
                    "Check again",
                    true,
                    0,
                ),
                UpdateResult::NotPublished if self.interactive => panel(
                    owner,
                    "No robocam release has been published yet.",
                    "Check again",
                    true,
                    0,
                ),
                UpdateResult::Failed(error) if self.interactive => self.error(owner, &error),
                _ => {}
            }
        }
        while let Some(event) = self.downloading.as_ref().and_then(|r| r.try_recv().ok()) {
            match event {
                DownloadEvent::Progress(percent) => {
                    let text = format!(
                        "{} {percent}%",
                        crate::i18n::translate("Downloading and verifying the update…")
                    );
                    panel(owner, &text, "Download and install", false, percent);
                }
                DownloadEvent::Ready(installer) => {
                    self.downloading = None;
                    self.cancel = None;
                    panel(
                        owner,
                        "Preparing the update. Head control stays active until installation begins…",
                        "Download and install",
                        false,
                        100,
                    );
                    return Some(installer);
                }
                DownloadEvent::Failed(error) => {
                    self.downloading = None;
                    self.cancel = None;
                    self.error(owner, &error);
                }
                DownloadEvent::Cancelled => {
                    self.downloading = None;
                    self.cancel = None;
                }
            }
        }
        if let Some(path) = &self.result
            && let Ok(bytes) = std::fs::read(path)
            && let Ok(result) = serde_json::from_slice::<serde_json::Value>(&bytes)
        {
            let state = result["state"].as_str().unwrap_or("");
            if matches!(state, "complete" | "failed" | "restart_required") {
                let text = match state {
                    "complete" => crate::i18n::translate(
                        "Update installed. Profiles and settings have been retained.",
                    ),
                    "restart_required" => crate::i18n::translate(
                        "Windows must be restarted to finish the update. Robocam will not restart Windows automatically.",
                    ),
                    _ => format!(
                        "{}\n\n{}",
                        crate::i18n::translate(
                            "Update failed. The previous installation was restored when possible."
                        ),
                        result["error"].as_str().unwrap_or("")
                    )
                    .into(),
                };
                panel(owner, &text, "Check again", true, 0);
                self.result = None;
            }
        }
        None
    }

    pub fn wait_for_helper(&mut self, directory: PathBuf) {
        self.handoff = Some((directory, Instant::now()));
    }

    pub unsafe fn ready_to_exit(
        &mut self,
        owner: HWND,
        enabled: bool,
        full_paused: bool,
        minimized: bool,
    ) -> bool {
        let Some((directory, started)) = &self.handoff else {
            return false;
        };
        let status = std::fs::read(directory.join("handoff.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok());
        if status.as_ref().is_some_and(|v| v["state"] == "ready") {
            let state = serde_json::json!({"enabled":enabled,"full_paused":full_paused,"minimized":minimized});
            if let Err(error) = std::fs::write(
                directory.join("restart-state.json"),
                serde_json::to_vec(&state).unwrap(),
            ) {
                self.cancel();
                self.error(owner, &error.to_string());
                return false;
            }
            self.handed_off = true;
            return true;
        }
        if let Some(status) = status
            && status["state"] == "failed"
        {
            self.error(
                owner,
                status["error"].as_str().unwrap_or("Update helper failed."),
            );
        } else if started.elapsed().as_secs() > 60 {
            let _ = std::fs::write(directory.join("cancel"), b"");
            self.error(
                owner,
                "The update helper did not become ready. Nothing was installed.",
            );
        }
        false
    }
}

pub(super) unsafe fn control_at(position: (i32, i32)) -> Option<(HWND, u16)> {
    let window = WINDOW.with(|w| w.get());
    if window.is_null() {
        return None;
    }
    let hit = WindowFromPoint(POINT {
        x: position.0,
        y: position.1,
    });
    [1u16, 2]
        .into_iter()
        .find(|id| hit == GetDlgItem(window, i32::from(*id)) && IsWindowEnabled(hit) != 0)
        .map(|id| (window, id))
}

/// Native smoke coverage without network requests or an actual installer.
pub(super) unsafe fn verify(owner: HWND) {
    let mut updater = Updater::default();
    let (sender, receiver) = std::sync::mpsc::channel();
    updater.interactive = true;
    updater.checking = Some(receiver);
    sender.send(UpdateResult::Current).unwrap();
    assert!(updater.poll(owner).is_none());
    let window = WINDOW.with(|window| window.get());
    assert!(!window.is_null() && IsWindowVisible(window) != 0);
    assert_ne!(IsWindowEnabled(GetDlgItem(window, 1)), 0);
    let cancel = Arc::new(AtomicBool::new(false));
    let (sender, receiver) = std::sync::mpsc::channel();
    updater.downloading = Some(receiver);
    updater.cancel = Some(cancel.clone());
    sender.send(DownloadEvent::Progress(37)).unwrap();
    assert!(updater.poll(owner).is_none());
    assert_eq!(SendMessageW(GetDlgItem(window, 101), PBM_GETPOS, 0, 0), 37);
    assert_eq!(IsWindowEnabled(GetDlgItem(window, 1)), 0);
    SendMessageW(window, WM_COMMAND, 2, 0);
    assert!(updater.poll(owner).is_none());
    assert!(cancel.load(Ordering::Relaxed));
    assert!(updater.downloading.is_none());
    assert_eq!(IsWindow(window), 0);
    updater.error(owner, "Installer checksum mismatch. Nothing was installed.");
    let window = WINDOW.with(|window| window.get());
    assert_ne!(IsWindowEnabled(GetDlgItem(window, 1)), 0);
    SendMessageW(window, WM_CLOSE, 0, 0);
    assert!(updater.poll(owner).is_none());
}

unsafe fn show_existing() {
    let window = WINDOW.with(|w| w.get());
    if !window.is_null() {
        ShowWindow(window, SW_SHOWNORMAL);
        SetForegroundWindow(window);
    }
}

pub(super) unsafe fn route(msg: &MSG) -> bool {
    let window = WINDOW.with(|w| w.get());
    !window.is_null() && IsDialogMessageW(window, msg) != 0
}

unsafe fn panel(owner: HWND, content: &str, primary: &str, enabled: bool, percent: u8) {
    let mut window = WINDOW.with(|w| w.get());
    if window.is_null() {
        let instance = GetModuleHandleW(null());
        let class = wide_raw(CLASS);
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: (COLOR_BTNFACE + 1) as HBRUSH,
            ..zeroed()
        });
        let dpi = GetDpiForWindow(owner).max(96);
        let mut bounds: RECT = zeroed();
        GetWindowRect(owner, &mut bounds);
        window = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            class.as_ptr(),
            wide("robocam — Updates").as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
            bounds.left,
            bounds.top,
            scale(570, dpi),
            scale(300, dpi),
            owner,
            null_mut(),
            instance,
            null(),
        );
        if window.is_null() {
            return;
        }
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
            wide_raw("Segoe UI").as_ptr(),
        );
        SetWindowLongPtrW(window, GWLP_USERDATA, font as isize);
        for (id, class, label, style, rect) in [
            (
                100,
                "EDIT",
                "",
                ES_MULTILINE as u32 | ES_READONLY as u32 | WS_VSCROLL,
                [16, 14, 522, 170],
            ),
            (101, "msctls_progress32", "", 0, [16, 192, 522, 16]),
            (1, "BUTTON", "", WS_TABSTOP, [16, 220, 310, 30]),
            (
                2,
                "BUTTON",
                "Cancel",
                WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
                [348, 220, 190, 30],
            ),
        ] {
            let control = CreateWindowExW(
                0,
                wide_raw(class).as_ptr(),
                wide(label).as_ptr(),
                WS_CHILD | WS_VISIBLE | style,
                scale(rect[0], dpi),
                scale(rect[1], dpi),
                scale(rect[2], dpi),
                scale(rect[3], dpi),
                window,
                id as usize as HMENU,
                instance,
                null(),
            );
            SendMessageW(control, WM_SETFONT, font as usize, 1);
        }
        WINDOW.with(|w| w.set(window));
        placement::ensure(window);
        ShowWindow(window, SW_SHOWNOACTIVATE);
    }
    let content = crate::i18n::translate(content)
        .replace("\r\n", "\n")
        .replace('\n', "\r\n");
    SetWindowTextW(GetDlgItem(window, 100), wide_raw(&content).as_ptr());
    set_text(GetDlgItem(window, 1), primary);
    EnableWindow(GetDlgItem(window, 1), i32::from(enabled));
    SendMessageW(GetDlgItem(window, 101), PBM_SETPOS, usize::from(percent), 0);
}

unsafe extern "system" fn procedure(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND if matches!(wp & 0xffff, 1 | 2) && wp >> 16 == 0 => {
            COMMAND.with(|value| value.set(wp as u16));
            if wp == 2 {
                DestroyWindow(hwnd);
            } else {
                EnableWindow(GetDlgItem(hwnd, 1), 0);
            }
            0
        }
        WM_CLOSE => {
            COMMAND.with(|value| value.set(2));
            DestroyWindow(hwnd);
            0
        }
        WM_NCDESTROY => {
            let font = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as HFONT;
            if !font.is_null() {
                DeleteObject(font);
            }
            WINDOW.with(|w| w.set(null_mut()));
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn update_restart_uses_live_visibility_and_handles_legacy_or_invalid_state() {
        let root =
            std::env::temp_dir().join(format!("robocam-update-visibility-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let state = root.join("restart-state.json");
        let report = root.join("result.json");
        for minimized in [false, true] {
            std::fs::write(
                &state,
                serde_json::json!({"minimized":minimized}).to_string(),
            )
            .unwrap();
            assert_eq!(restart_minimized(&report), Some(minimized));
            // The live state wins even when the saved startup preference differs.
            assert_eq!(start_minimized(false, !minimized, Some(&report)), minimized);
            assert!(start_minimized(true, false, Some(&report)));
        }
        for invalid in ["{}", "invalid", r#"{"minimized":"false"}"#] {
            std::fs::write(&state, invalid).unwrap();
            assert_eq!(restart_minimized(&report), None);
        }
        std::fs::remove_file(&state).unwrap();
        assert_eq!(restart_minimized(&report), None);
        assert!(start_minimized(false, true, Some(&report)));
        assert!(!start_minimized(false, false, None));
        std::fs::remove_dir(&root).unwrap();
    }
}
