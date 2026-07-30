use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;

use crate::archive;

pub(crate) const DMC_MANIFEST_PATH: &str = "dmc/manifest.json";
const FLIGHT_CATEGORIES: [&str; 2] = ["plane", "helicopter"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FlightInfo {
    pub id: String,
    pub name: String,
    pub aircraft_type: String,
    pub coalition: String,
    pub category: String,
}

const EMPTY_RED_COALITION: &str = r#"red = 
    {
        bullseye = 
        {
            x = 0,
            y = 0,
        },
        nav_points = {},
        name = "red",
        country = {},
    }"#;

const EMPTY_BLUE_COALITION: &str = r#"blue = 
    {
        bullseye = 
        {
            x = 0,
            y = 0,
        },
        nav_points = {},
        name = "blue",
        country = {},
    }"#;

pub fn export_planning_mission(
    source_path: &Path,
    output_path: &Path,
    coalition: &str,
) -> Result<(), String> {
    let normalized_coalition = normalize_coalition(coalition)?;
    archive::verify_miz_archive(source_path)?;
    let mission = archive::read_mission_file(source_path)?;
    let (removed_coalition, replacement) = match normalized_coalition {
        "blue" => ("red", EMPTY_RED_COALITION),
        "red" => ("blue", EMPTY_BLUE_COALITION),
        _ => unreachable!("normalize_coalition only returns supported coalitions"),
    };
    let cleaned_mission = replace_coalition(&mission, removed_coalition, replacement)?;
    let manifest = build_manifest(source_path, normalized_coalition, removed_coalition)?;

    archive::write_with_replaced_mission(
        source_path,
        output_path,
        &cleaned_mission,
        &[(DMC_MANIFEST_PATH, &manifest)],
    )
}

pub fn export_flight_planning_mission(
    source_path: &Path,
    output_path: &Path,
    flight_id: &str,
) -> Result<(), String> {
    archive::verify_miz_archive(source_path)?;
    let mission = archive::read_mission_file(source_path)?;
    let flight = detect_flights_in_mission(&mission)
        .into_iter()
        .find(|flight| flight.id == flight_id)
        .ok_or_else(|| "Selected flight was not found in the mission.".to_string())?;
    let removed_coalition = match flight.coalition.as_str() {
        "blue" => "red",
        "red" => "blue",
        _ => return Err("Flight coalition must be BLUE or RED.".to_string()),
    };
    let replacement = match removed_coalition {
        "red" => EMPTY_RED_COALITION,
        "blue" => EMPTY_BLUE_COALITION,
        _ => unreachable!("removed coalition is normalized"),
    };
    let cleaned_mission = replace_coalition(&mission, removed_coalition, replacement)?;
    let selected_coalition = extract_coalition(&cleaned_mission, &flight.coalition)?;
    let pruned_coalition = keep_only_selected_flight(selected_coalition, &flight.id)?;
    let cleaned_mission =
        replace_coalition(&cleaned_mission, &flight.coalition, &pruned_coalition)?;
    let manifest = build_flight_manifest(source_path, &flight, removed_coalition)?;

    archive::write_with_replaced_mission(
        source_path,
        output_path,
        &cleaned_mission,
        &[(DMC_MANIFEST_PATH, &manifest)],
    )
}

pub fn detect_flights(source_path: &Path) -> Result<Vec<FlightInfo>, String> {
    archive::verify_miz_archive(source_path)?;
    let mission = archive::read_mission_file(source_path)?;
    Ok(detect_flights_in_mission(&mission))
}

fn normalize_coalition(coalition: &str) -> Result<&'static str, String> {
    match coalition.to_ascii_lowercase().as_str() {
        "blue" => Ok("blue"),
        "red" => Ok("red"),
        _ => Err("Export coalition must be BLUE or RED.".to_string()),
    }
}

fn build_manifest(
    source_path: &Path,
    coalition: &str,
    removed_coalition: &str,
) -> Result<String, String> {
    let source_file = source_path
        .file_name()
        .and_then(|file_name| file_name.to_str())
        .unwrap_or("unknown.miz");
    let created_unix_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("Unable to create export timestamp: {error}"))?
        .as_secs();

    Ok(format!(
        concat!(
            "{{\n",
            "  \"formatVersion\": 1,\n",
            "  \"application\": \"DCS Mission Composer\",\n",
            "  \"exportType\": \"planning\",\n",
            "  \"coalition\": \"{}\",\n",
            "  \"removedCoalition\": \"{}\",\n",
            "  \"sourceFile\": \"{}\",\n",
            "  \"createdUnixSeconds\": {}\n",
            "}}\n"
        ),
        coalition,
        removed_coalition,
        escape_json_string(source_file),
        created_unix_seconds
    ))
}

fn build_flight_manifest(
    source_path: &Path,
    flight: &FlightInfo,
    removed_coalition: &str,
) -> Result<String, String> {
    let source_file = source_path
        .file_name()
        .and_then(|file_name| file_name.to_str())
        .unwrap_or("unknown.miz");
    let created_unix_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("Unable to create export timestamp: {error}"))?
        .as_secs();

    Ok(format!(
        concat!(
            "{{\n",
            "  \"formatVersion\": 1,\n",
            "  \"application\": \"DCS Mission Composer\",\n",
            "  \"exportType\": \"flight\",\n",
            "  \"coalition\": \"{}\",\n",
            "  \"removedCoalition\": \"{}\",\n",
            "  \"flightId\": \"{}\",\n",
            "  \"flightName\": \"{}\",\n",
            "  \"aircraftType\": \"{}\",\n",
            "  \"sourceFile\": \"{}\",\n",
            "  \"createdUnixSeconds\": {}\n",
            "}}\n"
        ),
        flight.coalition,
        removed_coalition,
        escape_json_string(&flight.id),
        escape_json_string(&flight.name),
        escape_json_string(&flight.aircraft_type),
        escape_json_string(source_file),
        created_unix_seconds
    ))
}

fn escape_json_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());

    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                escaped.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => escaped.push(character),
        }
    }

    escaped
}

pub(crate) fn extract_coalition<'a>(mission: &'a str, coalition: &str) -> Result<&'a str, String> {
    let (_, coalition_start, coalition_end) = find_table_assignment(mission, 0, "coalition")
        .ok_or_else(|| "Mission file does not contain coalition data.".to_string())?;
    let (start, _, end) = find_table_assignment(mission, coalition_start, coalition)
        .filter(|(_, _, end)| *end <= coalition_end)
        .ok_or_else(|| format!("Mission file does not contain {coalition} coalition data."))?;

    Ok(&mission[start..end])
}

pub(crate) fn replace_coalition(
    mission: &str,
    coalition: &str,
    replacement: &str,
) -> Result<String, String> {
    let (_, coalition_start, coalition_end) = find_table_assignment(mission, 0, "coalition")
        .ok_or_else(|| "Mission file does not contain coalition data.".to_string())?;
    let (start, _, end) = find_table_assignment(mission, coalition_start, coalition)
        .filter(|(_, _, end)| *end <= coalition_end)
        .ok_or_else(|| format!("Mission file does not contain {coalition} coalition data."))?;

    let mut cleaned = String::with_capacity(mission.len());
    cleaned.push_str(&mission[..start]);
    cleaned.push_str(replacement);
    cleaned.push_str(&mission[end..]);

    Ok(cleaned)
}

fn detect_flights_in_mission(mission: &str) -> Vec<FlightInfo> {
    ["blue", "red"]
        .into_iter()
        .filter_map(|coalition| {
            extract_coalition(mission, coalition)
                .ok()
                .map(|table| (coalition, table))
        })
        .flat_map(|(coalition, table)| detect_flights_in_coalition(table, coalition))
        .collect()
}

fn detect_flights_in_coalition(coalition_table: &str, coalition: &str) -> Vec<FlightInfo> {
    let Some((_, country_start, country_end)) =
        find_table_assignment(coalition_table, 0, "country")
    else {
        return Vec::new();
    };

    immediate_table_entries(coalition_table, country_start, country_end)
        .into_iter()
        .flat_map(|country| {
            FLIGHT_CATEGORIES.into_iter().flat_map(move |category| {
                let country_table = &coalition_table[country.brace_start..country.end];
                find_table_assignment(country_table, 0, category)
                    .into_iter()
                    .flat_map(move |(_, category_start, category_end)| {
                        let category_table = &country_table[category_start..category_end];
                        find_table_assignment(category_table, 0, "group")
                            .into_iter()
                            .flat_map(move |(_, group_start, group_end)| {
                                let group_table = &category_table[group_start..group_end];
                                immediate_table_entries(group_table, 0, group_end - group_start)
                                    .into_iter()
                                    .filter_map(move |group| {
                                        let source = &group_table[group.brace_start..group.end];
                                        let name =
                                            find_immediate_string_assignment(source, "name")?;
                                        let aircraft_type = player_flyable_aircraft_type(source)?;
                                        Some(FlightInfo {
                                            id: flight_id(coalition, category, &name),
                                            name,
                                            aircraft_type,
                                            coalition: coalition.to_string(),
                                            category: category.to_string(),
                                        })
                                    })
                            })
                    })
            })
        })
        .collect()
}

fn player_flyable_aircraft_type(group_source: &str) -> Option<String> {
    let (_, units_start, units_end) = find_immediate_table_assignment(group_source, "units")?;
    let units_table = &group_source[units_start..units_end];

    immediate_table_entries(units_table, 0, units_end - units_start)
        .into_iter()
        .find_map(|unit| {
            let unit_source = &units_table[unit.brace_start..unit.end];
            let skill = find_immediate_string_assignment(unit_source, "skill")?;
            if skill.eq_ignore_ascii_case("client") || skill.eq_ignore_ascii_case("player") {
                find_immediate_string_assignment(unit_source, "type")
            } else {
                None
            }
        })
}

fn keep_only_selected_flight(
    coalition_table: &str,
    selected_flight_id: &str,
) -> Result<String, String> {
    let mut cleaned = coalition_table.to_string();
    let coalition_name = if selected_flight_id.starts_with("blue|") {
        "blue"
    } else if selected_flight_id.starts_with("red|") {
        "red"
    } else {
        return Err("Selected flight id is invalid.".to_string());
    };

    for category in FLIGHT_CATEGORIES {
        cleaned = keep_only_selected_flight_in_category(
            &cleaned,
            coalition_name,
            category,
            selected_flight_id,
        )?;
    }

    Ok(cleaned)
}

pub(crate) fn extract_flight_group_body<'a>(
    coalition_table: &'a str,
    selected_flight_id: &str,
) -> Result<&'a str, String> {
    let location = find_flight_group(coalition_table, selected_flight_id)?
        .ok_or_else(|| "Selected flight was not found in the mission.".to_string())?;

    Ok(&coalition_table[location.brace_start..location.end])
}

pub(crate) fn replace_flight_group_body(
    coalition_table: &str,
    selected_flight_id: &str,
    replacement_group_body: &str,
) -> Result<String, String> {
    let location = find_flight_group(coalition_table, selected_flight_id)?
        .ok_or_else(|| "Selected flight was not found in the original mission.".to_string())?;
    let mut merged = String::with_capacity(
        coalition_table.len() - (location.end - location.brace_start)
            + replacement_group_body.len(),
    );

    merged.push_str(&coalition_table[..location.brace_start]);
    merged.push_str(replacement_group_body);
    merged.push_str(&coalition_table[location.end..]);

    Ok(merged)
}

fn keep_only_selected_flight_in_category(
    coalition_table: &str,
    coalition: &str,
    category: &str,
    selected_flight_id: &str,
) -> Result<String, String> {
    let Some((_, country_start, country_end)) =
        find_table_assignment(coalition_table, 0, "country")
    else {
        return Ok(coalition_table.to_string());
    };
    let mut removals = Vec::new();

    for country in immediate_table_entries(coalition_table, country_start, country_end) {
        let country_table = &coalition_table[country.brace_start..country.end];
        let Some((_, category_start, category_end)) =
            find_table_assignment(country_table, 0, category)
        else {
            continue;
        };
        let category_offset = country.brace_start + category_start;
        let category_table = &coalition_table[category_offset..country.brace_start + category_end];
        let Some((_, group_start, group_end)) = find_table_assignment(category_table, 0, "group")
        else {
            continue;
        };
        let group_offset = category_offset + group_start;
        let group_table = &coalition_table[group_offset..category_offset + group_end];

        for group in immediate_table_entries(group_table, 0, group_end - group_start) {
            let source = &group_table[group.brace_start..group.end];
            let Some(name) = find_immediate_string_assignment(source, "name") else {
                continue;
            };
            if flight_id(coalition, category, &name) != selected_flight_id {
                removals.push((
                    group_offset + group.start,
                    group_offset + group.end_with_trailing_comma,
                ));
            }
        }
    }

    if removals.is_empty() {
        return Ok(coalition_table.to_string());
    }

    let mut pruned = coalition_table.to_string();
    for (start, end) in removals.into_iter().rev() {
        pruned.replace_range(start..end, "");
    }

    Ok(pruned)
}

fn find_flight_group(
    coalition_table: &str,
    selected_flight_id: &str,
) -> Result<Option<TableEntry>, String> {
    let (coalition, category, _) = parse_flight_id(selected_flight_id)?;
    let Some((_, country_start, country_end)) =
        find_table_assignment(coalition_table, 0, "country")
    else {
        return Ok(None);
    };

    for country in immediate_table_entries(coalition_table, country_start, country_end) {
        let country_table = &coalition_table[country.brace_start..country.end];
        let Some((_, category_start, category_end)) =
            find_table_assignment(country_table, 0, category)
        else {
            continue;
        };
        let category_offset = country.brace_start + category_start;
        let category_table = &coalition_table[category_offset..country.brace_start + category_end];
        let Some((_, group_start, group_end)) = find_table_assignment(category_table, 0, "group")
        else {
            continue;
        };
        let group_offset = category_offset + group_start;
        let group_table = &coalition_table[group_offset..category_offset + group_end];

        for group in immediate_table_entries(group_table, 0, group_end - group_start) {
            let source = &group_table[group.brace_start..group.end];
            let Some(name) = find_immediate_string_assignment(source, "name") else {
                continue;
            };
            if flight_id(coalition, category, &name) == selected_flight_id {
                return Ok(Some(TableEntry {
                    start: group_offset + group.start,
                    brace_start: group_offset + group.brace_start,
                    end: group_offset + group.end,
                    end_with_trailing_comma: group_offset + group.end_with_trailing_comma,
                }));
            }
        }
    }

    Ok(None)
}

fn parse_flight_id(flight_id: &str) -> Result<(&str, &str, &str), String> {
    let mut parts = flight_id.splitn(3, '|');
    let coalition = parts
        .next()
        .filter(|value| *value == "blue" || *value == "red")
        .ok_or_else(|| "Selected flight id is invalid.".to_string())?;
    let category = parts
        .next()
        .filter(|value| FLIGHT_CATEGORIES.contains(value))
        .ok_or_else(|| "Selected flight id is invalid.".to_string())?;
    let name = parts
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Selected flight id is invalid.".to_string())?;

    Ok((coalition, category, name))
}

fn flight_id(coalition: &str, category: &str, name: &str) -> String {
    format!("{coalition}|{category}|{name}")
}

#[derive(Clone, Copy)]
struct TableEntry {
    start: usize,
    brace_start: usize,
    end: usize,
    end_with_trailing_comma: usize,
}

fn immediate_table_entries(source: &str, table_start: usize, table_end: usize) -> Vec<TableEntry> {
    let bytes = source.as_bytes();
    let mut entries = Vec::new();
    let mut cursor = table_start + 1;
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;

    while cursor < table_end.saturating_sub(1) {
        let byte = bytes[cursor];

        if let Some(active_quote) = quote {
            if byte == b'\\' {
                cursor += 2;
                continue;
            }

            if byte == active_quote {
                quote = None;
            }

            cursor += 1;
            continue;
        }

        match byte {
            b'\'' | b'"' => quote = Some(byte),
            b'{' => {
                depth += 1;
                cursor += 1;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                cursor += 1;
            }
            b'=' if depth == 0 => {
                let brace_start = skip_whitespace(source, cursor + 1);
                if bytes.get(brace_start) == Some(&b'{') {
                    if let Some(end) = find_matching_brace(source, brace_start) {
                        entries.push(TableEntry {
                            start: find_entry_start(source, cursor),
                            brace_start,
                            end,
                            end_with_trailing_comma: include_trailing_comma(source, end, table_end),
                        });
                        cursor = end;
                        continue;
                    }
                }
                cursor += 1;
            }
            _ => cursor += 1,
        }
    }

    entries
}

fn find_entry_start(source: &str, equals_index: usize) -> usize {
    source[..equals_index]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0)
}

fn include_trailing_comma(source: &str, end: usize, table_end: usize) -> usize {
    let mut cursor = skip_whitespace(source, end);
    if cursor < table_end && source.as_bytes().get(cursor) == Some(&b',') {
        cursor += 1;
    }
    if cursor < table_end && source.as_bytes().get(cursor) == Some(&b'\r') {
        cursor += 1;
    }
    if cursor < table_end && source.as_bytes().get(cursor) == Some(&b'\n') {
        cursor += 1;
    }
    cursor
}

fn find_immediate_string_assignment(source: &str, key: &str) -> Option<String> {
    let mut index = 0;

    while index < source.len() {
        let relative = source[index..].find(key)?;
        let key_start = index + relative;

        if table_depth_at(source, key_start) != 1 || !is_valid_key_match(source, key_start, key) {
            index = key_start + key.len();
            continue;
        }

        let mut cursor = key_start + key.len();
        if source[..key_start].ends_with("[\"") || source[..key_start].ends_with("['") {
            let quote = source.as_bytes()[key_start - 1];
            if source.as_bytes().get(cursor) != Some(&quote)
                || source.as_bytes().get(cursor + 1) != Some(&b']')
            {
                index = key_start + key.len();
                continue;
            }
            cursor += 2;
        }

        cursor = skip_whitespace(source, cursor);
        if source.as_bytes().get(cursor) != Some(&b'=') {
            index = key_start + key.len();
            continue;
        }

        cursor = skip_whitespace(source, cursor + 1);
        let quote = *source.as_bytes().get(cursor)?;
        if quote != b'\'' && quote != b'"' {
            index = key_start + key.len();
            continue;
        }

        return read_quoted_string(source, cursor, quote);
    }

    None
}

fn read_quoted_string(source: &str, quote_start: usize, quote: u8) -> Option<String> {
    let bytes = source.as_bytes();
    let mut cursor = quote_start + 1;
    let mut value = String::new();

    while cursor < bytes.len() {
        let byte = bytes[cursor];
        if byte == b'\\' {
            let escaped = *bytes.get(cursor + 1)?;
            value.push(escaped as char);
            cursor += 2;
            continue;
        }
        if byte == quote {
            return Some(value);
        }
        value.push(byte as char);
        cursor += 1;
    }

    None
}

fn find_table_assignment(
    source: &str,
    start_at: usize,
    key: &str,
) -> Option<(usize, usize, usize)> {
    let mut index = start_at;

    while index < source.len() {
        let relative = source[index..].find(key)?;
        let key_start = index + relative;

        if !is_valid_key_match(source, key_start, key) {
            index = key_start + key.len();
            continue;
        }

        let mut cursor = key_start + key.len();

        if source[..key_start].ends_with("[\"") || source[..key_start].ends_with("['") {
            let quote = source.as_bytes()[key_start - 1];
            if source.as_bytes().get(cursor) != Some(&quote)
                || source.as_bytes().get(cursor + 1) != Some(&b']')
            {
                index = key_start + key.len();
                continue;
            }
            cursor += 2;
        }

        cursor = skip_whitespace(source, cursor);
        if source.as_bytes().get(cursor) != Some(&b'=') {
            index = key_start + key.len();
            continue;
        }

        cursor = skip_whitespace(source, cursor + 1);
        if source.as_bytes().get(cursor) != Some(&b'{') {
            index = key_start + key.len();
            continue;
        }

        let table_end = find_matching_brace(source, cursor)?;
        let assignment_start = find_assignment_start(source, key_start);

        return Some((assignment_start, cursor, table_end));
    }

    None
}

fn find_immediate_table_assignment(source: &str, key: &str) -> Option<(usize, usize, usize)> {
    let mut index = 0;

    while index < source.len() {
        let relative = source[index..].find(key)?;
        let key_start = index + relative;

        if table_depth_at(source, key_start) != 1 || !is_valid_key_match(source, key_start, key) {
            index = key_start + key.len();
            continue;
        }

        let mut cursor = key_start + key.len();

        if source[..key_start].ends_with("[\"") || source[..key_start].ends_with("['") {
            let quote = source.as_bytes()[key_start - 1];
            if source.as_bytes().get(cursor) != Some(&quote)
                || source.as_bytes().get(cursor + 1) != Some(&b']')
            {
                index = key_start + key.len();
                continue;
            }
            cursor += 2;
        }

        cursor = skip_whitespace(source, cursor);
        if source.as_bytes().get(cursor) != Some(&b'=') {
            index = key_start + key.len();
            continue;
        }

        cursor = skip_whitespace(source, cursor + 1);
        if source.as_bytes().get(cursor) != Some(&b'{') {
            index = key_start + key.len();
            continue;
        }

        let table_end = find_matching_brace(source, cursor)?;
        let assignment_start = find_assignment_start(source, key_start);

        return Some((assignment_start, cursor, table_end));
    }

    None
}

fn is_valid_key_match(source: &str, key_start: usize, key: &str) -> bool {
    let before = source.as_bytes().get(key_start.wrapping_sub(1)).copied();
    let after = source.as_bytes().get(key_start + key.len()).copied();
    let bracketed = source[..key_start].ends_with("[\"") || source[..key_start].ends_with("['");

    if bracketed {
        return true;
    }

    !before.is_some_and(is_identifier_byte) && !after.is_some_and(is_identifier_byte)
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn table_depth_at(source: &str, index: usize) -> usize {
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    let mut cursor = 0usize;
    let mut quote: Option<u8> = None;

    while cursor < index && cursor < bytes.len() {
        let byte = bytes[cursor];

        if let Some(active_quote) = quote {
            if byte == b'\\' {
                cursor += 2;
                continue;
            }

            if byte == active_quote {
                quote = None;
            }

            cursor += 1;
            continue;
        }

        match byte {
            b'\'' | b'"' => quote = Some(byte),
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }

        cursor += 1;
    }

    depth
}

fn skip_whitespace(source: &str, mut cursor: usize) -> usize {
    while source
        .as_bytes()
        .get(cursor)
        .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        cursor += 1;
    }

    cursor
}

fn find_assignment_start(source: &str, key_start: usize) -> usize {
    if source[..key_start].ends_with("[\"") || source[..key_start].ends_with("['") {
        key_start - 2
    } else {
        key_start
    }
}

fn find_matching_brace(source: &str, open_brace: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    let mut cursor = open_brace;
    let mut quote: Option<u8> = None;

    while cursor < bytes.len() {
        let byte = bytes[cursor];

        if let Some(active_quote) = quote {
            if byte == b'\\' {
                cursor += 2;
                continue;
            }

            if byte == active_quote {
                quote = None;
            }

            cursor += 1;
            continue;
        }

        match byte {
            b'\'' | b'"' => quote = Some(byte),
            b'{' => depth += 1,
            b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(cursor + 1);
                }
            }
            _ => {}
        }

        cursor += 1;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::{build_manifest, export_planning_mission};
    use std::path::Path;

    #[test]
    fn removes_red_coalition_and_preserves_blue() {
        let mission = r#"mission = {
  ["coalition"] = {
    ["blue"] = { country = { [1] = { name = "USA" } } },
    ["red"] = { country = { [1] = { name = "Russia", plane = { group = {} } } } },
    ["neutrals"] = { country = {} },
  },
}"#;

        let cleaned = super::replace_coalition(mission, "red", super::EMPTY_RED_COALITION)
            .expect("remove red coalition");

        assert!(cleaned.contains(r#"["blue"] = { country"#));
        assert!(cleaned.contains("name = \"red\""));
        assert!(cleaned.contains("country = {}"));
        assert!(!cleaned.contains("Russia"));
    }

    #[test]
    fn handles_plain_lua_keys() {
        let mission = r#"mission = {
  coalition = {
    blue = { country = { "blue" } },
    red = { country = { "red aircraft" } },
  },
}"#;

        let cleaned = super::replace_coalition(mission, "red", super::EMPTY_RED_COALITION)
            .expect("remove red coalition");

        assert!(cleaned.contains("blue = { country"));
        assert!(!cleaned.contains("red aircraft"));
    }

    #[test]
    fn manifest_contains_planning_export_metadata() {
        let manifest = build_manifest(Path::new("training mission.miz"), "blue", "red")
            .expect("build manifest");

        assert!(manifest.contains("\"formatVersion\": 1"));
        assert!(manifest.contains("\"coalition\": \"blue\""));
        assert!(manifest.contains("\"removedCoalition\": \"red\""));
        assert!(manifest.contains("\"sourceFile\": \"training mission.miz\""));
    }

    #[test]
    fn supports_red_planning_export() {
        let mission = r#"mission = {
  coalition = {
    blue = { country = { "blue aircraft" } },
    red = { country = { "red aircraft" } },
  },
}"#;

        let cleaned = super::replace_coalition(mission, "blue", super::EMPTY_BLUE_COALITION)
            .expect("remove blue coalition");

        assert!(cleaned.contains("red = { country"));
        assert!(!cleaned.contains("blue aircraft"));
    }

    #[test]
    fn rejects_unsupported_export_coalition() {
        let result =
            export_planning_mission(Path::new("missing.miz"), Path::new("output.miz"), "green");

        assert_eq!(
            result,
            Err("Export coalition must be BLUE or RED.".to_string())
        );
    }

    #[test]
    fn detects_aircraft_flights_by_coalition() {
        let mission = r#"mission = {
  coalition = {
    blue = {
      country = {
        [1] = {
          plane = {
            group = {
              [1] = { name = "Viper 1", units = { [1] = { type = "F-16C_50", skill = "Client" } } },
              [2] = {
                name = "Hornet 1",
                route = {
                  points = {
                    [1] = {
                      task = {
                        id = "ComboTask",
                        params = {
                          units = { [1] = { type = "NotAnAircraft", skill = "Client" } },
                          tasks = { [1] = { name = "UAM", type = "WrappedAction", skill = "Player" } },
                        },
                      },
                    },
                  },
                },
                units = { [1] = { type = "FA-18C_hornet", ["skill"] = "Player" } },
              },
              [3] = { name = "Eagle AI", units = { [1] = { type = "F-15C", skill = "Excellent" } } },
            },
          },
        },
      },
    },
    red = {
      country = {
        [1] = {
          helicopter = {
            group = {
              [1] = { name = "Hind 1", units = { [1] = { type = "Mi-24P", skill = "Client" } } },
            },
          },
        },
      },
    },
  },
}"#;

        let flights = super::detect_flights_in_mission(mission);

        assert_eq!(flights.len(), 3);
        assert!(flights.iter().any(|flight| {
            flight.id == "blue|plane|Viper 1" && flight.aircraft_type == "F-16C_50"
        }));
        assert!(flights.iter().any(|flight| {
            flight.id == "blue|plane|Hornet 1" && flight.aircraft_type == "FA-18C_hornet"
        }));
        assert!(!flights
            .iter()
            .any(|flight| flight.id == "blue|plane|Eagle AI"));
        assert!(
            flights
                .iter()
                .any(|flight| flight.id == "red|helicopter|Hind 1"
                    && flight.aircraft_type == "Mi-24P")
        );
    }

    #[test]
    fn keeps_only_selected_flight_in_exported_coalition() {
        let coalition = r#"blue = {
  country = {
    [1] = {
      plane = {
        group = {
          [1] = { name = "Viper 1", units = { [1] = { type = "F-16C_50" } } },
          [2] = { name = "Hornet 1", units = { [1] = { type = "FA-18C_hornet" } } },
        },
      },
    },
  },
}"#;

        let pruned = super::keep_only_selected_flight(coalition, "blue|plane|Hornet 1")
            .expect("prune coalition");

        assert!(pruned.contains("Hornet 1"));
        assert!(pruned.contains("FA-18C_hornet"));
        assert!(!pruned.contains("Viper 1"));
        assert!(!pruned.contains("F-16C_50"));
    }
}
