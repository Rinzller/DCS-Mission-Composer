use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{archive, exporter};

#[derive(Serialize)]
pub struct MissionDiff {
    pub safe_to_merge: bool,
    pub summary: String,
    pub details: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Deserialize)]
struct DmcManifest {
    coalition: String,
    #[serde(rename = "exportType")]
    export_type: Option<String>,
    #[serde(rename = "flightId")]
    flight_id: Option<String>,
    #[serde(rename = "flightName")]
    flight_name: Option<String>,
    #[serde(rename = "aircraftType")]
    aircraft_type: Option<String>,
}

pub struct PlanningCoalition {
    pub coalition: String,
    pub inferred: bool,
    pub export_type: String,
    pub flight_id: Option<String>,
    pub flight_name: Option<String>,
    pub aircraft_type: Option<String>,
}

pub fn compare_planning_coalition(
    original_path: &Path,
    modified_path: &Path,
) -> Result<MissionDiff, String> {
    archive::verify_miz_archive(original_path)?;
    archive::verify_miz_archive(modified_path)?;

    let mut warnings = Vec::new();
    let planning_metadata = match resolve_planning_coalition(original_path, modified_path) {
        Ok(metadata) => metadata,
        Err(message) => {
            warnings.push(message);
            PlanningCoalition {
                coalition: "blue".to_string(),
                inferred: false,
                export_type: "unknown".to_string(),
                flight_id: None,
                flight_name: None,
                aircraft_type: None,
            }
        }
    };
    if planning_metadata.export_type == "flight" {
        return compare_flight_planning_mission(original_path, modified_path, planning_metadata);
    }
    let planning_coalition = planning_metadata.coalition;
    let protected_coalition = opposite_coalition(&planning_coalition)?;

    let original_mission = archive::read_mission_file(original_path)?;
    let modified_mission = archive::read_mission_file(modified_path)?;
    let original_coalition = exporter::extract_coalition(&original_mission, &planning_coalition)?;
    let modified_coalition = exporter::extract_coalition(&modified_mission, &planning_coalition)?;
    let changed = original_coalition != modified_coalition;
    let safe_to_merge = warnings.is_empty();
    let coalition_label = planning_coalition.to_ascii_uppercase();
    let summary = match (changed, safe_to_merge) {
        (true, true) => format!("{coalition_label} coalition changes found. Safe to merge."),
        (true, false) => {
            format!(
                "{coalition_label} coalition changes found, but review warnings before merging."
            )
        }
        (false, true) => {
            format!("No {coalition_label} coalition changes found. Safe to merge, but there is nothing to apply.")
        }
        (false, false) => {
            format!("No {coalition_label} coalition changes found, and the modified mission is not marked as a DMC export.")
        }
    };

    Ok(MissionDiff {
        safe_to_merge,
        summary,
        details: build_review_details(
            original_coalition,
            modified_coalition,
            &planning_coalition,
            protected_coalition,
            planning_metadata.inferred,
            safe_to_merge,
        ),
        warnings,
    })
}

pub fn resolve_planning_coalition(
    original_path: &Path,
    modified_path: &Path,
) -> Result<PlanningCoalition, String> {
    if let Ok(manifest) = read_manifest(modified_path) {
        let coalition = normalize_coalition(&manifest.coalition)
            .map(str::to_string)
            .map_err(|error| {
                format!("Modified mission contains an unsupported DMC manifest: {error}")
            })?;

        return Ok(PlanningCoalition {
            coalition,
            inferred: false,
            export_type: manifest
                .export_type
                .unwrap_or_else(|| "planning".to_string()),
            flight_id: manifest.flight_id,
            flight_name: manifest.flight_name,
            aircraft_type: manifest.aircraft_type,
        });
    }

    infer_planning_coalition(original_path, modified_path).map(|coalition| PlanningCoalition {
        coalition,
        inferred: true,
        export_type: "planning".to_string(),
        flight_id: None,
        flight_name: None,
        aircraft_type: None,
    })
}

fn compare_flight_planning_mission(
    original_path: &Path,
    modified_path: &Path,
    planning_metadata: PlanningCoalition,
) -> Result<MissionDiff, String> {
    let mut warnings = Vec::new();
    let planning_coalition = planning_metadata.coalition;
    let protected_coalition = opposite_coalition(&planning_coalition)?;
    let flight_id = match planning_metadata.flight_id {
        Some(flight_id) => flight_id,
        None => {
            warnings.push(
                "Modified mission contains a flight export manifest without a flight id."
                    .to_string(),
            );
            String::new()
        }
    };

    let original_mission = archive::read_mission_file(original_path)?;
    let modified_mission = archive::read_mission_file(modified_path)?;
    let original_coalition = exporter::extract_coalition(&original_mission, &planning_coalition)?;
    let modified_coalition = exporter::extract_coalition(&modified_mission, &planning_coalition)?;
    let changed = if flight_id.is_empty() {
        false
    } else {
        let original_flight = exporter::extract_flight_group_body(original_coalition, &flight_id)?;
        let modified_flight = exporter::extract_flight_group_body(modified_coalition, &flight_id)?;
        original_flight != modified_flight
    };
    let safe_to_merge = warnings.is_empty();
    let flight_label = flight_review_label(
        planning_metadata.aircraft_type.as_deref(),
        planning_metadata.flight_name.as_deref(),
        &flight_id,
    );
    let summary = match (changed, safe_to_merge) {
        (true, true) => format!("{flight_label} flight changes found. Safe to merge."),
        (true, false) => {
            format!("{flight_label} flight changes found, but review warnings before merging.")
        }
        (false, true) => {
            format!("No {flight_label} flight changes found. Safe to merge, but there is nothing to apply.")
        }
        (false, false) => {
            "Flight changes could not be reviewed because the manifest is incomplete.".to_string()
        }
    };

    Ok(MissionDiff {
        safe_to_merge,
        summary,
        details: build_flight_review_details(
            changed,
            &flight_label,
            &planning_coalition,
            protected_coalition,
            safe_to_merge,
        ),
        warnings,
    })
}

fn read_manifest(path: &Path) -> Result<DmcManifest, String> {
    let manifest = archive::read_optional_text_file(path, exporter::DMC_MANIFEST_PATH)?
        .ok_or_else(|| {
            "Modified mission is missing dmc/manifest.json. Export it from DCS Mission Composer before merging."
                .to_string()
        })?;
    serde_json::from_str(&manifest)
        .map_err(|error| format!("Modified mission contains an unreadable DMC manifest: {error}"))
}

fn infer_planning_coalition(original_path: &Path, modified_path: &Path) -> Result<String, String> {
    let original_mission = archive::read_mission_file(original_path)?;
    let modified_mission = archive::read_mission_file(modified_path)?;
    let original_blue = exporter::extract_coalition(&original_mission, "blue")?;
    let original_red = exporter::extract_coalition(&original_mission, "red")?;
    let modified_blue = exporter::extract_coalition(&modified_mission, "blue")?;
    let modified_red = exporter::extract_coalition(&modified_mission, "red")?;
    let blue_is_stripped = looks_like_stripped_coalition(modified_blue, "blue")
        && !looks_like_stripped_coalition(original_blue, "blue");
    let red_is_stripped = looks_like_stripped_coalition(modified_red, "red")
        && !looks_like_stripped_coalition(original_red, "red");

    match (blue_is_stripped, red_is_stripped) {
        (true, false) => Ok("red".to_string()),
        (false, true) => Ok("blue".to_string()),
        _ => Err(
            "Modified mission is missing dmc/manifest.json and DCS Mission Composer could not infer which coalition was exported."
                .to_string(),
        ),
    }
}

fn looks_like_stripped_coalition(coalition_data: &str, coalition: &str) -> bool {
    let compact: String = coalition_data
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    let name_marker = format!("name=\"{coalition}\"");

    compact.contains("country={}")
        && compact.contains(&name_marker)
        && !contains_group_data(&compact)
}

fn contains_group_data(compact_coalition_data: &str) -> bool {
    ["plane=", "helicopter=", "vehicle=", "ship=", "static="]
        .iter()
        .any(|token| compact_coalition_data.contains(token))
}

fn normalize_coalition(coalition: &str) -> Result<&'static str, String> {
    match coalition.to_ascii_lowercase().as_str() {
        "blue" => Ok("blue"),
        "red" => Ok("red"),
        _ => Err("coalition must be BLUE or RED.".to_string()),
    }
}

fn opposite_coalition(coalition: &str) -> Result<&'static str, String> {
    match coalition {
        "blue" => Ok("RED"),
        "red" => Ok("BLUE"),
        _ => Err("coalition must be BLUE or RED.".to_string()),
    }
}

fn build_review_details(
    original_coalition_data: &str,
    modified_coalition_data: &str,
    planning_coalition: &str,
    protected_coalition: &str,
    inferred_coalition: bool,
    safe_to_merge: bool,
) -> Vec<String> {
    let changed = original_coalition_data != modified_coalition_data;
    let planning_label = planning_coalition.to_ascii_uppercase();

    if !changed {
        return vec![format!(
            "No {planning_label} coalition changes were found in the modified mission."
        )];
    }

    let mut details = vec![
        format!("The modified mission changes {planning_label} coalition data."),
        format!(
            "Merge will copy those {planning_label} coalition changes into a new mission file."
        ),
        "The original mission is never overwritten.".to_string(),
        format!("{protected_coalition} coalition data remains from the original mission."),
    ];

    details.extend(build_domain_observations(
        original_coalition_data,
        modified_coalition_data,
    ));

    if inferred_coalition {
        details.push(format!(
            "dmc/manifest.json was not present after DCS saved the mission, so DCS Mission Composer inferred this is a {planning_label} planning mission."
        ));
    }

    if !safe_to_merge {
        details.push(
            "Normal merge is blocked until warnings are resolved. Override merge can be used deliberately."
                .to_string(),
        );
    }

    details
}

fn build_flight_review_details(
    changed: bool,
    flight_label: &str,
    planning_coalition: &str,
    protected_coalition: &str,
    safe_to_merge: bool,
) -> Vec<String> {
    let planning_label = planning_coalition.to_ascii_uppercase();

    if !safe_to_merge {
        return vec![
            format!("{flight_label} could not be safely reviewed."),
            "Normal merge is blocked until warnings are resolved. Override merge can be used deliberately."
                .to_string(),
        ];
    }

    if !changed {
        return vec![format!(
            "No changes were found for {flight_label} in the modified mission."
        )];
    }

    vec![
        format!("The modified mission changes {flight_label}."),
        "Merge will copy only that flight group into a new mission file.".to_string(),
        format!("Other {planning_label} coalition assets remain from the original mission."),
        format!("{protected_coalition} coalition data remains from the original mission."),
        "The original mission is never overwritten.".to_string(),
    ]
}

fn flight_review_label(
    aircraft_type: Option<&str>,
    flight_name: Option<&str>,
    flight_id: &str,
) -> String {
    match (aircraft_type, flight_name) {
        (Some(aircraft_type), Some(flight_name)) => format!("{aircraft_type} - {flight_name}"),
        (_, Some(flight_name)) => flight_name.to_string(),
        _ => flight_id
            .splitn(3, '|')
            .nth(2)
            .unwrap_or("Selected flight")
            .to_string(),
    }
}

fn build_domain_observations(original_blue: &str, modified_blue: &str) -> Vec<String> {
    [
        ("Aircraft group references", "plane"),
        ("Helicopter group references", "helicopter"),
        ("Ground vehicle references", "vehicle"),
        ("Ship group references", "ship"),
        ("Static object references", "static"),
        ("Route references", "route"),
    ]
    .iter()
    .filter_map(|(label, token)| {
        let original_count = original_blue.matches(token).count();
        let modified_count = modified_blue.matches(token).count();

        (original_count != modified_count)
            .then(|| format!("{label} changed from {original_count} to {modified_count}."))
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        build_review_details, compare_planning_coalition, read_manifest, resolve_planning_coalition,
    };
    use crate::exporter;
    use std::{env, fs, io::Write, path::PathBuf};
    use zip::{write::SimpleFileOptions, ZipWriter};

    fn temp_file(name: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "dcs_mission_composer_differ_{}_{}",
            std::process::id(),
            name
        ))
    }

    fn write_test_miz(path: &PathBuf, mission: &str, manifest: Option<&str>) {
        let file = fs::File::create(path).expect("create test archive");
        let mut archive = ZipWriter::new(file);

        archive
            .start_file("mission", SimpleFileOptions::default())
            .expect("start mission file");
        archive
            .write_all(mission.as_bytes())
            .expect("write mission file");

        if let Some(manifest) = manifest {
            archive
                .start_file(exporter::DMC_MANIFEST_PATH, SimpleFileOptions::default())
                .expect("start manifest");
            archive
                .write_all(manifest.as_bytes())
                .expect("write manifest");
        }

        archive.finish().expect("finish test archive");
    }

    #[test]
    fn builds_readable_change_summary() {
        let details = build_review_details(
            r#"blue = { plane = { group = {} }, route = {} }"#,
            r#"blue = { plane = { group = {} }, plane = { group = {} }, route = {} }"#,
            "blue",
            "RED",
            false,
            true,
        );

        assert!(details[0].contains("changes BLUE coalition data"));
        assert!(details
            .iter()
            .any(|detail| detail.contains("RED coalition")));
        assert!(details
            .iter()
            .any(|detail| detail.contains("Aircraft group references")));
    }

    #[test]
    fn reads_red_planning_coalition_from_manifest() {
        let path = temp_file("red_manifest.miz");
        write_test_miz(
            &path,
            "mission = {}",
            Some(
                r#"{"formatVersion":1,"application":"DCS Mission Composer","exportType":"planning","coalition":"red"}"#,
            ),
        );

        let manifest = read_manifest(&path).expect("read planning manifest");

        let _ = fs::remove_file(path);
        assert_eq!(manifest.coalition, "red");
        assert_eq!(manifest.export_type.as_deref(), Some("planning"));
    }

    #[test]
    fn infers_blue_planning_coalition_when_manifest_is_missing() {
        let original = temp_file("original_blue_infer.miz");
        let modified = temp_file("modified_blue_infer.miz");

        write_test_miz(
            &original,
            r#"mission = { coalition = { blue = { name = "blue", plane = { group = {} }, country = { "USA" } }, red = { name = "red", plane = { group = {} }, country = { "Russia" } } } }"#,
            None,
        );
        write_test_miz(
            &modified,
            r#"mission = { coalition = { blue = { name = "blue", plane = { group = {} }, plane = { group = {} }, country = { "USA" } }, red = { name = "red", country = {} } } }"#,
            None,
        );

        let metadata =
            resolve_planning_coalition(&original, &modified).expect("infer planning coalition");
        let diff = compare_planning_coalition(&original, &modified).expect("compare missions");

        let _ = fs::remove_file(original);
        let _ = fs::remove_file(modified);

        assert_eq!(metadata.coalition, "blue");
        assert!(metadata.inferred);
        assert!(diff.safe_to_merge);
        assert!(diff.warnings.is_empty());
    }

    #[test]
    fn reviews_flight_export_as_safe_when_manifest_identifies_flight() {
        let original = temp_file("original_flight_review.miz");
        let modified = temp_file("modified_flight_review.miz");
        let manifest = r#"{"formatVersion":1,"application":"DCS Mission Composer","exportType":"flight","coalition":"blue","removedCoalition":"red","flightId":"blue|plane|Hornet 1","flightName":"Hornet 1","aircraftType":"FA-18C_hornet"}"#;

        write_test_miz(
            &original,
            r#"mission = { coalition = { blue = { name = "blue", country = { [1] = { plane = { group = { [1] = { name = "Hornet 1", units = { [1] = { type = "FA-18C_hornet", skill = "Client" } }, payload = "original" }, [2] = { name = "Viper 1", units = { [1] = { type = "F-16C_50", skill = "Client" } } } } } } } }, red = { name = "red", country = { [1] = { plane = { group = {} } } } } } }"#,
            None,
        );
        write_test_miz(
            &modified,
            r#"mission = { coalition = { blue = { name = "blue", country = { [1] = { plane = { group = { [1] = { name = "Hornet 1", units = { [1] = { type = "FA-18C_hornet", skill = "Client" } }, payload = "edited" } } } } } }, red = { name = "red", country = {} } } }"#,
            Some(manifest),
        );

        let diff = compare_planning_coalition(&original, &modified).expect("compare flight export");

        let _ = fs::remove_file(original);
        let _ = fs::remove_file(modified);

        assert!(diff.safe_to_merge);
        assert!(diff.warnings.is_empty());
        assert!(diff.summary.contains("FA-18C_hornet - Hornet 1"));
        assert!(diff
            .details
            .iter()
            .any(|detail| detail.contains("copy only that flight group")));
    }
}
