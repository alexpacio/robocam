//! Physical desktop coordinates, including the space occupied by appbars.
use super::*;

/// Address the centre of a physical pixel in SendInput's 16-bit virtual desktop.
/// Never normalize against rcWork: that would offset input by the appbar/taskbar.
pub(super) fn absolute_position(position: (i32, i32), desktop: ScreenRect) -> (i32, i32) {
    let axis = |value: i32, origin: i32, extent: i32| {
        let extent = i64::from(extent.max(1));
        let pixel = (i64::from(value) - i64::from(origin)).clamp(0, extent - 1);
        ((pixel * 65_536 + 32_768) / extent).clamp(0, 65_535) as i32
    };
    (
        axis(position.0, desktop.left, desktop.width),
        axis(position.1, desktop.top, desktop.height),
    )
}

impl App {
    pub(super) unsafe fn verify_bar_pointer_geometry(&mut self) {
        let mut bar: RECT = zeroed();
        assert_ne!(GetWindowRect(self.bar, &mut bar), 0);
        let mut monitor = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..zeroed()
        };
        assert_ne!(
            GetMonitorInfoW(
                MonitorFromWindow(self.bar, MONITOR_DEFAULTTONEAREST),
                &mut monitor
            ),
            0
        );
        if self.config.profile().bar_position != 4 {
            let mut intersection: RECT = zeroed();
            assert_eq!(
                IntersectRect(&mut intersection, &bar, &monitor.rcWork),
                0,
                "the click bar overlaps the area available to maximized windows"
            );
            assert_eq!(
                APPBAR_RECT.with(|r| r.get().map(|(_, r)| r)),
                Some([bar.left, bar.top, bar.right, bar.bottom])
            );
        }
        let saved_profile = self.config.profile().clone();
        let saved_click = std::mem::take(&mut self.click);
        let saved_enabled = self.enabled;
        let saved_full_paused = self.full_paused;
        self.full_paused = false;
        self.config.profile_mut().fast_bar = false;
        self.config.profile_mut().click.mode = ClickMode::Dwell;
        let actions = self.click_actions;
        let position = cursor();
        let input_sent = self.input_sent.get();
        let mut time = now();
        for (id, _) in bar_buttons(self.config.profile().show_sketchup_buttons) {
            self.enabled = true;
            let mut button: RECT = zeroed();
            GetWindowRect(GetDlgItem(self.bar, i32::from(id)), &mut button);
            let centre = (
                (button.left + button.right) / 2,
                (button.top + button.bottom) / 2,
            );
            assert_eq!(self.bar_control_at(centre), id);
            self.pointer_actions(time, MotionOutput::default(), centre, Some(centre));
            time += self.config.profile().click.dwell_ms;
            self.pointer_actions(time, MotionOutput::default(), centre, Some(centre));
            assert!(self.hover_done, "toolbar dwell did not activate {id}");
            assert_eq!(
                self.click_actions, actions,
                "toolbar dwell generated a desktop click"
            );
            assert!(self.pending_input.is_empty());
            assert_eq!(cursor(), position, "toolbar command moved the real pointer");
            time += 1;
        }
        // Regression: the requested position is in the bar, but Windows still
        // reports a point in the browser's tab strip underneath it. Neither
        // location may trigger an action until movement is confirmed.
        let requested = (bar.left + 10, bar.top + 10);
        self.full_paused = false;
        let actual = (bar.left + 10, bar.bottom + 10);
        self.enabled = true;
        for _ in 0..3 {
            self.pointer_actions(time, MotionOutput::default(), actual, Some(requested));
            time += self.config.profile().click.dwell_ms + 1;
            assert_eq!(self.hover_id, 0);
            assert!(!self.hover_done);
            assert_eq!(self.click_actions, actions);
        }
        assert!(self.inside_window(self.bar, (bar.left, bar.bottom - 1)));
        assert!(!self.inside_window(self.bar, (bar.left, bar.bottom)));
        let mut after: RECT = zeroed();
        GetWindowRect(self.bar, &mut after);
        assert_eq!(
            [bar.left, bar.top, bar.right, bar.bottom],
            [after.left, after.top, after.right, after.bottom]
        );
        assert_eq!(self.input_sent.get(), input_sent);
        self.click = saved_click;
        self.enabled = saved_enabled;
        self.full_paused = saved_full_paused;
        *self.config.profile_mut() = saved_profile;
        self.hover_id = 0;
        self.hover_done = false;
        self.hover_since = now();
    }

    /// Hit-test only the position Windows reports after input delivery. A move
    /// may still be queued, clipped to a monitor, or rejected by Windows.
    pub(super) unsafe fn pointer_actions(
        &mut self,
        time: u64,
        movement: MotionOutput,
        position: (i32, i32),
        requested: Option<(i32, i32)>,
    ) {
        if self.full_paused {
            self.click.suspend_dwell();
            return;
        }
        if requested.is_some_and(|target| target != position) {
            self.click.suspend_dwell();
            self.hover_id = 0;
            self.hover_since = time;
            self.hover_done = false;
            return;
        }
        let hit = WindowFromPoint(POINT {
            x: position.0,
            y: position.1,
        });
        let main_control = [ENABLE, 212]
            .into_iter()
            .find(|id| hit == GetDlgItem(self.hwnd, i32::from(*id)));
        let confirmation = messages::pause_window();
        let update_control = updater::control_at(position);
        let answer = if confirmation.is_null() {
            None
        } else {
            [1u16, 2]
                .into_iter()
                .find(|id| hit == GetDlgItem(confirmation, i32::from(*id)))
        };
        // The entire bar, including gaps and padding, excludes desktop dwell.
        let over_bar = self.inside_window(self.bar, position)
            || update_control.is_some()
            || main_control.is_some()
            || (!confirmation.is_null() && self.inside_window(confirmation, position));
        let bar_control = main_control
            .or(update_control.map(|(_, id)| id + 214))
            .or(answer.map(|id| id + 212))
            .unwrap_or_else(|| self.bar_control_at(position));
        if bar_control != self.hover_id
            || movement.delta.0.abs().max(movement.delta.1.abs())
                > self.config.profile().click.tolerance_px as i32
        {
            self.hover_id = bar_control;
            self.hover_since = time;
            self.hover_done = false;
        }
        if over_bar {
            self.click.suspend_dwell();
            if self.enabled
                && self.config.profile().fast_bar
                && let Some((_, _, action)) =
                    BAR_ACTIONS.iter().find(|(id, _, _)| *id == bar_control)
            {
                self.click.hover(*action, true);
            }
            if bar_control != 0
                && (self.enabled || matches!(bar_control, 210 | ENABLE | 212..=216))
                && !self.hover_done
                && time.saturating_sub(self.hover_since) >= self.config.profile().click.dwell_ms
            {
                self.hover_done = true;
                // A toolbar action is a local command; no mouse click is sent
                // to the foreground application and no pointer move is needed.
                if let Some((window, id)) = update_control {
                    SendMessageW(window, WM_COMMAND, usize::from(id), 0);
                } else if let Some(answer) = answer {
                    SendMessageW(confirmation, WM_COMMAND, usize::from(answer), 0);
                    self.apply_pause_answer();
                } else {
                    self.command(
                        if main_control.is_some() {
                            self.hwnd
                        } else {
                            self.bar
                        },
                        bar_control,
                        0,
                    );
                }
            }
        } else if self.enabled && self.pending_input.is_empty() {
            let delta = if self.click.scrolling() {
                movement.unclamped_delta
            } else {
                movement.delta
            };
            let events = self.click.update(
                time,
                delta,
                position,
                true,
                true,
                false,
                &self.config.profile().click,
            );
            self.dispatch(events);
        } else if !self.enabled {
            self.click.suspend_dwell();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_input_roundtrips_physical_pixels_including_reserved_bar_space() {
        for desktop in [
            ScreenRect {
                left: 0,
                top: 0,
                width: 1920,
                height: 1080,
            },
            ScreenRect {
                left: -2560,
                top: -1440,
                width: 6400,
                height: 3600,
            },
            ScreenRect {
                left: -3840,
                top: -2160,
                width: 7680,
                height: 4320,
            },
        ] {
            for y in desktop.top..desktop.top + desktop.height {
                let (_, absolute_y) = absolute_position((desktop.left, y), desktop);
                assert_eq!(
                    desktop.top
                        + ((i64::from(absolute_y) * i64::from(desktop.height)) / 65_536) as i32,
                    y
                );
            }
            for x in desktop.left..desktop.left + desktop.width {
                let (absolute_x, _) = absolute_position((x, desktop.top), desktop);
                assert_eq!(
                    desktop.left
                        + ((i64::from(absolute_x) * i64::from(desktop.width)) / 65_536) as i32,
                    x
                );
            }
        }
    }

    #[test]
    fn toolbar_edges_have_no_dpi_or_work_area_offset() {
        let desktop = ScreenRect {
            left: -1920,
            top: -1080,
            width: 5760,
            height: 3240,
        };
        for dpi in [96, 120, 144, 192, 240, 288] {
            let bottom = desktop.top + scale(52, dpi);
            for y in [
                desktop.top,
                (desktop.top + bottom) / 2,
                bottom - 1,
                bottom,
                bottom + 1,
            ] {
                let (_, normalized) = absolute_position((0, y), desktop);
                assert_eq!(
                    desktop.top
                        + (i64::from(normalized) * i64::from(desktop.height) / 65_536) as i32,
                    y
                );
            }
        }
    }
}
