//! Camera acquisition and optical-flow feature tracking.
//!
//! The capture thread owns the camera. A second thread runs the comparatively
//! expensive face detector, so detecting a face never stalls pointer tracking.
//! Only the latest preview is retained; unconsumed motion is accumulated.
//!
//! Motion-tracking algorithms incorporate GPL-licensed contributions,
//! Copyright (C) 2008-19 Cesar Mauri Loba / CREA Software Systems, GPL-3.0+.

use crate::cameras::{self, CameraDevice, CameraPreference};
use opencv::{
    core::{self, Mat, Point2f, Rect, Scalar, Size, TermCriteria, Vector},
    imgproc, objdetect,
    prelude::*,
    video, videoio,
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const WIDTH: i32 = 320;
const HEIGHT: i32 = 240;
const CORNERS: i32 = 20;
const CASCADE: &str = include_str!("../assets/models/haarcascade_frontalface_default.xml");
// Serializes successive workers during a camera/profile change. `stop()` is
// deliberately nonblocking, but opening the replacement must wait for the old
// driver's handle to be released. Only capture threads take this lock.
static CAMERA_SESSION: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, PartialEq)]
pub struct VisionConfig {
    pub camera_index: i32,
    pub camera: Option<CameraPreference>,
    pub camera_zoom_steps: Option<u32>,
    pub track_face: bool,
    pub stop_on_face_loss: bool,
    pub face_timeout_ms: u64,
    pub detection_interval_ms: u64,
    /// Normalized x, y, width, height in the mirrored preview.
    pub manual_area: [f32; 4],
}

impl Default for VisionConfig {
    fn default() -> Self {
        Self {
            camera_index: 0,
            camera: None,
            camera_zoom_steps: None,
            track_face: true,
            stop_on_face_loss: true,
            face_timeout_ms: 5_000,
            detection_interval_ms: 400,
            manual_area: [0.25, 0.35, 0.5, 0.3],
        }
    }
}

#[derive(Debug)]
pub struct VisionFrame {
    pub width: usize,
    pub height: usize,
    /// Tightly packed, mirrored RGB pixels. Camera data stays on this computer.
    pub rgb: Vec<u8>,
    /// Displacement units: twice the motion in a 320 x 240 image.
    pub dx: f32,
    pub dy: f32,
    pub face_detected: bool,
    /// Enough reliable features, and a recent face when automatic mode is on.
    pub tracking_valid: bool,
    /// Current tracking area [x, y, width, height] in preview pixels.
    pub face_rect: Option<[i32; 4]>,
    pub fps: f32,
    pub tracked_points: usize,
    /// Timestamp allows the consumer to reject an old frame after a UI stall.
    pub captured_at: Instant,
}

impl VisionFrame {
    /// Pointer control, calibration and preview must agree about readiness.
    /// A valid optical-flow target can remain usable without a classifier hit
    /// when the selected profile disables classifier-based face-loss stopping.
    pub fn tracking_ready(&self) -> bool {
        self.tracking_valid && self.captured_at.elapsed() < Duration::from_millis(500)
    }
}

#[derive(Debug)]
pub enum VisionEvent {
    Cameras {
        devices: Vec<CameraDevice>,
        selected: Option<CameraDevice>,
    },
    Frame(VisionFrame),
    Zoom(Result<cameras::DriverZoom, String>),
    Error(String),
    Stopped,
}

#[derive(Default)]
struct Mailbox {
    cameras: Option<(Vec<CameraDevice>, Option<CameraDevice>)>,
    zoom: Option<Result<cameras::DriverZoom, String>>,
    frame: Option<VisionFrame>,
    pending_since: Option<Instant>,
    error: Option<String>,
    stopped: bool,
}

impl Mailbox {
    fn publish(&mut self, mut frame: VisionFrame) {
        let since = *self.pending_since.get_or_insert(frame.captured_at);
        if let Some(pending) = self.frame.take() {
            // A pause longer than a quarter second must not release a large
            // accumulated cursor jump when the GUI becomes responsive again.
            if frame.tracking_valid
                && pending.tracking_valid
                && frame.captured_at.duration_since(since) < Duration::from_millis(250)
            {
                frame.dx += pending.dx;
                frame.dy += pending.dy;
            } else {
                self.pending_since = Some(frame.captured_at);
            }
        }
        self.frame = Some(frame);
    }
}

pub struct VisionWorker {
    stop: Arc<AtomicBool>,
    settings_requested: Arc<AtomicBool>,
    mailbox: Arc<Mutex<Mailbox>>,
    thread: Option<JoinHandle<()>>,
}

impl VisionWorker {
    /// Starts immediately. Camera/model failures arrive as `VisionEvent::Error`.
    pub fn start(config: VisionConfig) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let settings_requested = Arc::new(AtomicBool::new(false));
        let mailbox = Arc::new(Mutex::new(Mailbox::default()));
        let worker_stop = Arc::clone(&stop);
        let worker_settings = Arc::clone(&settings_requested);
        let worker_mailbox = Arc::clone(&mailbox);
        let result = thread::Builder::new()
            .name("robocam-camera".into())
            .spawn(move || {
                let result = run_camera(config, &worker_stop, &worker_settings, &worker_mailbox);
                let mut mailbox = worker_mailbox.lock().unwrap_or_else(|p| p.into_inner());
                mailbox.frame = None;
                if !worker_stop.load(Ordering::Acquire)
                    && let Err(error) = result
                {
                    mailbox.error = Some(error);
                }
                mailbox.stopped = true;
            });
        let thread = match result {
            Ok(thread) => Some(thread),
            Err(error) => {
                let mut mailbox = mailbox.lock().unwrap_or_else(|p| p.into_inner());
                mailbox.error = Some(format!("Could not start camera worker: {error}"));
                mailbox.stopped = true;
                None
            }
        };
        Self {
            stop,
            settings_requested,
            mailbox,
            thread,
        }
    }

    pub fn try_recv(&self) -> Option<VisionEvent> {
        let mut mailbox = self.mailbox.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((devices, selected)) = mailbox.cameras.take() {
            return Some(VisionEvent::Cameras { devices, selected });
        }
        if let Some(error) = mailbox.error.take() {
            return Some(VisionEvent::Error(error));
        }
        if let Some(zoom) = mailbox.zoom.take() {
            return Some(VisionEvent::Zoom(zoom));
        }
        if mailbox.stopped {
            mailbox.stopped = false;
            return Some(VisionEvent::Stopped);
        }
        mailbox.pending_since = None;
        mailbox.frame.take().map(VisionEvent::Frame)
    }

    /// Nonblocking: release happens on the owning capture thread. A camera
    /// driver blocked inside `read` must never block the Windows message loop.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
        self.mailbox.lock().unwrap_or_else(|p| p.into_inner()).frame = None;
    }

    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// Opens the DirectShow driver's own property sheet when it provides one.
    /// Unsupported property sheets are ignored by OpenCV/Media Foundation.
    pub fn request_camera_settings(&self) {
        self.settings_requested.store(true, Ordering::Release);
    }
}

impl Drop for VisionWorker {
    fn drop(&mut self) {
        self.stop();
        // Joining an already-finished thread is safe on the GUI thread. If a
        // driver is still returning a frame, let the worker release it itself.
        if let Some(thread) = self.thread.take()
            && thread.is_finished()
        {
            let _ = thread.join();
        }
    }
}

struct DetectionRequest {
    gray: Mat,
    captured_at: Instant,
    position: Point2f,
}

struct Detection {
    area: Option<Rect>,
    captured_at: Instant,
    position: Point2f,
}

struct Detector {
    requests: Option<SyncSender<DetectionRequest>>,
    results: Receiver<Result<Detection, String>>,
    thread: Option<JoinHandle<()>>,
}

impl Detector {
    fn start() -> Result<Self, String> {
        let (requests, receiver) = mpsc::sync_channel::<DetectionRequest>(1);
        let (sender, results) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("robocam-face-detector".into())
            .spawn(move || {
                let mut classifier = match load_cascade() {
                    Ok(classifier) => classifier,
                    Err(error) => {
                        let _ = sender.send(Err(format!(
                            "Could not load the embedded face detector: {error}"
                        )));
                        return;
                    }
                };
                while let Ok(request) = receiver.recv() {
                    let result = detect_face(&mut classifier, &request.gray)
                        .map(|area| Detection {
                            area,
                            captured_at: request.captured_at,
                            position: request.position,
                        })
                        .map_err(|e| format!("Face detection failed: {e}"));
                    if sender.send(result).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| format!("Could not start face detection: {e}"))?;
        Ok(Self {
            requests: Some(requests),
            results,
            thread: Some(thread),
        })
    }
}

impl Drop for Detector {
    fn drop(&mut self) {
        self.requests.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn load_cascade() -> opencv::Result<objdetect::CascadeClassifier> {
    let storage =
        core::FileStorage::new_def(CASCADE, core::FileStorage_READ | core::FileStorage_MEMORY)?;
    let mut classifier = objdetect::CascadeClassifier::default()?;
    if !classifier.read(&storage.get_first_top_level_node()?)? || classifier.empty()? {
        return Err(opencv::Error::new(
            core::StsError,
            "Embedded face cascade is invalid",
        ));
    }
    Ok(classifier)
}

fn detect_face(
    classifier: &mut objdetect::CascadeClassifier,
    gray: &Mat,
) -> opencv::Result<Option<Rect>> {
    let mut faces = Vector::<Rect>::new();
    classifier.detect_multi_scale(
        gray,
        &mut faces,
        1.5,
        2,
        objdetect::CASCADE_FIND_BIGGEST_OBJECT | objdetect::CASCADE_DO_CANNY_PRUNING,
        Size::new(65, 65),
        Size::default(),
    )?;
    Ok(faces
        .into_iter()
        .max_by_key(|face| face.width * face.height))
}

fn open_camera(device: &CameraDevice, stop: &AtomicBool) -> Result<videoio::VideoCapture, String> {
    let mut errors = Vec::new();
    // DirectShow supports the familiar native camera settings dialog. Media
    // Foundation is a fallback for devices that do not expose DirectShow.
    for (index, backend) in [
        device.dshow_index.map(|index| (index, videoio::CAP_DSHOW)),
        device.msmf_index.map(|index| (index, videoio::CAP_MSMF)),
    ]
    .into_iter()
    .flatten()
    {
        if stop.load(Ordering::Acquire) {
            return Err("Camera stopped".into());
        }
        match videoio::VideoCapture::new(index, backend) {
            Ok(mut camera) => {
                if camera.is_opened().unwrap_or(false) {
                    let _ = camera.set(videoio::CAP_PROP_FRAME_WIDTH, WIDTH as f64);
                    let _ = camera.set(videoio::CAP_PROP_FRAME_HEIGHT, HEIGHT as f64);
                    let _ = camera.set(videoio::CAP_PROP_FPS, 30.0);
                    let _ = camera.set(videoio::CAP_PROP_BUFFERSIZE, 1.0);
                    return Ok(camera);
                }
            }
            Err(error) => errors.push(error.to_string()),
        }
    }
    let detail = if errors.is_empty() {
        String::new()
    } else {
        format!("\n{}", errors.join("\n"))
    };
    Err(format!(
        "Could not open camera {}. Check that it is connected, close other camera apps, and allow desktop camera access in Windows Settings > Privacy & security > Camera.{detail}",
        device.name
    ))
}

fn run_camera(
    config: VisionConfig,
    stop: &AtomicBool,
    settings: &AtomicBool,
    mailbox: &Mutex<Mailbox>,
) -> Result<(), String> {
    let _session = CAMERA_SESSION.lock().unwrap_or_else(|p| p.into_inner());
    if stop.load(Ordering::Acquire) {
        return Ok(());
    }
    if config.camera_index < 0 {
        return Err("Choose a camera index of zero or greater.".into());
    }
    let devices = cameras::enumerate()?;
    let selected = cameras::resolve(&devices, config.camera.as_ref()).cloned();
    mailbox.lock().unwrap_or_else(|p| p.into_inner()).cameras =
        Some((devices, selected.as_ref().ok().cloned()));
    let selected = selected?;
    let mut camera = open_camera(&selected, stop)?;
    // Apply after capture configuration: some drivers reset controls on open.
    let zoom = cameras::driver_zoom(&selected.preference(), config.camera_zoom_steps);
    mailbox.lock().unwrap_or_else(|p| p.into_inner()).zoom = Some(zoom.clone());
    let result = match zoom {
        Err(error) if config.camera_zoom_steps.is_some() => Err(error),
        _ => process_camera(&mut camera, &config, stop, settings, mailbox),
    };
    let _ = camera.release();
    result
}

fn process_camera(
    camera: &mut videoio::VideoCapture,
    config: &VisionConfig,
    stop: &AtomicBool,
    settings: &AtomicBool,
    mailbox: &Mutex<Mailbox>,
) -> Result<(), String> {
    let detector = if config.track_face {
        Some(Detector::start()?)
    } else {
        None
    };
    let mut tracker = FeatureTracker::new(config.manual_area);
    let mut last_detect_request: Option<Instant> = None;
    let mut last_seen: Option<Instant> = None;
    let mut previous_frame = Instant::now();
    let mut fps = 0.0;
    let mut missed_frames = 0;
    while !stop.load(Ordering::Acquire) {
        if settings.swap(false, Ordering::AcqRel) {
            let _ = camera.set(videoio::CAP_PROP_SETTINGS, 1.0);
            // Property dialogs can stop delivery for seconds; seed new features
            // instead of comparing against the image before the dialog.
            tracker.reset();
        }
        let mut captured = Mat::default();
        let read_ok = camera
            .read(&mut captured)
            .map_err(|e| format!("Camera capture failed: {e}"))?;
        if stop.load(Ordering::Acquire) {
            break;
        }
        if !read_ok || captured.empty() {
            missed_frames += 1;
            if missed_frames >= 15 {
                return Err(
                    "The camera stopped delivering frames. Reconnect it and restart the camera."
                        .into(),
                );
            }
            thread::sleep(Duration::from_millis(30));
            continue;
        }
        missed_frames = 0;
        let now = Instant::now();
        let elapsed = now.duration_since(previous_frame).as_secs_f32();
        previous_frame = now;
        if elapsed > 0.0 {
            let measured = (1.0 / elapsed).min(240.0);
            fps = if fps == 0.0 {
                measured
            } else {
                fps * 0.9 + measured * 0.1
            };
        }
        if elapsed > 0.25 {
            tracker.reset();
        }
        let frame = process_frame(
            &captured,
            &mut tracker,
            config,
            &detector,
            &mut last_detect_request,
            &mut last_seen,
            now,
            fps,
        )
        .map_err(|e| format!("Camera image processing failed: {e}"))?;
        if !stop.load(Ordering::Acquire) {
            mailbox
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .publish(frame);
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn process_frame(
    captured: &Mat,
    tracker: &mut FeatureTracker,
    config: &VisionConfig,
    detector: &Option<Detector>,
    last_request: &mut Option<Instant>,
    last_seen: &mut Option<Instant>,
    now: Instant,
    fps: f32,
) -> opencv::Result<VisionFrame> {
    let mut resized = Mat::default();
    imgproc::resize(
        captured,
        &mut resized,
        Size::new(WIDTH, HEIGHT),
        0.0,
        0.0,
        imgproc::INTER_AREA,
    )?;
    let mut mirrored = Mat::default();
    core::flip(&resized, &mut mirrored, 1)?;
    let mut gray = Mat::default();
    imgproc::cvt_color_def(&mirrored, &mut gray, imgproc::COLOR_BGR2GRAY)?;
    if let Some(detector) = detector {
        for detection in detector.results.try_iter() {
            let detection = detection.map_err(|e| opencv::Error::new(core::StsError, e))?;
            if let Some(mut area) = detection.area {
                // The detector ran on an older image. Move its result forward
                // by optical flow measured since that image was submitted.
                area.x += (tracker.position.x - detection.position.x).round() as i32;
                area.y += (tracker.position.y - detection.position.y).round() as i32;
                tracker.set_area(area);
                *last_seen = Some(detection.captured_at);
            }
        }
    }
    let (mut dx, mut dy) = tracker.track(&gray, config.track_face)?;
    if let Some(detector) = detector {
        let interval = Duration::from_millis(config.detection_interval_ms);
        if last_request.is_none_or(|last| now.duration_since(last) >= interval) {
            let request = DetectionRequest {
                gray: gray.try_clone()?,
                captured_at: now,
                position: tracker.position,
            };
            if detector
                .requests
                .as_ref()
                .is_some_and(|sender| sender.try_send(request).is_ok())
            {
                *last_request = Some(now);
            }
        }
    }
    let face_detected = last_seen.is_some_and(|seen| {
        now.duration_since(seen) <= Duration::from_millis(config.face_timeout_ms)
    });
    let tracking_valid = tracker.points.len() >= 3
        && (!config.track_face || !config.stop_on_face_loss || face_detected);
    // Reacquisition establishes a new reference. Never apply the displacement
    // measured while the pointer was disabled to the newly enabled pointer.
    if !tracking_valid || !tracker.was_tracking_valid {
        dx = 0.0;
        dy = 0.0;
    }
    tracker.was_tracking_valid = tracking_valid;
    let mut rgb = Mat::default();
    imgproc::cvt_color_def(&mirrored, &mut rgb, imgproc::COLOR_BGR2RGB)?;
    let area = tracker.area;
    Ok(VisionFrame {
        width: WIDTH as usize,
        height: HEIGHT as usize,
        rgb: rgb.data_bytes()?.to_vec(),
        dx: dx * 2.0,
        dy: dy * 2.0,
        face_detected,
        tracking_valid,
        face_rect: Some([area.x, area.y, area.width, area.height]),
        fps,
        tracked_points: tracker.points.len(),
        captured_at: now,
    })
}

struct FeatureTracker {
    previous: Mat,
    points: Vector<Point2f>,
    area: Rect,
    position: Point2f,
    refresh: bool,
    was_tracking_valid: bool,
}

impl FeatureTracker {
    fn new(area: [f32; 4]) -> Self {
        let mut area = area;
        if !area.iter().all(|value| value.is_finite()) {
            area = VisionConfig::default().manual_area;
        }
        let area = clamp_area(Rect::new(
            (area[0] * WIDTH as f32) as i32,
            (area[1] * HEIGHT as f32) as i32,
            (area[2] * WIDTH as f32) as i32,
            (area[3] * HEIGHT as f32) as i32,
        ));
        Self {
            previous: Mat::default(),
            points: Vector::new(),
            area,
            position: Point2f::default(),
            refresh: true,
            was_tracking_valid: false,
        }
    }

    fn reset(&mut self) {
        self.previous = Mat::default();
        self.points.clear();
        self.refresh = true;
        self.was_tracking_valid = false;
    }

    fn set_area(&mut self, area: Rect) {
        self.area = clamp_area(area);
        self.refresh = true;
    }

    fn seed(&mut self, image: &Mat) -> opencv::Result<()> {
        let mut mask =
            Mat::new_rows_cols_with_default(HEIGHT, WIDTH, core::CV_8UC1, Scalar::all(0.0))?;
        let inner = Rect::new(
            self.area.x + (self.area.width as f32 * 0.3) as i32,
            self.area.y + (self.area.height as f32 * 0.3) as i32,
            (self.area.width as f32 * 0.4) as i32,
            (self.area.height as f32 * 0.4) as i32,
        );
        imgproc::rectangle(
            &mut mask,
            inner,
            Scalar::all(255.0),
            imgproc::FILLED,
            imgproc::LINE_8,
            0,
        )?;
        imgproc::good_features_to_track(
            image,
            &mut self.points,
            CORNERS,
            0.001,
            2.0,
            &mask,
            3,
            false,
            0.04,
        )?;
        if !self.points.is_empty() {
            imgproc::corner_sub_pix(
                image,
                &mut self.points,
                Size::new(5, 5),
                Size::new(-1, -1),
                TermCriteria::new(core::TermCriteria_COUNT | core::TermCriteria_EPS, 20, 0.03)?,
            )?;
        }
        self.refresh = false;
        Ok(())
    }

    fn track(&mut self, gray: &Mat, follow: bool) -> opencv::Result<(f32, f32)> {
        if self.previous.empty() {
            self.seed(gray)?;
            self.previous = gray.try_clone()?;
            return Ok((0.0, 0.0));
        }
        if self.refresh || self.points.len() < CORNERS as usize {
            let previous = self.previous.try_clone()?;
            self.seed(&previous)?;
        }
        let mut next = Vector::<Point2f>::new();
        let mut status = Vector::<u8>::new();
        let mut error = Vector::<f32>::new();
        if !self.points.is_empty() {
            video::calc_optical_flow_pyr_lk(
                &self.previous,
                gray,
                &self.points,
                &mut next,
                &mut status,
                &mut error,
                Size::new(11, 11),
                0,
                TermCriteria::new(core::TermCriteria_COUNT | core::TermCriteria_EPS, 14, 0.03)?,
                0,
                0.0001,
            )?;
        }
        let mut candidates = Vec::new();
        for index in 0..self.points.len() {
            let old = self.points.get(index)?;
            let new = next.get(index)?;
            if status.get(index)? != 0
                && error.get(index)? < 35.0
                && inside(old, self.area)
                && inside(new, Rect::new(0, 0, WIDTH, HEIGHT))
                && new.x.is_finite()
                && new.y.is_finite()
            {
                candidates.push((new, new.x - old.x, new.y - old.y));
            }
        }
        let (dx, dy, retained) = robust_motion(&candidates);
        self.points = retained.into_iter().collect();
        if follow {
            self.area.x += dx.round() as i32;
            self.area.y += dy.round() as i32;
            self.area = clamp_area(self.area);
        }
        self.position.x += dx;
        self.position.y += dy;
        self.previous = gray.try_clone()?;
        Ok((dx, dy))
    }
}

fn inside(point: Point2f, area: Rect) -> bool {
    point.x >= area.x as f32
        && point.y >= area.y as f32
        && point.x < (area.x + area.width) as f32
        && point.y < (area.y + area.height) as f32
}

fn clamp_area(mut area: Rect) -> Rect {
    area.width = area.width.clamp(30, WIDTH);
    area.height = area.height.clamp(30, HEIGHT);
    area.x = area.x.clamp(0, WIDTH - area.width);
    area.y = area.y.clamp(0, HEIGHT - area.height);
    area
}

fn median(values: &mut [f32]) -> f32 {
    values.sort_unstable_by(f32::total_cmp);
    values[values.len() / 2]
}

fn robust_motion(candidates: &[(Point2f, f32, f32)]) -> (f32, f32, Vec<Point2f>) {
    if candidates.len() < 3 {
        return (0.0, 0.0, Vec::new());
    }
    let mx = median(&mut candidates.iter().map(|(_, dx, _)| *dx).collect::<Vec<_>>());
    let my = median(&mut candidates.iter().map(|(_, _, dy)| *dy).collect::<Vec<_>>());
    let mut distances: Vec<f32> = candidates
        .iter()
        .map(|(_, dx, dy)| (dx - mx).hypot(dy - my))
        .collect();
    let tolerance = (median(&mut distances) * 2.5).max(1.0);
    let mut retained = Vec::new();
    let (mut dx, mut dy) = (0.0, 0.0);
    for &(point, x, y) in candidates {
        if (x - mx).hypot(y - my) <= tolerance {
            retained.push(point);
            dx += x;
            dy += y;
        }
    }
    if retained.len() < 3 {
        return (0.0, 0.0, Vec::new());
    }
    dx /= retained.len() as f32;
    dy /= retained.len() as f32;
    // A large displacement is outside the useful range of the 11 x 11,
    // single-level LK window. Treat it as a lost reference instead of moving
    // the cursor on an ambiguous match (e.g. a camera bump or repeated texture).
    if dx.hypot(dy) > 12.0 {
        return (0.0, 0.0, Vec::new());
    }
    (dx, dy, retained)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_face_detector_loads() {
        let mut cascade = load_cascade().unwrap();
        let blank = Mat::new_rows_cols_with_default(HEIGHT, WIDTH, core::CV_8UC1, Scalar::all(0.0))
            .unwrap();
        assert!(detect_face(&mut cascade, &blank).unwrap().is_none());
    }

    /// Supply 320 x 240 grayscale bytes from a frontal-face reference image.
    /// Kept optional so tests need neither a webcam nor a third-party portrait.
    #[test]
    #[ignore = "requires ROBOCAM_FACE_TEST_GRAY pointing to a 320 x 240 grayscale reference"]
    fn reference_face_is_detected() {
        let path = std::env::var_os("ROBOCAM_FACE_TEST_GRAY").expect("set ROBOCAM_FACE_TEST_GRAY");
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(bytes.len(), (WIDTH * HEIGHT) as usize);
        let pixels = Mat::from_slice(&bytes).unwrap();
        let image = pixels.reshape(1, HEIGHT).unwrap().try_clone().unwrap();
        let mut cascade = load_cascade().unwrap();
        let face = detect_face(&mut cascade, &image)
            .unwrap()
            .expect("frontal face");
        assert!(face.width >= 65 && face.height >= 65);
    }

    #[test]
    fn mailbox_is_bounded_and_discards_motion_after_a_gui_stall() {
        let mut mailbox = Mailbox::default();
        let start = Instant::now();
        for millis in [0, 50, 100, 150, 200] {
            mailbox.publish(VisionFrame {
                width: 1,
                height: 1,
                rgb: vec![0, 0, 0],
                dx: 2.0,
                dy: 1.0,
                face_detected: true,
                tracking_valid: true,
                face_rect: None,
                fps: 30.0,
                tracked_points: 20,
                captured_at: start + Duration::from_millis(millis),
            });
        }
        assert_eq!(mailbox.frame.as_ref().unwrap().dx, 10.0);
        mailbox.publish(VisionFrame {
            width: 1,
            height: 1,
            rgb: vec![255, 0, 0],
            dx: 2.0,
            dy: 1.0,
            face_detected: true,
            tracking_valid: true,
            face_rect: None,
            fps: 30.0,
            tracked_points: 20,
            captured_at: start + Duration::from_millis(300),
        });
        let latest = mailbox.frame.unwrap();
        assert_eq!(latest.dx, 2.0);
        assert_eq!(latest.rgb, [255, 0, 0]);
    }

    #[test]
    fn invalid_camera_fails_asynchronously_and_stops() {
        let worker = VisionWorker::start(VisionConfig {
            camera_index: -1,
            ..VisionConfig::default()
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut saw_error = false;
        let mut saw_stop = false;
        while Instant::now() < deadline && !saw_stop {
            match worker.try_recv() {
                Some(VisionEvent::Error(_)) => saw_error = true,
                Some(VisionEvent::Stopped) => saw_stop = true,
                Some(VisionEvent::Frame(_)) => panic!("invalid camera delivered a frame"),
                Some(VisionEvent::Zoom(_)) => panic!("invalid camera exposed zoom controls"),
                Some(VisionEvent::Cameras { .. }) => {
                    panic!("invalid camera should fail before enumeration")
                }
                None => thread::sleep(Duration::from_millis(5)),
            }
        }
        assert!(saw_error && saw_stop);
    }

    #[test]
    fn replacement_session_waits_for_release_and_can_be_cancelled() {
        let previous_session = CAMERA_SESSION.lock().unwrap_or_else(|p| p.into_inner());
        let worker = VisionWorker::start(VisionConfig {
            camera_index: -1,
            ..VisionConfig::default()
        });
        thread::sleep(Duration::from_millis(20));
        // Opening camera -1 would fail immediately; no error means the worker
        // has correctly waited for the previous camera owner to release it.
        assert!(worker.try_recv().is_none());
        worker.stop();
        drop(previous_session);
        let deadline = Instant::now() + Duration::from_secs(2);
        while !worker.is_finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(worker.is_finished());
        assert!(matches!(worker.try_recv(), Some(VisionEvent::Stopped)));
        assert!(worker.try_recv().is_none());
    }

    #[test]
    fn optical_flow_recovers_translation_and_reset_prevents_jump() {
        let mut first =
            Mat::new_rows_cols_with_default(HEIGHT, WIDTH, core::CV_8UC1, Scalar::all(25.0))
                .unwrap();
        for y in (80..170).step_by(10) {
            for x in (100..230).step_by(10) {
                imgproc::rectangle(
                    &mut first,
                    Rect::new(x, y, 4, 4),
                    Scalar::all(230.0),
                    imgproc::FILLED,
                    imgproc::LINE_8,
                    0,
                )
                .unwrap();
            }
        }
        let transform = Mat::from_slice_2d(&[[1.0_f64, 0.0, 2.0], [0.0, 1.0, -1.0]]).unwrap();
        let mut second = Mat::default();
        imgproc::warp_affine(
            &first,
            &mut second,
            &transform,
            Size::new(WIDTH, HEIGHT),
            imgproc::INTER_LINEAR,
            core::BORDER_CONSTANT,
            Scalar::all(25.0),
        )
        .unwrap();
        let mut tracker = FeatureTracker::new([0.15, 0.1, 0.7, 0.8]);
        assert_eq!(tracker.track(&first, true).unwrap(), (0.0, 0.0));
        let (dx, dy) = tracker.track(&second, true).unwrap();
        assert!((dx - 2.0).abs() < 0.2, "dx={dx}");
        assert!((dy + 1.0).abs() < 0.2, "dy={dy}");
        tracker.reset();
        assert_eq!(tracker.track(&first, true).unwrap(), (0.0, 0.0));

        // Exercise the whole image pipeline too: mirroring reverses camera X,
        // and the output scale is two units per camera pixel.
        let mut first_bgr = Mat::default();
        let mut second_bgr = Mat::default();
        imgproc::cvt_color_def(&first, &mut first_bgr, imgproc::COLOR_GRAY2BGR).unwrap();
        imgproc::cvt_color_def(&second, &mut second_bgr, imgproc::COLOR_GRAY2BGR).unwrap();
        let config = VisionConfig {
            track_face: false,
            manual_area: [0.15, 0.1, 0.7, 0.8],
            ..VisionConfig::default()
        };
        let mut tracker = FeatureTracker::new(config.manual_area);
        let mut requested = None;
        let mut seen = None;
        let now = Instant::now();
        let initial = process_frame(
            &first_bgr,
            &mut tracker,
            &config,
            &None,
            &mut requested,
            &mut seen,
            now,
            30.0,
        )
        .unwrap();
        assert!(initial.tracking_valid);
        assert!(!initial.face_detected);
        assert_eq!((initial.dx, initial.dy), (0.0, 0.0));
        let moving = process_frame(
            &second_bgr,
            &mut tracker,
            &config,
            &None,
            &mut requested,
            &mut seen,
            now + Duration::from_millis(33),
            30.0,
        )
        .unwrap();
        assert!(moving.tracking_valid);
        assert!((moving.dx + 4.0).abs() < 0.4, "mirrored dx={}", moving.dx);
        assert!((moving.dy + 2.0).abs() < 0.4, "dy={}", moving.dy);
        // Expired automatic face detection gates all motion. Reacquisition
        // restores validity without a jump from the disabled period.
        let auto = VisionConfig {
            track_face: true,
            ..config
        };
        let lost = process_frame(
            &first_bgr,
            &mut tracker,
            &auto,
            &None,
            &mut requested,
            &mut seen,
            now + Duration::from_millis(66),
            30.0,
        )
        .unwrap();
        assert!(!lost.tracking_valid);
        assert_eq!((lost.dx, lost.dy), (0.0, 0.0));
        seen = Some(now + Duration::from_millis(99));
        let reacquired = process_frame(
            &second_bgr,
            &mut tracker,
            &auto,
            &None,
            &mut requested,
            &mut seen,
            now + Duration::from_millis(99),
            30.0,
        )
        .unwrap();
        assert!(reacquired.tracking_valid);
        assert_eq!((reacquired.dx, reacquired.dy), (0.0, 0.0));

        // Robo keeps following valid features when the face classifier misses
        // the face, but still stops if optical flow itself loses the target.
        let robo_tracking = VisionConfig {
            stop_on_face_loss: false,
            ..auto
        };
        seen = None;
        let without_face = process_frame(
            &first_bgr,
            &mut tracker,
            &robo_tracking,
            &None,
            &mut requested,
            &mut seen,
            now + Duration::from_millis(132),
            30.0,
        )
        .unwrap();
        assert!(!without_face.face_detected);
        assert!(without_face.tracking_valid);
        assert!(without_face.tracking_ready());
        assert!((without_face.dx - 4.0).abs() < 0.4);
        assert!((without_face.dy - 2.0).abs() < 0.4);
        let blank = Mat::new_rows_cols_with_default(HEIGHT, WIDTH, core::CV_8UC3, Scalar::all(0.0))
            .unwrap();
        let without_features = process_frame(
            &blank,
            &mut tracker,
            &robo_tracking,
            &None,
            &mut requested,
            &mut seen,
            now + Duration::from_millis(165),
            30.0,
        )
        .unwrap();
        assert!(!without_features.tracking_valid);
        assert!(!without_features.tracking_ready());
        assert_eq!((without_features.dx, without_features.dy), (0.0, 0.0));
    }

    #[test]
    fn inconsistent_feature_does_not_move_pointer() {
        let candidates = vec![
            (Point2f::default(), 2.0, 1.0),
            (Point2f::default(), 2.1, 0.9),
            (Point2f::default(), 1.9, 1.1),
            (Point2f::default(), 100.0, -80.0),
        ];
        let (dx, dy, retained) = robust_motion(&candidates);
        assert_eq!(retained.len(), 3);
        assert!((dx - 2.0).abs() < 0.001 && (dy - 1.0).abs() < 0.001);
        let implausible = vec![(Point2f::default(), 20.0, 0.0); 20];
        let (dx, dy, retained) = robust_motion(&implausible);
        assert_eq!((dx, dy), (0.0, 0.0));
        assert!(retained.is_empty());
    }
}
