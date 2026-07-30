use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

use tauri::{AppHandle, Manager};

static LOG_FILE: OnceLock<Mutex<File>> = OnceLock::new();
static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn init(app: &AppHandle) -> Result<(), String> {
    let log_dir = app
        .path()
        .app_log_dir()
        .map_err(|error| format!("Unable to resolve app log directory: {error}"))?;

    fs::create_dir_all(&log_dir)
        .map_err(|error| format!("Unable to create app log directory: {error}"))?;

    let log_path = log_dir.join("dcs-mission-composer.log");
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|error| format!("Unable to open app log file: {error}"))?;

    let _ = LOG_PATH.set(log_path);
    let _ = LOG_FILE.set(Mutex::new(file));

    info("Application started");
    Ok(())
}

pub fn log_path() -> Option<PathBuf> {
    LOG_PATH.get().cloned()
}

pub fn info(message: impl AsRef<str>) {
    write("INFO", message.as_ref());
}

pub fn error(message: impl AsRef<str>) {
    write("ERROR", message.as_ref());
}

fn write(level: &str, message: &str) {
    let Some(file) = LOG_FILE.get() else {
        eprintln!("[{level}] {message}");
        return;
    };

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "unknown-time".to_string());

    if let Ok(mut file) = file.lock() {
        let _ = writeln!(file, "{timestamp} [{level}] {message}");
    }
}
