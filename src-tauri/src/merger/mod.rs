use std::path::Path;

use crate::{archive, differ, exporter};

pub fn merge_planning_coalition(
    original_path: &Path,
    modified_path: &Path,
    output_path: &Path,
    force_merge: bool,
    coalition_override: Option<&str>,
) -> Result<(), String> {
    let diff = differ::compare_planning_coalition(original_path, modified_path)?;
    if !diff.safe_to_merge && !force_merge {
        return Err("Modified mission is not safe to merge. Review the diff warnings.".to_string());
    }

    let planning_metadata = differ::resolve_planning_coalition(original_path, modified_path).ok();
    if planning_metadata
        .as_ref()
        .is_some_and(|metadata| metadata.export_type == "flight")
    {
        let planning_metadata = planning_metadata.ok_or_else(|| {
            "DCS Mission Composer could not determine the planning coalition.".to_string()
        })?;
        return merge_flight_planning_mission(
            original_path,
            modified_path,
            output_path,
            planning_metadata,
        );
    }

    let planning_coalition = if force_merge {
        if let Some(coalition) = coalition_override {
            normalize_coalition(coalition)?
        } else {
            planning_metadata
                .ok_or_else(|| {
                    "DCS Mission Composer could not determine the planning coalition.".to_string()
                })?
                .coalition
        }
    } else {
        planning_metadata
            .ok_or_else(|| {
                "DCS Mission Composer could not determine the planning coalition.".to_string()
            })?
            .coalition
    };
    let protected_coalition = opposite_coalition(&planning_coalition)?;
    let original_mission = archive::read_mission_file(original_path)?;
    let modified_mission = archive::read_mission_file(modified_path)?;
    let original_protected_coalition =
        exporter::extract_coalition(&original_mission, protected_coalition)?;
    let merged_mission = exporter::replace_coalition(
        &modified_mission,
        protected_coalition,
        original_protected_coalition,
    )?;

    archive::write_with_replaced_mission_and_skipped_files(
        modified_path,
        output_path,
        &merged_mission,
        &[],
        &[exporter::DMC_MANIFEST_PATH],
    )
}

fn merge_flight_planning_mission(
    original_path: &Path,
    modified_path: &Path,
    output_path: &Path,
    planning_metadata: differ::PlanningCoalition,
) -> Result<(), String> {
    let flight_id = planning_metadata
        .flight_id
        .ok_or_else(|| "Flight export manifest does not contain a flight id.".to_string())?;
    let planning_coalition = planning_metadata.coalition;
    let protected_coalition = opposite_coalition(&planning_coalition)?;
    let original_mission = archive::read_mission_file(original_path)?;
    let modified_mission = archive::read_mission_file(modified_path)?;
    let original_protected_coalition =
        exporter::extract_coalition(&original_mission, protected_coalition)?;
    let original_planning_coalition =
        exporter::extract_coalition(&original_mission, &planning_coalition)?;
    let modified_planning_coalition =
        exporter::extract_coalition(&modified_mission, &planning_coalition)?;
    let modified_flight =
        exporter::extract_flight_group_body(modified_planning_coalition, &flight_id)?;
    let merged_planning_coalition = exporter::replace_flight_group_body(
        original_planning_coalition,
        &flight_id,
        modified_flight,
    )?;
    let merged_mission = exporter::replace_coalition(
        &modified_mission,
        protected_coalition,
        original_protected_coalition,
    )?;
    let merged_mission = exporter::replace_coalition(
        &merged_mission,
        &planning_coalition,
        &merged_planning_coalition,
    )?;

    archive::write_with_replaced_mission_and_skipped_files(
        modified_path,
        output_path,
        &merged_mission,
        &[],
        &[exporter::DMC_MANIFEST_PATH],
    )
}

fn normalize_coalition(coalition: &str) -> Result<String, String> {
    match coalition.to_ascii_lowercase().as_str() {
        "blue" => Ok("blue".to_string()),
        "red" => Ok("red".to_string()),
        _ => Err("Override coalition must be BLUE or RED.".to_string()),
    }
}

fn opposite_coalition(coalition: &str) -> Result<&'static str, String> {
    match coalition {
        "blue" => Ok("red"),
        "red" => Ok("blue"),
        _ => Err("Planning coalition must be BLUE or RED.".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::merge_planning_coalition;
    use crate::archive;
    use crate::exporter::replace_coalition;
    use std::{
        env, fs,
        io::{Read, Write},
        path::PathBuf,
    };
    use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

    #[test]
    fn replacing_blue_preserves_red_text() {
        let original =
            r#"mission = { coalition = { blue = { old = true }, red = { secret = true } } }"#;
        let merged =
            replace_coalition(original, "blue", "blue = { new = true }").expect("replace blue");

        assert!(merged.contains("blue = { new = true }"));
        assert!(merged.contains("red = { secret = true }"));
        assert!(!merged.contains("old = true"));
    }

    fn temp_file(name: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "dcs_mission_composer_merger_{}_{}",
            std::process::id(),
            name
        ))
    }

    fn write_test_miz(path: &PathBuf, mission: &str, extra_files: &[(&str, &str)]) {
        let file = fs::File::create(path).expect("create test archive");
        let mut archive = ZipWriter::new(file);

        archive
            .start_file("mission", SimpleFileOptions::default())
            .expect("start mission file");
        archive
            .write_all(mission.as_bytes())
            .expect("write mission file");

        for (name, contents) in extra_files {
            archive
                .start_file(*name, SimpleFileOptions::default())
                .expect("start extra file");
            archive
                .write_all(contents.as_bytes())
                .expect("write extra file");
        }

        archive.finish().expect("finish test archive");
    }

    #[test]
    fn merge_uses_modified_mission_as_base_and_restores_hidden_coalition() {
        let original = temp_file("original_base.miz");
        let modified = temp_file("modified_base.miz");
        let output = temp_file("merged_base.miz");

        write_test_miz(
            &original,
            r#"mission = { dtc = { jet = "original" }, coalition = { blue = { name = "blue", plane = { group = { old = true } }, country = { "USA" } }, red = { name = "red", plane = { group = { secret = true } }, country = { "Russia" } } } }"#,
            &[],
        );
        write_test_miz(
            &modified,
            r#"mission = { dtc = { jet = "edited" }, coalition = { blue = { name = "blue", plane = { group = { new = true } }, country = { "USA" } }, red = { name = "red", country = {} } } }"#,
            &[("DTC/jet.lua", "edited dtc file")],
        );

        merge_planning_coalition(&original, &modified, &output, true, Some("blue"))
            .expect("merge planning mission");

        let mission = archive::read_mission_file(&output).expect("read merged mission");
        let file = fs::File::open(&output).expect("open merged archive");
        let mut archive = ZipArchive::new(file).expect("read merged archive");
        let mut dtc_file = String::new();
        archive
            .by_name("DTC/jet.lua")
            .expect("preserve modified extra file")
            .read_to_string(&mut dtc_file)
            .expect("read dtc file");

        let _ = fs::remove_file(original);
        let _ = fs::remove_file(modified);
        let _ = fs::remove_file(output);

        assert!(mission.contains(r#"dtc = { jet = "edited" }"#));
        assert!(mission.contains("new = true"));
        assert!(mission.contains("secret = true"));
        assert!(!mission.contains("old = true"));
        assert_eq!(dtc_file, "edited dtc file");
    }

    #[test]
    fn flight_merge_replaces_only_selected_group_and_preserves_modified_archive_files() {
        let original = temp_file("original_flight_base.miz");
        let modified = temp_file("modified_flight_base.miz");
        let output = temp_file("merged_flight_base.miz");
        let manifest = r#"{"formatVersion":1,"application":"DCS Mission Composer","exportType":"flight","coalition":"blue","removedCoalition":"red","flightId":"blue|plane|Hornet 1","flightName":"Hornet 1","aircraftType":"FA-18C_hornet"}"#;

        write_test_miz(
            &original,
            r#"mission = { dtc = { jet = "original" }, coalition = { blue = { name = "blue", country = { [1] = { plane = { group = { [1] = { name = "Viper 1", units = { [1] = { type = "F-16C_50", skill = "Client" } }, payload = "original-viper" }, [2] = { name = "Hornet 1", units = { [1] = { type = "FA-18C_hornet", skill = "Client" } }, payload = "original-hornet" } } } } } }, red = { name = "red", country = { [1] = { plane = { group = { [1] = { name = "Bandit 1", payload = "secret-red" } } } } } } } }"#,
            &[],
        );
        write_test_miz(
            &modified,
            r#"mission = { dtc = { jet = "edited" }, coalition = { blue = { name = "blue", country = { [1] = { plane = { group = { [1] = { name = "Hornet 1", units = { [1] = { type = "FA-18C_hornet", skill = "Client" } }, payload = "edited-hornet" } } } } } }, red = { name = "red", country = {} } } }"#,
            &[
                ("dmc/manifest.json", manifest),
                ("DTC/jet.lua", "edited dtc file"),
            ],
        );

        merge_planning_coalition(&original, &modified, &output, false, None)
            .expect("merge flight planning mission");

        let mission = archive::read_mission_file(&output).expect("read merged mission");
        let file = fs::File::open(&output).expect("open merged archive");
        let mut archive = ZipArchive::new(file).expect("read merged archive");
        let mut dtc_file = String::new();
        archive
            .by_name("DTC/jet.lua")
            .expect("preserve modified extra file")
            .read_to_string(&mut dtc_file)
            .expect("read dtc file");
        assert!(archive.by_name("dmc/manifest.json").is_err());

        let _ = fs::remove_file(original);
        let _ = fs::remove_file(modified);
        let _ = fs::remove_file(output);

        assert!(mission.contains(r#"dtc = { jet = "edited" }"#));
        assert!(mission.contains("payload = \"original-viper\""));
        assert!(mission.contains("payload = \"edited-hornet\""));
        assert!(!mission.contains("payload = \"original-hornet\""));
        assert!(mission.contains("payload = \"secret-red\""));
        assert_eq!(dtc_file, "edited dtc file");
    }
}
