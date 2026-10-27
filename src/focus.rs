use std::{
    io,
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

pub fn focus(target: Target) {
    // Never block the AppKit event loop, and never invoke a shell.
    thread::spawn(move || {
        let kitty = [
            "/Applications/kitty.app/Contents/MacOS/kitty".to_string(),
            format!(
                "{}/Applications/kitty.app/Contents/MacOS/kitty",
                std::env::var("HOME").unwrap_or_default()
            ),
        ]
        .into_iter()
        .find(|p| std::path::Path::new(p).is_file());
        let result = (|| {
            let kitty = kitty.ok_or_else(|| io::Error::other("Kitty app not found"))?;
            bounded(Command::new(kitty).args([
                "@",
                "--to",
                &target.endpoint,
                "focus-window",
                "--match",
                &format!("id:{}", target.pane),
            ]))?;
            bounded(Command::new("/usr/bin/osascript").args([
                "-e",
                "tell application id \"net.kovidgoyal.kitty\" to activate",
            ]))
        })();
        if let Err(error) = result {
            eprintln!("Pi Watch could not focus Kitty pane: {error}");
        }
    });
}
