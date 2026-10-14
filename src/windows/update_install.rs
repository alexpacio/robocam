//! Start an independent, hidden helper before shutting down the GUI.
use super::*;
use std::{
    fs,
    os::windows::process::CommandExt,
    path::PathBuf,
    process::{Command, Stdio},
};

pub(super) fn staging_directory(version: &str) -> Result<PathBuf, String> {
    // Program Files keeps the elevated helper, job and downloaded code out of
    // the user's writable temp directory. Each attempt gets a fresh directory.
    let root = std::env::var_os("ProgramFiles").ok_or("Program Files is unavailable")?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let directory = PathBuf::from(root)
        .join("robocam-update-cache")
        .join(format!("{version}-{}-{stamp}", std::process::id()));
    fs::create_dir_all(&directory)
        .map_err(|e| format!("Could not prepare the update directory: {e}"))?;
    Ok(directory)
}

pub(super) fn launch(
    installer: updates::VerifiedInstaller,
    enabled: bool,
    paused: bool,
    minimized: bool,
) -> Result<PathBuf, String> {
    let directory = installer
        .path
        .parent()
        .ok_or("Invalid installer path")?
        .to_path_buf();
    let script = directory.join("apply-update.ps1");
    let job = directory.join("job.json");
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut arguments = vec![
        if enabled {
            "--resume-clicks"
        } else {
            "--paused"
        }
        .to_owned(),
    ];
    if paused {
        arguments.push("--fully-paused".into());
    }
    if minimized {
        arguments.push("--minimized".into());
    }
    let payload = serde_json::json!({
        "parent_id":std::process::id(), "executable":executable,
        "from_version":env!("CARGO_PKG_VERSION"), "to_version":installer.version,
        "installer":installer.path, "sha256":installer.sha256,
        "settings":Settings::path(), "restart_arguments":arguments,
    });
    fs::write(&job, serde_json::to_vec_pretty(&payload).unwrap()).map_err(|e| e.to_string())?;
    fs::write(&script, include_bytes!("../../assets/apply-update.ps1"))
        .map_err(|e| e.to_string())?;
    let system = std::env::var_os("SystemRoot").ok_or("Windows directory is unavailable")?;
    Command::new(PathBuf::from(system).join("System32/WindowsPowerShell/v1.0/powershell.exe"))
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-File",
        ])
        .arg(script)
        .arg("-JobPath")
        .arg(job)
        .current_dir(&directory)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not start the update installer: {e}"))?;
    Ok(directory)
}
