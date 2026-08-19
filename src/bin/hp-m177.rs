fn main() {
    let code = match hp_m177::cli::run_with_env(std::env::args_os()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("hp-m177: {e}");
            1
        }
    };
    std::process::exit(code);
}
