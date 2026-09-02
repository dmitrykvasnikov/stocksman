fn main() {
    if std::env::args().any(|argument| argument == "--stocksman-backend") {
        if let Err(error) = stocksman_lib::run_backend_sidecar() {
            eprintln!("Stocksman backend failed: {error}");
            std::process::exit(1);
        }
        return;
    }

    stocksman_lib::run();
}
