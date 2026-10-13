use crate::engine::{ClickSettings, MotionSettings};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, io, path::PathBuf};

pub const ROBO_PROFILE_NAME: &str = "Robo";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub motion: MotionSettings,
    pub click: ClickSettings,
    pub camera_index: i32,
    pub camera: Option<crate::cameras::CameraPreference>,
    pub track_face: bool,
    pub stop_on_face_loss: bool,
    pub face_timeout_ms: u64,
    pub detection_interval_ms: u64,
    pub manual_area: [f32; 4],
    pub fast_bar: bool,
    pub bar_position: u8,
    pub thin_bar: bool,
    pub auto_hide: bool,
    pub show_bar: bool,
    pub click_sound: bool,
    pub pause_on_mouse_ms: u64,
    pub keyboard_command: String,
    pub toggle_key: u32,
    pub center_key: u32,
    pub workspace_key: u32,
    pub arrow_sensitivity: bool,
    pub event_delay_ms: u64,
    /// None retains the startup preference saved by older robocam versions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled_at_startup: Option<bool>,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            motion: MotionSettings::default(),
            click: ClickSettings::default(),
            camera_index: 0,
            camera: None,
            track_face: true,
            stop_on_face_loss: true,
            face_timeout_ms: 500,
            detection_interval_ms: 250,
            manual_area: [0.25, 0.15, 0.5, 0.6],
            fast_bar: false,
            bar_position: 0,
            thin_bar: false,
            auto_hide: false,
            show_bar: true,
            click_sound: true,
            pause_on_mouse_ms: 1000,
            keyboard_command: "osk.exe".into(),
            toggle_key: 0x7A,
            center_key: 0x79,
            workspace_key: 0x78,
            arrow_sensitivity: false,
            event_delay_ms: 0,
            enabled_at_startup: None,
        }
    }
}

impl Profile {
    /// User-supplied configuration for the built-in Robo preset.
    pub fn robo() -> Self {
        Self {
            motion: MotionSettings {
                speed_x: 9,
                speed_y: 9,
                acceleration: 3,
                smoothing: 3,
                easy_stop: 2,
                restricted_workspace: false,
                margins_percent: [1; 4],
                wrap_pointer: false,
                ..MotionSettings::default()
            },
            click: ClickSettings {
                dwell_ms: 400,
                tolerance_px: 2,
                ..ClickSettings::default()
            },
            enabled_at_startup: Some(true),
            track_face: true,
            stop_on_face_loss: false,
            face_timeout_ms: 5_000,
            // High detection frequency (Italian: Elevato) runs every 100 ms.
            detection_interval_ms: 100,
            keyboard_command: "osk.exe".into(),
            event_delay_ms: 0,
            click_sound: false,
            // Do not add a physical-mouse cooldown before dwelling in this preset.
            pause_on_mouse_ms: 0,
            toggle_key: 0,
            center_key: 0,
            workspace_key: 0,
            manual_area: [0.309375, 0.2458335, 0.38125, 0.508333],
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub active_profile: String,
    pub profiles: BTreeMap<String, Profile>,
    pub enabled_at_startup: bool,
    pub minimized_at_startup: bool,
    pub wizard_completed: bool,
    pub check_updates: bool,
    pub language: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            active_profile: "Default".into(),
            profiles: [
                ("Default".into(), Profile::default()),
                (ROBO_PROFILE_NAME.into(), Profile::robo()),
            ]
            .into(),
            enabled_at_startup: false,
            minimized_at_startup: false,
            wizard_completed: false,
            check_updates: true,
            language: "en".into(),
        }
    }
}

impl Settings {
    pub fn path() -> PathBuf {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("robocam")
            .join("settings.json")
    }

    pub fn load() -> (Self, Option<String>) {
        let path = Self::path();
        match fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<Self>(&bytes) {
                Ok(mut settings) => {
                    settings.normalize();
                    (settings, None)
                }
                Err(e) => {
                    // Keep a recoverable copy before a future explicit save replaces invalid data.
                    let backup = path.with_extension("invalid.json");
                    let copy_error = fs::copy(&path, &backup).err();
                    (
                        Self::default(),
                        Some(format!(
                            "Could not read settings: {e}. {}",
                            copy_error
                                .map(|e| format!("Backup failed: {e}"))
                                .unwrap_or_else(|| format!(
                                    "Original saved to {}",
                                    backup.display()
                                ))
                        )),
                    )
                }
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => (Self::default(), None),
            Err(e) => (
                Self::default(),
                Some(format!("Could not open {}: {e}", path.display())),
            ),
        }
    }

    pub fn profile(&self) -> &Profile {
        &self.profiles[&self.active_profile]
    }
    pub fn profile_mut(&mut self) -> &mut Profile {
        self.profiles.get_mut(&self.active_profile).unwrap()
    }

    pub fn startup_enabled(&self) -> bool {
        self.profile()
            .enabled_at_startup
            .unwrap_or(self.enabled_at_startup)
    }

    fn normalize(&mut self) {
        if self.profiles.is_empty() {
            self.profiles.insert("Default".into(), Profile::default());
        }
        // Introduce the preset on upgrade without replacing an existing Robo.
        self.profiles
            .entry(ROBO_PROFILE_NAME.into())
            .or_insert_with(Profile::robo);
        if !self.profiles.contains_key(&self.active_profile) {
            self.active_profile = self.profiles.keys().next().unwrap().clone();
        }
        for profile in self.profiles.values_mut() {
            profile.sanitize();
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path();
        fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        let temporary = path.with_extension("tmp");
        {
            use std::io::Write;
            let mut file = fs::File::create(&temporary).map_err(|e| e.to_string())?;
            file.write_all(&bytes).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            let from: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
            let to: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            unsafe {
                if windows_sys::Win32::Storage::FileSystem::MoveFileExW(
                    from.as_ptr(),
                    to.as_ptr(),
                    windows_sys::Win32::Storage::FileSystem::MOVEFILE_REPLACE_EXISTING
                        | windows_sys::Win32::Storage::FileSystem::MOVEFILE_WRITE_THROUGH,
                ) == 0
                {
                    return Err(io::Error::last_os_error().to_string());
                }
            }
        }
        #[cfg(not(windows))]
        fs::rename(temporary, path).map_err(|e| e.to_string())?;
        Ok(())
    }
}

impl Profile {
    pub fn sanitize(&mut self) {
        self.camera_index = self.camera_index.clamp(0, 32);
        self.face_timeout_ms = self.face_timeout_ms.clamp(100, 59_000);
        self.detection_interval_ms = self.detection_interval_ms.clamp(50, 2000);
        self.bar_position = self.bar_position.min(4);
        self.pause_on_mouse_ms = self.pause_on_mouse_ms.min(10000);
        self.event_delay_ms = self.event_delay_ms.min(50);
        if self.manual_area.iter().any(|n| !n.is_finite()) {
            self.manual_area = Self::default().manual_area;
        }
        self.manual_area[0] = self.manual_area[0].clamp(0.0, 0.9);
        self.manual_area[1] = self.manual_area[1].clamp(0.0, 0.9);
        self.manual_area[2] = self.manual_area[2].clamp(0.1, 1.0 - self.manual_area[0]);
        self.manual_area[3] = self.manual_area[3].clamp(0.1, 1.0 - self.manual_area[1]);
        self.motion.speed_x = self.motion.speed_x.min(30);
        self.motion.speed_y = self.motion.speed_y.min(30);
        self.motion.acceleration = self.motion.acceleration.min(5);
        self.motion.smoothing = self.motion.smoothing.min(8);
        self.motion.easy_stop = self.motion.easy_stop.min(10);
        for margin in &mut self.motion.margins_percent {
            *margin = (*margin).min(50);
        }
        self.click.dwell_ms = self.click.dwell_ms.clamp(200, 5000);
        self.click.tolerance_px = self.click.tolerance_px.clamp(1, 8);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_choice_is_saved_and_can_be_forgotten_per_profile() {
        let mut settings = Settings {
            active_profile: ROBO_PROFILE_NAME.into(),
            ..Settings::default()
        };
        settings.profile_mut().camera = Some(crate::cameras::CameraPreference {
            id: "device-42".into(),
            name: "Desk camera".into(),
        });
        let mut loaded: Settings =
            serde_json::from_slice(&serde_json::to_vec(&settings).unwrap()).unwrap();
        assert_eq!(loaded.profile().camera.as_ref().unwrap().id, "device-42");
        assert!(loaded.profiles["Default"].camera.is_none());
        loaded.profile_mut().camera = None;
        let forgotten: Settings =
            serde_json::from_slice(&serde_json::to_vec(&loaded).unwrap()).unwrap();
        assert!(forgotten.profile().camera.is_none());
    }

    #[test]
    fn robo_preset_matches_reference_configuration() {
        let mut settings = Settings::default();
        settings.normalize();
        let robo = &settings.profiles[ROBO_PROFILE_NAME];
        assert_eq!((robo.motion.speed_x, robo.motion.speed_y), (9, 9));
        assert_eq!(
            (
                robo.motion.acceleration,
                robo.motion.smoothing,
                robo.motion.easy_stop
            ),
            (3, 3, 2)
        );
        assert!(!robo.motion.restricted_workspace);
        assert_eq!(robo.motion.margins_percent, [1; 4]);
        assert!(!robo.motion.wrap_pointer);
        assert!(robo.track_face);
        assert!(!robo.stop_on_face_loss);
        assert_eq!(robo.face_timeout_ms, 5_000);
        assert_eq!(robo.detection_interval_ms, 100);
        assert_eq!(robo.enabled_at_startup, Some(true));
        assert_eq!(robo.keyboard_command, "osk.exe");
        assert_eq!(robo.event_delay_ms, 0);
        assert_eq!(robo.click.dwell_ms, 400);
        assert_eq!(robo.click.tolerance_px, 2);
        assert!(!robo.click.consecutive);
        assert!(!robo.click_sound);
        assert_eq!(robo.pause_on_mouse_ms, 0);
        assert_eq!(
            (robo.toggle_key, robo.center_key, robo.workspace_key),
            (0, 0, 0)
        );
    }

    #[test]
    fn upgrade_adds_robo_without_changing_selected_profile_or_legacy_startup() {
        let mut settings: Settings = serde_json::from_str(
            r#"{
            "active_profile":"Alex", "enabled_at_startup":true,
            "profiles":{"Alex":{"camera_index":2,"motion":{"speed_x":18}}}
        }"#,
        )
        .unwrap();
        settings.normalize();
        assert_eq!(settings.active_profile, "Alex");
        assert_eq!(settings.profile().camera_index, 2);
        assert_eq!(settings.profile().motion.speed_x, 18);
        assert!(settings.startup_enabled());
        assert!(settings.profile().stop_on_face_loss);
        assert!(settings.profiles.contains_key(ROBO_PROFILE_NAME));
    }

    #[test]
    fn existing_robo_customizations_survive_loading_and_roundtrip() {
        let mut settings: Settings = serde_json::from_str(
            r#"{
            "active_profile":"Robo", "enabled_at_startup":true,
            "profiles":{"Robo":{"motion":{"speed_x":16},"enabled_at_startup":false}}
        }"#,
        )
        .unwrap();
        settings.normalize();
        let mut reloaded: Settings =
            serde_json::from_slice(&serde_json::to_vec(&settings).unwrap()).unwrap();
        reloaded.normalize();
        assert_eq!(reloaded.profile().motion.speed_x, 16);
        assert_eq!(reloaded.profile().enabled_at_startup, Some(false));
        assert!(!reloaded.startup_enabled());
    }

    #[test]
    fn startup_override_is_scoped_to_the_selected_profile() {
        let mut settings = Settings::default();
        assert!(!settings.startup_enabled());
        settings.active_profile = ROBO_PROFILE_NAME.into();
        assert!(settings.startup_enabled());
        settings.profile_mut().enabled_at_startup = Some(false);
        assert!(!settings.startup_enabled());
        settings.active_profile = "Default".into();
        assert!(!settings.startup_enabled());
        settings.enabled_at_startup = true;
        assert!(settings.startup_enabled());
        settings.active_profile = ROBO_PROFILE_NAME.into();
        assert!(!settings.startup_enabled());
    }

    #[test]
    fn old_partial_settings_keep_safe_defaults() {
        let settings: Settings = serde_json::from_str(
            r#"{"profiles":{"Alex":{"camera_index":2}},"active_profile":"Alex"}"#,
        )
        .unwrap();
        assert_eq!(settings.profile().camera_index, 2);
        assert!(!settings.enabled_at_startup);
        assert_eq!(settings.profile().click.dwell_ms, 1000);
    }
    #[test]
    fn profile_roundtrip_preserves_independent_settings() {
        let mut settings = Settings::default();
        let mut alternative = Profile::default();
        alternative.motion.speed_x = 20;
        settings.profiles.insert("Alternative".into(), alternative);
        let decoded: Settings =
            serde_json::from_slice(&serde_json::to_vec(&settings).unwrap()).unwrap();
        assert_eq!(decoded.profile().motion.speed_x, 10);
        assert_eq!(decoded.profiles["Alternative"].motion.speed_x, 20);
    }
}
