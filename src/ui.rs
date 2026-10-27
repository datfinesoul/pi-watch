use crate::monitor::{Sessions, Status};
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained, sel,
};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem,
};
use objc2_foundation::{NSObject, NSObjectProtocol, NSString, NSTimer};
use std::time::{Duration, Instant};

pub struct State {
    sessions: Sessions,
    item: Retained<NSStatusItem>,
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
        #[unsafe(method(quit:))]
        fn quit(&self, _sender: &NSObject) { NSApplication::sharedApplication(self.mtm()).terminate(None); }
    }
);
impl Controller {
    fn render(&self) {
        let mtm = self.mtm();
        let menu = NSMenu::new(mtm);
        let now = Instant::now();
        let state = self.ivars();
        let sessions = state.sessions.lock().expect("session state poisoned");
        let mut rows: Vec<_> = sessions
            .values()
            .filter(|s| now.duration_since(s.seen) < Duration::from_secs(300))
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
        if let Some(button) = state.item.button(mtm) {
            button.setTitle(&NSString::from_str(&format!("Pi {working}/{}", rows.len())));
        }
        let mut project = String::new();
        if rows.is_empty() {
            add_label(&menu, "No reporting Pi sessions", mtm);
            add_label(&menu, "Load extensions/pi-watch.ts in Pi", mtm);
        }
        for session in rows {
            if session.update.cwd != project {
                if !project.is_empty() {
                    menu.addItem(&NSMenuItem::separatorItem(mtm));
                }
                project = session.update.cwd.clone();
                add_label(&menu, &project, mtm);
            }
            let status = session.status(now);
            let (dot, name) = match status {
                Status::Working => ("●", "Working"),
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
            add_label(
                &menu,
                &format!(
                    "{dot} {name} · {}m {:02}s · pane {pane} · PID {}",
                    elapsed / 60,
                    elapsed % 60,
                    session.update.pid
                ),
                mtm,
            );
        }
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        add_label(&menu, "Pane focus unavailable in this version", mtm);
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
    }
}
fn add_label(menu: &NSMenu, text: &str, mtm: MainThreadMarker) {
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
}
pub fn run(sessions: Sessions) {
    let mtm = MainThreadMarker::new().expect("UI must run on main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    let item = NSStatusBar::systemStatusBar().statusItemWithLength(-1.0);
    let controller = Controller::alloc(mtm).set_ivars(State { sessions, item });
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
