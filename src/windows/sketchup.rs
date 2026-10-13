use super::*;

impl App {
    pub(super) unsafe fn sketchup_edit(&mut self) {
        let position = cursor();
        let target = GetAncestor(
            WindowFromPoint(POINT {
                x: position.0,
                y: position.1,
            }),
            GA_ROOT,
        );
        let mut process_id = 0;
        GetWindowThreadProcessId(target, &mut process_id);
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id);
        if process.is_null() {
            return;
        }
        let mut path = vec![0u16; 32768];
        let mut length = path.len() as u32;
        let found = QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut length) != 0;
        CloseHandle(process);
        let is_sketchup = found
            && String::from_utf16_lossy(&path[..length as usize])
                .rsplit(['\\', '/'])
                .next()
                .is_some_and(|name| name.eq_ignore_ascii_case("SketchUp.exe"));
        if !is_sketchup {
            message(
                self.hwnd,
                "Use the SketchUp button over a group or component in the SketchUp model view.",
            );
            return;
        }
        if GetForegroundWindow() != target {
            SetForegroundWindow(target);
        }
        if GetForegroundWindow() != target {
            return;
        }
        if self.dry_run {
            return;
        }
        let key = |flags| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_SPACE,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: INPUT_TAG,
                },
            },
        };
        let inputs = [key(0), key(KEYEVENTF_KEYUP)];
        let sent = SendInput(2, inputs.as_ptr(), size_of::<INPUT>() as i32);
        self.input_sent.set(self.input_sent.get() + u64::from(sent));
        if sent != 2 {
            let up = key(KEYEVENTF_KEYUP);
            SendInput(1, &up, size_of::<INPUT>() as i32);
            self.input_failed
                .set(self.input_failed.get() + u64::from(2 - sent));
            return;
        }
        self.click_actions += 1;
        self.pending_target = Some(target);
        let start = now() + 40;
        let gap = u64::from((GetDoubleClickTime() / 2).clamp(20, 60));
        for time in [start, start + gap] {
            self.pending_input
                .push_back((time, MouseButton::Left, true));
            self.pending_input
                .push_back((time + 10, MouseButton::Left, false));
        }
    }
}
