use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read},
    os::unix::{
        fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
        net::UnixListener,
    },
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

pub type Sessions = Arc<Mutex<BTreeMap<String, Session>>>;
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Working,
    Idle,
    Disconnected,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Update {
    pub version: u8,
    pub id: String,
    pub cwd: String,
    pub pid: u32,
    pub kitty_window_id: Option<String>,
    pub kitty_listen_on: Option<String>,
    pub status: Status,
}
pub struct Session {
    pub update: Update,
    pub seen: Instant,
    pub changed: Instant,
}
impl Session {
    pub fn status(&self, now: Instant) -> Status {
        if now.duration_since(self.seen) > Duration::from_secs(15) {
            Status::Disconnected
        } else {
            self.update.status
        }
    }
}
fn apply(sessions: &mut BTreeMap<String, Session>, update: Update, now: Instant) {
    // A Pi process has one current reporting runtime. Reloads and session
    // replacements create a new instance ID, but must replace its previous row.
    sessions.retain(|id, s| {
        now.duration_since(s.seen) < Duration::from_secs(300)
            && (s.update.pid != update.pid || *id == update.id)
    });
    if sessions.len() >= 512 && !sessions.contains_key(&update.id) {
        return;
    }
    let changed = sessions
        .get(&update.id)
        .filter(|s| s.status(now) == update.status)
        .map_or(now, |s| s.changed);
    sessions.insert(
        update.id.clone(),
        Session {
            update,
            seen: now,
            changed,
        },
    );
    sessions.retain(|_, s| now.duration_since(s.seen) < Duration::from_secs(300));
}
fn decode(bytes: &[u8]) -> Option<Update> {
    let u: Update = serde_json::from_slice(bytes).ok()?;
    if u.version != 1
        || u.id.is_empty()
        || u.id.len() > 200
        || u.cwd.len() > 4096
        || !u.cwd.starts_with('/')
        || u.pid == 0
        || u.kitty_listen_on.as_ref().is_some_and(|s| {
            !s.starts_with("unix:/") || s.len() > 4096 || s.chars().any(char::is_control)
        })
        || u.kitty_window_id
            .as_ref()
            .is_some_and(|s| s.is_empty() || s.len() > 20 || !s.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    Some(u)
}
pub fn start() -> io::Result<Sessions> {
    let home = std::env::var_os("HOME").ok_or_else(|| io::Error::other("HOME is not set"))?;
    let dir = PathBuf::from(home).join("Library/Application Support/Pi Watch");
    fs::create_dir_all(&dir)?;
    let meta = fs::symlink_metadata(&dir)?;
    // SAFETY: geteuid has no preconditions.
    if !meta.is_dir() || meta.uid() != unsafe { libc::geteuid() } {
        return Err(io::Error::other(
            "IPC directory must be owned by this user and not a symlink",
        ));
    }
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    // The lock is held for the lifetime of the process; never unlink a live app's socket.
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(dir.join("app.lock"))?;
    use std::os::fd::AsRawFd;
    // SAFETY: lock is a valid, open file descriptor.
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(io::Error::other("another Pi Watch instance is running"));
    }
    let path = dir.join("events.sock");
    match fs::symlink_metadata(&path) {
        Ok(meta) if meta.file_type().is_socket() => fs::remove_file(&path)?,
        Ok(_) => {
            return Err(io::Error::other(
                "refusing to replace a non-socket IPC path",
            ));
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let listener = UnixListener::bind(path)?;
    fs::set_permissions(dir.join("events.sock"), fs::Permissions::from_mode(0o600))?;
    let sessions = Sessions::default();
    let state = sessions.clone();
    thread::spawn(move || {
        let _lock = lock;
        for stream in listener.incoming().flatten() {
            let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
            let mut bytes = Vec::new();
            if stream.take(8193).read_to_end(&mut bytes).is_ok()
                && bytes.len() <= 8192
                && let Some(update) = decode(&bytes)
                && let Ok(mut sessions) = state.lock()
            {
                apply(&mut sessions, update, Instant::now());
            }
        }
    });
    Ok(sessions)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn update(status: Status) -> Update {
        Update {
            version: 1,
            id: "session".into(),
            cwd: "/tmp/project".into(),
            pid: 1,
            kitty_window_id: Some("100".into()),
            kitty_listen_on: Some("unix:/tmp/kitty.sock".into()),
            status,
        }
    }
    #[test]
    fn heartbeat_preserves_elapsed_and_stale_disconnects() {
        let mut sessions = BTreeMap::new();
        let now = Instant::now();
        apply(&mut sessions, update(Status::Working), now);
        apply(
            &mut sessions,
            update(Status::Working),
            now + Duration::from_secs(5),
        );
        assert_eq!(sessions["session"].changed, now);
        assert_eq!(
            sessions["session"].status(now + Duration::from_secs(21)),
            Status::Disconnected
        );
        apply(
            &mut sessions,
            update(Status::Idle),
            now + Duration::from_secs(22),
        );
        assert_eq!(sessions["session"].changed, now + Duration::from_secs(22));
    }
    #[test]
    fn reload_replaces_previous_reporting_instance() {
        let mut sessions = BTreeMap::new();
        let now = Instant::now();
        apply(&mut sessions, update(Status::Idle), now);
        let mut replacement = update(Status::Working);
        replacement.id = "reloaded".into();
        apply(&mut sessions, replacement, now + Duration::from_secs(1));
        assert_eq!(sessions.len(), 1);
        assert!(sessions.contains_key("reloaded"));
        let mut other_process = update(Status::Idle);
        other_process.pid = 2;
        apply(&mut sessions, other_process, now + Duration::from_secs(2));
        assert_eq!(sessions.len(), 2);
    }
    #[test]
    fn explicit_shutdown_is_immediately_disconnected() {
        let mut sessions = BTreeMap::new();
        let now = Instant::now();
        apply(&mut sessions, update(Status::Disconnected), now);
        assert_eq!(sessions["session"].status(now), Status::Disconnected);
    }
    #[test]
    fn accepts_only_local_kitty_endpoints() {
        let frame = |endpoint: &str| {
            format!(
                r#"{{"version":1,"id":"a","cwd":"/tmp","pid":1,"status":"idle","kitty_listen_on":"{endpoint}"}}"#
            )
        };
        assert!(decode(frame("unix:/tmp/kitty.sock").as_bytes()).is_some());
        assert!(decode(frame("tcp:localhost:1234").as_bytes()).is_none());
        assert!(decode(frame("unix:relative.sock").as_bytes()).is_none());
    }
    #[test]
    fn rejects_unknown_fields_and_versions() {
        assert!(
            decode(br#"{"version":2,"id":"a","cwd":"/tmp","pid":1,"status":"idle"}"#).is_none()
        );
        assert!(
            decode(
                br#"{"version":1,"id":"a","cwd":"/tmp","pid":1,"status":"idle","prompt":"secret"}"#
            )
            .is_none()
        );
        assert!(
            decode(br#"{"version":1,"id":"a","cwd":"/tmp","pid":1,"status":"idle"}"#).is_some()
        );
    }
}
