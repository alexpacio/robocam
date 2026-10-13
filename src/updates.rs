//! Optional release notification. No code is downloaded or executed.

#[cfg(windows)]
use std::{sync::mpsc, time::Duration};

pub const RELEASES_URL: &str = "https://github.com/alexpacio/robocam/releases";
const LATEST_RELEASE_API: &str = "https://api.github.com/repos/alexpacio/robocam/releases/latest";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateResult {
    NewRelease { version: String, url: String },
    Current,
    NotPublished,
    Failed(String),
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
            // Never present a service-provided link outside this project's releases.
            let prefix = format!("{RELEASES_URL}/tag/");
            if !release.html_url.starts_with(&prefix) {
                return UpdateResult::Failed(
                    "The release service returned an unexpected link.".into(),
                );
            }
            UpdateResult::NewRelease {
                version: release.tag_name,
                url: release.html_url,
            }
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
        .timeout_global(Some(Duration::from_secs(5)))
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
        let body = r#"{"tag_name":"v0.2.0","html_url":"https://github.com/alexpacio/robocam/releases/tag/v0.2.0"}"#;
        assert!(matches!(
            classify_release(body, "0.1.0"),
            UpdateResult::NewRelease { .. }
        ));
        assert_eq!(classify_release(body, "0.2.0"), UpdateResult::Current);
        assert!(matches!(
            classify_release("{}", "0.1.0"),
            UpdateResult::Failed(_)
        ));
        assert!(matches!(
            classify_release(&body.replace("github.com", "example.org"), "0.1.0"),
            UpdateResult::Failed(_)
        ));
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
