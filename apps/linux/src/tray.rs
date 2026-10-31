use crate::focus::{self, Target};
use crate::monitor::{self, Sessions, Status};
use ksni::{
    Category, Status as TrayStatus, ToolTip, Tray,
    blocking::TrayMethods,
    menu::{MenuItem, StandardItem, SubMenu},
};
use std::{
    path::Path,
    time::{Duration, Instant},
};

struct WatchTray {
    sessions: Sessions,
    icon_dir: String,
}

impl Tray for WatchTray {
    // A left click opens the menu, matching the macOS status item behavior.
    const MENU_ON_ACTIVATE: bool = true;

    fn id(&self) -> String {
        "pi-watch".into()
    }

    fn category(&self) -> Category {
        Category::ApplicationStatus
    }

    fn title(&self) -> String {
        // Icon only: the state lives in the menu content, not in the top bar.
        String::new()
    }

    fn status(&self) -> TrayStatus {
        TrayStatus::Active
    }

    fn icon_theme_path(&self) -> String {
        self.icon_dir.clone()
    }

    fn icon_name(&self) -> String {
        "pi-watch".into()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            icon_name: "pi-watch".into(),
            title: "Pi Watch".into(),
            ..ToolTip::default()
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        render(&self.sessions)
    }
}

// The icon ships as a committed SVG for the glyph the menu bar shows on macOS.
fn icon_svg() -> &'static str {
    include_str!("../resources/pi-watch.svg")
}

fn write_icon(data_dir: &Path) -> std::io::Result<()> {
    let path = data_dir.join("pi-watch.svg");
    if !path.exists() {
        std::fs::write(&path, icon_svg())?;
    }
    Ok(())
}

pub fn run(sessions: Sessions, data_dir: std::path::PathBuf) {
    if let Err(error) = write_icon(&data_dir) {
        eprintln!("Pi Watch could not install its tray icon: {error}");
    }
    let service = WatchTray {
        sessions,
        icon_dir: data_dir.to_string_lossy().into_owned(),
    };
    let handle = match service.spawn() {
        Ok(handle) => handle,
        Err(error) => {
            eprintln!("Pi Watch cannot show a tray icon: {error}");
            // Without a tray the app cannot show anything. Keep serving reports
            // in case the desktop tray comes back.
            loop {
                std::thread::sleep(Duration::from_secs(60));
            }
        }
    };
    // Menu content refreshes once per second during the normal run loop.
    loop {
        std::thread::sleep(Duration::from_secs(1));
        if handle.update(|_| {}).is_none() {
            // The tray service is gone; this run has nothing left to do.
            return;
        }
    }
}

fn escape_label(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .replace('_', "__")
}

fn label_item(label: &str) -> StandardItem<WatchTray> {
    StandardItem {
        label: escape_label(label),
        ..StandardItem::default()
    }
}

fn separator() -> MenuItem<WatchTray> {
    MenuItem::Separator
}

fn base_name(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Project")
}

fn format_row(dot: &str, name: &str, elapsed: &Duration, title: &str) -> String {
    format!(
        "{dot} {name} · {}m {:02}s · {title}",
        elapsed.as_secs() / 60,
        elapsed.as_secs() % 60
    )
}

fn completion_digit(index: usize) -> Option<String> {
    (1..=9).contains(&index).then(|| index.to_string())
}

fn quit_item() -> MenuItem<WatchTray> {
    let mut item = StandardItem {
        label: "Quit Pi Watch".into(),
        ..StandardItem::default()
    };
    item.activate = Box::new(|_| std::process::exit(0));
    item.into()
}

fn render(sessions: &Sessions) -> Vec<MenuItem<WatchTray>> {
    let Ok(state) = sessions.lock() else {
        return vec![label_item("Session state unavailable").into()];
    };
    let now = Instant::now();
    let mut rows: Vec<_> = state
        .values()
        .filter(|s| s.status(now) != Status::Disconnected)
        .collect();
    rows.sort_by(|a, b| {
        b.waiting
            .cmp(&a.waiting)
            .then(a.update.cwd.cmp(&b.update.cwd))
            .then(a.update.pid.cmp(&b.update.pid))
    });
    if rows.is_empty() {
        return vec![
            label_item("No connected Pi sessions").into(),
            label_item("Load extensions/watch/index.ts in Pi").into(),
            separator(),
            quit_item(),
        ];
    }
    let mut items = Vec::new();
    let mut in_done = rows.iter().any(|s| s.waiting);
    if in_done {
        items.push(label_item("Done — awaiting inspection").into());
    }
    let mut project = String::new();
    let mut numbered = 0;
    for session in rows {
        if in_done && !session.waiting {
            in_done = false;
            items.push(separator());
            project.clear();
        }
        if !session.waiting && session.update.cwd != project {
            if !project.is_empty() {
                items.push(separator());
            }
            project = session.update.cwd.clone();
            items.push(
                label_item(
                    session
                        .update
                        .project_label
                        .as_deref()
                        .unwrap_or_else(|| base_name(&project)),
                )
                .into(),
            );
        }
        let status = session.status(now);
        let (dot, name) = match status {
            Status::Working => ("●", "Active"),
            Status::Idle => ("○", "Idle"),
            Status::Disconnected => ("–", "Disconnected"),
        };
        let elapsed = if status == Status::Disconnected {
            now.duration_since(session.seen)
        } else {
            now.duration_since(session.changed)
        };
        let pane = session
            .update
            .kitty_window_id
            .as_deref()
            .unwrap_or("unknown");
        let title = session
            .update
            .kitty_title
            .as_deref()
            .map_or_else(|| format!("pane {pane}"), str::to_string);
        let mut label = match session.waiting {
            true => format!(
                "{} · ",
                session.update.project_label.as_deref().unwrap_or("Project")
            ),
            false => String::new(),
        };
        label.push_str(&format_row(dot, name, &elapsed, &title));
        if session.waiting {
            numbered += 1;
            if let Some(digit) = completion_digit(numbered) {
                label = format!("{digit}. {label}");
            }
        }
        // Only reachable, non-disconnected rows navigate, as on macOS.
        let target = match (
            &session.update.kitty_listen_on,
            &session.update.kitty_window_id,
        ) {
            (Some(endpoint), Some(pane)) if status != Status::Disconnected => Some(Target {
                endpoint: endpoint.clone(),
                pane: pane.clone(),
            }),
            _ => None,
        };
        let enabled = target.is_some() || session.update.preview.is_some();
        let id = session.update.id.clone();
        let completed = session.completed;
        let click_sessions = sessions.clone();
        let base = target.clone();
        let row = StandardItem {
            label: escape_label(&label),
            enabled,
            // Menu callbacks may run more than once; clone the navigation inputs.
            activate: match base {
                Some(base) => Box::new(move |_| {
                    let click_sessions = click_sessions.clone();
                    let id = id.clone();
                    let target = base.clone();
                    let completed = completed;
                    focus::focus(
                        target,
                        move || monitor::acknowledge(&click_sessions, &id, completed),
                        || {},
                    );
                }),
                None => Box::new(|_| {}),
            },
            ..StandardItem::default()
        };
        let row: MenuItem<WatchTray> = match &session.update.preview {
            Some(preview) => {
                let mut submenu = vec![label_item(&preview.first).into()];
                if !preview.last.is_empty() {
                    submenu.push(label_item("…").into());
                    submenu.push(label_item(&preview.last).into());
                }
                SubMenu {
                    label: row.label.clone(),
                    enabled: row.enabled,
                    submenu,
                    ..SubMenu::default()
                }
                .into()
            }
            None => row.into(),
        };
        items.push(row);
    }
    items.push(separator());
    items.push(quit_item());
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_first_nine_completions_receive_digits() {
        assert_eq!(completion_digit(0), None);
        assert_eq!(completion_digit(1).as_deref(), Some("1"));
        assert_eq!(completion_digit(9).as_deref(), Some("9"));
        assert_eq!(completion_digit(10), None);
    }

    #[test]
    fn rows_use_the_short_form() {
        assert_eq!(
            format_row("●", "Active", &Duration::from_secs(65), "vim"),
            "● Active · 1m 05s · vim"
        );
    }

    #[test]
    fn labels_drop_control_characters_and_escape_accels() {
        assert_eq!(escape_label("a\u{7}b_c"), "ab__c");
    }

    #[test]
    fn base_names_fall_back_to_project() {
        assert_eq!(base_name("/tmp/project"), "project");
        assert_eq!(base_name("/"), "Project");
    }
}
