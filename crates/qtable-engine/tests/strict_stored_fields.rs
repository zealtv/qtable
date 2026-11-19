//! The set format rejects a field it doesn't know, on every stored type.
//!
//! A hand-edited typo must fail on load rather than fall back to a default, and a field added
//! without a version bump must not vanish when an older build opens and resaves a file. The
//! test takes every fixture and example, lets the current writer save it, then plants an
//! unknown field in each struct of the saved text, one at a time, and expects the load to
//! name that field as unknown.
//!
//! When you add a stored type, give it `#[serde(deny_unknown_fields)]` and list it in
//! `STORED_TYPES`. If a fixture holds none of it, add one that does. The type names are those
//! serde reports: a `#[serde(rename = ...)]` shows under its new name.
use qtable_engine::file::{self, FileError};
use ron::Error as Code;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

const UNKNOWN: &str = "zz_unknown";

/// Every struct (and struct variant) of the current set format that some fixture or example
/// reaches, by the name serde reports.
const STORED_TYPES: &[&str] = &[
    "ArgumentRef",
    "AutomationEnvelope",
    "Breakpoint",
    "Clip",
    "ClipFollowActions",
    "ClipSlot",
    "Colour",
    "Destination",
    "Device",
    "EnumOption",
    "Event",
    "EventArgument",
    "EventDef",
    "EventLane",
    "EventRef",
    "FieldDef",
    "FieldRef",
    "FollowActions",
    "FollowMultiplier",
    "KeySource",
    "MappingData",
    "MidiSource",
    "MixerDevice",
    "MixerState",
    "Output",
    "OutputStage",
    "ParameterDef",
    "ParameterRef",
    "Project",
    "ProjectFile",
    "Region",
    "RemoteMapping",
    "Scene",
    "SceneFollowActions",
    "Slot",
    "TargetDevice",
    "TimeSignature",
    "Track",
    "VolumeOutput",
];

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn sources() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for dir in [
        manifest_dir().join("tests/fixtures"),
        manifest_dir().join("../../examples"),
    ] {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "ron") {
                paths.push(path);
            }
        }
    }
    paths.sort();
    assert!(paths.len() >= 15, "fixtures went missing: {paths:?}");
    paths
}

/// What the current writer saves for `path`.
fn saved_current(path: &Path, scratch: &Path) -> String {
    let project = file::load(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    file::save(scratch, &project).unwrap();
    fs::read_to_string(scratch).unwrap()
}

/// Byte offsets just after each `(` that opens a struct, meaning one whose first token is
/// `name:`, each with a key built from what precedes it and the names of its own fields.
/// Strings are skipped.
fn struct_openings(text: &str) -> Vec<(usize, String)> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                i += 1;
                while bytes[i] != b'"' {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
            }
            b'(' => {
                let rest = &text[i + 1..];
                let trimmed = rest.trim_start();
                let name_len = trimmed
                    .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .unwrap_or(0);
                if name_len > 0 && trimmed[name_len..].trim_start().starts_with(':') {
                    let before = text[..i].trim_end();
                    let before = &before[before.len().saturating_sub(24)..];
                    found.push((i + 1, format!("{before}|{}", field_names(rest))));
                }
            }
            _ => {}
        }
        i += 1;
    }
    found
}

/// The names of the fields written directly in the struct whose body starts `rest`.
fn field_names(rest: &str) -> String {
    let mut depth = 0usize;
    let mut names = Vec::new();
    let mut word = String::new();
    let mut chars = rest.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                while let Some(d) = chars.next() {
                    match d {
                        '\\' => {
                            chars.next();
                        }
                        '"' => break,
                        _ => {}
                    }
                }
                word.clear();
            }
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            ':' if depth == 0 && !word.is_empty() => {
                names.push(std::mem::take(&mut word));
            }
            c if c.is_ascii_alphanumeric() || c == '_' => word.push(c),
            _ => word.clear(),
        }
    }
    names.join(",")
}

fn context(text: &str, at: usize) -> String {
    let line = text[..at].matches('\n').count() + 1;
    let lines: Vec<_> = text.lines().skip(line.saturating_sub(4)).take(5).collect();
    format!("line {line}:\n{}", lines.join("\n"))
}

#[test]
fn every_stored_struct_rejects_an_unknown_field() {
    let scratch = std::env::temp_dir().join(format!("qtable-strict-{}.ron", std::process::id()));
    let mut covered = BTreeSet::new();
    let mut accepted = Vec::new();
    // Saved sets repeat the same few shapes thousands of times; plant once per shape.
    let mut seen = BTreeSet::new();
    for source in sources() {
        let saved = saved_current(&source, &scratch);
        for (at, shape) in struct_openings(&saved) {
            if !seen.insert(shape) {
                continue;
            }
            let planted = format!("{}\n{UNKNOWN}: 1,{}", &saved[..at], &saved[at..]);
            fs::write(&scratch, planted).unwrap();
            match file::load(&scratch) {
                Err(FileError::Ron(error)) => match error.code {
                    Code::NoSuchStructField { found, outer, .. } if found == UNKNOWN => {
                        covered.insert(outer.unwrap_or_default());
                    }
                    other => panic!(
                        "{}: expected an unknown-field error, got {other:?}\n{}",
                        source.display(),
                        context(&saved, at)
                    ),
                },
                other => accepted.push(format!(
                    "{}: loaded with an unknown field ({})\n{}",
                    source.display(),
                    other.map(|_| "ok").unwrap_or("another error"),
                    context(&saved, at)
                )),
            }
        }
    }
    let _ = fs::remove_file(&scratch);
    assert!(
        accepted.is_empty(),
        "a stored type accepts an unknown field; add #[serde(deny_unknown_fields)]:\n\n{}",
        accepted.join("\n\n")
    );
    let missing: Vec<_> = STORED_TYPES
        .iter()
        .filter(|name| !covered.contains(**name))
        .collect();
    assert!(
        missing.is_empty(),
        "no fixture or example reaches {missing:?}: add one, or drop the name from STORED_TYPES"
    );
}
