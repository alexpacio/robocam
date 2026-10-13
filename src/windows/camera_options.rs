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
}
