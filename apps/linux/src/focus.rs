use std::{
    io,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct Target {
    pub endpoint: String,
    pub pane: String,
}

fn bounded(command: &mut Command) -> io::Result<()> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = child.try_wait()? {
            return if status.success() {
                Ok(())
            } else {
                Err(io::Error::other("command failed"))
            };
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::new(io::ErrorKind::TimedOut, "command timed out"));
        }
        thread::sleep(Duration::from_millis(20));
    }
}

// The Linux app finds kitty through PATH, unlike the macOS app bundle search.
pub fn kitty() -> io::Result<String> {
    let paths = std::env::var_os("PATH").ok_or_else(|| io::Error::other("PATH is not set"))?;
    for directory in std::env::split_paths(&paths) {
        let candidate = directory.join("kitty");
        if candidate.is_file() && is_executable(&candidate) {
            return Ok(candidate.to_string_lossy().into_owned());
        }
    }
    Err(io::Error::other("no kitty command on PATH"))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|meta| meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

// Both callbacks run exactly once when the attempt completes on its own thread,
// so a caller can release a navigation gate in the failure case too.
pub fn focus(
    target: Target,
    on_success: impl FnOnce() + Send + 'static,
    on_release: impl FnOnce() + Send + 'static,
) {
    // Never block the tray event loop, and never invoke a shell.
    thread::spawn(move || {
        let result = (|| {
            let kitty = kitty()?;
            bounded(Command::new(kitty).args([
                "@",
                "--to",
                &target.endpoint,
                "focus-window",
                "--match",
                &format!("id:{}", target.pane),
            ]))
        })();
        match result {
            Ok(()) => on_success(),
            Err(error) => {
                eprintln!("Pi Watch could not focus Kitty pane: {error}");
                on_release();
            }
        }
    });
}
