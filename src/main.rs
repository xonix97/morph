fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(error) = morph::runtime::run_cli(&args) {
        eprintln!("morph: error: {error}");
        std::process::exit(error.exit_code());
    }
}
