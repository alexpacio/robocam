use super::*;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Debug)]
pub struct VerifiedInstaller {
    pub path: PathBuf,
    pub sha256: String,
    pub version: String,
}

#[derive(Debug)]
pub enum DownloadEvent {
    Progress(u8),
    Ready(VerifiedInstaller),
    Failed(String),
    Cancelled,
}

/// Download off the GUI thread; cancellation drops partial files and never starts setup.
pub fn download_installer(
    release: ReleaseInstaller,
    directory: PathBuf,
    cancel: Arc<AtomicBool>,
) -> mpsc::Receiver<DownloadEvent> {
    let (sender, receiver) = mpsc::channel();
    let fallback = sender.clone();
    if let Err(error) = std::thread::Builder::new()
        .name("robocam-download".into())
        .spawn(move || {
            let result = download(&release, directory, &cancel, &sender);
            let event = if cancel.load(Ordering::Relaxed) {
                DownloadEvent::Cancelled
            } else {
                match result {
                    Ok(installer) => DownloadEvent::Ready(installer),
                    Err(error) => DownloadEvent::Failed(error),
                }
            };
            let _ = sender.send(event);
        })
    {
        let _ = fallback.send(DownloadEvent::Failed(error.to_string()));
    }
    receiver
}

struct PartialFile(PathBuf);
impl Drop for PartialFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn download(
    release: &ReleaseInstaller,
    directory: PathBuf,
    cancel: &AtomicBool,
    sender: &mpsc::Sender<DownloadEvent>,
) -> Result<VerifiedInstaller, String> {
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .https_only(true)
            .timeout_global(Some(Duration::from_secs(300)))
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_response(Some(Duration::from_secs(15)))
            .build(),
    );
    let mut checksums = agent
        .get(&release.checksum_url)
        .header("User-Agent", concat!("robocam/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|e| e.to_string())?;
    let body = checksums
        .body_mut()
        .with_config()
        .limit(65536)
        .read_to_string()
        .map_err(|e| e.to_string())?;
    let name = installer_name(&release.version);
    let expected = installer_checksum(&body, &name)?;
    if cancel.load(Ordering::Relaxed) {
        return Err("Download cancelled.".into());
    }
    let temporary = PartialFile(directory.join(format!("{name}.partial")));
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary.0)
        .map_err(|e| e.to_string())?;
    let mut response = agent
        .get(&release.installer_url)
        .header("User-Agent", concat!("robocam/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|e| e.to_string())?;
    let mut input = response.body_mut().as_reader();
    let hash = stream_verified(
        &mut input,
        &mut output,
        release.size,
        &expected,
        cancel,
        |percent| {
            let _ = sender.send(DownloadEvent::Progress(percent));
        },
    )?;
    output.sync_all().map_err(|e| e.to_string())?;
    drop(output);
    let path = directory.join(name);
    fs::rename(&temporary.0, &path).map_err(|e| e.to_string())?;
    Ok(VerifiedInstaller {
        path,
        sha256: hash,
        version: release.version.clone(),
    })
}

fn stream_verified(
    input: &mut impl Read,
    output: &mut impl Write,
    size: u64,
    expected: &str,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u8),
) -> Result<String, String> {
    if size == 0 || size > MAX_INSTALLER_BYTES {
        return Err("Invalid installer size.".into());
    }
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    let mut received = 0u64;
    let mut last_progress = 255;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Download cancelled.".into());
        }
        let count = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        received += count as u64;
        if received > size {
            return Err("Installer is larger than the published size.".into());
        }
        output
            .write_all(&buffer[..count])
            .map_err(|e| e.to_string())?;
        hash.update(&buffer[..count]);
        let percent = (received * 100 / size) as u8;
        if percent != last_progress {
            progress(percent);
            last_progress = percent;
        }
    }
    let actual = format!("{:x}", hash.finalize());
    if received != size {
        return Err("Installer download is incomplete.".into());
    }
    if actual != expected {
        return Err("Installer checksum mismatch. Nothing was installed.".into());
    }
    Ok(actual)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_tampered_truncated_oversized_and_cancelled_downloads() {
        let data = b"a genuine installer";
        let hash = format!("{:x}", Sha256::digest(data));
        let cancel = AtomicBool::new(false);
        assert!(
            stream_verified(
                &mut &data[..],
                &mut Vec::new(),
                data.len() as u64,
                &hash,
                &cancel,
                |_| {}
            )
            .is_ok()
        );
        for (size, expected) in [
            (data.len() as u64, "bad"),
            (data.len() as u64 + 1, hash.as_str()),
            (1, hash.as_str()),
        ] {
            assert!(
                stream_verified(
                    &mut &data[..],
                    &mut Vec::new(),
                    size,
                    expected,
                    &cancel,
                    |_| {}
                )
                .is_err()
            );
        }
        cancel.store(true, Ordering::Relaxed);
        let mut output = Vec::new();
        assert!(
            stream_verified(
                &mut &data[..],
                &mut output,
                data.len() as u64,
                &hash,
                &cancel,
                |_| {}
            )
            .is_err()
        );
        assert!(output.is_empty());
    }
}
