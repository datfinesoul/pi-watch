use crate::focus::{self, Target};
use crate::monitor::{self, Sessions, Status};
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained,
    runtime::ProtocolObject, sel,
};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSEventModifierFlags, NSMenu, NSMenuDelegate,
    NSMenuItem, NSPopover, NSPopoverBehavior, NSStatusBar, NSStatusItem, NSTextField, NSView,
    NSViewController,
};
use objc2_foundation::{
    NSObject, NSObjectProtocol, NSPoint, NSRect, NSRectEdge, NSSize, NSString, NSTimer,
};
use std::{
    cell::{Cell, RefCell},
    time::{Duration, Instant},
};

#[derive(Clone)]
struct NavigationTarget {
    target: Target,
    id: String,
    completed: Option<Instant>,
}

pub struct State {
    sessions: Sessions,
    item: Retained<NSStatusItem>,
    targets: RefCell<Vec<NavigationTarget>>,
    popover: Retained<NSPopover>,
    popover_until: Cell<Option<Instant>>,
    last_checked: Cell<Instant>,
    menu_open: Cell<bool>,
}
define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = State]
    struct Controller;
    unsafe impl NSObjectProtocol for Controller {}
    unsafe impl NSMenuDelegate for Controller {
        #[unsafe(method(menuWillOpen:))]
        fn menu_will_open(&self, _menu: &NSMenu) {
            crate::hotkey::trace(format_args!("delegate menuWillOpen menu={:p} previous_open={}", _menu, self.ivars().menu_open.get()));
            if let Ok(sessions) = self.ivars().sessions.lock() {
                let now = Instant::now();
                for session in sessions.values().filter(|s| s.status(now) != Status::Disconnected) {
                    crate::hotkey::trace(format_args!(
                        "menu session pid={} preview={} first_chars={} last_chars={}",
                        session.update.pid, session.update.preview.is_some(),
                        session.update.preview.as_ref().map_or(0, |p| p.first.chars().count()),
                        session.update.preview.as_ref().map_or(0, |p| p.last.chars().count()),
                    ));
                }
            }
            self.ivars().menu_open.set(true);
        }
        #[unsafe(method(menuDidClose:))]
        fn menu_did_close(&self, _menu: &NSMenu) {
            crate::hotkey::trace(format_args!("delegate menuDidClose menu={:p}", _menu));
            crate::hotkey::discard_pending();
            self.ivars().menu_open.set(false);
        }
    }
    impl Controller {
        #[unsafe(method(refresh:))]
        fn refresh(&self, _timer: &NSTimer) { self.render(); }
        #[unsafe(method(focus:))]
        fn focus(&self, sender: &NSMenuItem) {
            if let Some(target) = self.ivars().targets.borrow().get(sender.tag() as usize).cloned() {
                let sessions = self.ivars().sessions.clone();
                focus::focus(target.target, move || {
                    monitor::acknowledge(&sessions, &target.id, target.completed);
                });
            }
        }
        #[unsafe(method(quit:))]
        fn quit(&self, _sender: &NSObject) { NSApplication::sharedApplication(self.mtm()).terminate(None); }
    }
);
impl Controller {
    fn open_menu(&self) {
        crate::hotkey::trace(format_args!(
            "open_menu request open={}",
            self.ivars().menu_open.get()
        ));
        if self.ivars().menu_open.get() {
            crate::hotkey::trace(format_args!("open_menu ignored: menu already open"));
            return;
        }
        self.render();
        self.ivars().popover.close();
        if let Some(button) = self.ivars().item.button(self.mtm()) {
            // Use the native status-item menu for system appearance and placement.
            // Mark it open before dispatching the synthetic click.
            self.ivars().menu_open.set(true);
            crate::hotkey::trace(format_args!("status button performClick begin"));
            unsafe {
                button.performClick(None);
            }
            crate::hotkey::trace(format_args!(
                "status button performClick returned open={}",
                self.ivars().menu_open.get()
            ));
        } else {
            crate::hotkey::trace(format_args!("open_menu unavailable: no status button"));
        }
    }

    fn show_completions(&self, messages: &[String], now: Instant) {
        let state = self.ivars();
        if messages.is_empty() {
            if state.popover_until.get().is_some_and(|until| now >= until) {
                state.popover.close();
                state.popover_until.set(None);
            }
            return;
        }
        let Some(button) = state.item.button(self.mtm()) else {
            return;
        };
        let mut lines: Vec<_> = messages.iter().take(3).cloned().collect();
        if messages.len() > 3 {
            lines.push(format!("… and {} more", messages.len() - 3));
        }
        let text: String = lines
            .join("\n")
            .chars()
            .filter(|c| !c.is_control() || *c == '\n')
            .collect();
        let height = 28.0 + lines.len() as f64 * 42.0;
        let view = NSView::initWithFrame(
            NSView::alloc(self.mtm()),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(360.0, height)),
        );
        let label = NSTextField::labelWithString(&NSString::from_str(&text), self.mtm());
        label.setMaximumNumberOfLines(0);
        label.setFrame(NSRect::new(
            NSPoint::new(14.0, 14.0),
            NSSize::new(332.0, height - 28.0),
        ));
        view.addSubview(&label);
        let content = NSViewController::new(self.mtm());
        content.setView(&view);
        state.popover.setContentViewController(Some(&content));
        state.popover.setContentSize(NSSize::new(360.0, height));
        state.popover.showRelativeToRect_ofView_preferredEdge(
            button.bounds(),
            &button,
            NSRectEdge::MinY,
        );
        state.popover_until.set(Some(now + Duration::from_secs(5)));
    }

    fn render(&self) {
        // Freeze both menu labels and navigation targets during tracking, including timer callbacks.
        if self.ivars().menu_open.get() {
            return;
        }
        let mtm = self.mtm();
        let menu = NSMenu::new(mtm);
        menu.setDelegate(Some(ProtocolObject::from_ref(self)));
        let now = Instant::now();
        let state = self.ivars();
        let sessions = state.sessions.lock().expect("session state poisoned");
        let since = state.last_checked.replace(now);
        let completions: Vec<_> = sessions
            .values()
            .filter(|s| s.completed.is_some_and(|at| at > since && at <= now))
            .map(|s| {
                let project = s.update.project_label.as_deref().unwrap_or("Pi session");
                let title = s.update.kitty_title.as_deref().unwrap_or("Session");
                format!(
                    "○ {} — {} is idle",
                    project.chars().take(45).collect::<String>(),
                    title.chars().take(55).collect::<String>()
                )
            })
            .collect();
        let mut rows: Vec<_> = sessions
            .values()
            .filter(|s| s.status(now) != Status::Disconnected)
            .collect();
        rows.sort_by(|a, b| {
            b.waiting
                .cmp(&a.waiting)
                .then(a.update.cwd.cmp(&b.update.cwd))
                .then(a.update.pid.cmp(&b.update.pid))
        });
        let working = rows
            .iter()
            .filter(|s| s.status(now) == Status::Working)
            .count();
        let waiting = rows.iter().filter(|s| s.waiting).count();
        if let Some(button) = state.item.button(mtm) {
            button.setTitle(&NSString::from_str(&menu_bar_title(waiting, working)));
        }
        let mut targets = state.targets.borrow_mut();
        targets.clear();
        let mut project = String::new();
        if rows.is_empty() {
            add_label(&menu, "No connected Pi sessions", mtm);
            add_label(&menu, "Load extensions/watch/index.ts in Pi", mtm);
        }
        let mut in_done = waiting > 0;
        let mut numbered = 0;
        if in_done {
            add_label(&menu, "Done — awaiting inspection", mtm);
        }
        for session in rows {
            if in_done && !session.waiting {
                in_done = false;
                menu.addItem(&NSMenuItem::separatorItem(mtm));
                project.clear();
            }
            if !session.waiting && session.update.cwd != project {
                if !project.is_empty() {
                    menu.addItem(&NSMenuItem::separatorItem(mtm));
                }
                project = session.update.cwd.clone();
                let label = session.update.project_label.as_deref().unwrap_or_else(|| {
                    std::path::Path::new(&project)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("Project")
                });
                add_label(&menu, label, mtm);
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
            }
            .as_secs();
            let pane = session
                .update
                .kitty_window_id
                .as_deref()
                .unwrap_or("unknown");
            let title = session
                .update
                .kitty_title
                .as_deref()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("pane {pane}"));
            let marker = if session.waiting {
                format!(
                    "{} · ",
                    session.update.project_label.as_deref().unwrap_or("Project")
                )
            } else {
                String::new()
            };
            let item = add_label(
                &menu,
                &format!(
                    "{marker}{dot} {name} · {}m {:02}s · {title}",
                    elapsed / 60,
                    elapsed % 60
                ),
                mtm,
            );
            if status != Status::Disconnected
                && let (Some(endpoint), Some(pane)) = (
                    &session.update.kitty_listen_on,
                    &session.update.kitty_window_id,
                )
            {
                if session.waiting {
                    numbered += 1;
                    if let Some(digit) = completion_digit(numbered) {
                        item.setTitle(&NSString::from_str(&format!("{digit}. {}", item.title())));
                        item.setKeyEquivalent(&NSString::from_str(&digit));
                        item.setKeyEquivalentModifierMask(NSEventModifierFlags::empty());
                    }
                }
                item.setTag(targets.len() as isize);
                targets.push(NavigationTarget {
                    target: Target {
                        endpoint: endpoint.clone(),
                        pane: pane.clone(),
                    },
                    id: session.update.id.clone(),
                    completed: session.completed,
                });
                unsafe {
                    item.setAction(Some(sel!(focus:)));
                    item.setTarget(Some(self));
                }
                item.setEnabled(true);
            }
            if let Some(preview) = &session.update.preview {
                let submenu = NSMenu::new(mtm);
                add_preview(&submenu, &preview.first, mtm);
                if !preview.last.is_empty() {
                    add_label(&submenu, "…", mtm);
                    add_preview(&submenu, &preview.last, mtm);
                }
                item.setSubmenu(Some(&submenu));
                item.setEnabled(true);
            }
        }
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        let quit = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &NSString::from_str("Quit Pi Watch"),
                Some(sel!(quit:)),
                &NSString::from_str("q"),
            )
        };
        unsafe {
            quit.setTarget(Some(self));
        }
        menu.addItem(&quit);
        state.item.setMenu(Some(&menu));
        drop(sessions);
        if cfg!(feature = "popovers") {
            self.show_completions(&completions, now);
        }
    }
}
fn menu_bar_title(waiting: usize, working: usize) -> String {
    if waiting > 0 {
        format!("π {waiting}")
    } else if working > 0 {
        "π ●".into()
    } else {
        "π".into()
    }
}
fn completion_digit(index: usize) -> Option<String> {
    (1..=9).contains(&index).then(|| index.to_string())
}

fn add_preview(menu: &NSMenu, text: &str, mtm: MainThreadMarker) {
    let item = add_label(menu, "", mtm);
    let label = NSTextField::wrappingLabelWithString(&NSString::from_str(text), mtm);
    label.setMaximumNumberOfLines(0);
    label.setFrame(NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(400.0, 0.0)));
    label.sizeToFit();
    let height = label.frame().size.height.max(20.0);
    let view = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(428.0, height + 16.0)),
    );
    label.setFrame(NSRect::new(
        NSPoint::new(14.0, 8.0),
        NSSize::new(400.0, height),
    ));
    view.addSubview(&label);
    item.setView(Some(&view));
}

fn add_label(menu: &NSMenu, text: &str, mtm: MainThreadMarker) -> Retained<NSMenuItem> {
    let clean: String = text.chars().filter(|c| !c.is_control()).collect();
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(&clean),
            None,
            &NSString::new(),
        )
    };
    item.setEnabled(false);
    menu.addItem(&item);
    item
}
pub fn run(sessions: Sessions) {
    let mtm = MainThreadMarker::new().expect("UI must run on main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    let item = NSStatusBar::systemStatusBar().statusItemWithLength(-1.0);
    let popover = NSPopover::new(mtm);
    popover.setBehavior(NSPopoverBehavior::Transient);
    let controller = Controller::alloc(mtm).set_ivars(State {
        sessions,
        item,
        targets: RefCell::new(Vec::new()),
        popover,
        popover_until: Cell::new(None),
        last_checked: Cell::new(Instant::now()),
        menu_open: Cell::new(false),
    });
    let controller: Retained<Controller> = unsafe { msg_send![super(controller), init] };
    controller.render();
    let shortcut_controller = controller.clone();
    let _hotkey = crate::hotkey::HotKey::register(move || shortcut_controller.open_menu())
        .map_err(|status| eprintln!("Pi Watch could not register Cmd+Shift+' (macOS error {status}); use the menu bar instead"));
    let _timer = unsafe {
        NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
            1.0,
            &controller,
            sel!(refresh:),
            None,
            true,
        )
    };
    app.run();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn completion_count_wins_over_work_and_idle_has_no_count() {
        assert_eq!(menu_bar_title(3, 5), "π 3");
        assert_eq!(menu_bar_title(3, 0), "π 3");
        assert_eq!(menu_bar_title(0, 5), "π ●");
        assert_eq!(menu_bar_title(0, 0), "π");
    }
    #[test]
    fn only_first_nine_navigation_targets_get_digits() {
        assert_eq!(completion_digit(0), None);
        assert_eq!(completion_digit(1).as_deref(), Some("1"));
        assert_eq!(completion_digit(9).as_deref(), Some("9"));
        assert_eq!(completion_digit(10), None);
    }
}
