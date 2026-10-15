use super::*;
use ::windows::{
    Win32::{
        Foundation::VARIANT_BOOL,
        System::{Com::*, TaskScheduler::*, Variant::VARIANT},
    },
    core::{BSTR, Interface},
};

struct Com;
impl Drop for Com {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

fn task_name(sid: &str) -> BSTR {
    BSTR::from(format!("robocam-{sid}"))
}

fn service() -> ::windows::core::Result<ITaskService> {
    unsafe {
        let service: ITaskService = CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER)?;
        let empty = VARIANT::default();
        service.Connect(&empty, &empty, &empty, &empty)?;
        Ok(service)
    }
}

pub(super) fn enabled() -> bool {
    let Ok(sid) = (unsafe { runtime::current_sid() }) else {
        return false;
    };
    std::thread::spawn(move || unsafe {
        if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
            return false;
        }
        let _com = Com;
        service()
            .and_then(|service| service.GetFolder(&BSTR::from("\\")))
            .and_then(|folder| folder.GetTask(&task_name(&sid)))
            .and_then(|task| task.Enabled())
            .is_ok_and(|value| value.0 != 0)
    })
    .join()
    .unwrap_or(false)
}

pub(super) fn configure(enable: bool) -> Result<(), String> {
    let sid = unsafe { runtime::current_sid()? };
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    std::thread::spawn(move || -> Result<(), String> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(|e| e.to_string())?;
            let _com = Com;
            let result = (|| -> ::windows::core::Result<()> {
                let service = service()?;
                let folder = service.GetFolder(&BSTR::from("\\"))?;
                if !enable {
                    if folder.GetTask(&task_name(&sid)).is_ok() {
                        folder.DeleteTask(&task_name(&sid), 0)?;
                    }
                    return Ok(());
                }
                let task = service.NewTask(0)?;
                let principal = task.Principal()?;
                principal.SetUserId(&BSTR::from(&sid))?;
                principal.SetLogonType(TASK_LOGON_INTERACTIVE_TOKEN)?;
                principal.SetRunLevel(TASK_RUNLEVEL_HIGHEST)?;
                let settings = task.Settings()?;
                settings.SetDisallowStartIfOnBatteries(VARIANT_BOOL(0))?;
                settings.SetStopIfGoingOnBatteries(VARIANT_BOOL(0))?;
                settings.SetExecutionTimeLimit(&BSTR::from("PT0S"))?;
                let trigger: ILogonTrigger = task.Triggers()?.Create(TASK_TRIGGER_LOGON)?.cast()?;
                trigger.SetUserId(&BSTR::from(&sid))?;
                let action: IExecAction = task.Actions()?.Create(TASK_ACTION_EXEC)?.cast()?;
                // Task Scheduler's direct CreateProcess launch rejects UIAccess
                // executables (ERROR_ELEVATION_REQUIRED). Start-Process uses the
                // Windows shell, which validates the signature and grants UIAccess.
                let system = std::env::var_os("SystemRoot")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| std::path::PathBuf::from("C:\\Windows"));
                let shell = system.join("System32/WindowsPowerShell/v1.0/powershell.exe");
                let literal = |path: &std::path::Path| {
                    format!("'{}'", path.to_string_lossy().replace('\'', "''"))
                };
                action.SetPath(&BSTR::from(shell.to_string_lossy().as_ref()))?;
                action.SetArguments(&BSTR::from(format!(
                    "-NoProfile -NonInteractive -WindowStyle Hidden -Command \"Start-Process -FilePath {} -ArgumentList '--minimized' -WorkingDirectory {} -WindowStyle Hidden\"",
                    literal(&exe), literal(exe.parent().unwrap())
                )))?;
                action.SetWorkingDirectory(&BSTR::from(
                    exe.parent().unwrap().to_string_lossy().as_ref(),
                ))?;
                let empty = VARIANT::default();
                folder.RegisterTaskDefinition(
                    &task_name(&sid),
                    &task,
                    TASK_CREATE_OR_UPDATE.0,
                    &empty,
                    &empty,
                    TASK_LOGON_INTERACTIVE_TOKEN,
                    &empty,
                )?;
                Ok(())
            })();
            result.map_err(|e| e.to_string())
        }
    })
    .join()
    .map_err(|_| "Startup configuration worker stopped.".to_owned())??;
    // Remove the obsolete unelevated Run entry after successful migration.
    unsafe {
        use windows_sys::Win32::System::Registry::*;
        let mut key = null_mut();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            wide_raw("Software\\Microsoft\\Windows\\CurrentVersion\\Run").as_ptr(),
            0,
            KEY_SET_VALUE,
            &mut key,
        ) == ERROR_SUCCESS
        {
            RegDeleteValueW(key, wide_raw("robocam").as_ptr());
            RegCloseKey(key);
        }
    }
    Ok(())
}
