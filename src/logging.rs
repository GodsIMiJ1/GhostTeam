use env_logger::Env;

pub fn init_logging() {
    let env = Env::default().filter_or("GHOSTTEAM_LOG", "info");
    let _ = env_logger::Builder::from_env(env).try_init();
}
