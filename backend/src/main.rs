use std::env;
use std::path::PathBuf;
use std::process;

fn main() {
    let socket_path = match parse_socket_path() {
        Ok(socket_path) => socket_path,
        Err(message) => {
            eprintln!("{message}");
            process::exit(2);
        }
    };

    if let Err(error) = capture_delegate_backend::run(&socket_path) {
        eprintln!("backend error: {error}");
        process::exit(1);
    }
}

fn parse_socket_path() -> Result<PathBuf, &'static str> {
    let mut arguments = env::args().skip(1);
    match (arguments.next().as_deref(), arguments.next()) {
        (Some("--socket"), Some(path)) if arguments.next().is_none() => Ok(PathBuf::from(path)),
        _ => Err("usage: capture-delegate-backend --socket <path>"),
    }
}
