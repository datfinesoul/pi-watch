mod focus;
mod monitor;
mod ui;

fn main() {
    match monitor::start() {
        Ok(sessions) => ui::run(sessions),
        Err(error) => {
            eprintln!("Pi Watch could not start: {error}");
            std::process::exit(1);
        }
    }
}
