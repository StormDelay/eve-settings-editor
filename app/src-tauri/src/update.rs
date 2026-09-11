//! Launch-time "a newer release exists" check. Reads the latest GitHub release
//! and hands the frontend a version + download URL, or nothing. It never
//! downloads or runs anything itself — the user takes the link to the browser.

use serde::{Deserialize, Serialize};

const LATEST: &str = "https://api.github.com/repos/StormDelay/eve-settings-editor/releases/latest";

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

#[derive(Serialize)]
pub struct Update {
    pub version: String,
    pub url: String,
}

/// Release tags are `vX.Y.Z` (release.yml). Dotted-integer compare so 0.10 > 0.9;
/// an unparsable tag is never "newer" — better a missed nag than a false one.
fn is_newer(current: &str, tag: &str) -> bool {
    let parse = |s: &str| -> Option<Vec<u64>> {
        s.trim_start_matches('v').split('.').map(|p| p.parse().ok()).collect()
    };
    match (parse(current), parse(tag)) {
        (Some(cur), Some(new)) => new > cur,
        _ => false,
    }
}

/// The asset for this OS and install kind, as named by release.yml's
/// `label-assets` step (OS prefix) and the bundler (extension).
fn pick_asset<'a>(assets: &'a [Asset], prefix: &str, ext: &str) -> Option<&'a str> {
    assets
        .iter()
        .find(|a| a.name.starts_with(prefix) && a.name.ends_with(ext))
        .map(|a| a.browser_download_url.as_str())
}

/// `(prefix, ext)` of the download that matches how this copy was installed.
// ponytail: heuristics — NSIS leaves uninstall.exe beside the binary, AppImage
// sets $APPIMAGE, Debian-likes have /etc/debian_version. A miss just falls
// back to the release page.
fn install_kind() -> (&'static str, &'static str) {
    match std::env::consts::OS {
        "macos" => ("macOS-", ".dmg"),
        "windows" => {
            let nsis = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("uninstall.exe").exists()))
                .unwrap_or(false);
            ("Windows-", if nsis { ".exe" } else { ".msi" })
        }
        _ => {
            let ext = if std::env::var_os("APPIMAGE").is_some() {
                ".AppImage"
            } else if std::path::Path::new("/etc/debian_version").exists() {
                ".deb"
            } else {
                ".rpm"
            };
            ("Linux-", ext)
        }
    }
}

/// Untested (network), like names::esi_fetch.
pub fn check_blocking() -> Result<Option<Update>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let rel: Release = client
        .get(LATEST)
        .header(reqwest::header::USER_AGENT, "eve-settings-editor")
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .map_err(|e| e.to_string())?;
    if !is_newer(env!("CARGO_PKG_VERSION"), &rel.tag_name) {
        return Ok(None);
    }
    let (prefix, ext) = install_kind();
    let url = pick_asset(&rel.assets, prefix, ext).unwrap_or(&rel.html_url).to_string();
    Ok(Some(Update { version: rel.tag_name.trim_start_matches('v').to_string(), url }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_tag_is_newer() {
        assert!(is_newer("0.35.0", "v0.36.0"));
        assert!(is_newer("0.9.0", "v0.10.0"));
    }

    #[test]
    fn same_or_older_tag_is_not_newer() {
        assert!(!is_newer("0.35.0", "v0.35.0"));
        assert!(!is_newer("0.35.0", "v0.34.9"));
        assert!(!is_newer("0.35.0", "garbage"));
    }

    fn assets(names: &[&str]) -> Vec<Asset> {
        names
            .iter()
            .map(|n| Asset { name: n.to_string(), browser_download_url: format!("https://x/{n}") })
            .collect()
    }

    #[test]
    fn picks_the_asset_for_this_install() {
        let a = assets(&[
            "Linux-EVE.Settings.Editor_0.36.0_amd64.deb",
            "Windows-EVE.Settings.Editor_0.36.0_x64-setup.exe",
            "Windows-EVE.Settings.Editor_0.36.0_x64_en-US.msi",
        ]);
        assert_eq!(
            pick_asset(&a, "Windows-", ".msi"),
            Some("https://x/Windows-EVE.Settings.Editor_0.36.0_x64_en-US.msi"),
        );
        assert_eq!(pick_asset(&a, "macOS-", ".dmg"), None);
    }

    /// The one real call. Proves the GitHub JSON still has the fields `Release`
    /// names; a build of the latest release sees `None`, an older build sees
    /// `Some`. Never a CI gate — the network is not ours to fail on.
    #[test]
    #[ignore = "network"]
    fn latest_release_parses() {
        let latest = check_blocking().unwrap();
        eprintln!("{:?}", latest.as_ref().map(|u| (&u.version, &u.url)));
    }
}
