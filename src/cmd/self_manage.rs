//! `godspeed self update` and `godspeed self uninstall`: the CLI looking after its own installation, the
//! way `rustup self` does.
//!
//! `self update` does what the installer does - download the latest release's build for this machine and
//! the release's SHA256SUMS, check one against the other, run the new binary - and only then puts it in
//! place of this one, in a single rename. Until that rename, nothing on disk has changed.
//!
//! `self uninstall` undoes the installer: the binary, its folder if that is the installer's and is left
//! empty, and the PATH entries the installer added. It keeps the configuration directory, because that holds
//! the signing key, which nothing can replace (see "Files" in the reference).
//!
//! A copy built with `cargo install` is Cargo's to manage, so both commands send it back to Cargo.
//!
//! A checksum shows the download is the file the release lists; it does not show who made the release.
//! Releases are not signed yet - the installer's caveat, unchanged.

use crate::diag::{Diagnostic, Kind, Stage};
use crate::update;
use godspeed_package::{Sha256Digest, Version};
use std::io::{BufRead, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// The largest build this will download.
const LIMIT: u64 = 64 << 20;
/// What the installer writes after the PATH line it adds to a shell profile.
const PROFILE_MARKER: &str = "# added by the godspeed installer";

/// The release's build for this machine, named as release.yml names it. This is the build the installer
/// would choose, which is not always the one running: a Linux binary built by hand updates to the static
/// build.
fn asset() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("godspeed-x86_64-unknown-linux-musl"),
        ("linux", "aarch64") => Some("godspeed-aarch64-unknown-linux-musl"),
        ("macos", "x86_64") => Some("godspeed-x86_64-apple-darwin"),
        ("macos", "aarch64") => Some("godspeed-aarch64-apple-darwin"),
        ("windows", "x86_64") => Some("godspeed-x86_64-pc-windows-msvc.exe"),
        ("windows", "aarch64") => Some("godspeed-aarch64-pc-windows-msvc.exe"),
        _ => None,
    }
}

// ---- self update ------------------------------------------------------------------------------------

pub fn update() -> Result<(), Diagnostic> {
    let doing = "update godspeed";
    let exe = this_exe(doing)?;
    managed_by_cargo(&exe, doing)?;
    let Some(asset) = asset() else {
        return Err(Diagnostic::new(
            Kind::UnavailableTarget,
            Stage::Setup,
            doing,
            format!(
                "No release build exists for {} on {}.",
                std::env::consts::OS,
                std::env::consts::ARCH
            ),
        )
        .expected("Linux or macOS on x86_64 or Arm64, or Windows on x64 or Arm64")
        .next(format!("Build the latest release yourself: {}", cargo_install(None))));
    };

    eprintln!("info: checking for self-update");
    let current = update::current();
    let latest = match update::latest_release() {
        Ok(Some(v)) => v,
        Ok(None) => {
            return Err(Diagnostic::new(
                Kind::NotFound,
                Stage::Setup,
                doing,
                "GitHub lists no release of the CLI that this version can compare itself with.",
            )
            .next(format!("See {}", update::RELEASES)))
        }
        Err(()) => {
            return Err(Diagnostic::new(
                Kind::RegistryUnavailable,
                Stage::Setup,
                doing,
                "GitHub could not be asked which release is the latest.",
            )
            .next("Check the network and run `godspeed self update` again. Nothing was changed."))
        }
    };
    if latest <= current {
        println!("  godspeed unchanged - {current}");
        if latest < current {
            println!("  (this build is newer than the latest release, {latest})");
        }
        return Ok(());
    }

    eprintln!("info: downloading godspeed {latest}");
    let base = format!("{}/download/cli-v{latest}", update::releases_url());
    let sums = download(&format!("{base}/SHA256SUMS"), 1 << 20, doing)?;
    let bytes = download(&format!("{base}/{asset}"), LIMIT, doing)?;

    let expected = listed_digest(&String::from_utf8_lossy(&sums), asset).ok_or_else(|| {
        Diagnostic::new(
            Kind::IntegrityFailure,
            Stage::Verification,
            doing,
            format!("The release's SHA256SUMS does not list {asset}, so the download cannot be checked."),
        )
        .next("Nothing was changed. Report it: https://github.com/godspeedhq/godspeed-cli/issues")
    })?;
    let actual = hex(&Sha256Digest::of(&bytes));
    if actual != expected {
        return Err(Diagnostic::new(
            Kind::IntegrityFailure,
            Stage::Verification,
            doing,
            format!("{asset} is not the file the release lists: expected sha256 {expected}, got {actual}."),
        )
        .next("Nothing was changed. Run `godspeed self update` again; if it persists, report it."));
    }
    eprintln!("info: verified sha256 {actual}");

    // Written beside this binary, so that putting it in place is a rename within one folder.
    let dir = exe.parent().unwrap_or(Path::new("."));
    let new = dir.join(format!(".godspeed.new{}", std::env::consts::EXE_SUFFIX));
    let written = write_executable(&new, &bytes);
    if let Err(e) = written {
        let _ = std::fs::remove_file(&new);
        return Err(not_writable(doing, dir, &e));
    }
    if let Err(why) = runs_as(&new, &latest) {
        let _ = std::fs::remove_file(&new);
        return Err(Diagnostic::new(Kind::Environment, Stage::Verification, doing, why)
            .next("Nothing was changed. Report it, with your OS version and CPU."));
    }
    if let Err(e) = replace(&new, &exe) {
        let _ = std::fs::remove_file(&new);
        return Err(not_writable(doing, &exe, &e));
    }

    println!("  godspeed updated - {latest} (from {current})");
    println!("  What changed: {}/tag/cli-v{latest}", update::RELEASES);
    Ok(())
}

/// The digest SHA256SUMS gives for `asset`: lines of `<hex>  <name>`, or `<hex> *<name>`.
fn listed_digest(sums: &str, asset: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (digest, name) = line.trim().split_once(char::is_whitespace)?;
        let name = name.trim_start();
        (name.strip_prefix('*').unwrap_or(name) == asset).then(|| digest.to_ascii_lowercase())
    })
}

fn hex(d: &Sha256Digest) -> String {
    d.to_string().trim_start_matches("sha256:").to_string()
}

fn download(url: &str, limit: u64, doing: &str) -> Result<Vec<u8>, Diagnostic> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(60))
        .https_only(!cfg!(debug_assertions))
        .user_agent(concat!("godspeed/", env!("CARGO_PKG_VERSION")))
        .build();
    let failed = |why: String| {
        Diagnostic::new(Kind::RegistryUnavailable, Stage::Setup, doing, why)
            .next("Check the network and run `godspeed self update` again. Nothing was changed.")
    };
    let response = agent.get(url).call().map_err(|e| match e {
        ureq::Error::Status(code, _) => failed(format!("{url} answered HTTP {code}.")),
        ureq::Error::Transport(t) => failed(format!("{url} could not be downloaded: {t}.")),
    })?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| failed(format!("the download of {url} stopped: {e}.")))?;
    if bytes.len() as u64 > limit {
        return Err(failed(format!(
            "{url} is larger than {limit} bytes, more than any build should be."
        )));
    }
    Ok(bytes)
}

fn write_executable(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

/// Whether the new binary runs here and says it is the version that was downloaded.
fn runs_as(binary: &Path, version: &Version) -> Result<(), String> {
    let out = Command::new(binary)
        .arg("--version")
        .env(update::OFF_ENV, "1")
        .output()
        .map_err(|e| format!("The downloaded godspeed {version} could not be started: {e}."))?;
    let said = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if out.status.success() && said == format!("godspeed {version}") {
        Ok(())
    } else {
        Err(format!(
            "The downloaded godspeed {version} did not run as expected: `--version` said {said:?}."
        ))
    }
}

/// Put `new` where `exe` is. On Unix one rename does it, even while `exe` runs. Windows will not replace a
/// running program, but will rename it: the old one steps aside to `<exe>.old`, removed by a later run
/// (`tidy`), and comes back if the new one cannot be moved in.
fn replace(new: &Path, exe: &Path) -> std::io::Result<()> {
    if cfg!(windows) {
        let old = old_path(exe);
        let _ = std::fs::remove_file(&old);
        std::fs::rename(exe, &old)?;
        if let Err(e) = std::fs::rename(new, exe) {
            let _ = std::fs::rename(&old, exe);
            return Err(e);
        }
        Ok(())
    } else {
        std::fs::rename(new, exe)
    }
}

fn old_path(exe: &Path) -> PathBuf {
    let mut name = exe.file_name().unwrap_or_default().to_os_string();
    name.push(".old");
    exe.with_file_name(name)
}

/// Remove the binary a Windows `self update` set aside, once it is no longer running. Best effort.
pub fn tidy() {
    if cfg!(windows) {
        if let Ok(exe) = std::env::current_exe() {
            let _ = std::fs::remove_file(old_path(&exe));
        }
    }
}

fn not_writable(doing: &str, path: &Path, e: &std::io::Error) -> Diagnostic {
    let kind = if e.kind() == std::io::ErrorKind::PermissionDenied {
        Kind::PermissionDenied
    } else {
        Kind::Io
    };
    Diagnostic::new(
        kind,
        Stage::Setup,
        doing,
        format!("{} could not be written: {e}.", path.display()),
    )
    .next(
        "Nothing was changed. If godspeed was installed somewhere you cannot write, update it the way it was \
         installed (as that user, or by running the installer again).",
    )
}

// ---- self uninstall ---------------------------------------------------------------------------------

pub fn uninstall(yes: bool) -> Result<(), Diagnostic> {
    let doing = "uninstall godspeed";
    let exe = this_exe(doing)?;
    managed_by_cargo(&exe, doing)?;
    let bin = exe.parent().unwrap_or(Path::new(".")).to_path_buf();
    // The folders themselves are removed only when they are the installer's own, and empty.
    let own = home()
        .map(|h| h.join(".godspeed"))
        .filter(|g| g.join("bin") == bin || g.join("bin").canonicalize().is_ok_and(|b| b == bin));
    let profiles = profiles_with_path_line();
    let user_path = windows_user_path(&bin, false);

    println!("This will uninstall godspeed:");
    println!("  remove  {}", exe.display());
    if let Some(own) = &own {
        println!("  remove  {} (if nothing else is in it)", own.display());
    }
    for p in &profiles {
        println!("  remove  the PATH line the installer added to {}", p.display());
    }
    if user_path {
        println!("  remove  {} from your user PATH", bin.display());
    }
    if let Some(config) = crate::registry::config_dir().filter(|c| c.exists()) {
        println!("  keep    {}", config.display());
        println!("          your signing key, tokens and trust records. Delete it yourself if you no longer");
        println!("          need them - back up signing-key.toml first: nothing can replace it.");
    }
    confirm(yes, doing)?;

    for p in &profiles {
        remove_path_line(p).map_err(|e| Diagnostic::io(doing, p, &e))?;
    }
    if user_path {
        windows_user_path(&bin, true);
    }
    if cfg!(windows) {
        // A running program cannot be deleted on Windows: a helper does it once this one has exited.
        delete_after_exit(&exe, own.as_deref()).map_err(|e| Diagnostic::io(doing, &exe, &e))?;
        println!(
            "godspeed is uninstalled; {} goes when this command ends.",
            exe.display()
        );
    } else {
        std::fs::remove_file(&exe).map_err(|e| Diagnostic::io(doing, &exe, &e))?;
        if let Some(own) = &own {
            let _ = std::fs::remove_dir(own.join("bin"));
            let _ = std::fs::remove_dir(own);
        }
        println!("godspeed is uninstalled.");
    }
    if !profiles.is_empty() || user_path {
        println!("Terminals already open keep the old PATH until they are closed.");
    }
    Ok(())
}

fn confirm(yes: bool, doing: &str) -> Result<(), Diagnostic> {
    if yes {
        return Ok(());
    }
    let cancelled = |why: &str| {
        Diagnostic::new(Kind::Cancelled, Stage::Setup, doing, why.to_string())
            .next("Nothing was changed. Run `godspeed self uninstall --yes` to uninstall without being asked.")
    };
    if !std::io::stdin().is_terminal() {
        return Err(cancelled(
            "There is no terminal to ask for confirmation, and --yes was not given.",
        ));
    }
    print!("Continue? [y/N] ");
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    let _ = std::io::stdin().lock().read_line(&mut answer);
    match answer.trim() {
        "y" | "Y" | "yes" | "Yes" => Ok(()),
        _ => Err(cancelled("You did not confirm.")),
    }
}

/// The shell profiles holding the line install.sh adds.
fn profiles_with_path_line() -> Vec<PathBuf> {
    let Some(home) = home().filter(|_| cfg!(unix)) else {
        return Vec::new();
    };
    [".profile", ".bashrc", ".zshrc"]
        .iter()
        .map(|f| home.join(f))
        .filter(|p| std::fs::read_to_string(p).is_ok_and(|t| t.lines().any(|l| l.ends_with(PROFILE_MARKER))))
        .collect()
}

/// Remove the installer's line, and the blank line it put before it.
fn remove_path_line(profile: &Path) -> std::io::Result<()> {
    let text = std::fs::read_to_string(profile)?;
    std::fs::write(profile, without_path_line(&text))
}

fn without_path_line(text: &str) -> String {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut out = String::with_capacity(text.len());
    for (i, line) in lines.iter().enumerate() {
        if line.trim_end().ends_with(PROFILE_MARKER) {
            continue;
        }
        let next_is_ours = lines.get(i + 1).is_some_and(|n| n.trim_end().ends_with(PROFILE_MARKER));
        if line.trim().is_empty() && next_is_ours {
            continue;
        }
        out.push_str(line);
    }
    out
}

/// On Windows: whether `bin` is in the user PATH (the one install.ps1 adds to), and with `remove`, take it
/// out. The registry value is edited as it is stored - unexpanded, and keeping its type - so the other
/// entries come back exactly as they were. Anywhere else, or if PowerShell cannot be run: false.
fn windows_user_path(bin: &Path, remove: bool) -> bool {
    if !cfg!(windows) {
        return false;
    }
    const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$key = Get-Item -LiteralPath 'HKCU:\Environment'
if ($key.GetValueNames() -notcontains 'Path') { exit }
$kind = $key.GetValueKind('Path')
$raw = $key.GetValue('Path', '', 'DoNotExpandEnvironmentNames')
$dir = $env:GODSPEED_BIN.TrimEnd('\')
$all = @($raw -split ';' | Where-Object { $_ })
$keep = @($all | Where-Object { [Environment]::ExpandEnvironmentVariables($_).TrimEnd('\') -ine $dir })
if ($keep.Count -eq $all.Count) { exit }
'present'
if ($env:GODSPEED_REMOVE -eq '1') {
    New-ItemProperty -LiteralPath 'HKCU:\Environment' -Name Path -Value ($keep -join ';') -PropertyType $kind -Force | Out-Null
    # Setting a variable through .NET tells running programs (Explorer, so new terminals) that the
    # environment changed; writing the registry does not.
    [Environment]::SetEnvironmentVariable('GODSPEED_PATH_CHANGED', '1', 'User')
    [Environment]::SetEnvironmentVariable('GODSPEED_PATH_CHANGED', $null, 'User')
}
"#;
    Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            SCRIPT,
        ])
        .env("GODSPEED_BIN", bin)
        .env("GODSPEED_REMOVE", if remove { "1" } else { "0" })
        .stdin(std::process::Stdio::null())
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("present"))
}

/// Start a hidden helper that waits for this process to end, then deletes the binary (and one set aside by
/// `self update`) and the installer's folders if they are empty.
fn delete_after_exit(exe: &Path, own: Option<&Path>) -> std::io::Result<()> {
    const SCRIPT: &str = r#"
$p = Get-Process -Id $env:GODSPEED_PID -ErrorAction SilentlyContinue
if ($p) { [void]$p.WaitForExit(30000) }
foreach ($f in @($env:GODSPEED_EXE, "$env:GODSPEED_EXE.old")) { Remove-Item -LiteralPath $f -Force -ErrorAction SilentlyContinue }
if ($env:GODSPEED_OWN) {
    foreach ($d in @((Join-Path $env:GODSPEED_OWN 'bin'), $env:GODSPEED_OWN)) {
        if ((Test-Path -LiteralPath $d) -and -not (Get-ChildItem -LiteralPath $d -Force)) { Remove-Item -LiteralPath $d -Force }
    }
}
"#;
    let mut c = Command::new("powershell.exe");
    c.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        SCRIPT,
    ])
    .env("GODSPEED_PID", std::process::id().to_string())
    .env("GODSPEED_EXE", exe)
    .env("GODSPEED_OWN", own.map(Path::as_os_str).unwrap_or_default())
    .stdin(std::process::Stdio::null())
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        c.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
    }
    c.spawn().map(drop)
}

// ---- shared -----------------------------------------------------------------------------------------

fn this_exe(doing: &str) -> Result<PathBuf, Diagnostic> {
    let exe = std::env::current_exe().map_err(|e| {
        Diagnostic::new(
            Kind::Io,
            Stage::Setup,
            doing,
            format!("This program could not find its own file: {e}."),
        )
    })?;
    // On Windows, canonical paths carry a `\\?\` prefix nobody wants to read; current_exe is already whole.
    Ok(if cfg!(windows) {
        exe
    } else {
        exe.canonicalize().unwrap_or(exe)
    })
}

fn home() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var).filter(|h| !h.is_empty()).map(PathBuf::from)
}

/// Refuse to manage a copy that `cargo install` put in Cargo's bin folder: Cargo keeps a record of it, and
/// would be left believing in a version that is no longer there.
fn managed_by_cargo(exe: &Path, doing: &str) -> Result<(), Diagnostic> {
    let cargo_bin = std::env::var_os("CARGO_HOME")
        .filter(|c| !c.is_empty())
        .map(PathBuf::from)
        .or_else(|| home().map(|h| h.join(".cargo")))
        .map(|c| c.join("bin"));
    let same = |a: &Path, b: &Path| {
        let (a, b) = (a.to_string_lossy(), b.to_string_lossy());
        if cfg!(windows) {
            a.eq_ignore_ascii_case(&b)
        } else {
            a == b
        }
    };
    match (exe.parent(), cargo_bin) {
        (Some(dir), Some(bin)) if same(dir, &bin) || same(dir, &bin.canonicalize().unwrap_or_default()) => {
            let next = if doing.starts_with("update") {
                cargo_install(Some("--tag cli-v<version> --force"))
            } else {
                "cargo uninstall godspeed-cli".to_string()
            };
            Err(Diagnostic::new(
                Kind::WrongProgram,
                Stage::Setup,
                doing,
                format!(
                    "This godspeed was installed by `cargo install` ({}), so Cargo manages it.",
                    exe.display()
                ),
            )
            .next(format!("Use Cargo: {next}")))
        }
        _ => Ok(()),
    }
}

fn cargo_install(tag: Option<&str>) -> String {
    format!(
        "cargo install --git https://github.com/godspeedhq/godspeed-cli {}--locked godspeed-cli",
        tag.map(|t| format!("{t} ")).unwrap_or_default()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_this_machines_digest_in_sha256sums() {
        let sums = "AB12  godspeed-x86_64-pc-windows-msvc.exe\n\
                    cd34 *godspeed-x86_64-unknown-linux-musl\n\
                    ef56  install.sh\n";
        assert_eq!(
            listed_digest(sums, "godspeed-x86_64-pc-windows-msvc.exe").as_deref(),
            Some("ab12")
        );
        assert_eq!(
            listed_digest(sums, "godspeed-x86_64-unknown-linux-musl").as_deref(),
            Some("cd34")
        );
        assert_eq!(listed_digest(sums, "godspeed-x86_64-unknown-linux"), None);
        assert_eq!(listed_digest(sums, "godspeed-riscv64"), None);
    }

    #[test]
    fn every_supported_host_has_a_release_build() {
        // release.yml builds exactly these; this host is one of them, or update says there is none.
        if let Some(a) = asset() {
            assert!(a.starts_with("godspeed-") && a.ends_with(std::env::consts::EXE_SUFFIX));
        }
    }

    #[test]
    fn removes_only_the_installers_profile_line() {
        let before = "export A=1\n\nexport PATH=\"/h/.godspeed/bin:$PATH\"  # added by the godspeed installer\n\
                      alias x=y\n";
        assert_eq!(without_path_line(before), "export A=1\nalias x=y\n");
        let untouched = "export A=1\n\n# a comment\nexport PATH=/opt:$PATH\n";
        assert_eq!(without_path_line(untouched), untouched);
    }

    #[test]
    fn a_windows_update_sets_the_old_binary_aside_beside_it() {
        let exe = Path::new("dir").join("godspeed.exe");
        assert_eq!(old_path(&exe), Path::new("dir").join("godspeed.exe.old"));
    }
}
