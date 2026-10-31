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
pub struct Preview {
    pub first: String,
    pub last: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Update {
    pub version: u8,
    pub id: String,
    pub cwd: String,
    pub project_label: Option<String>,
    pub kitty_title: Option<String>,
    pub pid: u32,
    pub kitty_window_id: Option<String>,
    pub kitty_listen_on: Option<String>,
    pub kitty_focused: Option<bool>,
    pub status: Status,
    pub preview: Option<Preview>,
}
pub struct Session {
    pub update: Update,
    pub seen: Instant,
    pub changed: Instant,
    pub completed: Option<Instant>,
    pub waiting: bool,
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
    let previous = sessions.get(&update.id);
    let completed = if previous.is_some_and(|s| s.status(now) == Status::Working)
        && update.status == Status::Idle
    {
        Some(now)
    } else {
        previous.and_then(|s| s.completed)
    };
    let waiting = update.status == Status::Idle
        && update.kitty_focused != Some(true)
        && previous.is_some_and(|s| {
            s.status(now) != Status::Disconnected && (s.waiting || s.status(now) == Status::Working)
        });
    sessions.insert(
        update.id.clone(),
        Session {
            update,
            seen: now,
            changed,
            completed,
            waiting,
        },
    );
    sessions.retain(|_, s| now.duration_since(s.seen) < Duration::from_secs(300));
}
pub fn acknowledge(sessions: &Sessions, id: &str, completed: Option<Instant>) {
    if let Ok(mut sessions) = sessions.lock()
        && let Some(session) = sessions.get_mut(id)
        && session.completed == completed
    {
        session.waiting = false;
    }
}

fn decode(bytes: &[u8]) -> Option<Update> {
    let u: Update = serde_json::from_slice(bytes).ok()?;
    if u.version != 1
        || u.id.is_empty()
        || u.id.len() > 200
        || u.cwd.len() > 4096
        || !u.cwd.starts_with('/')
        || u.project_label.as_ref().is_some_and(|s| s.len() > 4096)
        || u.kitty_title.as_ref().is_some_and(|s| s.len() > 1000)
        || u.preview.as_ref().is_some_and(|p| {
            [&p.first, &p.last]
                .iter()
                .any(|s| s.chars().count() > 240 || s.chars().any(char::is_control))
        })
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
pub fn start() -> io::Result<(Sessions, PathBuf)> {
    let dir = data_dir()?.join("Pi Watch");
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
    let state_dir = dir;
    thread::spawn(move || {
        let _lock = lock;
        for stream in listener.incoming().flatten() {
            let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
            let mut bytes = Vec::new();
            if stream.take(8193).read_to_end(&mut bytes).is_err() {
                crate::shortcut::trace(format_args!(
                    "report rejected: read failed bytes={}",
                    bytes.len()
                ));
                continue;
            }
            if bytes.len() > 8192 {
                crate::shortcut::trace(format_args!("report rejected: exceeds 8192-byte limit"));
                continue;
            }
            let Some(update) = decode(&bytes) else {
                // Never print serde errors: they can contain rejected input values.
                crate::shortcut::trace(format_args!(
                    "report rejected: schema or validation bytes={}",
                    bytes.len()
                ));
                continue;
            };
            crate::shortcut::trace(format_args!(
                "report received pid={} status={:?} bytes={} preview={} first_chars={} last_chars={}",
                update.pid,
                update.status,
                bytes.len(),
                update.preview.is_some(),
                update
                    .preview
                    .as_ref()
                    .map_or(0, |p| p.first.chars().count()),
                update
                    .preview
                    .as_ref()
                    .map_or(0, |p| p.last.chars().count()),
            ));
            if let Ok(mut sessions) = state.lock() {
                apply(&mut sessions, update, Instant::now());
            } else {
                crate::shortcut::trace(format_args!("report not applied: session lock poisoned"));
            }
        }
    });
    Ok((sessions, state_dir))
}

// This directory must match the extension's `ipcDirectory` for this platform.
fn data_dir() -> io::Result<PathBuf> {
    if let Some(base) = std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|value| value.starts_with('/'))
    {
        return Ok(PathBuf::from(base));
    }
    let home = std::env::var_os("HOME").ok_or_else(|| io::Error::other("HOME is not set"))?;
    Ok(PathBuf::from(home).join(".local/share"))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn update(status: Status) -> Update {
        Update {
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
    #[test]
    fn preview_accepts_bounded_unicode_and_rejects_oversize_or_controls() {
        let base = include_str!("../../../test/fixtures/protocol-v1.json");
        let mut value: serde_json::Value = serde_json::from_str(base).unwrap();
        value["preview"] = serde_json::json!({"first": "😀".repeat(240), "last": "end"});
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_some());
        value["preview"]["first"] = serde_json::json!("a".repeat(241));
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_none());
        value["preview"]["first"] = serde_json::json!("bad\ntext");
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_none());
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
    fn completion_requires_connected_active_to_idle_transition() {
        let mut sessions = BTreeMap::new();
        let now = Instant::now();
        apply(&mut sessions, update(Status::Idle), now);
        assert_eq!(sessions["session"].completed, None);
        apply(&mut sessions, update(Status::Working), now);
        let finished = now + Duration::from_secs(1);
        apply(&mut sessions, update(Status::Idle), finished);
        assert_eq!(sessions["session"].completed, Some(finished));
        apply(
            &mut sessions,
            update(Status::Idle),
            now + Duration::from_secs(2),
        );
        assert_eq!(sessions["session"].completed, Some(finished));
        apply(
            &mut sessions,
            update(Status::Working),
            now + Duration::from_secs(3),
        );
        apply(
            &mut sessions,
            update(Status::Idle),
            now + Duration::from_secs(30),
        );
        assert_eq!(sessions["session"].completed, Some(finished));
        let mut reload = update(Status::Idle);
        reload.id = "reload".into();
        apply(&mut sessions, reload, now + Duration::from_secs(31));
        assert_eq!(sessions["reload"].completed, None);
    }
    #[test]
    fn waiting_persists_until_focus_and_does_not_repeat_on_heartbeats() {
        let mut sessions = BTreeMap::new();
        let now = Instant::now();
        apply(&mut sessions, update(Status::Idle), now);
        assert!(!sessions["session"].waiting);
        apply(&mut sessions, update(Status::Working), now);
        apply(
            &mut sessions,
            update(Status::Idle),
            now + Duration::from_secs(1),
        );
        assert!(sessions["session"].waiting);
        apply(
            &mut sessions,
            update(Status::Idle),
            now + Duration::from_secs(2),
        );
        assert!(sessions["session"].waiting);
        let mut focused = update(Status::Idle);
        focused.kitty_focused = Some(true);
        apply(&mut sessions, focused, now + Duration::from_secs(3));
        assert!(!sessions["session"].waiting);
        apply(
            &mut sessions,
            update(Status::Idle),
            now + Duration::from_secs(4),
        );
        assert!(!sessions["session"].waiting);
    }
    #[test]
    fn waiting_resets_for_active_disconnect_reload_and_stale_reconnect() {
        let now = Instant::now();
        for status in [Status::Working, Status::Disconnected] {
            let mut sessions = BTreeMap::new();
            apply(&mut sessions, update(Status::Working), now);
            apply(&mut sessions, update(Status::Idle), now);
            assert!(sessions["session"].waiting);
            apply(&mut sessions, update(status), now);
            assert!(!sessions["session"].waiting);
        }
        let mut sessions = BTreeMap::new();
        apply(&mut sessions, update(Status::Working), now);
        apply(&mut sessions, update(Status::Idle), now);
        apply(
            &mut sessions,
            update(Status::Idle),
            now + Duration::from_secs(20),
        );
        assert!(!sessions["session"].waiting);
        apply(
            &mut sessions,
            update(Status::Working),
            now + Duration::from_secs(21),
        );
        let mut focused = update(Status::Idle);
        focused.kitty_focused = Some(true);
        apply(&mut sessions, focused, now + Duration::from_secs(22));
        assert!(!sessions["session"].waiting);
        apply(
            &mut sessions,
            update(Status::Working),
            now + Duration::from_secs(23),
        );
        apply(
            &mut sessions,
            update(Status::Idle),
            now + Duration::from_secs(24),
        );
        let mut reload = update(Status::Idle);
        reload.id = "reload".into();
        apply(&mut sessions, reload, now + Duration::from_secs(25));
        assert!(!sessions["reload"].waiting);
    }
    #[test]
    fn navigation_acknowledges_only_the_selected_completion() {
        let sessions = Sessions::default();
        let now = Instant::now();
        {
            let mut state = sessions.lock().unwrap();
            apply(&mut state, update(Status::Working), now);
            apply(
                &mut state,
                update(Status::Idle),
                now + Duration::from_secs(1),
            );
        }
        acknowledge(&sessions, "session", Some(now));
        assert!(sessions.lock().unwrap()["session"].waiting);
        acknowledge(&sessions, "other", Some(now + Duration::from_secs(1)));
        assert!(sessions.lock().unwrap()["session"].waiting);
        acknowledge(&sessions, "session", Some(now + Duration::from_secs(1)));
        assert!(!sessions.lock().unwrap()["session"].waiting);
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
    fn accepts_the_extension_protocol_fixture() {
        let update = decode(include_bytes!("../../../test/fixtures/protocol-v1.json"))
            .expect("the extension's protocol 1 report must remain compatible");
        assert_eq!(update.version, 1);
        assert_eq!(update.pid, 42);
        assert_eq!(update.cwd, "/tmp/project");
        assert_eq!(update.status, Status::Idle);
        assert_eq!(update.kitty_window_id.as_deref(), Some("123"));
        assert_eq!(
            update.kitty_listen_on.as_deref(),
            Some("unix:/tmp/kitty-test.sock")
        );
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
