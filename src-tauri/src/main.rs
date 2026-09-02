fn main() {
    let mut arguments = std::env::args_os().skip(1);
    if arguments.next().as_deref() == Some(std::ffi::OsStr::new("--stocksman-backend")) {
        let Some(database_path) = arguments.next() else {
            eprintln!("Stocksman backend requires a database path");
            std::process::exit(2);
        };

        if let Err(error) = stocksman_lib::run_backend_sidecar(database_path.into()) {
            eprintln!("Stocksman backend failed: {error}");
            std::process::exit(1);
        }
        return;
    }

    stocksman_lib::run();
}
