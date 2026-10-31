//! The registered Super+Shift+' X11 shortcut with its popup state machine.
use std::{
    ffi::{c_int, c_uint},
    ptr,
    sync::{
        OnceLock,
        atomic::{AtomicU32, Ordering},
    },
};
use x11::xlib;

use crate::{
    focus,
    monitor::{self, Sessions},
    popup::{self, Outcome, Popup},
};

pub fn trace(message: std::fmt::Arguments<'_>) {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    if *ENABLED.get_or_init(|| std::env::var_os("PI_WATCH_DEBUG").is_some_and(|value| value == "1"))
    {
        let elapsed = START
            .get_or_init(std::time::Instant::now)
            .elapsed()
            .as_secs_f64();
        eprintln!(
            "[pi-watch debug pid={} +{elapsed:.6}s] {message}",
            std::process::id()
        );
    }
}

pub type XResult = Result<(), i32>;

// The default X error handler exits the process. Grabs fail with BadAccess when
// another program owns the combination, which is a normal condition to report.
static GRAB_ERROR: AtomicU32 = AtomicU32::new(0);

extern "C" fn grab_error_handler(
    _display: *mut xlib::Display,
    event: *mut xlib::XErrorEvent,
) -> c_int {
    // SAFETY: X provides a valid error for the duration of the callback.
    let code = unsafe { (*event).error_code };
    GRAB_ERROR.store(u32::from(code), Ordering::SeqCst);
    0
}

// Key state must be exactly Super+Shift, with Caps- and Num-lock allowed freely.
fn state_is_super_shift(state: c_uint) -> bool {
    const SUPER_SHIFT: c_uint = xlib::Mod4Mask | xlib::ShiftMask;
    const LOCKS: c_uint = xlib::LockMask | xlib::Mod2Mask;
    state & !LOCKS == SUPER_SHIFT
}

pub fn register(sessions: Sessions) -> XResult {
    trace(format_args!(
        "register Super+Shift+apostrophe version={}",
        env!("CARGO_PKG_VERSION")
    ));
    // SAFETY: a null display name requests the environment default.
    let display = unsafe { xlib::XOpenDisplay(ptr::null()) };
    if display.is_null() {
        return Err(-1);
    }
    // SAFETY: keysym names are NUL-terminated UTF-8 strings.
    let keysym = unsafe { xlib::XStringToKeysym(c"apostrophe".as_ptr()) };
    // SAFETY: the display is valid for the rest of this function.
    let keycode = unsafe { xlib::XKeysymToKeycode(display, keysym) };
    if keycode == 0 {
        // SAFETY: this connection belongs to the caller and no other thread uses it.
        unsafe { xlib::XCloseDisplay(display) };
        return Err(-2);
    }
    let grab = grab(display, keycode);
    if let Err(code) = grab {
        // SAFETY: this connection belongs to the caller and no other thread uses it.
        unsafe { xlib::XCloseDisplay(display) };
        return Err(code);
    }
    listen(display, c_uint::from(keycode), sessions);
    Ok(())
}

fn grab(display: *mut xlib::Display, keycode: xlib::KeyCode) -> XResult {
    let previous = unsafe { xlib::XSetErrorHandler(Some(grab_error_handler)) };
    GRAB_ERROR.store(0, Ordering::SeqCst);
    let root = unsafe { xlib::XDefaultRootWindow(display) };
    // Exclusive grabs registered for each Caps- and Num-lock variation.
    const SUPER_SHIFT: c_uint = xlib::Mod4Mask | xlib::ShiftMask;
    for extra in [
        0,
        xlib::LockMask,
        xlib::Mod2Mask,
        xlib::LockMask | xlib::Mod2Mask,
    ] {
        // SAFETY: the display, root window, and keycode are valid for the lifetime
        // of the connection, which the event-loop thread then owns.
        unsafe {
            xlib::XGrabKey(
                display,
                c_int::from(keycode),
                SUPER_SHIFT | extra,
                root,
                xlib::False,
                xlib::GrabModeAsync,
                xlib::GrabModeAsync,
            );
        }
    }
    // SAFETY: the grab requests above are flushed so errors surface here.
    unsafe { xlib::XSync(display, xlib::False) };
    let code = GRAB_ERROR.swap(0, Ordering::SeqCst) as i32;
    // SAFETY: the handler was returned by the matching earlier call.
    unsafe { xlib::XSetErrorHandler(previous) };
    if code != 0 {
        return Err(code);
    }
    Ok(())
}

// Raw display pointers are not marked Send; the event-loop thread is the only
// owner after handover.
struct XDisplay(*mut xlib::Display);
// SAFETY: listen hands the display to exactly one thread and nobody else keeps it.
unsafe impl Send for XDisplay {}

fn listen(display: *mut xlib::Display, keycode: c_uint, sessions: Sessions) {
    let fd = unsafe { xlib::XConnectionNumber(display) };
    // SAFETY: Only this thread touches the display from here on.
    let owned = XDisplay(display);
    std::thread::spawn(move || run(owned, fd, keycode, sessions));
}

fn run(XDisplay(display): XDisplay, fd: c_int, keycode: c_uint, sessions: Sessions) {
    let mut poll = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    // Two states: hidden, or a popup open with input grabbed. The popup is
    // driven entirely from this thread so the display stays single-owner.
    let mut popup: Option<Popup> = None;
    loop {
        let pending = unsafe { xlib::XPending(display) };
        if pending <= 0 {
            // SAFETY: a valid descriptor with its events field set once.
            let ready = unsafe { libc::poll(&mut poll, 1, 500) };
            if ready == 0
                || (ready < 0
                    && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted)
            {
                continue;
            }
            if ready < 0 || poll.revents & libc::POLLHUP != 0 {
                // The shortcut display ended: the popup window, grabs, and font
                // all die with this connection, so only the Rust side stays.
                if let Some(current) = popup.take() {
                    current.abandon();
                }
                trace(format_args!("shortcut display connection ended"));
                return;
            }
            continue;
        }
        let mut event = xlib::XEvent { pad: [0; 24] };
        // SAFETY: XNextEvent writes the event before any read.
        unsafe { xlib::XNextEvent(display, &mut event) };
        match unsafe { event.type_ } {
            xlib::KeyPress => {
                // SAFETY: key events alias the key fields of the union.
                let key = unsafe { event.key };
                handle_key(display, keycode, &sessions, &mut popup, &key);
            }
            xlib::ButtonPress if popup.is_some() => {
                // SAFETY: button events alias the button fields of the union.
                let button = unsafe { event.button };
                let outcome = popup
                    .as_ref()
                    .unwrap()
                    .press_at(button.x_root, button.y_root);
                handle(display, &sessions, &mut popup, outcome);
            }
            xlib::Expose if popup.is_some() => {
                if let Some(current) = popup.as_ref() {
                    current.draw(display);
                }
            }
            _ => {}
        }
    }
}

fn handle_key(
    display: *mut xlib::Display,
    keycode: c_uint,
    sessions: &Sessions,
    popup: &mut Option<Popup>,
    key: &xlib::XKeyEvent,
) {
    if let Some(current) = popup.as_ref() {
        let outcome = current.key(key);
        handle(display, sessions, popup, outcome);
        return;
    }
    if key.keycode == keycode && state_is_super_shift(key.state) {
        trace(format_args!("shortcut delivered"));
        match popup::open(display, sessions) {
            Ok(opened) => *popup = Some(opened),
            Err(code) => trace(format_args!("popup not opened: X status {code}")),
        }
    }
}

fn handle(
    display: *mut xlib::Display,
    sessions: &Sessions,
    popup: &mut Option<Popup>,
    outcome: Outcome,
) {
    match outcome {
        Outcome::StayOpen => {}
        Outcome::Dismiss => {
            trace(format_args!("popup dismissed"));
            close(display, popup);
        }
        Outcome::Quit => {
            trace(format_args!("popup requested quit"));
            close(display, popup);
            std::process::exit(0);
        }
        Outcome::Navigate(nav) => {
            trace(format_args!("popup activated a completion"));
            close(display, popup);
            // Same bounded navigation as the tray rows: focus first, then
            // acknowledge only on success.
            let nav_sessions = sessions.clone();
            focus::focus(
                nav.target(),
                move || monitor::acknowledge(&nav_sessions, nav.id(), nav.completed()),
                || {},
            );
        }
    }
}

fn close(display: *mut xlib::Display, popup: &mut Option<Popup>) {
    if let Some(current) = popup.take() {
        current.close(display);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unexpected_modifier_sets_are_rejected_and_lock_variations_accepted() {
        assert!(state_is_super_shift(
            (xlib::Mod4Mask | xlib::ShiftMask) as c_uint
        ));
        let locks = (xlib::LockMask | xlib::Mod2Mask) as c_uint;
        assert!(state_is_super_shift(
            ((xlib::Mod4Mask | xlib::ShiftMask) | locks) as c_uint
        ));
        assert!(!state_is_super_shift(xlib::Mod4Mask as c_uint));
        assert!(!state_is_super_shift(
            (xlib::Mod4Mask | xlib::ControlMask) as c_uint
        ));
        assert!(!state_is_super_shift(0));
    }

    #[test]
    fn superseded_error_codes_are_reported_exactly_once() {
        GRAB_ERROR.store(10, Ordering::SeqCst);
        assert_eq!(GRAB_ERROR.swap(0, Ordering::SeqCst), 10);
        assert_eq!(GRAB_ERROR.swap(0, Ordering::SeqCst), 0);
    }
}
