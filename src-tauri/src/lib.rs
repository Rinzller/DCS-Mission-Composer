mod app_log;
mod archive;
mod differ;
mod exporter;
mod merger;

use std::path::{Path, PathBuf};

const GITHUB_REPO_URL: &str = "https://github.com/Rinzller/DCS-Mission-Composer";
const GITHUB_RELEASES_URL: &str = "https://github.com/Rinzller/DCS-Mission-Composer/releases";

#[derive(serde::Serialize)]
struct AppInfo {
    version: &'static str,
    github_repo_url: &'static str,
    github_releases_url: &'static str,
}

fn ensure_distinct_paths(paths: &[(&str, &PathBuf)]) -> Result<(), String> {
    for (left_index, (left_label, left_path)) in paths.iter().enumerate() {
        for (right_label, right_path) in paths.iter().skip(left_index + 1) {
            if paths_match(left_path, right_path) {
                if *left_label == "output" || *right_label == "output" {
                    return Err(
                        "Choose a different output file. The saved mission cannot overwrite a loaded mission."
                            .to_string(),
                    );
                }

                return Err(
                    "The original and modified missions must be different files.".to_string(),
                );
            }
        }
    }

    Ok(())
}

fn paths_match(left: &Path, right: &Path) -> bool {
    let left = left.to_string_lossy();
    let right = right.to_string_lossy();

    if cfg!(windows) {
        left.eq_ignore_ascii_case(&right)
    } else {
        left == right
    }
}

fn file_label(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown file")
        .to_string()
}

fn log_result<T>(operation: &str, result: Result<T, String>) -> Result<T, String> {
    match &result {
        Ok(_) => app_log::info(format!("{operation} succeeded")),
        Err(error) => app_log::error(format!("{operation} failed: {error}")),
    }

    result
}

async fn run_blocking<T, F>(operation: String, task: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    let result = tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|error| format!("{operation} was interrupted: {error}"))?;

    log_result(&operation, result)
}

#[tauri::command]
async fn validate_miz(path: String) -> Result<String, String> {
    let path = PathBuf::from(path);
    let operation = format!("Validate mission '{}'", file_label(&path));
    app_log::info(format!("{operation} requested"));

    run_blocking(operation, move || {
        archive::verify_miz_archive(&path).map(|_| "Valid DCS .miz file".to_string())
    })
    .await
}

#[tauri::command]
async fn export_planning_miz(
    source_path: String,
    output_path: String,
    coalition: String,
    flight_id: Option<String>,
) -> Result<String, String> {
    let source_path = PathBuf::from(source_path);
    let output_path = PathBuf::from(&output_path);
    let operation = format!(
        "Export {} planning mission from '{}' to '{}'",
        flight_id
            .as_ref()
            .map(|flight_id| format!("flight '{flight_id}'"))
            .unwrap_or_else(|| coalition.to_ascii_uppercase()),
        file_label(&source_path),
        file_label(&output_path)
    );
    app_log::info(format!("{operation} requested"));

    run_blocking(operation, move || {
        ensure_distinct_paths(&[("original", &source_path), ("output", &output_path)])
            .and_then(|_| {
                if let Some(flight_id) = flight_id {
                    exporter::export_flight_planning_mission(&source_path, &output_path, &flight_id)
                } else {
                    exporter::export_planning_mission(&source_path, &output_path, &coalition)
                }
            })
            .map(|_| output_path.to_string_lossy().into_owned())
    })
    .await
}

#[tauri::command]
async fn detect_flights(path: String) -> Result<Vec<exporter::FlightInfo>, String> {
    let path = PathBuf::from(path);
    let operation = format!("Detect flights in '{}'", file_label(&path));
    app_log::info(format!("{operation} requested"));

    let operation_for_error = operation.clone();
    let result = tauri::async_runtime::spawn_blocking(move || exporter::detect_flights(&path))
        .await
        .map_err(|error| format!("{operation_for_error} was interrupted: {error}"))?;

    match &result {
        Ok(flights) => app_log::info(format!("{operation} succeeded: flights={}", flights.len())),
        Err(error) => app_log::error(format!("{operation} failed: {error}")),
    }

    result
}

#[tauri::command]
async fn compare_modified_miz(
    original_path: String,
    modified_path: String,
) -> Result<differ::MissionDiff, String> {
    let original_path = PathBuf::from(original_path);
    let modified_path = PathBuf::from(modified_path);
    let operation = format!(
        "Compare original '{}' with modified '{}'",
        file_label(&original_path),
        file_label(&modified_path)
    );
    app_log::info(format!("{operation} requested"));

    let operation_for_error = operation.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        differ::compare_planning_coalition(&original_path, &modified_path)
    })
    .await
    .map_err(|error| format!("{operation_for_error} was interrupted: {error}"))?;

    match &result {
        Ok(diff) => app_log::info(format!(
            "{operation} succeeded: safe_to_merge={}, warnings={}, details={}",
            diff.safe_to_merge,
            diff.warnings.len(),
            diff.details.len()
        )),
        Err(error) => app_log::error(format!("{operation} failed: {error}")),
    }

    result
}

#[tauri::command]
async fn merge_planning_miz(
    original_path: String,
    modified_path: String,
    output_path: String,
    force_merge: bool,
    coalition_override: Option<String>,
) -> Result<String, String> {
    let original_path = PathBuf::from(original_path);
    let modified_path = PathBuf::from(modified_path);
    let output_path = PathBuf::from(&output_path);
    let operation = format!(
        "Merge original '{}' with modified '{}' to '{}'{}",
        file_label(&original_path),
        file_label(&modified_path),
        file_label(&output_path),
        if force_merge { " using override" } else { "" }
    );
    app_log::info(format!("{operation} requested"));

    run_blocking(operation, move || {
        ensure_distinct_paths(&[
            ("original", &original_path),
            ("modified", &modified_path),
            ("output", &output_path),
        ])
        .and_then(|_| {
            merger::merge_planning_coalition(
                &original_path,
                &modified_path,
                &output_path,
                force_merge,
                coalition_override.as_deref(),
            )
        })
        .map(|_| output_path.to_string_lossy().into_owned())
    })
    .await
}

#[tauri::command]
fn get_log_file_path() -> Result<String, String> {
    app_log::log_path()
        .map(|path| path.to_string_lossy().into_owned())
        .ok_or_else(|| "Log file has not been initialized.".to_string())
}

#[tauri::command]
fn get_app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        github_repo_url: GITHUB_REPO_URL,
        github_releases_url: GITHUB_RELEASES_URL,
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            if let Err(error) = app_log::init(app.handle()) {
                eprintln!("Unable to initialize app logging: {error}");
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            validate_miz,
            export_planning_miz,
            detect_flights,
            compare_modified_miz,
            merge_planning_miz,
            get_log_file_path,
            get_app_info
        ])
        .run(tauri::generate_context!())
        .expect("error while running DCS Mission Composer");
}
