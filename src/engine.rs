//! Deterministic, platform independent pointer and click state machines.
//!
//! Pointer transfer, dwell timing, gesture recognition and click selection are
//! deterministic and independent of the native interface.
//! Native input injection belongs to the caller; every held button is released
//! by `ClickEngine::reset`, including on face loss and disabling control.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MotionSettings {
    pub speed_x: u8,
    pub speed_y: u8,
    pub acceleration: u8,
    pub smoothing: u8,
    pub easy_stop: u8,
    pub reverse_x: bool,
    pub reverse_y: bool,
    pub restricted_workspace: bool,
    /// Left, top, right, bottom margins, expressed as screen percentages.
    pub margins_percent: [u8; 4],
    pub wrap_pointer: bool,
}

impl Default for MotionSettings {
    fn default() -> Self {
        Self {
            speed_x: 10,
            speed_y: 10,
            acceleration: 2,
            smoothing: 2,
            easy_stop: 1,
            reverse_x: false,
            reverse_y: false,
            restricted_workspace: false,
            margins_percent: [1; 4],
            wrap_pointer: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenRect {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MotionOutput {
    pub position: (i32, i32),
    pub delta: (i32, i32),
    /// Use for scrolling while the pointer is frozen, so reaching a screen or
    /// restricted-workspace edge does not prevent further wheel movement.
    pub unclamped_delta: (i32, i32),
}

#[derive(Debug, Default)]
pub struct MotionEngine {
    previous: (f32, f32),
}

impl MotionEngine {
    pub fn reset(&mut self) {
        self.previous = (0.0, 0.0);
    }

    /// Converts feature displacement in camera pixels into desktop pixels.
    pub fn process(
        &mut self,
        dx: f32,
        dy: f32,
        cursor: (i32, i32),
        screen: ScreenRect,
        settings: &MotionSettings,
    ) -> MotionOutput {
        if !dx.is_finite() || !dy.is_finite() {
            self.reset();
            return MotionOutput {
                position: cursor,
                delta: (0, 0),
                unclamped_delta: (0, 0),
            };
        }
        let old_weight = f32::from(settings.smoothing.min(8) + 1).log10();
        let axis = |motion: f32, previous: f32, speed: u8, reversed: bool| {
            let sign = if reversed { -1.0 } else { 1.0 };
            let scaled = motion * (f32::from(speed.min(30)) / 6.0).exp() * sign;
            scaled * (1.0 - old_weight) + previous * old_weight
        };
        let x = axis(dx, self.previous.0, settings.speed_x, settings.reverse_x);
        let y = axis(dy, self.previous.1, settings.speed_y, settings.reverse_y);
        self.previous = (x, y);

        // Use a 30-entry acceleration table indexed by rounded length.
        let distance_index = (x.hypot(y).round() as u32).min(29);
        let factor = acceleration_factor(settings.acceleration.min(5), distance_index);
        let threshold = f32::from(settings.easy_stop.min(10));
        let pixels = |value: f32| {
            let value = value * factor;
            if value.abs() < threshold {
                0
            } else {
                value.round() as i32
            }
        };
        let delta = (pixels(x), pixels(y));
        let width = screen.width.max(1);
        let height = screen.height.max(1);
        let margin = |index: usize, size: i32| {
            if settings.restricted_workspace {
                // A 50% margin on both sides leaves one centre pixel.
                ((i64::from(size) * i64::from(settings.margins_percent[index].min(50))) / 100)
                    as i32
            } else {
                0
            }
        };
        let min_x = screen.left.saturating_add(margin(0, width));
        let min_y = screen.top.saturating_add(margin(1, height));
        let max_x = screen
            .left
            .saturating_add(width - 1)
            .saturating_sub(margin(2, width))
            .max(min_x);
        let max_y = screen
            .top
            .saturating_add(height - 1)
            .saturating_sub(margin(3, height))
            .max(min_y);
        let limit = |value: i64, minimum: i32, maximum: i32| {
            if settings.wrap_pointer {
                (i64::from(minimum)
                    + (value - i64::from(minimum))
                        .rem_euclid(i64::from(maximum) - i64::from(minimum) + 1))
                    as i32
            } else {
                value.clamp(i64::from(minimum), i64::from(maximum)) as i32
            }
        };
        let position = (
            limit(i64::from(cursor.0) + i64::from(delta.0), min_x, max_x),
            limit(i64::from(cursor.1) + i64::from(delta.1), min_y, max_y),
        );
        MotionOutput {
            position,
            unclamped_delta: delta,
            delta: if settings.wrap_pointer {
                // Wrapping itself must not inflate the gesture displacement.
                delta
            } else {
                (
                    position.0.saturating_sub(cursor.0),
                    position.1.saturating_sub(cursor.1),
                )
            },
        }
    }
}

fn acceleration_factor(level: u8, index: u32) -> f32 {
    if level == 0 || index < 7 {
        return 1.0;
    }
    match level {
        1 => 1.5,
        2 => 2.0,
        3..=5 if index < 14 => {
            if level == 3 {
                1.5
            } else {
                2.0
            }
        }
        3 | 4 => 3.0 + (index - 14) as f32 * 0.1,
        _ => 4.0 + (index - 14) as f32 * 0.1,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClickMode {
    #[default]
    Dwell,
    Gesture,
    None,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClickAction {
    #[default]
    Left,
    Double,
    Triple,
    Middle,
    Right,
    Drag,
    Scroll,
    NoClick,
    SketchUpEdit,
    SketchUpConnected,
}

impl ClickAction {
    pub const ALL: [Self; 10] = [
        Self::NoClick,
        Self::Left,
        Self::Double,
        Self::Triple,
        Self::Middle,
        Self::Right,
        Self::Drag,
        Self::Scroll,
        Self::SketchUpEdit,
        Self::SketchUpConnected,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Left => "Left click",
            Self::Double => "Double click",
            Self::Triple => "Triple click",
            Self::Middle => "Middle click",
            Self::Right => "Right click",
            Self::Drag => "Drag",
            Self::Scroll => "Scroll",
            Self::NoClick => "No click",
            Self::SketchUpEdit => "SketchUp: edit group/component",
            Self::SketchUpConnected => "SketchUp: select connected (triple click)",
        }
    }

    fn button(self) -> Option<MouseButton> {
        match self {
            Self::Left => Some(MouseButton::Left),
            Self::Middle => Some(MouseButton::Middle),
            Self::Right => Some(MouseButton::Right),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputEvent {
    /// Select tool (Space) followed by a paced double click at the same point.
    SketchUpEdit,
    /// Select tool followed by three clicks using SketchUp's paced timing.
    SketchUpConnected,
    MoveTo(i32, i32),
    /// Count is 1, 2 or 3. The platform layer spaces multi-clicks appropriately.
    Click(MouseButton, u8),
    ButtonDown(MouseButton),
    ButtonUp(MouseButton),
    /// Horizontal and vertical Win32 wheel units (120 units per detent).
    Scroll(i32, i32),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClickSettings {
    pub mode: ClickMode,
    pub dwell_ms: u64,
    /// None preserves the legacy platform event delay for existing profiles.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub press_ms: Option<u64>,
    pub tolerance_px: u8,
    pub consecutive: bool,
    pub fast_gesture: bool,
    /// Left, right, up, down gesture directions.
    pub gestures: [ClickAction; 4],
    /// Wheel units generated per pixel of head-driven pointer movement.
    pub scroll_speed: u8,
}

impl Default for ClickSettings {
    fn default() -> Self {
        Self {
            mode: ClickMode::Dwell,
            dwell_ms: 1000,
            press_ms: None,
            tolerance_px: 3,
            consecutive: false,
            fast_gesture: false,
            gestures: [
                ClickAction::Left,
                ClickAction::Right,
                ClickAction::Double,
                ClickAction::Drag,
            ],
            scroll_speed: 12,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum GestureState {
    Dwell,
    Direction { origin: (i32, i32), since: u64 },
    WaitMotion,
}

#[derive(Debug)]
pub struct ClickEngine {
    selected: ClickAction,
    locked: ClickAction,
    clicks_enabled: bool,
    drag_modifier: bool,
    drag_button: MouseButton,
    held: Option<MouseButton>,
    scrolling: bool,
    since: Option<u64>,
    fired: bool,
    progress: f32,
    gesture: GestureState,
    previous_mode: Option<ClickMode>,
    previous_controls: bool,
    last_dwell_duration_ms: Option<u64>,
}

impl Default for ClickEngine {
    fn default() -> Self {
        Self {
            selected: ClickAction::Left,
            locked: ClickAction::Left,
            clicks_enabled: true,
            drag_modifier: false,
            drag_button: MouseButton::Left,
            held: None,
            scrolling: false,
            since: None,
            fired: false,
            progress: 0.0,
            gesture: GestureState::Dwell,
            previous_mode: None,
            previous_controls: false,
            last_dwell_duration_ms: None,
        }
    }
}

impl ClickEngine {
    /// Time spent in the countdown which most recently produced an action.
    pub fn last_dwell_duration_ms(&self) -> Option<u64> {
        self.last_dwell_duration_ms
    }
    pub fn selected(&self) -> ClickAction {
        if self.clicks_enabled {
            self.selected
        } else {
            ClickAction::NoClick
        }
    }

    pub fn locked(&self) -> ClickAction {
        self.locked
    }

    pub fn drag_enabled(&self) -> bool {
        self.drag_modifier || self.selected == ClickAction::Drag || self.held.is_some()
    }

    pub fn held_button(&self) -> Option<MouseButton> {
        self.held
    }
    pub fn drag_button(&self) -> MouseButton {
        self.drag_button
    }

    pub fn scrolling(&self) -> bool {
        self.scrolling
    }

    pub fn progress(&self) -> f32 {
        self.progress
    }

    pub fn awaiting_gesture(&self) -> bool {
        matches!(self.gesture, GestureState::Direction { .. })
    }

    /// A normal selection is used once; selecting it twice locks it. In fast
    /// mode hovering selects once and clicking locks the action.
    pub fn select(&mut self, action: ClickAction, fast_mode: bool) -> Vec<InputEvent> {
        if action == ClickAction::NoClick {
            let over_controls = self.previous_controls;
            let fired = self.fired;
            self.clicks_enabled = !self.clicks_enabled;
            let events = self.reset();
            self.previous_controls = over_controls;
            self.fired = over_controls && fired;
            return events;
        }
        if !self.clicks_enabled {
            return Vec::new();
        }
        if action == ClickAction::Drag {
            // Drag can extend any single-button click, including middle/right.
            if let Some(button) = self.selected.button() {
                self.drag_button = button;
            }
        }
        if fast_mode || (self.selected != self.locked && action == self.selected) {
            self.locked = action;
        }
        self.selected = action;
        if !self.previous_controls {
            self.reset_countdown();
        }
        Vec::new()
    }

    pub fn hover(&mut self, action: ClickAction, fast_mode: bool) {
        if fast_mode
            && self.clicks_enabled
            && action != ClickAction::NoClick
            && action != self.selected
        {
            if action == ClickAction::Drag
                && let Some(button) = self.selected.button()
            {
                self.drag_button = button;
            }
            self.selected = action;
            if !self.previous_controls {
                self.reset_countdown();
            }
        }
    }

    /// Applies drag to a selected single click. Turning it off never abandons
    /// a pressed button: the next moved-then-stopped dwell still releases it.
    pub fn set_drag(&mut self, enabled: bool) {
        self.drag_modifier = enabled;
        if enabled && let Some(button) = self.selected.button() {
            self.drag_button = button;
        }
        if !self.previous_controls {
            self.reset_countdown();
        }
    }

    /// Suppresses desktop dwell while a native click bar performs its own
    /// button hit-testing/countdown. Preserve active drag, scroll and selection;
    /// the first frame outside the bar starts a fresh dwell interval.
    pub fn suspend_dwell(&mut self) {
        self.reset_countdown();
        self.previous_controls = false;
    }

    fn reset_countdown(&mut self) {
        self.since = None;
        self.fired = false;
        self.progress = 0.0;
        self.gesture = GestureState::Dwell;
    }

    /// The caller MUST dispatch these events before camera shutdown / exit.
    pub fn reset(&mut self) -> Vec<InputEvent> {
        let events = self
            .held
            .take()
            .map(InputEvent::ButtonUp)
            .into_iter()
            .collect();
        self.scrolling = false;
        self.selected = self.locked;
        self.reset_countdown();
        self.previous_controls = false;
        events
    }

    /// `now_ms` is monotonic elapsed time, not wall-clock time. Call once per
    /// camera frame, and also with `tracking=false` on a camera error/timeout.
    /// `over_controls` forces a plain left click so the click bar remains usable
    /// with right click, gesture click or no-click selected.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        now_ms: u64,
        delta: (i32, i32),
        cursor: (i32, i32),
        tracking: bool,
        enabled: bool,
        over_controls: bool,
        settings: &ClickSettings,
    ) -> Vec<InputEvent> {
        if !tracking || !enabled {
            return self.reset();
        }
        let mut events = Vec::new();
        if self.previous_mode.is_some_and(|mode| mode != settings.mode) {
            events.extend(self.reset());
        }
        self.previous_mode = Some(settings.mode);
        if settings.mode == ClickMode::None {
            events.extend(self.reset());
            return events;
        }
        if over_controls != self.previous_controls {
            self.reset_countdown();
            self.previous_controls = over_controls;
        }
        if !self.clicks_enabled && !over_controls {
            self.reset_countdown();
            return events;
        }
        let moving =
            f64::from(delta.0).hypot(f64::from(delta.1)) > f64::from(settings.tolerance_px.min(8));
        let dwell = settings.dwell_ms.clamp(100, 5000);

        if self.scrolling && !over_controls && (delta.0 != 0 || delta.1 != 0) {
            let speed = i32::from(settings.scroll_speed.clamp(1, 120));
            events.push(InputEvent::Scroll(
                delta.0.saturating_mul(speed),
                delta.1.saturating_mul(speed).saturating_neg(),
            ));
        }
        // Existing drag and scroll operations finish by stopping again, even
        // if they were initiated through a gesture.
        if settings.mode == ClickMode::Gesture
            && !over_controls
            && self.held.is_none()
            && !self.scrolling
        {
            self.update_gesture(now_ms, moving, cursor, settings, &mut events);
            return events;
        }
        if moving {
            self.since = Some(now_ms);
            self.fired = false;
            self.progress = 0.0;
            return events;
        }
        if self.fired {
            self.progress = 0.0;
            return events;
        }
        let since = *self.since.get_or_insert(now_ms);
        let elapsed = now_ms.saturating_sub(since);
        self.progress = (elapsed as f32 / dwell as f32).min(1.0);
        if elapsed < dwell {
            return events;
        }
        self.last_dwell_duration_ms = Some(elapsed);
        if over_controls {
            // Release a held button before interacting with the application's
            // controls, rather than accidentally dragging its window.
            if let Some(button) = self.held.take() {
                events.push(InputEvent::ButtonUp(button));
            }
            self.scrolling = false;
            events.push(InputEvent::Click(MouseButton::Left, 1));
        } else {
            self.perform(self.selected, &mut events);
        }
        // Starting a drag/scroll never immediately releases it while stationary,
        // even when ordinary consecutive clicking is enabled.
        let repeat = settings.consecutive && self.held.is_none() && !self.scrolling;
        self.since = Some(now_ms);
        self.fired = !repeat;
        self.progress = 0.0;
        events
    }

    fn update_gesture(
        &mut self,
        now_ms: u64,
        moving: bool,
        cursor: (i32, i32),
        settings: &ClickSettings,
        events: &mut Vec<InputEvent>,
    ) {
        let dwell = settings.dwell_ms.clamp(100, 5000);
        match self.gesture {
            GestureState::Dwell => {
                if moving {
                    self.since = Some(now_ms);
                    self.progress = 0.0;
                } else {
                    let since = *self.since.get_or_insert(now_ms);
                    self.progress = (now_ms.saturating_sub(since) as f32 / dwell as f32).min(1.0);
                    if now_ms.saturating_sub(since) >= dwell {
                        self.gesture = GestureState::Direction {
                            origin: cursor,
                            since: now_ms,
                        };
                        self.progress = 0.0;
                    }
                }
            }
            GestureState::Direction { origin, mut since } => {
                if moving {
                    if !settings.fast_gesture {
                        since = now_ms;
                    }
                    self.gesture = GestureState::Direction { origin, since };
                    self.progress = 0.0;
                } else {
                    self.progress = (now_ms.saturating_sub(since) as f32 / dwell as f32).min(1.0);
                    if now_ms.saturating_sub(since) >= dwell {
                        let dx = i64::from(cursor.0) - i64::from(origin.0);
                        let dy = i64::from(cursor.1) - i64::from(origin.1);
                        if (dx as f64).hypot(dy as f64) > f64::from(settings.tolerance_px.min(8)) {
                            let direction = if dx.abs() > dy.abs() {
                                if dx < 0 { 0 } else { 1 }
                            } else if dy < 0 {
                                2
                            } else {
                                3
                            };
                            let action = settings.gestures[direction];
                            if action != ClickAction::NoClick {
                                events.push(InputEvent::MoveTo(origin.0, origin.1));
                                self.perform(action, events);
                            }
                        }
                        self.gesture = GestureState::WaitMotion;
                        self.since = Some(now_ms);
                        self.fired = true;
                        self.progress = 0.0;
                    }
                }
            }
            GestureState::WaitMotion => {
                if moving {
                    self.gesture = GestureState::Dwell;
                    self.since = Some(now_ms);
                    self.progress = 0.0;
                }
            }
        }
    }

    fn perform(&mut self, action: ClickAction, events: &mut Vec<InputEvent>) {
        if let Some(button) = self.held.take() {
            events.push(InputEvent::ButtonUp(button));
            self.selected = self.locked;
            return;
        }
        if self.scrolling {
            self.scrolling = false;
            self.selected = self.locked;
            return;
        }
        if action == ClickAction::Drag || (self.drag_modifier && action.button().is_some()) {
            let button = action.button().unwrap_or(self.drag_button);
            self.held = Some(button);
            events.push(InputEvent::ButtonDown(button));
            return;
        }
        match action {
            ClickAction::Left => events.push(InputEvent::Click(MouseButton::Left, 1)),
            ClickAction::Double => events.push(InputEvent::Click(MouseButton::Left, 2)),
            ClickAction::Triple => events.push(InputEvent::Click(MouseButton::Left, 3)),
            ClickAction::Middle => events.push(InputEvent::Click(MouseButton::Middle, 1)),
            ClickAction::Right => events.push(InputEvent::Click(MouseButton::Right, 1)),
            ClickAction::Scroll => {
                self.scrolling = true;
                return;
            }
            ClickAction::SketchUpEdit => events.push(InputEvent::SketchUpEdit),
            ClickAction::SketchUpConnected => events.push(InputEvent::SketchUpConnected),
            ClickAction::Drag | ClickAction::NoClick => return,
        }
        self.selected = self.locked;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: ScreenRect = ScreenRect {
        left: 0,
        top: 0,
        width: 1920,
        height: 1080,
    };

    fn linear() -> MotionSettings {
        MotionSettings {
            speed_x: 0,
            speed_y: 0,
            acceleration: 0,
            smoothing: 0,
            easy_stop: 0,
            ..Default::default()
        }
    }

    fn tick(engine: &mut ClickEngine, time: u64, delta: (i32, i32)) -> Vec<InputEvent> {
        engine.update(
            time,
            delta,
            (500, 500),
            true,
            true,
            false,
            &ClickSettings::default(),
        )
    }

    #[test]
    fn upstream_default_transfer_curve() {
        let mut engine = MotionEngine::default();
        // exp(10/6) * (1-log10(3)) is 2.768 pixels at the first frame.
        let result = engine.process(1.0, 0.0, (500, 500), SCREEN, &MotionSettings::default());
        assert_eq!(result.delta, (3, 0));
        assert_eq!(result.position, (503, 500));
        engine.reset();
        let result = engine.process(3.0, 0.0, (500, 500), SCREEN, &MotionSettings::default());
        assert_eq!(result.delta, (17, 0));
    }

    #[test]
    fn acceleration_preserves_upstream_thresholds_and_table_cap() {
        assert_eq!(acceleration_factor(2, 6), 1.0);
        assert_eq!(acceleration_factor(2, 7), 2.0);
        assert_eq!(acceleration_factor(3, 13), 1.5);
        assert_eq!(acceleration_factor(3, 14), 3.0);
        assert_eq!(acceleration_factor(5, 29), 5.5);
        assert_eq!(acceleration_factor(0, 29), 1.0);
    }

    #[test]
    fn axes_reversal_and_easy_stop() {
        let mut settings = linear();
        settings.reverse_x = true;
        settings.easy_stop = 2;
        let result = MotionEngine::default().process(3.0, 1.99, (500, 500), SCREEN, &settings);
        assert_eq!(result.delta, (-3, 0));
    }

    #[test]
    fn reset_discards_smoothing_momentum_and_invalid_tracking() {
        let mut engine = MotionEngine::default();
        let settings = MotionSettings::default();
        engine.process(5.0, 5.0, (500, 500), SCREEN, &settings);
        engine.reset();
        assert_eq!(
            engine
                .process(0.0, 0.0, (500, 500), SCREEN, &settings)
                .delta,
            (0, 0)
        );
        assert_eq!(
            engine
                .process(f32::NAN, 1.0, (500, 500), SCREEN, &settings)
                .delta,
            (0, 0)
        );
    }

    #[test]
    fn workspace_handles_negative_monitor_origins_and_wraps() {
        let screen = ScreenRect {
            left: -1000,
            top: -100,
            width: 1000,
            height: 500,
        };
        let mut settings = linear();
        settings.restricted_workspace = true;
        settings.margins_percent = [10; 4];
        let mut engine = MotionEngine::default();
        assert_eq!(
            engine
                .process(-100.0, 0.0, (-890, 0), screen, &settings)
                .position,
            (-900, 0)
        );
        settings.wrap_pointer = true;
        assert_eq!(
            engine
                .process(-20.0, 0.0, (-890, 0), screen, &settings)
                .position,
            (-110, 0)
        );
        settings.margins_percent = [50; 4];
        assert_eq!(
            engine
                .process(100.0, 0.0, (-500, 150), screen, &settings)
                .position,
            (-500, 150)
        );
    }

    #[test]
    fn scrolling_keeps_unclamped_motion_at_desktop_edges() {
        let result = MotionEngine::default().process(20.0, -10.0, (1919, 0), SCREEN, &linear());
        assert_eq!(result.position, (1919, 0));
        assert_eq!(result.delta, (0, 0));
        assert_eq!(result.unclamped_delta, (20, -10));
    }

    #[test]
    fn sketchup_edit_is_a_separate_one_shot_action() {
        let mut engine = ClickEngine::default();
        engine.select(ClickAction::SketchUpEdit, false);
        assert!(tick(&mut engine, 0, (0, 0)).is_empty());
        assert_eq!(
            tick(&mut engine, 1000, (0, 0)),
            vec![InputEvent::SketchUpEdit]
        );
        assert_eq!(engine.selected(), ClickAction::Left);
    }

    #[test]
    fn sketchup_connected_selection_is_one_shot_and_can_be_locked() {
        let mut engine = ClickEngine::default();
        engine.select(ClickAction::SketchUpConnected, false);
        assert!(tick(&mut engine, 0, (0, 0)).is_empty());
        assert_eq!(
            tick(&mut engine, 1000, (0, 0)),
            vec![InputEvent::SketchUpConnected]
        );
        assert_eq!(engine.selected(), ClickAction::Left);
        engine.select(ClickAction::SketchUpConnected, false);
        engine.select(ClickAction::SketchUpConnected, false);
        assert_eq!(engine.locked(), ClickAction::SketchUpConnected);
    }

    #[test]
    fn robo_dwell_matches_expected_26_fps_timing() {
        let mut engine = ClickEngine::default();
        let settings = ClickSettings {
            dwell_ms: 400,
            tolerance_px: 2,
            ..ClickSettings::default()
        };
        assert!(
            engine
                .update(0, (10, 0), (500, 500), true, true, false, &settings)
                .is_empty()
        );
        let mut fired_at = None;
        for frame in 1..=20 {
            let time = frame * 38;
            let events = engine.update(time, (0, 0), (500, 500), true, true, false, &settings);
            if !events.is_empty() {
                fired_at = Some(time);
                break;
            }
        }
        // The countdown fires on the first camera frame after 400 ms.
        assert_eq!(fired_at, Some(418));
        assert_eq!(engine.last_dwell_duration_ms(), Some(418));
    }

    #[test]
    fn dwell_fires_once_and_movement_rearms() {
        let mut engine = ClickEngine::default();
        assert!(tick(&mut engine, 0, (0, 0)).is_empty());
        assert!(tick(&mut engine, 999, (0, 0)).is_empty());
        assert_eq!(
            tick(&mut engine, 1000, (0, 0)),
            vec![InputEvent::Click(MouseButton::Left, 1)]
        );
        assert!(tick(&mut engine, 5000, (0, 0)).is_empty());
        assert!(tick(&mut engine, 5100, (4, 0)).is_empty());
        assert!(tick(&mut engine, 6099, (0, 0)).is_empty());
        assert_eq!(
            tick(&mut engine, 6100, (0, 0)),
            vec![InputEvent::Click(MouseButton::Left, 1)]
        );
    }

    #[test]
    fn tolerance_uses_each_frame_displacement() {
        let mut engine = ClickEngine::default();
        tick(&mut engine, 0, (0, 0));
        tick(&mut engine, 500, (3, 0));
        assert_eq!(
            tick(&mut engine, 1000, (3, 0)),
            vec![InputEvent::Click(MouseButton::Left, 1)]
        );
        tick(&mut engine, 1100, (3, 3));
        assert!(tick(&mut engine, 2000, (0, 0)).is_empty());
    }

    #[test]
    fn temporary_click_and_lock_rules() {
        let mut engine = ClickEngine::default();
        engine.select(ClickAction::Triple, false);
        tick(&mut engine, 0, (0, 0));
        assert_eq!(
            tick(&mut engine, 1000, (0, 0)),
            vec![InputEvent::Click(MouseButton::Left, 3)]
        );
        assert_eq!(engine.selected(), ClickAction::Left);
        engine.select(ClickAction::Middle, false);
        engine.select(ClickAction::Middle, false);
        assert_eq!(engine.locked(), ClickAction::Middle);
        tick(&mut engine, 1100, (0, 0));
        assert_eq!(
            tick(&mut engine, 2100, (0, 0)),
            vec![InputEvent::Click(MouseButton::Middle, 1)]
        );
        assert_eq!(engine.selected(), ClickAction::Middle);
    }

    #[test]
    fn fast_hover_selects_and_click_locks() {
        let mut engine = ClickEngine::default();
        engine.hover(ClickAction::Right, false);
        assert_eq!(engine.selected(), ClickAction::Left);
        engine.hover(ClickAction::Right, true);
        assert_eq!(engine.selected(), ClickAction::Right);
        assert_eq!(engine.locked(), ClickAction::Left);
        engine.select(ClickAction::Right, true);
        assert_eq!(engine.locked(), ClickAction::Right);
    }

    #[test]
    fn drag_stays_down_until_motion_then_later_dwell() {
        let mut engine = ClickEngine::default();
        engine.select(ClickAction::Right, false);
        engine.select(ClickAction::Drag, false);
        tick(&mut engine, 0, (0, 0));
        assert_eq!(
            tick(&mut engine, 1000, (0, 0)),
            vec![InputEvent::ButtonDown(MouseButton::Right)]
        );
        assert_eq!(engine.held_button(), Some(MouseButton::Right));
        assert!(tick(&mut engine, 5000, (0, 0)).is_empty());
        tick(&mut engine, 5100, (10, 0));
        assert_eq!(
            tick(&mut engine, 6100, (0, 0)),
            vec![InputEvent::ButtonUp(MouseButton::Right)]
        );
        assert_eq!(engine.selected(), ClickAction::Left);
    }

    #[test]
    fn drag_modifier_applies_only_to_single_clicks() {
        let mut engine = ClickEngine::default();
        engine.select(ClickAction::Middle, false);
        engine.set_drag(true);
        tick(&mut engine, 0, (0, 0));
        assert_eq!(
            tick(&mut engine, 1000, (0, 0)),
            vec![InputEvent::ButtonDown(MouseButton::Middle)]
        );
        assert_eq!(
            engine.reset(),
            vec![InputEvent::ButtonUp(MouseButton::Middle)]
        );
        engine.select(ClickAction::Double, false);
        tick(&mut engine, 2000, (0, 0));
        assert_eq!(
            tick(&mut engine, 3000, (0, 0)),
            vec![InputEvent::Click(MouseButton::Left, 2)]
        );
    }

    #[test]
    fn face_loss_pause_and_mode_change_release_held_buttons() {
        for reason in 0..3 {
            let mut engine = ClickEngine::default();
            engine.select(ClickAction::Drag, false);
            tick(&mut engine, 0, (0, 0));
            tick(&mut engine, 1000, (0, 0));
            let mut settings = ClickSettings::default();
            if reason == 2 {
                settings.mode = ClickMode::None;
            }
            assert_eq!(
                engine.update(
                    1100,
                    (0, 0),
                    (500, 500),
                    reason != 0,
                    reason != 1,
                    false,
                    &settings
                ),
                vec![InputEvent::ButtonUp(MouseButton::Left)]
            );
            assert_eq!(engine.held_button(), None);
            assert!(engine.reset().is_empty());
        }
    }

    #[test]
    fn consecutive_does_not_release_drag_without_movement() {
        let mut engine = ClickEngine::default();
        let settings = ClickSettings {
            consecutive: true,
            ..Default::default()
        };
        engine.select(ClickAction::Drag, false);
        engine.update(0, (0, 0), (500, 500), true, true, false, &settings);
        assert_eq!(
            engine.update(1000, (0, 0), (500, 500), true, true, false, &settings),
            vec![InputEvent::ButtonDown(MouseButton::Left)]
        );
        assert!(
            engine
                .update(5000, (0, 0), (500, 500), true, true, false, &settings)
                .is_empty()
        );
    }

    #[test]
    fn consecutive_clicks_repeat_after_a_full_interval() {
        let mut engine = ClickEngine::default();
        let settings = ClickSettings {
            consecutive: true,
            ..Default::default()
        };
        for time in [0, 1000, 2000] {
            let events = engine.update(time, (0, 0), (500, 500), true, true, false, &settings);
            assert_eq!(events.len(), usize::from(time != 0));
        }
    }

    #[test]
    fn no_click_can_be_resumed_from_the_toolbar() {
        let mut engine = ClickEngine::default();
        engine.select(ClickAction::NoClick, false);
        tick(&mut engine, 0, (0, 0));
        assert!(tick(&mut engine, 1000, (0, 0)).is_empty());
        assert_eq!(engine.selected(), ClickAction::NoClick);
        engine.update(
            1000,
            (0, 0),
            (500, 500),
            true,
            true,
            true,
            &ClickSettings::default(),
        );
        assert_eq!(
            engine.update(
                2000,
                (0, 0),
                (500, 500),
                true,
                true,
                true,
                &ClickSettings::default()
            ),
            vec![InputEvent::Click(MouseButton::Left, 1)]
        );
        engine.select(ClickAction::NoClick, false);
        assert_eq!(engine.selected(), ClickAction::Left);
    }

    #[test]
    fn scroll_is_latched_until_moved_then_stopped() {
        let mut engine = ClickEngine::default();
        engine.select(ClickAction::Scroll, false);
        tick(&mut engine, 0, (0, 0));
        assert!(tick(&mut engine, 1000, (0, 0)).is_empty());
        assert!(engine.scrolling());
        assert!(tick(&mut engine, 5000, (0, 0)).is_empty());
        assert_eq!(
            tick(&mut engine, 5100, (10, -20)),
            vec![InputEvent::Scroll(120, 240)]
        );
        assert!(tick(&mut engine, 6100, (0, 0)).is_empty());
        assert!(!engine.scrolling());
        assert_eq!(engine.selected(), ClickAction::Left);
    }

    #[test]
    fn gestures_return_to_start_before_clicking() {
        let mut engine = ClickEngine::default();
        let settings = ClickSettings {
            mode: ClickMode::Gesture,
            ..Default::default()
        };
        engine.update(0, (0, 0), (500, 500), true, true, false, &settings);
        engine.update(1000, (0, 0), (500, 500), true, true, false, &settings);
        assert!(engine.awaiting_gesture());
        engine.update(1500, (-20, 0), (480, 500), true, true, false, &settings);
        assert!(
            engine
                .update(2000, (0, 0), (480, 500), true, true, false, &settings)
                .is_empty()
        );
        assert_eq!(
            engine.update(2500, (0, 0), (480, 500), true, true, false, &settings),
            vec![
                InputEvent::MoveTo(500, 500),
                InputEvent::Click(MouseButton::Left, 1)
            ]
        );
        assert!(
            engine
                .update(5000, (0, 0), (500, 500), true, true, false, &settings)
                .is_empty()
        );
    }

    #[test]
    fn fast_gesture_counts_movement_time() {
        let mut engine = ClickEngine::default();
        let settings = ClickSettings {
            mode: ClickMode::Gesture,
            fast_gesture: true,
            ..Default::default()
        };
        engine.update(0, (0, 0), (500, 500), true, true, false, &settings);
        engine.update(1000, (0, 0), (500, 500), true, true, false, &settings);
        engine.update(1900, (20, 0), (520, 500), true, true, false, &settings);
        assert_eq!(
            engine.update(2000, (0, 0), (520, 500), true, true, false, &settings),
            vec![
                InputEvent::MoveTo(500, 500),
                InputEvent::Click(MouseButton::Right, 1)
            ]
        );
    }

    #[test]
    fn short_or_disabled_gestures_never_click() {
        for disabled in [false, true] {
            let mut engine = ClickEngine::default();
            let settings = ClickSettings {
                mode: ClickMode::Gesture,
                gestures: [ClickAction::NoClick; 4],
                ..Default::default()
            };
            engine.update(0, (0, 0), (500, 500), true, true, false, &settings);
            engine.update(1000, (0, 0), (500, 500), true, true, false, &settings);
            let cursor = if disabled { (550, 500) } else { (501, 500) };
            assert!(
                engine
                    .update(2000, (0, 0), cursor, true, true, false, &settings)
                    .is_empty()
            );
        }
    }

    #[test]
    fn settings_deserialize_with_upstream_defaults() {
        let settings: MotionSettings = serde_json::from_str("{\"speed_x\":12}").unwrap();
        assert_eq!(settings.speed_x, 12);
        assert_eq!(settings.speed_y, 10);
        let settings: ClickSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings.dwell_ms, 1000);
        assert_eq!(settings.gestures[3], ClickAction::Drag);
    }

    #[test]
    fn toolbar_selection_does_not_repeatedly_toggle_without_motion() {
        let mut engine = ClickEngine::default();
        let settings = ClickSettings::default();
        engine.update(0, (0, 0), (500, 500), true, true, true, &settings);
        assert_eq!(
            engine.update(1000, (0, 0), (500, 500), true, true, true, &settings),
            vec![InputEvent::Click(MouseButton::Left, 1)]
        );
        engine.select(ClickAction::NoClick, false);
        assert!(
            engine
                .update(2000, (0, 0), (500, 500), true, true, true, &settings)
                .is_empty()
        );
        assert_eq!(engine.selected(), ClickAction::NoClick);
    }

    #[test]
    fn native_toolbar_suspends_countdown_without_releasing_drag() {
        let mut engine = ClickEngine::default();
        engine.select(ClickAction::Middle, false);
        engine.select(ClickAction::Drag, false);
        tick(&mut engine, 0, (0, 0));
        tick(&mut engine, 1000, (0, 0));
        engine.suspend_dwell();
        assert_eq!(engine.held_button(), Some(MouseButton::Middle));
        assert_eq!(engine.selected(), ClickAction::Drag);
        assert!(tick(&mut engine, 5000, (0, 0)).is_empty());
        assert!(tick(&mut engine, 5999, (0, 0)).is_empty());
        assert_eq!(
            tick(&mut engine, 6000, (0, 0)),
            vec![InputEvent::ButtonUp(MouseButton::Middle)]
        );
    }
}
