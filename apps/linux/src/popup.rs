//! The Super+Shift+' popup window for the Linux session list.
//!
//! Pure X11 core calls on an override-redirect window: no GTK, no extra
//! connection, and one core Latin-1 font. The tray keeps the π SVG; the popup
//! avoids text that core fonts cannot render.

use crate::focus::Target;
use crate::monitor::{self, Sessions, Status};
use crate::shortcut::trace;
use std::{
    ffi::{CString, c_char, c_int, c_long, c_uchar, c_uint, c_ulong},
    mem, ptr,
    time::Instant,
};
use x11::xlib;

// One core font and one window: cap visible text and truncate to that width.
const MAX_CHARS: usize = 80;
const DOT_GAP: c_int = 4;
// GNOME's top panel is 32 CSS px; its reported workarea top reveals the UI scale.
const PANEL_CSS_PX: f64 = 32.0;
// The π indicator is GL-composited by the shell, so its exact x is not
// discoverable from X; anchor the right edge this many CSS px from the edge,
// roughly inside the right status cluster.
const RIGHT_CLUSTER_CSS: i32 = 190;
const FONT_HEIGHTS: [i32; 8] = [32, 30, 28, 26, 24, 20, 18, 15];

/// Status dots are drawn as arcs because core Latin-1 fonts have no ●/○/–.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dot {
    Filled,
    Hollow,
    Dash,
}

/// Everything one session row needs to navigate and acknowledge.
#[derive(Clone, Debug)]
pub struct NavTarget {
    endpoint: String,
    pane: String,
    id: String,
    completed: Option<Instant>,
}

impl NavTarget {
    pub fn target(&self) -> Target {
        Target {
            endpoint: self.endpoint.clone(),
            pane: self.pane.clone(),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn completed(&self) -> Option<Instant> {
        self.completed
    }
}

/// A frozen content entry, mirroring the tray menu ordering.
#[derive(Clone, Debug)]
pub enum Entry {
    Heading(String),
    Gap,
    Session(Row),
    Quit,
}

#[derive(Clone, Debug)]
pub struct Row {
    pub dot: Dot,
    pub text: String,
    pub preview: Option<String>,
    pub nav: Option<NavTarget>,
    pub digit: Option<u8>,
}

/// One rendered line. Rows with previews contribute two lines, and gaps
/// become empty lines that keep the tray's separator spacing.
#[derive(Clone, Default)]
struct Line {
    text: String,
    dim: bool,
    dot: Option<Dot>,
    nav: Option<NavTarget>,
    quit: bool,
}

pub struct View {
    entries: Vec<Entry>,
    lines: Vec<Line>,
}

#[derive(Clone, Debug)]
pub enum Outcome {
    StayOpen,
    Dismiss,
    Navigate(NavTarget),
    Quit,
}

// Core Latin-1 font: drop control characters, map the few recurring glyphs to
// readable ASCII, and mark the rest '?' instead of risking missing glyphs.
pub fn sanitize(text: &str) -> String {
    // One pass, one meaning per glyph; only frequent UI glyphs get a mapping.
    let replace = |c: char| -> &str {
        match c {
            'π' => "pi",
            '–' | '—' => "-",
            '‘' | '’' | '“' | '”' => "'",
            '…' => "...",
            '↳' => ">",
            _ => "",
        }
    };
    text.chars()
        .filter(|c| !c.is_control())
        .map(|c| match c {
            c if c <= 'ÿ' => c.to_string(),
            c if !replace(c).is_empty() => replace(c).to_string(),
            _ => "?".to_string(),
        })
        .collect()
}

// Visible text is truncated to the window's column budget.
pub fn truncate_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

fn fit(text: &str) -> String {
    truncate_chars(&sanitize(text), MAX_CHARS)
}

fn base_name(path: &str) -> &str {
    std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Project")
}

// Same short form as the tray, minus the leading dot, which is drawn.
fn format_row_text(name: &str, elapsed: &std::time::Duration, title: &str) -> String {
    format!(
        "{name} · {}m {:02}s · {title}",
        elapsed.as_secs() / 60,
        elapsed.as_secs() % 60
    )
}

fn completion_digit(index: usize) -> Option<u8> {
    u8::try_from(index)
        .ok()
        .filter(|digit| (1..=9).contains(digit))
}

/// A tray-mirroring snapshot of the current sessions. The rows freeze here;
/// close and reopen to see changes.
pub fn view(sessions: &Sessions) -> View {
    let Ok(state) = sessions.lock() else {
        // Matches the tray's degraded message: keep it readable, no content.
        return layout(vec![Entry::Heading("Session state unavailable".into())]);
    };
    let now = Instant::now();
    let mut entries = Vec::new();
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
        entries.push(Entry::Heading("No connected Pi sessions".into()));
        entries.push(Entry::Heading(
            "Load extensions/watch/index.ts in Pi".into(),
        ));
    } else {
        let mut in_done = rows.iter().any(|s| s.waiting);
        if in_done {
            entries.push(Entry::Heading("Done — awaiting inspection".into()));
        }
        let mut project = String::new();
        let mut numbered = 0usize;
        for session in rows {
            if in_done && !session.waiting {
                in_done = false;
                entries.push(Entry::Gap);
                project.clear();
            }
            if !session.waiting && session.update.cwd != project {
                if !project.is_empty() {
                    entries.push(Entry::Gap);
                }
                project = session.update.cwd.clone();
                let label = session
                    .update
                    .project_label
                    .as_deref()
                    .unwrap_or_else(|| base_name(&project));
                entries.push(Entry::Heading(label.into()));
            }
            let row = session_row(session, now, &mut numbered);
            entries.push(Entry::Session(row));
        }
    }
    entries.push(Entry::Gap);
    entries.push(Entry::Quit);
    layout(entries)
}

fn session_row(session: &monitor::Session, now: Instant, numbered: &mut usize) -> Row {
    let status = session.status(now);
    let (dot, name) = match status {
        Status::Working => (Dot::Filled, "Active"),
        Status::Idle => (Dot::Hollow, "Idle"),
        Status::Disconnected => (Dot::Dash, "Disconnected"),
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
    // Waiting rows carry the project marker on Linux, as macOS rows do.
    let mut text = match (session.waiting, waiting_marker(session)) {
        (true, marker) => format!("{marker} · "),
        (false, _) => String::new(),
    };
    text.push_str(&format_row_text(name, &elapsed, &title));
    let digit = session
        .waiting
        .then(|| {
            *numbered += 1;
            completion_digit(*numbered)
        })
        .flatten();
    let nav = nav_target(session, status);
    if let Some(digit) = digit {
        text = format!("{digit}. {text}");
    }
    Row {
        dot,
        text: fit(&text),
        preview: preview_line(&session.update.preview),
        nav,
        digit,
    }
}

fn waiting_marker(session: &monitor::Session) -> String {
    session
        .update
        .project_label
        .clone()
        .unwrap_or_else(|| "Project".into())
}

fn nav_target(session: &monitor::Session, status: Status) -> Option<NavTarget> {
    // Only reachable, non-disconnected rows navigate, as on macOS.
    let (Some(endpoint), Some(pane)) = (
        &session.update.kitty_listen_on,
        &session.update.kitty_window_id,
    ) else {
        return None;
    };
    if status == Status::Disconnected {
        return None;
    }
    Some(NavTarget {
        endpoint: endpoint.clone(),
        pane: pane.clone(),
        id: session.update.id.clone(),
        completed: session.completed,
    })
}

fn preview_line(preview: &Option<monitor::Preview>) -> Option<String> {
    let preview = preview.as_ref()?;
    // A plain ASCII separator: the core font has no '…' glyph.
    Some(fit(&match preview.last.is_empty() {
        true => preview.first.clone(),
        false => format!("{} ... {}", preview.first, preview.last),
    }))
}

fn layout(entries: Vec<Entry>) -> View {
    let mut lines = Vec::new();
    for entry in &entries {
        match entry {
            Entry::Heading(text) => lines.push(Line {
                text: fit(text),
                dim: true,
                ..Line::default()
            }),
            Entry::Gap => lines.push(Line::default()),
            Entry::Quit => lines.push(Line {
                text: "Quit Pi Watch".into(),
                quit: true,
                ..Line::default()
            }),
            Entry::Session(row) => {
                lines.push(Line {
                    text: row.text.clone(),
                    dim: false,
                    dot: Some(row.dot),
                    nav: row.nav.clone(),
                    quit: false,
                });
                // The tray shows submenu previews; the popup shows an inline
                // dim second line under the row (no submenu yet).
                if let Some(preview) = &row.preview {
                    lines.push(Line {
                        text: preview.clone(),
                        dim: true,
                        nav: row.nav.clone(),
                        ..Line::default()
                    });
                }
            }
        }
    }
    View { entries, lines }
}

/// Digits 1–9 activate waiting completions after the modifiers release.
pub fn digit_outcome(view: &View, digit: u8) -> Outcome {
    for entry in &view.entries {
        if let Entry::Session(row) = entry
            && row.digit == Some(digit)
            && let Some(nav) = &row.nav
        {
            return Outcome::Navigate(nav.clone());
        }
    }
    Outcome::StayOpen
}

/// Clicks use window-relative coordinates: outside the window dismisses,
/// navigable rows navigate, and everything else keeps the popup open.
pub fn hit(
    view: &View,
    x: c_int,
    y: c_int,
    pad: c_int,
    line_h: c_int,
    width: c_int,
    height: c_int,
) -> Outcome {
    if x < 0 || y < 0 || x >= width || y >= height {
        return Outcome::Dismiss;
    }
    let index = (y - pad) / line_h;
    let Some(line) = view.lines.get(index.max(0) as usize) else {
        return Outcome::StayOpen;
    };
    if let Some(nav) = &line.nav {
        return Outcome::Navigate(nav.clone());
    }
    if line.quit {
        return Outcome::Quit;
    }
    Outcome::StayOpen
}

struct Colors {
    background: c_ulong,
    text: c_ulong,
    dim: c_ulong,
}

impl Colors {
    // Flat dark single-font styling is the documented v1 appearance.
    unsafe fn alloc(display: *mut xlib::Display) -> Colors {
        // SAFETY: the caller owns the display for this call.
        unsafe {
            let screen = xlib::XDefaultScreen(display);
            let colormap = xlib::XDefaultColormap(display, screen);
            Colors {
                background: Self::named(
                    display,
                    colormap,
                    "gray15",
                    xlib::XBlackPixel(display, screen),
                ),
                text: Self::named(
                    display,
                    colormap,
                    "gray90",
                    xlib::XWhitePixel(display, screen),
                ),
                dim: Self::named(
                    display,
                    colormap,
                    "gray60",
                    xlib::XWhitePixel(display, screen),
                ),
            }
        }
    }

    unsafe fn named(
        display: *mut xlib::Display,
        colormap: c_ulong,
        name: &str,
        fallback: c_ulong,
    ) -> c_ulong {
        // SAFETY: the caller owns the display and the colormap is its default.
        unsafe {
            let mut exact: xlib::XColor = mem::zeroed();
            let mut closest: xlib::XColor = mem::zeroed();
            // Static names never contain interior NUL bytes.
            let name = CString::new(name).unwrap();
            if xlib::XAllocNamedColor(display, colormap, name.as_ptr(), &mut exact, &mut closest)
                != 0
            {
                closest.pixel
            } else {
                fallback
            }
        }
    }
}

pub struct Popup {
    window: c_ulong,
    font: *mut xlib::XFontStruct,
    gc: xlib::GC,
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
    pad: c_int,
    line_h: c_int,
    previous_focus: c_ulong,
    colors: Colors,
    view: View,
}

impl Popup {
    pub fn lines(&self) -> usize {
        self.view.lines.len()
    }

    /// Translates root coordinates: the owner-off pointer grab reports
    /// events with respect to the root window, not the popup's origin.
    pub fn press_at(&self, x_root: c_int, y_root: c_int) -> Outcome {
        hit(
            &self.view,
            x_root - self.x,
            y_root - self.y,
            self.pad,
            self.line_h,
            self.width,
            self.height,
        )
    }

    pub fn key(&self, key: &xlib::XKeyEvent) -> Outcome {
        let mut event = *key;
        let keysym = unsafe { xlib::XLookupKeysym(&mut event, 0) };
        const ESCAPE: c_ulong = 0xFF1B;
        const KEYPAD_ONE: c_ulong = 0xFFB1;
        match keysym {
            ESCAPE => Outcome::Dismiss,
            // Plain digits and keypad digits share the digit_outcome path.
            0x31..=0x39 => digit_outcome(&self.view, (keysym - 0x30) as u8),
            KEYPAD_ONE..=0xFFB9 => digit_outcome(&self.view, (keysym - KEYPAD_ONE + 1) as u8),
            _ => Outcome::StayOpen,
        }
    }

    /// Redraws the frozen view. Expose events also arrive after this call.
    pub fn draw(&self, display: *mut xlib::Display) {
        unsafe {
            xlib::XSetForeground(display, self.gc, self.colors.background);
            xlib::XFillRectangle(
                display,
                self.window,
                self.gc,
                0,
                0,
                self.width as c_uint,
                self.height as c_uint,
            );
            let font = &*self.font;
            let ascent = font.ascent;
            let radius = (self.line_h / 5).max(2);
            let mut top = self.pad;
            for line in &self.view.lines {
                if line.text.is_empty() && line.dot.is_none() {
                    top += self.line_h;
                    continue;
                }
                let offset = match line.dot.is_some() {
                    true => self.line_h + DOT_GAP,
                    false => 0,
                };
                let cx = self.pad + self.line_h / 2;
                let cy = top + self.line_h / 2;
                if let Some(dot) = line.dot {
                    xlib::XSetForeground(display, self.gc, self.colors.text);
                    match dot {
                        Dot::Filled => {
                            xlib::XFillArc(
                                display,
                                self.window,
                                self.gc,
                                cx - radius,
                                cy - radius,
                                2 * radius as c_uint,
                                2 * radius as c_uint,
                                0,
                                360 * 64,
                            );
                        }
                        Dot::Hollow => {
                            xlib::XDrawArc(
                                display,
                                self.window,
                                self.gc,
                                cx - radius,
                                cy - radius,
                                2 * radius as c_uint,
                                2 * radius as c_uint,
                                0,
                                360 * 64,
                            );
                        }
                        Dot::Dash => {
                            xlib::XDrawLine(
                                display,
                                self.window,
                                self.gc,
                                cx - radius,
                                cy,
                                cx + radius,
                                cy,
                            );
                        }
                    }
                }
                xlib::XSetForeground(
                    display,
                    self.gc,
                    match line.dim {
                        true => self.colors.dim,
                        false => self.colors.text,
                    },
                );
                // XDrawString reads Latin-1 bytes; post-sanitize characters are
                // all Latin-1 or already '?', so the UTF-8 String must fold to
                // one byte per character.
                let bytes: Vec<u8> = line
                    .text
                    .chars()
                    .map(|c| match c as u32 {
                        code if code <= 0xFF => code as u8,
                        _ => b'?',
                    })
                    .collect();
                xlib::XDrawString(
                    display,
                    self.window,
                    self.gc,
                    self.pad + offset,
                    top + ascent,
                    bytes.as_ptr().cast::<c_char>(),
                    bytes.len() as c_int,
                );
                top += self.line_h;
            }
        }
    }

    /// Consumes the popup after its display connection ended: the X resources
    /// die with the connection, and ungrab or free calls would raise X errors.
    pub fn abandon(self) {}

    /// Consumes the popup: ungrabs input, restores focus, frees X resources.
    pub fn close(self, display: *mut xlib::Display) {
        extern "C" fn ignore_errors(_: *mut xlib::Display, _: *mut xlib::XErrorEvent) -> c_int {
            0
        }
        unsafe {
            // Old saved windows can be gone; X errors must not abort here.
            let previous = xlib::XSetErrorHandler(Some(ignore_errors));
            xlib::XUngrabPointer(display, xlib::CurrentTime);
            xlib::XUngrabKeyboard(display, xlib::CurrentTime);
            // A grab rarely moves focus, but window managers can on click.
            if self.previous_focus != 0 && self.previous_focus != xlib::PointerRoot as c_ulong {
                xlib::XSetInputFocus(
                    display,
                    self.previous_focus,
                    xlib::RevertToPointerRoot,
                    xlib::CurrentTime,
                );
            }
            xlib::XDestroyWindow(display, self.window);
            xlib::XFreeGC(display, self.gc);
            xlib::XFreeFont(display, self.font);
            xlib::XSync(display, xlib::False);
            xlib::XSetErrorHandler(previous);
        }
    }
}

/// Attempts to open the popup. Both grabs must succeed; otherwise the
/// desktop keeps the pre-popup behavior.
pub fn open(display: *mut xlib::Display, sessions: &Sessions) -> Result<Popup, c_int> {
    let scale = unsafe { display_scale(display) };
    let target = (15.0 * scale).round() as c_int;
    let Some(font) = load_font(display, target) else {
        trace(format_args!("popup skipped: no core fixed font found"));
        return Err(-3);
    };
    // Content freezes at open: rows and digits never change while visible.
    let view = view(sessions);
    let char_w = unsafe { (*font).max_bounds.width }.max(1) as c_int;
    let line_h = unsafe { (*font).ascent + (*font).descent };
    if line_h <= 0 {
        unsafe { xlib::XFreeFont(display, font) };
        trace(format_args!("popup skipped: font metrics are unusable"));
        return Err(-3);
    }
    let pad = (8.0 * scale).round() as c_int;
    let (width, height) = size(&view, char_w, line_h, pad);
    let (x, y) = unsafe { place(display, width, height, scale) };
    let colors = unsafe { Colors::alloc(display) };
    let root = unsafe { xlib::XDefaultRootWindow(display) };
    let mut attrs: xlib::XSetWindowAttributes = unsafe { mem::zeroed() };
    attrs.background_pixel = colors.background;
    attrs.event_mask = xlib::ExposureMask
        | xlib::KeyPressMask
        | xlib::KeyReleaseMask
        | xlib::ButtonPressMask
        | xlib::ButtonReleaseMask;
    attrs.override_redirect = xlib::True;
    let window = unsafe {
        xlib::XCreateWindow(
            display,
            root,
            x,
            y,
            width as c_uint,
            height as c_uint,
            0,
            xlib::CopyFromParent,
            xlib::InputOutput as c_uint,
            ptr::null_mut(),
            xlib::CWBackPixel | xlib::CWEventMask | xlib::CWOverrideRedirect,
            &mut attrs,
        )
    };
    let gc = unsafe { xlib::XCreateGC(display, window, 0, ptr::null_mut()) };
    let mut previous_focus: c_ulong = 0;
    let mut focus_revert: c_int = 0;
    unsafe {
        xlib::XGetInputFocus(display, &mut previous_focus, &mut focus_revert);
    }
    // An active grab requires a viewable grab window, so the window maps
    // and the server confirms it before both grabs happen.
    unsafe {
        xlib::XMapWindow(display, window);
        xlib::XRaiseWindow(display, window);
        xlib::XSync(display, xlib::False);
    }
    // An active keyboard grab receives digits and Escape from any window,
    // replacing the passive shortcut grab until the popup closes.
    let status = unsafe {
        xlib::XGrabKeyboard(
            display,
            window,
            xlib::True,
            xlib::GrabModeAsync,
            xlib::GrabModeAsync,
            xlib::CurrentTime,
        )
    };
    if status != xlib::GrabSuccess {
        unsafe {
            xlib::XDestroyWindow(display, window);
            xlib::XFreeGC(display, gc);
            xlib::XFreeFont(display, font);
        }
        trace(format_args!("popup skipped: keyboard grab status={status}"));
        return Err(status);
    }
    let status = unsafe {
        xlib::XGrabPointer(
            display,
            window,
            xlib::False,
            (xlib::ButtonPressMask | xlib::ButtonReleaseMask) as c_uint,
            xlib::GrabModeAsync,
            xlib::GrabModeAsync,
            0,
            0,
            xlib::CurrentTime,
        )
    };
    if status != xlib::GrabSuccess {
        unsafe {
            xlib::XUngrabKeyboard(display, xlib::CurrentTime);
            xlib::XDestroyWindow(display, window);
            xlib::XFreeGC(display, gc);
            xlib::XFreeFont(display, font);
        }
        trace(format_args!("popup skipped: pointer grab status={status}"));
        return Err(status);
    }
    let popup = Popup {
        window,
        font,
        gc,
        x,
        y,
        width,
        height,
        pad,
        line_h,
        previous_focus,
        colors,
        view,
    };
    popup.draw(display);
    unsafe { xlib::XSync(display, xlib::False) };
    trace(format_args!(
        "popup delivered lines={} window={window:#x} at {x},{y} {width}x{height} scale={scale}",
        popup.lines()
    ));
    Ok(popup)
}

// Picks the fixed font closest to the display's UI scale. Candidates run big
// to small, so the first ascent+descent that fits the target wins.
fn load_font(display: *mut xlib::Display, target: c_int) -> Option<*mut xlib::XFontStruct> {
    for height in FONT_HEIGHTS {
        let pattern = format!("-misc-fixed-medium-r-normal--{height}-*-*-*-*-*-iso8859-1");
        let bytes = CString::new(pattern).unwrap();
        let font = unsafe { xlib::XLoadQueryFont(display, bytes.as_ptr()) };
        if font.is_null() {
            continue;
        }
        let line = unsafe { (*font).ascent + (*font).descent };
        if line <= target + 2 {
            trace(format_args!(
                "popup font {height}px target={target} line={line}"
            ));
            return Some(font);
        }
        // SAFETY: this candidate was loaded but did not fit the target.
        unsafe { xlib::XFreeFont(display, font) };
    }
    // "9x15" and "fixed" are the standard xfonts-base fallback aliases.
    for name in ["9x15", "fixed"] {
        let bytes = CString::new(name).unwrap();
        let font = unsafe { xlib::XLoadQueryFont(display, bytes.as_ptr()) };
        if !font.is_null() {
            return Some(font);
        }
    }
    None
}

fn size(view: &View, char_w: c_int, line_h: c_int, pad: c_int) -> (c_int, c_int) {
    let mut widest = 0;
    for line in &view.lines {
        let x = match line.dot.is_some() {
            true => pad + line_h + DOT_GAP,
            false => pad,
        };
        widest = widest.max(x + line.text.chars().count() as c_int * char_w);
    }
    (
        widest.max(pad * 2 + line_h) + pad,
        pad * 2 + view.lines.len() as c_int * line_h,
    )
}

// Reads mutter's reported workarea top: GNOME's top panel bottom edge in
// server pixels. The panel itself is GL-composited and invisible to X.
unsafe fn panel_top(display: *mut xlib::Display) -> Option<c_int> {
    // SAFETY: the caller owns the display and the root window is its default.
    unsafe {
        let atom = xlib::XInternAtom(display, c"_NET_WORKAREA".as_ptr(), xlib::False);
        if atom == 0 {
            return None;
        }
        let root = xlib::XDefaultRootWindow(display);
        let mut actual_type: c_ulong = 0;
        let mut format: c_int = 0;
        let (mut nitems, mut after): (c_ulong, c_ulong) = (0, 0);
        let mut prop: *mut c_uchar = ptr::null_mut();
        let status = xlib::XGetWindowProperty(
            display,
            root,
            atom,
            0,
            32,
            xlib::False,
            xlib::XA_CARDINAL,
            &mut actual_type,
            &mut format,
            &mut nitems,
            &mut after,
            &mut prop,
        );
        if status != 0 || prop.is_null() || format != 32 || nitems < 4 {
            if !prop.is_null() {
                xlib::XFree(prop.cast::<c_uchar>().cast());
            }
            return None;
        }
        // Format 32 returns longs: [x, y, width, height] per viewport.
        let values = prop.cast::<c_long>();
        let y = *values.add(1);
        xlib::XFree(prop.cast());
        c_int::try_from(y).ok()
    }
}

// The desktop's UI scale: prefer mutter's panel offset (panel CSS height is
// a known constant), else the screen height over one-1080p-equivalent.
unsafe fn display_scale(display: *mut xlib::Display) -> f64 {
    // SAFETY: the caller owns the display.
    unsafe {
        let scale = match panel_top(display) {
            Some(top) => top as f64 / PANEL_CSS_PX,
            None => {
                let screen = xlib::XDefaultScreen(display);
                xlib::XDisplayHeight(display, screen) as f64 / 1080.0
            }
        };
        // GNOME's text stays legible at the integer scale the compositor picked.
        scale.round().clamp(1.0, 4.0)
    }
}

// Anchors the popup under the top panel's right status cluster, which is
// where the shell renders the π tray icon. The shell's GL surface hides any
// exact icon coordinates from X, so the offset stays a calibrated estimate.
unsafe fn place(
    display: *mut xlib::Display,
    width: c_int,
    height: c_int,
    scale: f64,
) -> (c_int, c_int) {
    // SAFETY: the caller owns the display.
    unsafe {
        let screen = xlib::XDefaultScreen(display);
        let screen_w = xlib::XDisplayWidth(display, screen);
        let screen_h = xlib::XDisplayHeight(display, screen);
        let top = panel_top(display).unwrap_or(0);
        let gap = (4.0 * scale).round() as c_int;
        let margin = ((RIGHT_CLUSTER_CSS as f64) * scale).round() as c_int;
        let x = (screen_w - width - margin).max(0);
        let y = (top + gap).min((screen_h - height).max(0));
        (x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update(status: Status) -> monitor::Update {
        monitor::Update {
            version: 1,
            id: "session".into(),
            cwd: "/tmp/project".into(),
            project_label: None,
            kitty_title: Some("term".into()),
            pid: 1,
            kitty_window_id: Some("100".into()),
            kitty_listen_on: Some("unix:/tmp/kitty.sock".into()),
            kitty_focused: None,
            preview: None,
            status,
        }
    }

    fn insert(sessions: &Sessions, id: &str, mut session: monitor::Session) {
        session.update.id = id.into();
        session.update.cwd = format!("/tmp/{id}");
        session.update.pid = (name_index(id) as u32) * 100;
        sessions.lock().unwrap().insert(id.into(), session);
    }

    fn name_index(id: &str) -> usize {
        match id {
            "a" => 1,
            "b" => 2,
            "c" => 3,
            _ => 4,
        }
    }

    fn working() -> monitor::Session {
        monitor::Session {
            update: update(Status::Working),
            seen: Instant::now(),
            changed: Instant::now(),
            completed: None,
            waiting: false,
        }
    }

    fn waiting() -> monitor::Session {
        monitor::Session {
            update: update(Status::Idle),
            seen: Instant::now(),
            changed: Instant::now(),
            completed: Some(Instant::now()),
            waiting: true,
        }
    }

    fn idle() -> monitor::Session {
        monitor::Session {
            update: update(Status::Idle),
            ..working()
        }
    }

    #[test]
    fn view_shows_done_section_then_project_groups() {
        let sessions = Sessions::default();
        insert(&sessions, "a", idle());
        insert(&sessions, "b", working());
        insert(&sessions, "c", waiting());
        let view = view(&sessions);
        let headings: Vec<&str> = view
            .entries
            .iter()
            .filter_map(|entry| match entry {
                Entry::Heading(text) => Some(text.as_str()),
                _ => None,
            })
            .collect();
        // Waiting rows carry the inline project marker instead of a heading.
        assert_eq!(headings, vec!["Done — awaiting inspection", "a", "b"]);
    }

    #[test]
    fn waiting_rows_get_digits_and_project_markers() {
        let sessions = Sessions::default();
        insert(&sessions, "a", waiting());
        let view = view(&sessions);
        let session = view
            .entries
            .iter()
            .find_map(|entry| match entry {
                Entry::Session(row) => Some(row),
                _ => None,
            })
            .unwrap();
        assert_eq!(session.digit, Some(1));
        assert!(
            session
                .text
                .starts_with("1. Project · Idle · 0m 00s · term")
        );
        assert!(session.nav.is_some());
    }

    #[test]
    fn digits_stop_after_nine_waiting_rows() {
        let sessions = Sessions::default();
        let count = 12;
        for index in 0..count {
            let id = format!("session{index}");
            let mut session = waiting();
            session.update.cwd = "/tmp/shared".into();
            session.update.pid = (index as u32) + 1;
            sessions.lock().unwrap().insert(id.clone(), session);
        }
        let view = view(&sessions);
        let digits: Vec<Option<u8>> = view
            .entries
            .iter()
            .filter_map(|entry| match entry {
                Entry::Session(row) => Some(row.digit),
                _ => None,
            })
            .collect();
        assert_eq!(digits.len(), count);
        assert_eq!(
            digits,
            (1..=9u8)
                .map(Some)
                .chain(std::iter::repeat_n(None, count - 9))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn digit_outcome_navigates_only_waiting_rows_with_a_target() {
        let sessions = Sessions::default();
        insert(&sessions, "a", waiting());
        insert(&sessions, "b", idle_without_target());
        let view = view(&sessions);
        assert!(matches!(digit_outcome(&view, 1), Outcome::Navigate(_)));
        assert!(matches!(digit_outcome(&view, 2), Outcome::StayOpen));
        assert!(matches!(digit_outcome(&view, 9), Outcome::StayOpen));
    }

    fn idle_without_target() -> monitor::Session {
        let mut session = waiting();
        session.update.kitty_listen_on = None;
        session
    }

    #[test]
    fn empty_state_shows_the_pi_hints() {
        let sessions = Sessions::default();
        let view = view(&sessions);
        let texts: Vec<String> = view.lines.iter().map(|line| line.text.clone()).collect();
        assert_eq!(
            texts[..2],
            [
                "No connected Pi sessions".to_string(),
                "Load extensions/watch/index.ts in Pi".to_string()
            ]
        );
        assert!(view.lines.last().is_some_and(|line| line.quit));
    }

    // A preview line shares the row's target, so clicking either navigates.
    #[test]
    fn previews_render_as_one_dim_second_line() {
        let sessions = Sessions::default();
        let mut session = working();
        session.update.preview = Some(monitor::Preview {
            first: "start".into(),
            last: "end".into(),
        });
        insert(&sessions, "a", session);
        let view = view(&sessions);
        let session_lines: Vec<&Line> = view
            .lines
            .iter()
            .filter(|line| line.nav.is_some())
            .collect();
        assert_eq!(session_lines.len(), 2);
        assert!(!session_lines[0].dim);
        assert_eq!(session_lines[1].text, "start ... end");
        assert!(session_lines[1].dim);
    }

    #[test]
    fn sanitize_maps_gems_of_the_ui_and_marks_unknowns() {
        // The recurring glyphs map to ASCII; everything else marks '?' or drops?
        assert_eq!(
            sanitize("— π ↳ … “q” 😊").chars().collect::<String>(),
            "- pi > ... 'q' ?"
        );
        assert_eq!(sanitize("a\u{7}b😀").chars().collect::<String>(), "ab?");
        assert_eq!(sanitize("·").chars().collect::<String>(), "·");
    }

    #[test]
    fn truncate_chars_limits_text_to_the_window_budget() {
        assert_eq!(truncate_chars("abcdef", 4), "abcd");
        assert_eq!(truncate_chars("abcdef", 10), "abcdef");
        let long = "😀".repeat(MAX_CHARS * 2);
        assert_eq!(
            truncate_chars(long.as_str(), MAX_CHARS).chars().count(),
            MAX_CHARS
        );
    }

    #[test]
    fn hit_dismisses_outside_and_navigates_rows() {
        let lines: Vec<Line> = vec![
            Line {
                nav: NavTarget {
                    endpoint: "unix:/tmp/kitty.sock".into(),
                    pane: "100".into(),
                    id: "session".into(),
                    completed: None,
                }
                .into(),
                ..Line::default()
            },
            Line::default(),
            Line {
                quit: true,
                ..Line::default()
            },
        ];
        let view = View {
            entries: Vec::new(),
            lines,
        };
        let line_h = 10;
        let pad = 10;
        let (width, height) = size(&view, 10, line_h, pad);
        // Clicks inside the text column: first row navigates, gap stays.
        assert!(matches!(
            hit(&view, pad + 5, pad + 5, pad, line_h, width, height),
            Outcome::Navigate(_)
        ));
        assert!(matches!(
            hit(&view, pad + 5, pad + 15, pad, line_h, width, height),
            Outcome::StayOpen
        ));
        assert!(matches!(
            hit(&view, pad + 5, pad + 25, pad, line_h, width, height),
            Outcome::Quit
        ));
        assert!(matches!(
            hit(&view, width + 5, 5, pad, line_h, width, height),
            Outcome::Dismiss
        ));
        assert!(matches!(
            hit(&view, 5, height - 1, pad, line_h, width, height),
            Outcome::StayOpen
        ));
    }

    #[test]
    fn size_scales_with_lines_and_text() {
        let view = view(&Sessions::default());
        let (_, height) = size(&view, 10, 15, 10);
        assert_eq!(height, 20 + view.lines.len() as c_int * 15);
    }
}
