use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};

use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

pub fn verify_miz_archive(path: &Path) -> Result<(), String> {
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("miz"))
    {
        return Err("Expected a .miz file.".to_string());
    }

    let file = File::open(path).map_err(|error| format!("Unable to open mission file: {error}"))?;

    zip::ZipArchive::new(file)
        .map(|_| ())
        .map_err(|error| format!("Invalid ZIP archive: {error}"))
}

pub fn read_mission_file(path: &Path) -> Result<String, String> {
    read_text_file(path, "mission")
}

pub fn read_text_file(path: &Path, file_name: &str) -> Result<String, String> {
    read_optional_text_file(path, file_name)?
        .ok_or_else(|| format!("Mission archive does not contain {file_name}."))
}

pub fn read_optional_text_file(path: &Path, file_name: &str) -> Result<Option<String>, String> {
    let file = File::open(path).map_err(|error| format!("Unable to open mission file: {error}"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("Invalid ZIP archive: {error}"))?;
    let Ok(mut mission) = archive.by_name(file_name) else {
        return Ok(None);
    };
    let mut contents = String::with_capacity(mission.size().min(usize::MAX as u64) as usize);

    mission
        .read_to_string(&mut contents)
        .map_err(|error| format!("Unable to read mission file: {error}"))?;

    Ok(Some(contents))
}

pub fn write_with_replaced_mission(
    source_path: &Path,
    output_path: &Path,
    mission_contents: &str,
    extra_files: &[(&str, &str)],
) -> Result<(), String> {
    write_with_replaced_mission_and_skipped_files(
        source_path,
        output_path,
        mission_contents,
        extra_files,
        &[],
    )
}

pub fn write_with_replaced_mission_and_skipped_files(
    source_path: &Path,
    output_path: &Path,
    mission_contents: &str,
    extra_files: &[(&str, &str)],
    skipped_files: &[&str],
) -> Result<(), String> {
    if source_path == output_path {
        return Err(
            "Choose a different output file so the original mission is preserved.".to_string(),
        );
    }

    let source = File::open(source_path)
        .map_err(|error| format!("Unable to open source mission: {error}"))?;
    let mut source_archive =
        ZipArchive::new(source).map_err(|error| format!("Invalid ZIP archive: {error}"))?;
    let output =
        File::create(output_path).map_err(|error| format!("Unable to create export: {error}"))?;
    let mut output_archive = ZipWriter::new(output);

    for index in 0..source_archive.len() {
        let mut entry = source_archive
            .by_index(index)
            .map_err(|error| format!("Unable to read archive entry: {error}"))?;
        let name = entry.name().to_string();
        let options = SimpleFileOptions::default().compression_method(entry.compression());

        if entry.is_dir() {
            output_archive
                .add_directory(name, options)
                .map_err(|error| format!("Unable to write archive directory: {error}"))?;
            continue;
        }

        if extra_files
            .iter()
            .any(|(extra_name, _)| name == *extra_name)
            || skipped_files
                .iter()
                .any(|skipped_name| name == *skipped_name)
        {
            continue;
        }

        output_archive
            .start_file(&name, options)
            .map_err(|error| format!("Unable to write archive entry: {error}"))?;

        if name == "mission" {
            output_archive
                .write_all(mission_contents.as_bytes())
                .map_err(|error| format!("Unable to write cleaned mission file: {error}"))?;
        } else {
            std::io::copy(&mut entry, &mut output_archive)
                .map_err(|error| format!("Unable to copy archive entry: {error}"))?;
        }
    }

    for (name, contents) in extra_files {
        output_archive
            .start_file(*name, SimpleFileOptions::default())
            .map_err(|error| format!("Unable to write {name}: {error}"))?;
        output_archive
            .write_all(contents.as_bytes())
            .map_err(|error| format!("Unable to write {name}: {error}"))?;
    }

    output_archive
        .finish()
        .map_err(|error| format!("Unable to finish export archive: {error}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{verify_miz_archive, write_with_replaced_mission};
    use std::{
        env, fs,
        io::{Read, Write},
        path::PathBuf,
    };
    use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

    const EMPTY_ZIP: &[u8] = &[
        0x50, 0x4b, 0x05, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    fn temp_file(name: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "dcs_mission_composer_{}_{}",
            std::process::id(),
            name
        ))
    }

    fn write_test_miz(path: &PathBuf, mission: &str) {
        let file = fs::File::create(path).expect("create test archive");
        let mut archive = ZipWriter::new(file);

        archive
            .start_file("mission", SimpleFileOptions::default())
            .expect("start mission file");
        archive
            .write_all(mission.as_bytes())
            .expect("write mission file");
        archive.finish().expect("finish test archive");
    }

    #[test]
    fn accepts_miz_zip_archive() {
        let path = temp_file("valid.miz");
        fs::write(&path, EMPTY_ZIP).expect("write valid test archive");

        let result = verify_miz_archive(&path);

        let _ = fs::remove_file(path);
        assert!(result.is_ok());
    }

    #[test]
    fn rejects_non_miz_extension() {
        let path = temp_file("mission.zip");
        fs::write(&path, EMPTY_ZIP).expect("write non-miz test archive");

        let result = verify_miz_archive(&path);

        let _ = fs::remove_file(path);
        assert_eq!(result, Err("Expected a .miz file.".to_string()));
    }

    #[test]
    fn rejects_invalid_zip_archive() {
        let path = temp_file("invalid.miz");
        fs::write(&path, b"not a zip").expect("write invalid test archive");

        let result = verify_miz_archive(&path);

        let _ = fs::remove_file(path);
        assert!(result.is_err());
    }

    #[test]
    fn writes_extra_files_at_archive_root() {
        let source = temp_file("source.miz");
        let output = temp_file("output.miz");
        write_test_miz(&source, "mission = {}");

        write_with_replaced_mission(
            &source,
            &output,
            "mission = { cleaned = true }",
            &[("dmc/manifest.json", "{\"formatVersion\":1}")],
        )
        .expect("write archive with manifest");

        let file = fs::File::open(&output).expect("open output archive");
        let mut archive = ZipArchive::new(file).expect("read output archive");
        let mut manifest = String::new();
        archive
            .by_name("dmc/manifest.json")
            .expect("find dmc manifest")
            .read_to_string(&mut manifest)
            .expect("read manifest");

        let _ = fs::remove_file(source);
        let _ = fs::remove_file(output);

        assert_eq!(manifest, "{\"formatVersion\":1}");
    }
}
