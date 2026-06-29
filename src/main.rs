mod agent;
mod cli;
mod db;
mod model;
mod logging;
mod roles;
mod tasks;

fn main() {
    logging::init_logging();

    if let Err(error) = cli::run() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}
