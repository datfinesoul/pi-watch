//! A registered shortcut, not a global keyboard monitor. No Input Monitoring permission.
use std::{cell::Cell, ffi::c_void, ptr, sync::OnceLock, time::Instant};

pub fn trace(message: std::fmt::Arguments<'_>) {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    static START: OnceLock<Instant> = OnceLock::new();
    if *ENABLED.get_or_init(|| std::env::var_os("PI_WATCH_DEBUG").is_some_and(|v| v == "1")) {
        let elapsed = START.get_or_init(Instant::now).elapsed().as_secs_f64();
        eprintln!(
            "[pi-watch debug pid={} +{elapsed:.6}s] {message}",
            std::process::id()
        );
    }
}

// Guard callbacks delivered reentrantly as well as queued presses (see discard_pending).
#[derive(Default)]
struct InvocationGate(Cell<bool>);
impl InvocationGate {
    fn run(&self, callback: impl FnOnce()) {
        if self.0.replace(true) {
            trace(format_args!("shortcut ignored: callback already active"));
            return;
        }
        trace(format_args!("shortcut callback enter"));
        struct Reset<'a>(&'a Cell<bool>);
        impl Drop for Reset<'_> {
            fn drop(&mut self) {
                self.0.set(false);
                trace(format_args!("shortcut callback leave"));
            }
        }
        let _reset = Reset(&self.0);
        callback();
    }
}

thread_local! {
    static DISCARDED_THROUGH: Cell<f64> = const { Cell::new(0.0) };
}

/// Application-target Carbon events can remain queued during AppKit tracking.
/// Do not let presses made before dismissal open another menu afterward.
pub fn discard_pending() {
    // SAFETY: Carbon's clock has no preconditions, and this state is main-thread local.
    let now = unsafe { GetCurrentEventTime() };
    DISCARDED_THROUGH.with(|cutoff| cutoff.set(now));
    trace(format_args!("menu dismissal cutoff={now:.6}"));
}

fn is_fresh(pressed: f64, dismissed: f64) -> bool {
    pressed > dismissed
}

type Ref = *mut c_void;
type Handler = unsafe extern "C" fn(Ref, Ref, *mut c_void) -> i32;
#[repr(C)]
struct EventType {
    class: u32,
    kind: u32,
}
#[repr(C)]
struct HotKeyId {
    signature: u32,
    id: u32,
}
#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn GetApplicationEventTarget() -> Ref;
    fn GetEventTime(event: Ref) -> f64;
    fn GetCurrentEventTime() -> f64;
    fn InstallEventHandler(
        target: Ref,
        handler: Handler,
        count: u32,
        types: *const EventType,
        data: *mut c_void,
        result: *mut Ref,
    ) -> i32;
    fn RemoveEventHandler(handler: Ref) -> i32;
    fn RegisterEventHotKey(
        key: u32,
        modifiers: u32,
        id: HotKeyId,
        target: Ref,
        options: u32,
        result: *mut Ref,
    ) -> i32;
    fn UnregisterEventHotKey(key: Ref) -> i32;
}

pub struct HotKey {
    key: Ref,
    handler: Ref,
    // The outer box gives Carbon a stable pointer to the trait object.
    _callback: Box<Box<dyn Fn()>>,
}
unsafe extern "C" fn invoke(_: Ref, event: Ref, data: *mut c_void) -> i32 {
    // SAFETY: Carbon supplies a valid event for the duration of this callback.
    let pressed = unsafe { GetEventTime(event) };
    let cutoff = DISCARDED_THROUGH.with(Cell::get);
    // SAFETY: Carbon's clock has no preconditions.
    let delivered = unsafe { GetCurrentEventTime() };
    trace(format_args!(
        "shortcut delivered event={pressed:.6} now={delivered:.6} delay={:.6} cutoff={cutoff:.6}",
        delivered - pressed
    ));
    if !is_fresh(pressed, cutoff) {
        trace(format_args!("shortcut ignored: event predates dismissal"));
        return 0;
    }
    // SAFETY: registration owns this box until after the handler is removed.
    let callback = unsafe { &*data.cast::<Box<dyn Fn()>>() };
    // Never unwind across the C event dispatcher.
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(callback)).is_err() {
        eprintln!("Pi Watch shortcut handler failed");
    }
    0
}
impl HotKey {
    pub fn register(callback: impl Fn() + 'static) -> Result<Self, i32> {
        trace(format_args!(
            "register Cmd+Shift+quote version={}",
            env!("CARGO_PKG_VERSION")
        ));
        let gate = InvocationGate::default();
        let mut registration = Self {
            key: ptr::null_mut(),
            handler: ptr::null_mut(),
            _callback: Box::new(Box::new(move || gate.run(&callback))),
        };
        let event = EventType {
            class: u32::from_be_bytes(*b"keyb"),
            kind: 5,
        };
        // SAFETY: called on the AppKit main thread. All pointers remain valid until Drop.
        unsafe {
            let target = GetApplicationEventTarget();
            let status = InstallEventHandler(
                target,
                invoke,
                1,
                &event,
                (&mut *registration._callback as *mut Box<dyn Fn()>).cast(),
                &mut registration.handler,
            );
            if status != 0 {
                return Err(status);
            }
            // ANSI quote key (39), Command (256) + Shift (512), exclusive registration.
            let status = RegisterEventHotKey(
                39,
                256 | 512,
                HotKeyId {
                    signature: u32::from_be_bytes(*b"piwt"),
                    id: 1,
                },
                target,
                1,
                &mut registration.key,
            );
            if status != 0 {
                return Err(status);
            }
        }
        trace(format_args!("shortcut registration complete"));
        Ok(registration)
    }
}
impl Drop for HotKey {
    fn drop(&mut self) {
        // SAFETY: these handles belong to this registration. Remove before releasing callback.
        unsafe {
            if !self.key.is_null() {
                UnregisterEventHotKey(self.key);
            }
            if !self.handler.is_null() {
                RemoveEventHandler(self.handler);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_presses_from_before_dismissal_do_not_reopen_menu() {
        let dismissed = 12.0;
        assert!(!is_fresh(10.0, dismissed));
        assert!(!is_fresh(11.0, dismissed));
        assert!(!is_fresh(12.0, dismissed));
        assert!(is_fresh(13.0, dismissed));
    }

    #[test]
    fn repeated_shortcuts_cannot_nest_and_work_again_after_dismissal() {
        let gate = InvocationGate::default();
        let opens = Cell::new(0);
        gate.run(|| {
            opens.set(opens.get() + 1);
            // Simulate two Carbon events delivered during menu tracking.
            gate.run(|| opens.set(opens.get() + 1));
            gate.run(|| opens.set(opens.get() + 1));
        });
        assert_eq!(opens.get(), 1);
        gate.run(|| opens.set(opens.get() + 1));
        assert_eq!(opens.get(), 2);
    }

    #[test]
    fn panic_does_not_leave_shortcut_disabled() {
        let gate = InvocationGate::default();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            gate.run(|| panic!("callback failed"));
        }));
        let called = Cell::new(false);
        gate.run(|| called.set(true));
        assert!(called.get());
    }
}
