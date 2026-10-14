//! Camera watchdog and pointer-stall diagnostics. Every 2 seconds it restarts a
//! camera that keeps delivering the same image, stops delivering frames or has
//! failed, with increasing delays between attempts. It also records stall
//! episodes (face lost while tracking continues, or a frozen image) so the
//! diagnostics report shows which of the two stopped the pointer.
use super::*;

const CHECK_MS: u64 = 2_000;
const FROZEN_MS: u64 = 2_000;
const NO_FRAMES_MS: u64 = 2_000;
/// Opening, selecting and zooming a camera can take a while before the first frame.
const OPENING_MS: u64 = 15_000;
/// Healthy frames for this long forget earlier restart attempts.
const HEALTHY_MS: u64 = 10_000;
/// Shorter interruptions are ordinary detector misses, not stalls.
const EPISODE_MS: u64 = 1_000;
const RECENT: usize = 5;

/// Wait after the previous restart before attempt `attempt` (0-based).
fn restart_delay_ms(attempt: u32) -> u64 {
    match attempt {
        0 => 0,
        1 => 2_000,
        2 => 5_000,
        3 => 15_000,
        _ => 30_000,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Problem {
    /// The camera repeats one image.
    Frozen,
    NoFrames,
    /// The camera worker reported an error or stopped.
    Failed,
}

struct Episode {
    kind: &'static str,
    start: u64,
    clicks: u64,
    motion: Vec<f32>,
}

#[derive(Default)]
pub(super) struct Watchdog {
    last_check: u64,
    camera_started: u64,
    last_frame: u64,
    unchanged_ms: u64,
    failed: bool,
    healthy_since: Option<u64>,
    attempts: u32,
    last_restart: u64,
    restarts: u64,
    recent_restarts: VecDeque<serde_json::Value>,
    episode: Option<Episode>,
    episodes: VecDeque<serde_json::Value>,
    motion: Vec<f32>,
}

fn median(values: &[f32]) -> Option<f32> {
    let mut values = values.to_vec();
    values.sort_unstable_by(f32::total_cmp);
    values.get(values.len() / 2).copied()
}

impl Watchdog {
    pub fn camera_started(&mut self, time: u64) {
        self.camera_started = time;
        self.last_frame = 0;
        self.unchanged_ms = 0;
        self.failed = false;
    }

    pub fn camera_failed(&mut self) {
        self.failed = true;
    }

    pub fn frame(&mut self, time: u64, frame: &VisionFrame, clicks: u64, track_face: bool) {
        self.last_frame = time;
        self.unchanged_ms = frame.unchanged_ms;
        let motion = frame.dx.hypot(frame.dy);
        self.motion.push(motion);
        let kind = if frame.unchanged_ms >= EPISODE_MS {
            Some("frozen_image")
        } else if track_face && frame.tracking_valid && !frame.face_detected {
            Some("face_lost")
        } else {
            None
        };
        if self.episode.as_ref().map(|episode| episode.kind) != kind {
            self.close_episode(time, clicks);
            self.episode = kind.map(|kind| Episode {
                kind,
                start: time,
                clicks,
                motion: Vec::new(),
            });
        }
        if let Some(episode) = &mut self.episode {
            episode.motion.push(motion);
        }
    }

    fn close_episode(&mut self, time: u64, clicks: u64) {
        let Some(episode) = self.episode.take() else {
            return;
        };
        if time.saturating_sub(episode.start) < EPISODE_MS {
            return;
        }
        if self.episodes.len() == RECENT {
            self.episodes.pop_front();
        }
        self.episodes
            .push_back(Self::describe(&episode, time, clicks));
    }

    fn describe(episode: &Episode, time: u64, clicks: u64) -> serde_json::Value {
        serde_json::json!({
            "kind":episode.kind,"start_ms":episode.start,
            "duration_ms":time.saturating_sub(episode.start),
            "clicks":clicks.saturating_sub(episode.clicks),
            "median_motion":median(&episode.motion),
        })
    }

    /// A camera problem that should be fixed by restarting the camera now.
    /// `suppressed` defers restarts while the user may be adjusting the camera.
    pub fn due(&mut self, time: u64, suppressed: bool) -> Option<Problem> {
        if time.saturating_sub(self.last_check) < CHECK_MS {
            return None;
        }
        self.last_check = time;
        let problem = if self.failed {
            Some(Problem::Failed)
        } else if self.last_frame == 0 {
            (time.saturating_sub(self.camera_started) >= OPENING_MS).then_some(Problem::NoFrames)
        } else if time.saturating_sub(self.last_frame) >= NO_FRAMES_MS {
            Some(Problem::NoFrames)
        } else if self.unchanged_ms >= FROZEN_MS {
            Some(Problem::Frozen)
        } else {
            None
        };
        let Some(problem) = problem else {
            let healthy = *self.healthy_since.get_or_insert(time);
            if time.saturating_sub(healthy) >= HEALTHY_MS {
                self.attempts = 0;
            }
            return None;
        };
        self.healthy_since = None;
        let wait = restart_delay_ms(self.attempts);
        (!suppressed && time.saturating_sub(self.last_restart) >= wait).then_some(problem)
    }

    pub fn restarted(&mut self, time: u64, problem: Problem) {
        self.attempts += 1;
        self.last_restart = time;
        self.restarts += 1;
        if self.recent_restarts.len() == RECENT {
            self.recent_restarts.pop_front();
        }
        self.recent_restarts
            .push_back(serde_json::json!({"time_ms":time,"reason":format!("{problem:?}")}));
    }

    /// Diagnostics; motion statistics cover the frames since the last report.
    pub fn report(&mut self, time: u64, clicks: u64) -> serde_json::Value {
        let motion = median(&self.motion);
        self.motion.clear();
        serde_json::json!({
            "camera_restarts":self.restarts,"recent_camera_restarts":self.recent_restarts,
            "restart_attempts":self.attempts,"unchanged_image_ms":self.unchanged_ms,
            "last_frame_age_ms":(self.last_frame > 0).then(|| time.saturating_sub(self.last_frame)),
            "median_motion":motion,
            "current_stall":self.episode.as_ref().map(|episode| Self::describe(episode, time, clicks)),
            "stall_episodes":self.episodes,
        })
    }
}

impl App {
    /// Restart the camera when the watchdog finds it frozen, silent or failed.
    pub(super) unsafe fn watch_camera(&mut self, time: u64) {
        if self.dry_run || self.camera.is_none() {
            return;
        }
        // The camera's settings dialog and calibration may legitimately pause frames.
        let suppressed = !self.options.is_null() || self.calibration_started > 0;
        if let Some(problem) = self.watchdog.due(time, suppressed) {
            self.watchdog.restarted(time, problem);
            self.start_camera();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(unchanged_ms: u64, face_detected: bool) -> VisionFrame {
        VisionFrame {
            width: 1,
            height: 1,
            rgb: vec![0, 0, 0],
            dx: 0.0,
            dy: 0.0,
            face_detected,
            tracking_valid: true,
            face_rect: None,
            fps: 30.0,
            tracked_points: 20,
            unchanged_ms,
            captured_at: Instant::now(),
        }
    }

    #[test]
    fn a_frozen_image_restarts_the_camera_with_increasing_delays() {
        let mut watchdog = Watchdog::default();
        watchdog.camera_started(1_000);
        watchdog.frame(3_000, &frame(0, true), 0, true);
        assert_eq!(watchdog.due(3_000, false), None);
        watchdog.frame(5_000, &frame(2_000, true), 0, true);
        assert_eq!(watchdog.due(5_000, false), Some(Problem::Frozen));
        watchdog.restarted(5_000, Problem::Frozen);
        watchdog.camera_started(5_000);
        // No frames yet: a reopening camera gets time before it counts as silent.
        assert_eq!(watchdog.due(7_000, false), None);
        assert_eq!(watchdog.due(20_000, false), Some(Problem::NoFrames));
        watchdog.restarted(20_000, Problem::NoFrames);
        watchdog.camera_failed();
        assert_eq!(watchdog.due(22_000, false), None);
        assert_eq!(watchdog.due(25_000, false), Some(Problem::Failed));
        // Deferred while the user has the configuration open.
        watchdog.restarted(25_000, Problem::Failed);
        assert_eq!(watchdog.due(60_000, true), None);
        assert_eq!(watchdog.due(62_000, false), Some(Problem::Failed));
    }

    #[test]
    fn healthy_frames_forget_earlier_attempts() {
        let mut watchdog = Watchdog::default();
        watchdog.camera_started(0);
        for attempt in 0..4 {
            watchdog.restarted(attempt, Problem::Failed);
        }
        watchdog.camera_started(10);
        for time in (2_000..=14_000).step_by(2_000) {
            watchdog.frame(time, &frame(0, true), 0, true);
            assert_eq!(watchdog.due(time, false), None);
        }
        assert_eq!(watchdog.attempts, 0);
    }

    #[test]
    fn stalls_are_recorded_by_cause_with_their_clicks() {
        let mut watchdog = Watchdog::default();
        watchdog.frame(0, &frame(0, true), 10, true);
        watchdog.frame(100, &frame(0, false), 10, true);
        watchdog.frame(3_100, &frame(0, false), 14, true);
        watchdog.frame(3_200, &frame(1_200, false), 14, true);
        watchdog.frame(3_300, &frame(0, true), 15, true);
        // A brief miss is not an episode.
        watchdog.frame(3_400, &frame(0, false), 15, true);
        watchdog.frame(3_500, &frame(0, true), 15, true);
        let episodes: Vec<_> = watchdog.episodes.iter().collect();
        assert_eq!(episodes.len(), 1);
        assert_eq!(episodes[0]["kind"], "face_lost");
        assert_eq!(episodes[0]["duration_ms"], 3_100);
        assert_eq!(episodes[0]["clicks"], 4);
    }
}
