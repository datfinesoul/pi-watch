mod focus;
mod monitor;
mod shortcut;
mod tray;

fn main() {
    match monitor::start() {
        Ok((sessions, data_dir)) => {
            let sessions_for_shortcut = sessions.clone();
            if let Err(error) = shortcut::register(move || {
                focus::focus_most_recent(&sessions_for_shortcut);
            }) {
                eprintln!(
                    "Pi Watch could not register Super+Shift+' (X11 error {error}); use the tray menu instead"
                );
            }
            tray::run(sessions, data_dir);
        }
        Err(error) => {
            eprintln!("Pi Watch could not start: {error}");
            std::process::exit(1);
        }
    }
}
