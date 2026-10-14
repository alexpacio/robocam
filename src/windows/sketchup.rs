use super::*;

fn sequence(
    start: u64,
    count: u8,
    press: Option<u64>,
    double_click_ms: u32,
) -> impl Iterator<Item = (u64, MouseButton, bool)> {
    let gap = u64::from((double_click_ms / 2).clamp(20, 60));
    click_sequence(
        start,
        MouseButton::Left,
        count,
        press.unwrap_or(10),
        gap.saturating_sub(10),
    )
}

impl App {
    pub(super) unsafe fn sketchup_click(&mut self, count: u8) {
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
                "Use a SketchUp action over geometry in the SketchUp model view.",
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
        self.pending_input.extend(sequence(
            start,
            count,
            self.config.profile().click.press_ms,
            GetDoubleClickTime(),
        ));
        self.update_click_cursor();
        self.play_click_sound(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn triple_extends_the_same_paced_double_sequence() {
        for press in [None, Some(0), Some(120)] {
            let double: Vec<_> = sequence(40, 2, press, 500).collect();
            let triple: Vec<_> = sequence(40, 3, press, 500).collect();
            assert_eq!(double, triple[..4]);
            assert_eq!(triple.len(), 6);
            assert_eq!(triple[4].0 - triple[3].0, 50);
            assert_eq!(triple[5].0 - triple[4].0, press.unwrap_or(10));
        }
    }
}
