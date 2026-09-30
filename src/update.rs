use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc::{self, Receiver, TryRecvError},
    time::Duration,
};

use eframe::egui::{Context, ViewportId};
use semver::Version;
use serde::Deserialize;

const LATEST_RELEASE: &str =
    "https://api.github.com/repos/MSpiechowicz/useful-timer/releases/latest";
pub const RELEASES_URL: &str = "https://github.com/MSpiechowicz/useful-timer/releases";

#[derive(Clone, Debug)]
pub struct Release {
    pub version: Version,
}

pub enum UpdateState {
    Checking,
    Current,
    Available(Release),
    Installing,
    Installed,
    Closing,
    Failed(String),
    Dismissed,
}

enum Outcome {
    Checked(Option<Release>),
    #[cfg(not(target_os = "windows"))]
    Installed,
    #[cfg(target_os = "windows")]
    Closing,
}

pub struct Updater {
    pub state: UpdateState,
    receiver: Option<Receiver<Result<Outcome, String>>>,
    installing_release: Option<Release>,
    installation_result: Option<Result<(), String>>,
}

impl Updater {
    pub fn new(ctx: &Context) -> Self {
        let mut updater = Self {
            state: UpdateState::Current,
            receiver: None,
            installing_release: None,
            installation_result: None,
        };
        updater.check(ctx);
        updater
    }

    pub fn check(&mut self, ctx: &Context) {
        self.state = UpdateState::Checking;
        self.spawn(ctx, || check_latest().map(Outcome::Checked));
    }

    pub fn install(&mut self, ctx: &Context) {
        let UpdateState::Available(release) = &self.state else {
            return;
        };
        let version = release.version.to_string();
        self.installing_release = Some(release.clone());
        self.installation_result = None;
        self.state = UpdateState::Installing;
        self.spawn(ctx, move || {
            let executable = std::env::current_exe().map_err(|error| error.to_string())?;
            install_release(&version, &executable)
        });
    }

    fn spawn(
        &mut self,
        ctx: &Context,
        job: impl FnOnce() -> Result<Outcome, String> + Send + 'static,
    ) {
        let (sender, receiver) = mpsc::channel();
        let ctx = ctx.clone();
        match std::thread::Builder::new()
            .name("useful-timer-update".into())
            .spawn(move || {
                let result = job();
                let _ = sender.send(result);
                ctx.request_repaint_of(ViewportId::ROOT);
            }) {
            Ok(_) => self.receiver = Some(receiver),
            Err(error) => self.fail(error.to_string()),
        }
    }

    pub fn poll(&mut self) {
        let Some(receiver) = &self.receiver else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                Err("The update worker stopped unexpectedly.".into())
            }
        };
        self.receiver = None;
        match result {
            Ok(Outcome::Checked(Some(release))) => self.state = UpdateState::Available(release),
            Ok(Outcome::Checked(None)) => self.state = UpdateState::Current,
            #[cfg(not(target_os = "windows"))]
            Ok(Outcome::Installed) => {
                self.state = UpdateState::Installed;
                self.installing_release = None;
                self.installation_result = Some(Ok(()));
            }
            #[cfg(target_os = "windows")]
            Ok(Outcome::Closing) => {
                self.state = UpdateState::Closing;
                self.installing_release = None;
                self.installation_result = Some(Ok(()));
            }
            Err(error) => self.fail(error),
        }
    }

    fn fail(&mut self, error: String) {
        if let Some(release) = self.installing_release.take() {
            self.state = UpdateState::Available(release);
            self.installation_result = Some(Err(error));
        } else {
            self.state = UpdateState::Failed(error);
        }
    }

    pub fn take_installation_result(&mut self) -> Option<Result<(), String>> {
        self.installation_result.take()
    }
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
}

fn release_target() -> Result<(&'static str, &'static str), String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Ok(("x86_64-unknown-linux-gnu", "tar.gz")),
        ("macos", "x86_64") => Ok(("x86_64-apple-darwin", "tar.gz")),
        ("macos", "aarch64") => Ok(("aarch64-apple-darwin", "tar.gz")),
        ("windows", "x86_64") => Ok(("x86_64-pc-windows-msvc", "zip")),
        _ => Err("No release installer is available for this platform.".into()),
    }
}

fn select_release(
    release: GithubRelease,
    current: &Version,
    target: &str,
    extension: &str,
) -> Result<Option<Release>, String> {
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let version = Version::parse(
        release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name),
    )
    .map_err(|_| "The latest release has an invalid version.".to_owned())?;
    if !version.pre.is_empty() || !version.build.is_empty() || version <= *current {
        return Ok(None);
    }
    let archive = format!("useful-timer-{version}-{target}.{extension}");
    let checksum = format!("{archive}.sha256");
    if !release.assets.iter().any(|asset| asset.name == archive)
        || !release.assets.iter().any(|asset| asset.name == checksum)
    {
        return Err(format!(
            "Version {version} does not yet include an archive and checksum for this platform."
        ));
    }
    Ok(Some(Release { version }))
}

fn check_latest() -> Result<Option<Release>, String> {
    let (target, extension) = release_target()?;
    check_at(LATEST_RELEASE, target, extension)
}

fn check_at(url: &str, target: &str, extension: &str) -> Result<Option<Release>, String> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .https_only(url.starts_with("https:"))
        .build()
        .new_agent();
    let mut response = match agent
        .get(url)
        .header(
            "User-Agent",
            concat!("useful-timer/", env!("CARGO_PKG_VERSION")),
        )
        .header("Accept", "application/vnd.github+json")
        .call()
    {
        Ok(response) => response,
        // A repository without published releases is not an update failure.
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        Err(error) => return Err(format!("Could not check GitHub for updates: {error}")),
    };
    let body = response
        .body_mut()
        .with_config()
        .limit(1024 * 1024)
        .read_to_string()
        .map_err(|error| format!("Could not read the release information: {error}"))?;
    let release = serde_json::from_str(&body)
        .map_err(|error| format!("Could not read the release information: {error}"))?;
    let current = Version::parse(env!("CARGO_PKG_VERSION")).map_err(|error| error.to_string())?;
    select_release(release, &current, target, extension)
}

fn install_directory(executable: &Path) -> Result<PathBuf, String> {
    let expected_name = if cfg!(target_os = "windows") {
        "useful-timer.exe"
    } else {
        "useful-timer"
    };
    if executable
        .file_name()
        .is_none_or(|name| name != expected_name)
    {
        return Err("The updater requires the executable to keep its original name.".into());
    }
    if executable.parent().is_none() {
        return Err("Could not locate the installation directory.".into());
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").ok_or("HOME is not set.")?;
        let installed =
            PathBuf::from(&home).join("Applications/Useful Timer.app/Contents/MacOS/useful-timer");
        if installed
            .canonicalize()
            .map_err(|error| error.to_string())?
            != executable
                .canonicalize()
                .map_err(|error| error.to_string())?
        {
            return Err("In-app updates on macOS require the app in ~/Applications. Use the release installer for source builds or moved bundles.".into());
        }
        Ok(PathBuf::from(home).join(".local/bin"))
    }
    #[cfg(not(target_os = "macos"))]
    Ok(executable.parent().unwrap().to_owned())
}

fn install_release(version: &str, executable: &Path) -> Result<Outcome, String> {
    let directory = install_directory(executable)?;
    let mut script = tempfile::Builder::new()
        .prefix("useful-timer-update-")
        .suffix(if cfg!(target_os = "windows") {
            ".ps1"
        } else {
            ".sh"
        })
        .tempfile()
        .map_err(|error| format!("Could not create the installer: {error}"))?;
    use std::io::Write;
    let source = if cfg!(target_os = "windows") {
        include_str!("../install.ps1")
    } else {
        include_str!("../install.sh")
    };
    script
        .write_all(source.as_bytes())
        .map_err(|error| error.to_string())?;
    script.flush().map_err(|error| error.to_string())?;
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let (_, script_path) = script.keep().map_err(|error| error.to_string())?;
        let ready_path = script_path.with_extension("ready");
        let result = Command::new("powershell.exe")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&script_path)
            .arg("-Version")
            .arg(version)
            .arg("-InstallDir")
            .arg(directory)
            .arg("-WaitForProcessId")
            .arg(std::process::id().to_string())
            .args(["-Restart", "-SelfRemove"])
            .arg("-ReadyFile")
            .arg(&ready_path)
            .creation_flags(0x08000000) // CREATE_NO_WINDOW; preflight errors return to the GUI.
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn();
        let mut child = match result {
            Ok(child) => child,
            Err(error) => {
                let _ = std::fs::remove_file(script_path);
                return Err(format!("Could not launch the installer: {error}"));
            }
        };
        loop {
            if ready_path.exists() {
                let _ = std::fs::remove_file(&ready_path);
                return Ok(Outcome::Closing);
            }
            if child
                .try_wait()
                .map_err(|error| error.to_string())?
                .is_some()
            {
                let output = child
                    .wait_with_output()
                    .map_err(|error| error.to_string())?;
                let _ = std::fs::remove_file(&script_path);
                let _ = std::fs::remove_file(&ready_path);
                return Err(format!(
                    "Installation failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let output = Command::new("bash")
            .arg(script.path())
            .arg("--version")
            .arg(version)
            .arg("--bin-dir")
            .arg(directory)
            .output()
            .map_err(|error| format!("Could not launch the installer: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "Installation failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(Outcome::Installed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };

    fn release(version: &str, names: &[&str]) -> GithubRelease {
        GithubRelease {
            tag_name: version.into(),
            draft: false,
            prerelease: false,
            assets: names
                .iter()
                .map(|name| GithubAsset {
                    name: (*name).into(),
                })
                .collect(),
        }
    }

    #[test]
    fn compares_versions_numerically_and_requires_matching_verified_assets() {
        let current = Version::parse("1.9.0").unwrap();
        let names = [
            "useful-timer-1.10.0-target.tar.gz",
            "useful-timer-1.10.0-target.tar.gz.sha256",
        ];
        let found = select_release(release("v1.10.0", &names), &current, "target", "tar.gz")
            .unwrap()
            .unwrap();
        assert_eq!(found.version, Version::parse("1.10.0").unwrap());
        assert!(
            select_release(
                release("v1.10.0", &names[..1]),
                &current,
                "target",
                "tar.gz"
            )
            .is_err()
        );
        assert!(
            select_release(
                release("v1.10.0", &names),
                &current,
                "another-target",
                "zip"
            )
            .is_err()
        );
        for version in ["v1.9.0", "v1.8.9", "v1.11.0-rc.1", "v1.11.0+metadata"] {
            assert!(
                select_release(release(version, &[]), &current, "target", "tar.gz")
                    .unwrap()
                    .is_none()
            );
        }
        assert!(
            select_release(
                release("../../invalid", &names),
                &current,
                "target",
                "tar.gz"
            )
            .is_err()
        );
        for draft in [true, false] {
            let mut release = release("v1.10.0", &names);
            release.draft = draft;
            release.prerelease = !draft;
            assert!(
                select_release(release, &current, "target", "tar.gz")
                    .unwrap()
                    .is_none()
            );
        }
    }

    #[test]
    fn http_discovery_handles_new_release_no_release_rate_limit_and_bad_response() {
        let body = r#"{"tag_name":"v99.0.0","draft":false,"prerelease":false,"assets":[{"name":"useful-timer-99.0.0-test.zip"},{"name":"useful-timer-99.0.0-test.zip.sha256"}]}"#;
        for (status, body, expected) in [
            (200, body, 1),
            (404, "{}", 0),
            (403, "{}", -1),
            (200, "not JSON", -1),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/latest", listener.local_addr().unwrap());
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 4096];
                let mut length = 0;
                while !request[..length]
                    .windows(4)
                    .any(|bytes| bytes == b"\r\n\r\n")
                {
                    let count = stream.read(&mut request[length..]).unwrap();
                    assert!(count > 0, "incomplete HTTP request");
                    length += count;
                }
                write!(stream, "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            });
            let result = check_at(&url, "test", "zip");
            match expected {
                1 => assert_eq!(result.unwrap().unwrap().version.to_string(), "99.0.0"),
                0 => assert!(result.unwrap().is_none()),
                _ => assert!(result.is_err()),
            }
            server.join().unwrap();
        }
    }
}
