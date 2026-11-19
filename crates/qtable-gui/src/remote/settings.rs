//! Per-machine input preferences; never part of a project or its dirty revision.
//!
//! The file reads `(version: 1, midi: (remote: { "Port name": true }))`. Each kind of input gets
//! its own section, so OSC input and theming can join without renaming anything. A build reads
//! its own version and refuses a newer one, never overwriting what it could not read.
//!
//! The first builds stored an unversioned `(remote: {...})` in `settings.ron`. Newer builds
//! read that shape once, when `config.ron` does not exist yet, and write only `config.ron`. They
//! never touch `settings.ron`, so a build from before the version keeps working beside them.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{fs, io};

/// The settings format this build reads and writes.
const VERSION: u32 = 1;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct Settings {
    pub midi: Midi,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Midi {
    /// A MIDI input port's name to its Remote switch.
    #[serde(default)]
    pub remote: BTreeMap<String, bool>,
}

/// The file as stored: the version beside one section per kind of input.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    version: u32,
    #[serde(default)]
    midi: Midi,
}

/// Only the version, read first so a newer file's unknown sections cannot hide it.
#[derive(Deserialize)]
struct Header {
    version: u32,
}

/// The unversioned shape of the first builds.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Unversioned {
    remote: BTreeMap<String, bool>,
}

/// Where settings are read and written.
pub(super) struct Location {
    pub current: PathBuf,
    /// The unversioned file, read once while `current` does not exist.
    pub legacy: Option<PathBuf>,
}

pub(super) fn location() -> Result<Location, String> {
    // A separate path lets unattended MIDI captures leave real preferences alone.
    if let Some(path) = std::env::var_os("QTABLE_SETTINGS_PATH") {
        return Ok(Location {
            current: path.into(),
            legacy: None,
        });
    }
    let root = if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
    };
    let directory = root
        .map(|root| root.join("qtable"))
        .ok_or("MIDI settings location unavailable")?;
    Ok(Location {
        current: directory.join("config.ron"),
        legacy: Some(directory.join("settings.ron")),
    })
}

fn read(path: &Path) -> Result<Option<String>, String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("Could not read MIDI settings: {error}")),
    }
}

fn unreadable(error: impl std::fmt::Display) -> String {
    format!("Could not read MIDI settings: {error}")
}

fn parse(text: &str) -> Result<Settings, String> {
    let header: Header = ron::from_str(text).map_err(unreadable)?;
    match header.version {
        version if version > VERSION => Err(
            "MIDI settings were written by a newer qtable; changes apply for this session only"
                .into(),
        ),
        VERSION => {
            let stored: Stored = ron::from_str(text).map_err(unreadable)?;
            Ok(Settings { midi: stored.midi })
        }
        _ => Err(unreadable("unknown settings version")),
    }
}

pub(super) fn load(location: &Location) -> Result<Settings, String> {
    if let Some(text) = read(&location.current)? {
        return parse(&text);
    }
    let Some(text) = location.legacy.as_deref().map(read).transpose()?.flatten() else {
        return Ok(Settings::default());
    };
    let old: Unversioned = ron::from_str(&text).map_err(unreadable)?;
    Ok(Settings {
        midi: Midi { remote: old.remote },
    })
}

pub(super) fn save(path: &Path, settings: &Settings) -> Result<(), String> {
    let save = || -> Result<(), Box<dyn std::error::Error>> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        let stored = Stored {
            version: VERSION,
            midi: settings.midi.clone(),
        };
        let text = ron::ser::to_string_pretty(&stored, ron::ser::PrettyConfig::default())?;
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let result = (|| -> io::Result<()> {
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
            fs::rename(&temporary, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result?;
        Ok(())
    };
    save().map_err(|error| format!("Could not save MIDI settings: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh directory holding the two files, removed on drop.
    struct Dir(PathBuf);
    impl Dir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "qtable-settings-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn location(&self) -> Location {
            Location {
                current: self.0.join("config.ron"),
                legacy: Some(self.0.join("settings.ron")),
            }
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn with_port(name: &str, on: bool) -> Settings {
        let mut settings = Settings::default();
        settings.midi.remote.insert(name.into(), on);
        settings
    }

    #[test]
    fn preferences_survive_restart_and_missing_ports() {
        let dir = Dir::new("round-trip");
        let location = dir.location();
        let mut settings = load(&location).unwrap();
        assert!(settings.midi.remote.is_empty());
        settings
            .midi
            .remote
            .insert("Disconnected Controller".into(), true);
        save(&location.current, &settings).unwrap();
        assert_eq!(load(&location).unwrap(), settings);
        settings
            .midi
            .remote
            .insert("Disconnected Controller".into(), false);
        save(&location.current, &settings).unwrap();
        assert_eq!(load(&location).unwrap(), settings);
    }

    #[test]
    fn saved_file_names_its_version_and_a_midi_section() {
        let dir = Dir::new("shape");
        let location = dir.location();
        save(&location.current, &with_port("Pad", true)).unwrap();
        let text = fs::read_to_string(&location.current).unwrap();
        assert!(text.contains("version: 1"), "{text}");
        assert!(text.contains("midi: ("), "{text}");
        assert!(text.contains("remote: {"), "{text}");
    }

    #[test]
    fn the_unversioned_shape_migrates_and_its_file_stays_for_older_builds() {
        let dir = Dir::new("migrate");
        let location = dir.location();
        let old = "(remote: {\"Pad\": true, \"Keys\": false})";
        fs::write(location.legacy.as_ref().unwrap(), old).unwrap();
        let mut settings = load(&location).unwrap();
        assert_eq!(settings.midi.remote.get("Pad"), Some(&true));
        assert_eq!(settings.midi.remote.get("Keys"), Some(&false));
        // Reading writes nothing; a change writes the new file and leaves the old one alone.
        assert!(!location.current.exists());
        settings.midi.remote.insert("Keys".into(), true);
        save(&location.current, &settings).unwrap();
        assert_eq!(
            fs::read_to_string(location.legacy.as_ref().unwrap()).unwrap(),
            old
        );
        assert_eq!(load(&location).unwrap(), settings);
    }

    #[test]
    fn the_new_file_wins_once_it_exists() {
        let dir = Dir::new("precedence");
        let location = dir.location();
        fs::write(
            location.legacy.as_ref().unwrap(),
            "(remote: {\"Pad\": true})",
        )
        .unwrap();
        save(&location.current, &with_port("Pad", false)).unwrap();
        assert_eq!(load(&location).unwrap(), with_port("Pad", false));
    }

    #[test]
    fn a_newer_version_is_refused_and_left_intact() {
        let dir = Dir::new("newer");
        let location = dir.location();
        // A newer build may add sections this one does not know.
        let newer = "(version: 2, midi: (remote: {\"Pad\": true}), osc: (port: 9000))";
        fs::write(&location.current, newer).unwrap();
        let error = load(&location).unwrap_err();
        assert!(error.contains("written by a newer qtable"), "{error}");
        assert_eq!(fs::read_to_string(&location.current).unwrap(), newer);
    }

    #[test]
    fn unreadable_settings_are_reported_and_left_intact() {
        let dir = Dir::new("invalid");
        let location = dir.location();
        for (file, text) in [
            (&location.current, "(unknown: true)"),
            (&location.current, "(version: 1, midi: (typo: {}))"),
            (&location.current, "(remote: {\"Pad\": true})"),
            (&location.current, "(version: 0)"),
            (location.legacy.as_ref().unwrap(), "(unknown: true)"),
        ] {
            let _ = fs::remove_file(&location.current);
            fs::write(file, text).unwrap();
            assert!(
                load(&location)
                    .unwrap_err()
                    .starts_with("Could not read MIDI settings:"),
                "{text}"
            );
            assert_eq!(fs::read_to_string(file).unwrap(), text);
        }
    }

    #[test]
    fn settings_that_could_not_be_read_are_not_saved_over() {
        use super::super::State;
        let mut state = State::default();
        // `start` leaves the path unset when loading fails, so nothing is written.
        assert!(state.set_remote("Pad".into(), true).is_err());
    }
}
