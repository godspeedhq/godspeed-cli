//! Telling the developer that a newer `godspeed` has been released.
//!
//! At most once a day, a command asks GitHub - in the background, while the command does its work - which
//! release is the latest, and remembers the answer in the config directory. When the command is done, if
//! that answer is newer than this binary, one notice goes to stderr. It never changes what a command does,
//! never makes one fail, and never updates anything: updating is `godspeed self update`, which checks what
//! it downloads before it replaces anything (`cmd::self_manage`).
//!
//! It stays quiet where a notice would be noise or a surprise: when stderr is not a terminal (a script, a
//! pipe, the tests), when `CI` is set, and when `GODSPEED_NO_UPDATE_CHECK` is set. The only thing it sends
//! is the request itself - no identifier, no usage data - and GitHub sees what any download of a release
//! would show it.

use crate::registry::config_dir;
use godspeed_package::Version;
use serde::{Deserialize, Serialize};
use std::io::IsTerminal;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const RELEASES: &str = "https://github.com/godspeedhq/godspeed-cli/releases";
pub const OFF_ENV: &str = "GODSPEED_NO_UPDATE_CHECK";
/// How often to ask.
const EVERY_SECS: u64 = 24 * 60 * 60;
/// How long the question may take, in all.
const TIMEOUT: Duration = Duration::from_secs(2);
/// How long a command that finished first waits for the answer before leaving without it.
const WAIT_AT_EXIT: Duration = Duration::from_millis(800);
const CACHE_FILE: &str = "update-check.toml";

/// This binary's version.
pub fn current() -> Version {
    env!("CARGO_PKG_VERSION")
        .parse()
        .expect("Cargo.toml's version is MAJOR.MINOR.PATCH")
}

/// What was last learned, and when.
#[derive(Serialize, Deserialize, Default)]
struct Cache {
    /// Seconds since 1970 of the last attempt, answered or not: an offline machine is asked once a day too.
    checked: u64,
    /// The latest release seen, if any answer ever came.
    latest: Option<String>,
}

/// A check under way: the last answer remembered, and perhaps a fresh one on its way.
pub struct Check {
    remembered: Option<Version>,
    fresh: Option<mpsc::Receiver<Option<Version>>>,
}

/// Start a check, if one is wanted: decide from the remembered answer whether to ask again, and if so,
/// ask in the background.
pub fn start() -> Option<Check> {
    if !wanted(std::io::stderr().is_terminal()) {
        return None;
    }
    let dir = config_dir()?;
    let cache = read(&dir);
    let remembered = cache.latest.as_deref().and_then(|s| s.parse().ok());
    let now = now_secs();
    let due = now.saturating_sub(cache.checked) >= EVERY_SECS || cache.checked > now;
    let fresh = due.then(|| {
        // Record the attempt before making it: the command may end before an answer comes (a refused
        // connection can take seconds), and an offline machine must still be asked only once a day.
        write(
            &dir,
            &Cache {
                checked: now,
                latest: cache.latest.clone(),
            },
        );
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let latest = latest_release().ok().flatten();
            if let Some(v) = latest {
                write(
                    &dir,
                    &Cache {
                        checked: now,
                        latest: Some(v.to_string()),
                    },
                );
            }
            let _ = tx.send(latest);
        });
        rx
    });
    Some(Check { remembered, fresh })
}

/// After the command: tell the developer, if there is something newer.
pub fn finish(check: Option<Check>) {
    let Some(check) = check else { return };
    let fresh = check.fresh.and_then(|rx| rx.recv_timeout(WAIT_AT_EXIT).ok().flatten());
    if let Some(latest) = fresh.or(check.remembered).filter(|l| *l > current()) {
        eprintln!();
        eprintln!("A newer godspeed is available: {latest} (this is {}).", current());
        eprintln!("  What changed: {RELEASES}/tag/cli-v{latest}");
        eprintln!("  To update: godspeed self update");
        eprintln!("  ({OFF_ENV}=1 turns this notice off.)");
    }
}

/// Whether to check at all.
fn wanted(stderr_is_terminal: bool) -> bool {
    let set = |name: &str| std::env::var_os(name).is_some_and(|v| !v.is_empty() && v != "0");
    #[cfg(debug_assertions)]
    if set(TEST_URL_ENV) {
        return !set(OFF_ENV);
    }
    stderr_is_terminal && !set("CI") && !set(OFF_ENV)
}

/// Ask GitHub which release is the latest. `Ok(None)`: it answered, and there is no release with a
/// version this binary can compare (none yet, or a pre-release tag). `Err`: no answer.
pub fn latest_release() -> Result<Option<Version>, ()> {
    let agent = ureq::AgentBuilder::new()
        .timeout(TIMEOUT)
        .redirects(0)
        .user_agent(concat!("godspeed/", env!("CARGO_PKG_VERSION")))
        .build();
    // `/releases/latest` redirects to the latest release's page; its address names the tag. No API call,
    // so no rate limit and no JSON.
    let response = agent
        .head(&format!("{}/latest", releases_url()))
        .call()
        .map_err(|_| ())?;
    Ok(response.header("location").and_then(version_in))
}

/// The version in a release page's address: `.../releases/tag/cli-v1.2.3`.
fn version_in(location: &str) -> Option<Version> {
    let tag = location.split_once("/releases/tag/cli-v")?.1;
    let end = tag.find(['/', '?', '#']).unwrap_or(tag.len());
    tag[..end].parse().ok()
}

pub fn releases_url() -> String {
    #[cfg(debug_assertions)]
    if let Some(url) = std::env::var_os(TEST_URL_ENV).filter(|u| !u.is_empty()) {
        return url.to_string_lossy().trim_end_matches('/').to_string();
    }
    RELEASES.to_string()
}

/// Debug builds only, for testing against a local server: where `releases/latest` lives instead, and
/// check even though stderr is not a terminal.
#[cfg(debug_assertions)]
const TEST_URL_ENV: &str = "GODSPEED_TEST_RELEASES_URL";

fn read(dir: &Path) -> Cache {
    // A missing or damaged file is the same as never having checked: it costs one question.
    std::fs::read_to_string(dir.join(CACHE_FILE))
        .ok()
        .and_then(|t| toml::from_str(&t).ok())
        .unwrap_or_default()
}

fn write(dir: &Path, cache: &Cache) {
    // Best effort, and never half-written: a command must not fail because this could not be saved.
    let Ok(text) = toml::to_string(cache) else { return };
    let tmp = dir.join(format!("{CACHE_FILE}.tmp"));
    if std::fs::create_dir_all(dir).is_ok() && std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, dir.join(CACHE_FILE));
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_version_from_a_release_address() {
        let v = |s: &str| version_in(s).map(|v| v.to_string());
        assert_eq!(
            v("https://github.com/godspeedhq/godspeed-cli/releases/tag/cli-v0.2.0").as_deref(),
            Some("0.2.0")
        );
        assert_eq!(
            v("/godspeedhq/godspeed-cli/releases/tag/cli-v1.10.3?x=1").as_deref(),
            Some("1.10.3")
        );
        // No release yet: GitHub sends /releases/latest to the releases list.
        assert_eq!(v("https://github.com/godspeedhq/godspeed-cli/releases"), None);
        // A pre-release, or a tag that is not the CLI's: nothing to compare, so no notice.
        assert_eq!(
            v("https://github.com/godspeedhq/godspeed-cli/releases/tag/cli-v0.2.0-rc.1"),
            None
        );
        assert_eq!(
            v("https://github.com/godspeedhq/godspeed-cli/releases/tag/v0.2.0"),
            None
        );
    }

    #[test]
    fn this_binary_has_a_comparable_version() {
        assert!("999.0.0".parse::<Version>().unwrap() > current());
        assert!(!("0.0.0".parse::<Version>().unwrap() > current()));
    }

    #[test]
    fn remembers_and_survives_a_damaged_file() {
        let dir = std::env::temp_dir().join(format!("godspeed-update-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(read(&dir).checked, 0);
        write(
            &dir,
            &Cache {
                checked: 42,
                latest: Some("0.3.1".into()),
            },
        );
        let c = read(&dir);
        assert_eq!((c.checked, c.latest.as_deref()), (42, Some("0.3.1")));
        std::fs::write(dir.join(CACHE_FILE), "not = [toml").unwrap();
        assert_eq!(read(&dir).checked, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
