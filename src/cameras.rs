//! Device identities are persisted instead of volatile capture-device indices.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CameraPreference {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CameraDevice {
    pub id: String,
    pub name: String,
    pub dshow_index: Option<i32>,
    pub msmf_index: Option<i32>,
    pub aliases: Vec<String>,
}

impl CameraDevice {
    pub fn matches_id(&self, id: &str) -> bool {
        self.id.eq_ignore_ascii_case(id)
            || self
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(id))
    }
    pub fn preference(&self) -> CameraPreference {
        CameraPreference {
            id: self.id.clone(),
            name: self.name.clone(),
        }
    }
}

pub fn resolve<'a>(
    devices: &'a [CameraDevice],
    saved: Option<&CameraPreference>,
) -> Result<&'a CameraDevice, String> {
    if let Some(saved) = saved {
        return devices.iter().find(|device| device.matches_id(&saved.id))
            .ok_or_else(|| format!("Saved camera unavailable: {}. Connect it or choose another camera in Configuration.", saved.name));
    }
    match devices {
        [device] => Ok(device),
        [] => Err(
            "No cameras detected. Connect a camera and refresh the camera list in Configuration."
                .into(),
        ),
        _ => Err(
            "Multiple cameras detected. Choose a camera in Configuration and save the selection."
                .into(),
        ),
    }
}

#[cfg(windows)]
pub fn discover() -> std::sync::mpsc::Receiver<Result<Vec<CameraDevice>, String>> {
    let (sender, receiver) = std::sync::mpsc::channel();
    let fallback = sender.clone();
    if let Err(error) = std::thread::Builder::new()
        .name("robocam-camera-list".into())
        .spawn(move || {
            let _ = sender.send(enumerate());
        })
    {
        let _ = fallback.send(Err(error.to_string()));
    }
    receiver
}

#[cfg(windows)]
pub fn enumerate() -> Result<Vec<CameraDevice>, String> {
    native::enumerate()
}

#[cfg(windows)]
mod native {
    use super::*;
    use ::windows::{
        Win32::{
            Devices::DeviceAndDriverInstallation::*,
            Media::{DirectShow::ICreateDevEnum, MediaFoundation::*},
            System::{
                Com::{StructuredStorage::IPropertyBag, *},
                Variant::{VARIANT, VT_BSTR, VariantClear},
            },
        },
        core::{GUID, PWSTR, w},
    };

    struct Com;
    impl Drop for Com {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }
    struct Foundation;
    impl Drop for Foundation {
        fn drop(&mut self) {
            unsafe {
                let _ = MFShutdown();
            }
        }
    }

    fn canonical_id(path: &str) -> String {
        path.strip_prefix("@device:pnp:")
            .unwrap_or(path)
            .to_lowercase()
    }

    unsafe fn device_identity(path: &str) -> String {
        struct DeviceSet(HDEVINFO);
        impl Drop for DeviceSet {
            fn drop(&mut self) {
                unsafe {
                    let _ = SetupDiDestroyDeviceInfoList(self.0);
                }
            }
        }
        let fallback = canonical_id(path);
        let identity = (|| -> Option<String> {
            unsafe {
                let set = DeviceSet(SetupDiCreateDeviceInfoList(None, None).ok()?);
                let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
                let mut interface = SP_DEVICE_INTERFACE_DATA {
                    cbSize: std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
                    ..Default::default()
                };
                SetupDiOpenDeviceInterfaceW(
                    set.0,
                    ::windows::core::PCWSTR(wide.as_ptr()),
                    0,
                    Some(&mut interface),
                )
                .ok()?;
                let mut size = 0;
                let _ = SetupDiGetDeviceInterfaceDetailW(
                    set.0,
                    &interface,
                    None,
                    0,
                    Some(&mut size),
                    None,
                );
                if size < std::mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32
                    || size > 1_048_576
                {
                    return None;
                }
                let mut buffer =
                    vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
                let detail = buffer
                    .as_mut_ptr()
                    .cast::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>();
                (*detail).cbSize = std::mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
                let mut device = SP_DEVINFO_DATA {
                    cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
                    ..Default::default()
                };
                SetupDiGetDeviceInterfaceDetailW(
                    set.0,
                    &interface,
                    Some(detail),
                    size,
                    None,
                    Some(&mut device),
                )
                .ok()?;
                let mut identifier = [0u16; 1024];
                SetupDiGetDeviceInstanceIdW(set.0, &device, Some(&mut identifier), None).ok()?;
                let end = identifier
                    .iter()
                    .position(|value| *value == 0)
                    .unwrap_or(identifier.len());
                let instance = String::from_utf16_lossy(&identifier[..end]).to_lowercase();
                let reference = fallback
                    .rsplit_once('}')
                    .map_or("", |(_, reference)| reference);
                Some(format!("device:{instance}|{reference}"))
            }
        })();
        identity.unwrap_or(fallback)
    }

    unsafe fn property(bag: &IPropertyBag, key: ::windows::core::PCWSTR) -> Option<String> {
        unsafe {
            let mut value = VARIANT::default();
            let result = bag.Read(key, &mut value, None::<&IErrorLog>);
            let text = if result.is_ok() && value.Anonymous.Anonymous.vt == VT_BSTR {
                Some(value.Anonymous.Anonymous.Anonymous.bstrVal.to_string())
            } else {
                None
            };
            let _ = VariantClear(&mut value);
            text
        }
    }

    unsafe fn direct_show() -> ::windows::core::Result<Vec<CameraDevice>> {
        unsafe {
            let enumerator: ICreateDevEnum =
                CoCreateInstance(&CLSID_SystemDeviceEnum, None, CLSCTX_INPROC_SERVER)?;
            let mut monikers = None;
            enumerator.CreateClassEnumerator(&CLSID_VideoInputDeviceCategory, &mut monikers, 0)?;
            let Some(monikers) = monikers else {
                return Ok(Vec::new());
            };
            let mut devices = Vec::new();
            let mut index = 0;
            loop {
                let mut item = [None];
                let mut fetched = 0;
                let status = monikers.Next(&mut item, Some(&mut fetched));
                status.ok()?;
                if fetched == 0 {
                    break;
                }
                if let Some(moniker) = item[0].take() {
                    let bag: IPropertyBag =
                        moniker.BindToStorage(None::<&IBindCtx>, None::<&IMoniker>)?;
                    let name = property(&bag, w!("FriendlyName"))
                        .unwrap_or_else(|| format!("Camera {}", index + 1));
                    let path = if let Some(path) = property(&bag, w!("DevicePath")) {
                        path
                    } else {
                        let context = CreateBindCtx(0)?;
                        let value = moniker.GetDisplayName(&context, None::<&IMoniker>)?;
                        let text = value.to_string();
                        CoTaskMemFree(Some(value.0.cast()));
                        text?
                    };
                    devices.push(CameraDevice {
                        id: device_identity(&path),
                        name,
                        dshow_index: Some(index),
                        msmf_index: None,
                        aliases: vec![canonical_id(&path)],
                    });
                }
                index += 1;
            }
            Ok(devices)
        }
    }

    unsafe fn mf_string(device: &IMFActivate, key: &GUID) -> Option<String> {
        unsafe {
            let mut value = PWSTR::null();
            let mut length = 0;
            device
                .GetAllocatedString(key, &mut value, &mut length)
                .ok()?;
            let text = value.to_string().ok();
            CoTaskMemFree(Some(value.0.cast()));
            text
        }
    }

    unsafe fn media_foundation() -> ::windows::core::Result<Vec<CameraDevice>> {
        unsafe {
            MFStartup(MF_VERSION, MFSTARTUP_LITE)?;
            let _foundation = Foundation;
            let mut attributes = None;
            MFCreateAttributes(&mut attributes, 1)?;
            let attributes = attributes.expect("MFCreateAttributes succeeded without attributes");
            attributes.SetGUID(
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
            )?;
            let mut array = std::ptr::null_mut();
            let mut count = 0;
            MFEnumDeviceSources(&attributes, &mut array, &mut count)?;
            let mut devices = Vec::new();
            for index in 0..count {
                if let Some(device) = (*array.add(index as usize)).take()
                    && let Some(path) = mf_string(
                        &device,
                        &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
                    )
                {
                    let name = mf_string(&device, &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME)
                        .unwrap_or_else(|| format!("Camera {}", index + 1));
                    devices.push(CameraDevice {
                        id: device_identity(&path),
                        name,
                        dshow_index: None,
                        msmf_index: Some(index as i32),
                        aliases: vec![canonical_id(&path)],
                    });
                }
            }
            CoTaskMemFree(Some(array.cast()));
            Ok(devices)
        }
    }

    pub fn enumerate() -> Result<Vec<CameraDevice>, String> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(|error| error.to_string())?;
            let _com = Com;
            let dshow = direct_show();
            let mf = media_foundation();
            if let (Err(first), Err(second)) = (&dshow, &mf) {
                return Err(format!("Could not enumerate cameras: {first}; {second}"));
            }
            let mut devices = dshow.unwrap_or_default();
            for camera in mf.unwrap_or_default() {
                if let Some(existing) = devices
                    .iter_mut()
                    .find(|device| device.id.eq_ignore_ascii_case(&camera.id))
                {
                    existing.msmf_index = camera.msmf_index;
                    existing.aliases.extend(camera.aliases);
                } else {
                    devices.push(camera);
                }
            }
            Ok(devices)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn device(id: &str, index: i32) -> CameraDevice {
        CameraDevice {
            id: id.into(),
            name: "Identical camera name".into(),
            dshow_index: Some(index),
            msmf_index: None,
            aliases: Vec::new(),
        }
    }
    #[test]
    fn legacy_interface_selection_matches_its_physical_device() {
        let mut camera = device("device:physical-camera", 0);
        camera.aliases.push("old-interface-path".into());
        let saved = CameraPreference {
            id: "old-interface-path".into(),
            name: camera.name.clone(),
        };
        assert_eq!(
            resolve(&[camera], Some(&saved)).unwrap().id,
            "device:physical-camera"
        );
    }

    #[test]
    fn identity_survives_index_reordering_and_duplicate_names() {
        let saved = device("device-a", 0).preference();
        let changed = [device("device-b", 0), device("device-a", 1)];
        assert_eq!(
            resolve(&changed, Some(&saved)).unwrap().dshow_index,
            Some(1)
        );
    }
    #[test]
    fn disconnected_saved_camera_never_silently_uses_a_different_device() {
        assert!(
            resolve(
                &[device("other", 0)],
                Some(&device("saved", 0).preference())
            )
            .is_err()
        );
    }
    #[test]
    fn forgetting_requires_choice_again_when_multiple_cameras_exist() {
        let devices = [device("a", 0), device("b", 1)];
        assert!(resolve(&devices, Some(&devices[1].preference())).is_ok());
        assert!(resolve(&devices, None).is_err());
        assert!(resolve(&devices[..1], None).is_ok());
        assert!(resolve(&[], None).is_err());
    }
}
