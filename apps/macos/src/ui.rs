use crate::focus::{self, Target};
use crate::monitor::{self, Sessions, Status};
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained, sel,
};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSMenu, NSMenuItem, NSPopover, NSPopoverBehavior,
    NSStatusBar, NSStatusItem, NSTextField, NSView, NSViewController,
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
}
define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = State]
    struct Controller;
    unsafe impl NSObjectProtocol for Controller {}
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
        let mtm = self.mtm();
        let menu = NSMenu::new(mtm);
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
            a.update
                .cwd
                .cmp(&b.update.cwd)
                .then(a.update.pid.cmp(&b.update.pid))
        });
        let working = rows
            .iter()
            .filter(|s| s.status(now) == Status::Working)
            .count();
        let waiting = rows.iter().filter(|s| s.waiting).count();
        let alert = if waiting > 0 {
            format!("  ! {waiting}")
        } else {
            String::new()
        };
        if let Some(button) = state.item.button(mtm) {
            button.setTitle(&NSString::from_str(&format!(
                "π ● {working}  ○ {}{alert}",
                rows.len() - working
            )));
        }
        let mut targets = state.targets.borrow_mut();
        targets.clear();
        let mut project = String::new();
        if rows.is_empty() {
            add_label(&menu, "No connected Pi sessions", mtm);
            add_label(&menu, "Load extensions/watch/index.ts in Pi", mtm);
        }
        for session in rows {
            if session.update.cwd != project {
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
            let marker = if session.waiting { "! " } else { "" };
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
    });
    let controller: Retained<Controller> = unsafe { msg_send![super(controller), init] };
    controller.render();
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
