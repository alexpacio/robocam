//! Stable GitHub release discovery and checksum-verified installer downloads.

#[cfg(windows)]
mod download;
#[cfg(windows)]
pub use download::{DownloadEvent, VerifiedInstaller, download_installer};

#[cfg(windows)]
use std::{sync::mpsc, time::Duration};

pub const RELEASES_URL: &str = "https://github.com/alexpacio/robocam/releases";
const LATEST_RELEASE_API: &str = "https://api.github.com/repos/alexpacio/robocam/releases/latest";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateResult {
    NewRelease(ReleaseInstaller),
    Current,
    NotPublished,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseInstaller {
    pub version: String,
    pub url: String,
    pub installer_url: String,
    pub checksum_url: String,
    pub size: u64,
}

pub fn installer_name(version: &str) -> String {
    format!("robocam-{version}-windows-x64-setup.exe")
}

const MAX_INSTALLER_BYTES: u64 = 128 * 1024 * 1024;

/// Reject duplicate, malformed or missing entries rather than trusting a partial file.
pub fn installer_checksum(body: &str, filename: &str) -> Result<String, String> {
    let mut found = None;
    for line in body.lines().filter(|line| !line.trim().is_empty()) {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() != 2
            || fields[0].len() != 64
            || !fields[0].bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("Invalid release checksums.".into());
        }
        if fields[1].trim_start_matches('*') == filename {
            if found.is_some() {
                return Err("Duplicate installer checksum.".into());
            }
            found = Some(fields[0].to_ascii_lowercase());
        }
    }
    found.ok_or_else(|| "Installer checksum is missing.".into())
}

/// Compare SemVer precedence, accepting the conventional `v` release-tag prefix.
/// Build metadata is deliberately ignored, as required by SemVer.
fn is_newer_release(latest: &str, current: &str) -> Result<bool, String> {
    let parse = |value: &str| {
        semver::Version::parse(value.strip_prefix('v').unwrap_or(value))
            .map_err(|error| format!("Invalid release version {value:?}: {error}"))
    };
    Ok(parse(latest)?.cmp_precedence(&parse(current)?).is_gt())
}

#[derive(serde::Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(serde::Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
}

fn classify_release(body: &str, current: &str) -> UpdateResult {
    let release: Release = match serde_json::from_str(body) {
        Ok(release) => release,
        Err(_) => return UpdateResult::Failed("The release service returned invalid data.".into()),
    };
    if release.draft || release.prerelease {
        return UpdateResult::Current;
    }
    match is_newer_release(&release.tag_name, current) {
        Ok(true) => {
            if semver::Version::parse(
                release
                    .tag_name
                    .strip_prefix('v')
                    .unwrap_or(&release.tag_name),
            )
            .is_ok_and(|version| !version.pre.is_empty())
            {
                return UpdateResult::Current;
            }
            let version = release
                .tag_name
                .strip_prefix('v')
                .unwrap_or(&release.tag_name);
            let parsed = semver::Version::parse(version).unwrap();
            if !parsed.build.is_empty() || release.tag_name != format!("v{parsed}") {
                return UpdateResult::Failed("Invalid stable release tag.".into());
            }
            if release.html_url != format!("{RELEASES_URL}/tag/{}", release.tag_name) {
                return UpdateResult::Failed(
                    "The release service returned an unexpected link.".into(),
                );
            }
            let installer = installer_name(version);
            let asset = |name: &str, max_size| -> Option<&Asset> {
                let matches: Vec<_> = release.assets.iter().filter(|a| a.name == name).collect();
                if matches.len() != 1 {
                    return None;
                }
                let a = matches[0];
                (a.size > 0
                    && a.size <= max_size
                    && a.browser_download_url
                        == format!("{RELEASES_URL}/download/{}/{name}", release.tag_name))
                .then_some(a)
            };
            let (Some(installer), Some(checksum)) = (
                asset(&installer, MAX_INSTALLER_BYTES),
                asset("SHA256SUMS.txt", 65536),
            ) else {
                return UpdateResult::Failed(
                    "The release installer or checksums are missing or invalid.".into(),
                );
            };
            UpdateResult::NewRelease(ReleaseInstaller {
                version: version.to_owned(),
                url: release.html_url.clone(),
                installer_url: installer.browser_download_url.clone(),
                checksum_url: checksum.browser_download_url.clone(),
                size: installer.size,
            })
        }
        Ok(false) => UpdateResult::Current,
        Err(error) => UpdateResult::Failed(error),
    }
}

/// Start a bounded HTTPS request off the UI thread. Dropping the receiver is safe.
#[cfg(windows)]
pub fn check_for_updates(current_version: &str) -> mpsc::Receiver<UpdateResult> {
    let (sender, receiver) = mpsc::channel();
    let fallback = sender.clone();
    let current = current_version.to_owned();
    if let Err(error) = std::thread::Builder::new()
        .name("robocam-updates".into())
        .spawn(move || {
            let _ = sender.send(fetch_latest(&current));
        })
    {
        let _ = fallback.send(UpdateResult::Failed(format!(
            "Could not start the update check: {error}"
        )));
    }
    receiver
}

#[cfg(windows)]
fn fetch_latest(current: &str) -> UpdateResult {
    let config = ureq::Agent::config_builder()
        .https_only(true)
        .timeout_global(Some(Duration::from_secs(15)))
        .http_status_as_error(false)
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let mut response = match agent
        .get(LATEST_RELEASE_API)
        .header("User-Agent", concat!("robocam/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .call()
    {
        Ok(response) => response,
        Err(_) => {
            return UpdateResult::Failed(
                "Could not reach the release service. Check your connection and try again.".into(),
            );
        }
    };
    match response.status().as_u16() {
        404 => return UpdateResult::NotPublished,
        403 | 429 => {
            return UpdateResult::Failed(
                "The release service is limiting requests. Please try again later.".into(),
            );
        }
        200 => {}
        _ => return UpdateResult::Failed("The release service is temporarily unavailable.".into()),
    }
    match response
        .body_mut()
        .with_config()
        .limit(1024 * 1024)
        .read_to_string()
    {
        Ok(body) => classify_release(&body, current),
        Err(_) => UpdateResult::Failed("Could not read the release information.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_numeric_components_not_strings() {
        assert_eq!(is_newer_release("v0.10.0", "0.9.9"), Ok(true));
        assert_eq!(is_newer_release("1.0.0", "0.99.99"), Ok(true));
        assert_eq!(is_newer_release("0.1.0", "0.1.0"), Ok(false));
        assert_eq!(is_newer_release("0.1.0", "0.2.0"), Ok(false));
    }

    #[test]
    fn respects_prerelease_precedence() {
        assert_eq!(is_newer_release("1.0.0", "1.0.0-rc.9"), Ok(true));
        assert_eq!(is_newer_release("1.0.0-rc.10", "1.0.0-rc.2"), Ok(true));
        assert_eq!(is_newer_release("1.0.0-alpha", "1.0.0"), Ok(false));
    }

    #[test]
    fn ignores_build_metadata() {
        assert_eq!(is_newer_release("1.0.0+new", "1.0.0+old"), Ok(false));
        assert_eq!(is_newer_release("1.0.0+001", "1.0.0"), Ok(false));
    }

    #[test]
    fn rejects_incomplete_or_malformed_versions() {
        for value in ["", "1.2", "v", "01.2.3", "1.2.3.4", "1.2.3-beta.01"] {
            assert!(is_newer_release(value, "0.1.0").is_err());
        }
    }

    #[test]
    fn validates_release_payload_and_link() {
        let body = valid_release();
        assert!(matches!(
            classify_release(&body, "0.1.0"),
            UpdateResult::NewRelease(_)
        ));
        assert_eq!(classify_release(&body, "0.2.0"), UpdateResult::Current);
        assert!(matches!(
            classify_release("{}", "0.1.0"),
            UpdateResult::Failed(_)
        ));
        assert!(matches!(
            classify_release(&body.replace("github.com", "example.org"), "0.1.0"),
            UpdateResult::Failed(_)
        ));
    }

    fn valid_release() -> String {
        serde_json::json!({"tag_name":"v0.2.0", "html_url":format!("{RELEASES_URL}/tag/v0.2.0"),
            "assets":[{"name":installer_name("0.2.0"),"browser_download_url":format!("{RELEASES_URL}/download/v0.2.0/{}",installer_name("0.2.0")),"size":100},
            {"name":"SHA256SUMS.txt","browser_download_url":format!("{RELEASES_URL}/download/v0.2.0/SHA256SUMS.txt"),"size":200}]}).to_string()
    }

    #[test]
    fn requires_exact_assets_and_https_urls() {
        for changed in [
            valid_release().replace("https://", "http://"),
            valid_release().replace("windows-x64-setup.exe", "windows-arm64-setup.exe"),
            valid_release().replace("SHA256SUMS.txt", "checksums.txt"),
            valid_release().replace("v0.2.0/robocam", "v0.3.0/robocam"),
        ] {
            assert!(matches!(
                classify_release(&changed, "0.1.0"),
                UpdateResult::Failed(_)
            ));
        }
        let mut body: serde_json::Value = serde_json::from_str(&valid_release()).unwrap();
        let duplicate = body["assets"][0].clone();
        body["assets"].as_array_mut().unwrap().push(duplicate);
        assert!(matches!(
            classify_release(&body.to_string(), "0.1.0"),
            UpdateResult::Failed(_)
        ));
    }

    #[test]
    fn checksum_requires_exact_unique_filename_and_64_hex_digits() {
        let hash = "a".repeat(64);
        let name = installer_name("0.2.0");
        let line = format!("{hash}  {name}\r\n");
        assert_eq!(installer_checksum(&line, &name), Ok(hash));
        for bad in [
            line.repeat(2),
            line.replace(&name, "other.exe"),
            line.replace('a', "g"),
            "123 file.exe".into(),
        ] {
            assert!(installer_checksum(&bad, &name).is_err());
        }
    }

    #[test]
    fn does_not_offer_draft_or_prerelease_builds() {
        let body = r#"{"tag_name":"2.0.0","html_url":"https://github.com/alexpacio/robocam/releases/tag/2.0.0","prerelease":true}"#;
        assert_eq!(classify_release(body, "0.1.0"), UpdateResult::Current);
        assert_eq!(
            classify_release(&body.replace("prerelease", "draft"), "0.1.0"),
            UpdateResult::Current
        );
        assert_eq!(
            classify_release(
                &body.replace("2.0.0", "2.0.0-rc.1").replace("true", "false"),
                "0.1.0"
            ),
            UpdateResult::Current
        );
    }
}
