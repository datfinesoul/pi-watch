//! A registered X11 shortcut, not a global keyboard monitor.
use std::{
    ffi::{c_int, c_uint},
    ptr,
    sync::{
        OnceLock,
        atomic::{AtomicU32, Ordering},
    },
    time::Instant,
};
use x11::xlib;

pub fn trace(message: std::fmt::Arguments<'_>) {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    static START: OnceLock<Instant> = OnceLock::new();
    if *ENABLED.get_or_init(|| std::env::var_os("PI_WATCH_DEBUG").is_some_and(|value| value == "1"))
    {
        let elapsed = START.get_or_init(Instant::now).elapsed().as_secs_f64();
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

pub fn register(callback: impl Fn() + Send + 'static) -> XResult {
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
    // SAFETY replaced below
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
    listen(display, c_uint::from(keycode), Box::new(callback));
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

fn listen(display: *mut xlib::Display, keycode: c_uint, callback: Box<dyn Fn() + Send>) {
    let fd = unsafe { xlib::XConnectionNumber(display) };
    // SAFETY: Only this thread touches the display from here on.
    let owned = XDisplay(display);
    std::thread::spawn(move || run(owned, fd, keycode, callback));
}

fn run(XDisplay(display): XDisplay, fd: c_int, keycode: c_uint, callback: Box<dyn Fn() + Send>) {
    let mut poll = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
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
                trace(format_args!("shortcut display connection ended"));
                return;
            }
            continue;
        }
        let mut event = xlib::XEvent { pad: [0; 24] };
        // SAFETY: XNextEvent writes the event before any read.
        unsafe { xlib::XNextEvent(display, &mut event) };
        // Union reads stay safe: the type field is the first member of every variant.
        if unsafe { event.type_ } != xlib::KeyPress {
            continue;
        }
        // The rest of the union members alias the key fields only for key events.
        let (pressed, state) = unsafe { (event.key.keycode, event.key.state) };
        if pressed != keycode {
            continue;
        }
        if !state_is_super_shift(state) {
            trace(format_args!(
                "shortcut ignored: unexpected state={state:#x}"
            ));
            continue;
        }
        trace(format_args!("shortcut delivered"));
        // Never unwind across the C event dispatcher.
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(&*callback)).is_err() {
            eprintln!("Pi Watch shortcut handler failed");
        }
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
