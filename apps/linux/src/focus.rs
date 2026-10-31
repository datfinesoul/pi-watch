use crate::monitor::{self, Sessions};
use std::{
    io,
    path::Path,
    process::{Command, Stdio},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
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

// One navigation at a time, so a double press cannot spawn duplicate attempts.
#[derive(Clone, Default)]
struct NavigationGuard(Arc<AtomicBool>);

impl NavigationGuard {
    // Returns false when a navigation is already in flight.
    fn begin(&self) -> bool {
        !self.0.swap(true, Ordering::SeqCst)
    }

    fn release(&self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn shared_guard() -> NavigationGuard {
    static GUARD: OnceLock<NavigationGuard> = OnceLock::new();
    GUARD.get_or_init(NavigationGuard::default).clone()
}

// The Linux shortcut keeps the macOS shortcut's urgency role without the GNOME
// shell menu: it jumps straight to the most recent unseen completion.
pub fn focus_most_recent(sessions: &Sessions) {
    let guard = shared_guard();
    if !guard.begin() {
        crate::shortcut::trace(format_args!(
            "shortcut ignored: navigation already in flight"
        ));
        return;
    }
    let Some((target, id, completed)) = most_recent(sessions, Instant::now()) else {
        crate::shortcut::trace(format_args!("shortcut ignored: no awaiting inspection"));
        guard.release();
        return;
    };
    let acknowledge_sessions = sessions.clone();
    let release = guard;
    focus(
        target,
        move || monitor::acknowledge(&acknowledge_sessions, &id, completed),
        move || release.release(),
    );
}

fn most_recent(sessions: &Sessions, now: Instant) -> Option<(Target, String, Option<Instant>)> {
    let guard = sessions.lock().ok()?;
    let mut best: Option<(Target, String, Option<Instant>, Instant)> = None;
    for session in guard.values() {
        if !session.waiting {
            continue;
        }
        let (Some(endpoint), Some(pane)) = (
            &session.update.kitty_listen_on,
            &session.update.kitty_window_id,
        ) else {
            continue;
        };
        // A waiting session without a completion timestamp is the oldest case.
        let tie_break = session.completed.unwrap_or(now);
        if best.as_ref().is_none_or(|(_, _, _, at)| tie_break > *at) {
            best = Some((
                Target {
                    endpoint: endpoint.clone(),
                    pane: pane.clone(),
                },
                session.update.id.clone(),
                session.completed,
                tie_break,
            ));
        }
    }
    best.map(|(target, id, completed, _)| (target, id, completed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update(status: monitor::Status) -> monitor::Update {
        monitor::Update {
            version: 1,
            id: "session".into(),
            cwd: "/tmp/project".into(),
            project_label: None,
            kitty_title: None,
            pid: 1,
            kitty_window_id: Some("100".into()),
            kitty_listen_on: Some("unix:/tmp/kitty.sock".into()),
            kitty_focused: None,
            preview: None,
            status,
        }
    }

    fn waiting_session(completed: Option<Instant>) -> monitor::Session {
        monitor::Session {
            update: update(monitor::Status::Idle),
            seen: Instant::now(),
            changed: Instant::now(),
            completed,
            waiting: true,
        }
    }

    #[test]
    fn most_recent_prefers_the_latest_completion() {
        let sessions = Sessions::default();
        let now = Instant::now();
        let mut older = waiting_session(Some(now));
        older.update.id = "older".into();
        let mut newer = waiting_session(Some(now + Duration::from_secs(5)));
        newer.update.id = "newer".into();
        {
            let mut state = sessions.lock().unwrap();
            state.insert("older".into(), older);
            state.insert("newer".into(), newer);
        }
        let (target, id, completed) = most_recent(&sessions, now).expect("a target");
        assert_eq!(id, "newer");
        assert_eq!(target.pane, "100");
        assert_eq!(completed, Some(now + Duration::from_secs(5)));
    }

    #[test]
    fn waiting_sessions_without_a_completion_are_the_oldest_candidates() {
        let sessions = Sessions::default();
        let now = Instant::now();
        let unnamed = waiting_session(None);
        let mut named = waiting_session(Some(now));
        named.update.id = "named".into();
        {
            let mut state = sessions.lock().unwrap();
            state.insert("unnamed".into(), unnamed);
            state.insert("named".into(), named);
        }
        let (_, id, _) = most_recent(&sessions, now).expect("a target");
        assert_eq!(id, "named");
    }

    #[test]
    fn waiting_sessions_without_a_target_are_skipped() {
        let sessions = Sessions::default();
        let now = Instant::now();
        let mut session = waiting_session(Some(now));
        session.update.kitty_window_id = None;
        sessions.lock().unwrap().insert("session".into(), session);
        assert!(most_recent(&sessions, now).is_none());
    }

    #[test]
    fn a_poisoned_lock_skips_navigation() {
        let sessions = Sessions::default();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = sessions.lock().unwrap();
            panic!("poison the lock");
        }));
        assert!(most_recent(&sessions, Instant::now()).is_none());
    }

    #[test]
    fn navigation_guard_skips_concurrent_presses_and_recovers() {
        let guard = shared_guard();
        assert!(guard.begin());
        assert!(
            !guard.begin(),
            "a second press while in flight must be skipped"
        );
        guard.release();
        assert!(guard.begin());
        guard.release();
    }
}
