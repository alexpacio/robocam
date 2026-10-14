use super::*;

impl App {
    pub(super) unsafe fn fill_camera_choices(&mut self, selected: Option<CameraPreference>) {
        if self.options.is_null() {
            return;
        }
        let chooser = GetDlgItem(self.options, 440);
        if chooser.is_null() {
            return;
        }
        SendMessageW(chooser, CB_RESETCONTENT, 0, 0);
        SendMessageW(
            chooser,
            CB_ADDSTRING,
            0,
            wide("Automatic (ask when multiple)").as_ptr() as isize,
        );
        self.camera_choices = vec![None];
        let mut selected_index = 0;
        for (index, device) in self.camera_devices.iter().enumerate() {
            let duplicates = self
                .camera_devices
                .iter()
                .filter(|other| other.name == device.name)
                .count()
                > 1;
            let label = if duplicates {
                format!("{} ({})", device.name, index + 1)
            } else {
                device.name.clone()
            };
            SendMessageW(chooser, CB_ADDSTRING, 0, wide_raw(&label).as_ptr() as isize);
            self.camera_choices.push(Some(device.preference()));
            if selected
                .as_ref()
                .is_some_and(|saved| device.matches_id(&saved.id))
            {
                selected_index = index + 1;
            }
        }
        if selected_index == 0
            && let Some(saved) = selected
        {
            let label = format!("{}: {}", crate::i18n::translate("Unavailable"), saved.name);
            SendMessageW(chooser, CB_ADDSTRING, 0, wide_raw(&label).as_ptr() as isize);
            self.camera_choices.push(Some(saved));
            selected_index = self.camera_choices.len() - 1;
        }
        SendMessageW(chooser, CB_SETCURSEL, selected_index, 0);
        if !GetDlgItem(self.options, 449).is_null() {
            self.query_zoom_range();
        }
    }

    pub(super) unsafe fn set_camera_devices(&mut self, devices: Vec<CameraDevice>) {
        let selected = if !self.options.is_null() && !self.camera_choices.is_empty() {
            self.camera_choices
                .get(self.selected(440))
                .cloned()
                .flatten()
        } else {
            self.config.profile().camera.clone()
        };
        self.camera_devices = devices;
        self.fill_camera_choices(selected);
    }

    pub(super) unsafe fn poll_camera_discovery(&mut self) {
        if let Some(result) = self
            .camera_discovery
            .as_ref()
            .and_then(|receiver| receiver.try_recv().ok())
        {
            self.camera_discovery = None;
            match result {
                Ok(devices) => self.set_camera_devices(devices),
                Err(error) => message(self.options, &error),
            }
        }
    }

    unsafe fn selected_zoom_camera(&self) -> Result<CameraPreference, String> {
        if let Some(Some(camera)) = self.camera_choices.get(self.selected(440)) {
            return Ok(camera.clone());
        }
        cameras::resolve(&self.camera_devices, None).map(CameraDevice::preference)
    }

    pub(super) unsafe fn query_zoom_range(&mut self) {
        self.zoom_query = None;
        self.zoom_capabilities = None;
        if self.options.is_null() {
            return;
        }
        EnableWindow(GetDlgItem(self.options, 448), 0);
        match self.selected_zoom_camera() {
            Ok(camera) => {
                set_text(GetDlgItem(self.options, 449), "Reading driver zoom range…");
                let (sender, receiver) = std::sync::mpsc::channel();
                self.zoom_query = Some(receiver);
                let _ = std::thread::Builder::new()
                    .name("robocam-zoom-range".into())
                    .spawn(move || {
                        let result = cameras::driver_zoom(&camera, None);
                        let _ = sender.send((camera, result));
                    });
            }
            Err(error) => set_text(GetDlgItem(self.options, 449), &error),
        }
    }

    pub(super) unsafe fn poll_zoom_range(&mut self) {
        let Some((camera, result)) = self.zoom_query.as_ref().and_then(|r| r.try_recv().ok())
        else {
            return;
        };
        self.zoom_query = None;
        if self.options.is_null() || self.selected_zoom_camera().ok().as_ref() != Some(&camera) {
            return;
        }
        if let Ok(range) = &result
            && self.config.profile().camera_zoom_steps.is_none()
        {
            let increments =
                (i64::from(range.current) - i64::from(range.minimum)) / i64::from(range.step);
            set_text(GetDlgItem(self.options, 448), &increments.to_string());
        }
        self.zoom_capabilities = Some((camera, result));
        self.update_zoom_options();
    }

    pub(super) unsafe fn update_zoom_options(&self) {
        if self.options.is_null() {
            return;
        }
        let range = self.zoom_capabilities.as_ref().map(|(_, result)| result);
        EnableWindow(
            GetDlgItem(self.options, 448),
            i32::from(self.checked(447) && range.is_some_and(Result::is_ok)),
        );
        if let Some(result) = range {
            let label = match result {
                Ok(z) => format!(
                    "{} {}–{}; {} {}; {} {}.\n{} 0–{}; {} {}.",
                    crate::i18n::translate("Driver range:"),
                    z.minimum,
                    z.maximum,
                    crate::i18n::translate("step:"),
                    z.step,
                    crate::i18n::translate("current:"),
                    z.current,
                    crate::i18n::translate("Increments:"),
                    z.maximum_increments().unwrap_or(0),
                    crate::i18n::translate("0 = driver value"),
                    z.minimum
                ),
                Err(error) => format!(
                    "{} {error}",
                    crate::i18n::translate("Native zoom unavailable.")
                ),
            };
            set_text_raw(GetDlgItem(self.options, 449), &label);
        }
    }
}
