use std::env;
use std::path::PathBuf;
use std::process;

const USAGE: &str = "usage: capture-delegate-backend --socket <path> [--store <path>]";

fn main() {
    let (socket_path, store_path) = match parse_arguments() {
        Ok(paths) => paths,
        Err(message) => {
            eprintln!("{message}");
            process::exit(2);
        }
    };

    if let Err(error) = capture_delegate_backend::run(&socket_path, &store_path) {
        eprintln!("backend error: {error}");
        process::exit(1);
    }
}

fn parse_arguments() -> Result<(PathBuf, PathBuf), &'static str> {
    let mut socket_path = None;
    let mut store_path = None;
    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        let slot = match argument.as_str() {
            "--socket" => &mut socket_path,
            "--store" => &mut store_path,
            _ => return Err(USAGE),
        };
        match arguments.next() {
            Some(value) if slot.is_none() && !value.is_empty() => {
                *slot = Some(PathBuf::from(value))
            }
            _ => return Err(USAGE),
        }
    }

    let socket_path = socket_path.ok_or(USAGE)?;
    let store_path = match store_path {
        Some(store_path) => store_path,
        None => default_store_path()?,
    };
    Ok((socket_path, store_path))
}

fn default_store_path() -> Result<PathBuf, &'static str> {
    let home = env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .ok_or("HOME must be set to derive the default store path; pass --store <path>")?;
    Ok(PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("CaptureDelegate")
        .join("store.sqlite"))
}
