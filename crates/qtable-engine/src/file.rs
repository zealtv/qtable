//! Versioned RON project persistence.

use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::model::Project;

pub const FORMAT_VERSION: u32 = 12;
mod legacy;
mod v10;
mod v11;
mod v2;
mod v3;
mod v4;
mod v5;
mod v6;
mod v7;
mod v8;
mod v9;

#[cfg(test)]
mod migration_tests;
#[cfg(test)]
mod v7_tests;

#[derive(Debug)]
pub enum FileError {
    Io(std::io::Error),
    Ron(ron::error::SpannedError),
    Model(crate::model::ModelError),
    UnsupportedVersion(u32),
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "project file I/O error: {error}"),
            Self::Ron(error) => write!(f, "invalid project RON: {error}"),
            Self::Model(error) => write!(f, "invalid project: {error}"),
            Self::UnsupportedVersion(version) => write!(
                f,
                "unsupported project format version {version}; this build supports version {FORMAT_VERSION}"
            ),
        }
    }
}

impl std::error::Error for FileError {}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectFile {
    version: u32,
    project: Project,
}

/// A project read at its stored format, not yet walked forward to the current model.
///
/// Adding a new format means: one new variant, one `read` arm, and one `upgrade` arm
/// from the previous latest version.
///
/// Bump checklist, in order:
/// 1. Before reshaping or adding to a stored type, copy its old shape into the previous
///    format's `file/vN.rs`. Frozen readers (v8 to v11 so far) deserialise some live model types;
///    if the live type changes first, every older format silently reads the new shape.
/// 2. Bump `FORMAT_VERSION` for any new stored field or enum variant, defaulted or not.
/// 3. Capture a fixture from the previous writer before the bump, and add the new golden.
/// 4. Give each new stored type `#[serde(deny_unknown_fields)]` and list it in `STORED_TYPES`
///    in `tests/strict_stored_fields.rs`.
/// 5. Note the format in CHANGELOG.md.
///
/// The policy for readers is in `docs/file-format.md`.
enum Loaded {
    V1(legacy::Project),
    V2(v2::Project),
    V3(v3::Project),
    V4(v4::Project),
    V5(v5::Project),
    V6(v6::Project),
    V7(v7::Project),
    V8(v8::Project),
    V9(v9::Project),
    V10(v10::Project),
    V11(v11::Project),
    Current(Project),
}

/// Deserialise the stored project for `version`, without migrating it.
fn read(version: u32, source: &str, options: &ron::Options) -> Result<Loaded, FileError> {
    macro_rules! file_of {
        ($ty:ty) => {{
            #[derive(Deserialize)]
            struct File {
                project: $ty,
            }
            options
                .from_str::<File>(source)
                .map_err(FileError::Ron)?
                .project
        }};
    }
    Ok(match version {
        1 => Loaded::V1(file_of!(legacy::Project)),
        2 => Loaded::V2(file_of!(v2::Project)),
        3 => Loaded::V3(file_of!(v3::Project)),
        4 => Loaded::V4(file_of!(v4::Project)),
        5 => Loaded::V5(file_of!(v5::Project)),
        6 => Loaded::V6(file_of!(v6::Project)),
        7 => Loaded::V7(file_of!(v7::Project)),
        8 => Loaded::V8(file_of!(v8::Project)),
        9 => Loaded::V9(file_of!(v9::Project)),
        10 => Loaded::V10(file_of!(v10::Project)),
        11 => Loaded::V11(file_of!(v11::Project)),
        FORMAT_VERSION => Loaded::Current(
            options
                .from_str::<ProjectFile>(source)
                .map_err(FileError::Ron)?
                .project,
        ),
        version => return Err(FileError::UnsupportedVersion(version)),
    })
}

/// Walk one step toward the current model, in the chain's existing, irregular shape
/// (v4 and v5 both migrate straight to v6, skipping v5 from v4).
fn upgrade(loaded: Loaded) -> Result<Loaded, FileError> {
    Ok(match loaded {
        Loaded::V1(project) => Loaded::V2(legacy::migrate(project).map_err(FileError::Model)?),
        Loaded::V2(project) => Loaded::V3(v2::migrate(project)),
        Loaded::V3(project) => Loaded::V4(v3::migrate(project)),
        Loaded::V4(project) => Loaded::V6(v4::migrate(project)),
        Loaded::V5(project) => Loaded::V6(v5::migrate(project)),
        Loaded::V6(project) => Loaded::V7(v6::migrate(project)),
        Loaded::V7(project) => Loaded::V8(v7::migrate(project)),
        Loaded::V8(project) => Loaded::V9(v8::migrate(project)),
        Loaded::V9(project) => Loaded::V10(v9::migrate(project)),
        Loaded::V10(project) => Loaded::V11(v10::migrate(project).map_err(FileError::Model)?),
        Loaded::V11(project) => Loaded::Current(v11::migrate(project)),
        current @ Loaded::Current(_) => current,
    })
}

/// Load a versioned RON project, retaining newly invalid address characters for repair.
pub fn load(path: impl AsRef<Path>) -> Result<Project, FileError> {
    let source = fs::read_to_string(path).map_err(FileError::Io)?;
    #[derive(Deserialize)]
    struct Header {
        version: u32,
    }
    let options =
        ron::Options::default().with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME);
    let header: Header = options.from_str(&source).map_err(FileError::Ron)?;
    let mut loaded = read(header.version, &source, &options)?;
    let mut project = loop {
        match loaded {
            Loaded::Current(project) => break project,
            other => loaded = upgrade(other)?,
        }
    };
    project.renumber_colliding_outputs();
    project.normalise_main();
    project.fill_unset_colours();
    project.seed_event_values();
    project.validate_stored().map_err(FileError::Model)?;
    Ok(project)
}

/// Validate stored invariants and atomically save as pretty, hand-editable RON.
/// Newly invalid address characters are preserved, so an old set can be repaired gradually.
/// A symlinked project file is saved through the link, so the link survives.
/// A stale `<path>.tmp` is removed before writing, even if it is a user's file. On Unix, directory sync errors
/// are reported even though the replacement is already visible at that point.
pub fn save(path: impl AsRef<Path>, project: &Project) -> Result<(), FileError> {
    project.validate_stored().map_err(FileError::Model)?;
    let file = ProjectFile {
        version: FORMAT_VERSION,
        project: project.clone(),
    };
    let config =
        ron::ser::PrettyConfig::default().extensions(ron::extensions::Extensions::IMPLICIT_SOME);
    let source = ron::ser::to_string_pretty(&file, config)
        .expect("serializing the project model to RON cannot fail");
    // Renaming over a symlink would replace the link itself, so write to its target.
    // A missing path (a new project) cannot be resolved and is used as given.
    let path = path.as_ref();
    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    replace_file(&target, |file| file.write_all(source.as_bytes())).map_err(FileError::Io)
}

/// Keep the destination intact until the complete replacement is synced.
/// The writer callback also permits deterministic partial-write failure tests.
fn replace_file(
    path: &Path,
    write: impl FnOnce(&mut fs::File) -> io::Result<()>,
) -> io::Result<()> {
    let mut temporary = path.as_os_str().to_os_string();
    temporary.push(".tmp");
    let temporary = std::path::PathBuf::from(temporary);
    // qtable has one writer per project, so an existing temporary file is stale.
    // Unlink it rather than opening it, so symlinks are never followed.
    match fs::remove_file(&temporary) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = (|| {
        // Replacement should retain an existing project's access permissions.
        match fs::metadata(path) {
            Ok(metadata) => file.set_permissions(metadata.permissions())?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        write(&mut file)?;
        file.sync_all()
    })();
    // Close before renaming or cleaning up, including on platforms that forbid
    // renaming an open file.
    drop(file);
    let result = result.and_then(|()| fs::rename(&temporary, path));
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result?;
    #[cfg(unix)]
    {
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        // File sync does not make the rename durable. Report directory sync
        // failures so callers know durability could not be confirmed.
        fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    fn path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("qtable-{name}-{}.ron", std::process::id()))
    }

    fn sample() -> Project {
        let mut project = Project::default();
        project
            .outputs
            .push(Track::new_output(TrackId(201), "main", "127.0.0.1", 9000).with_path("/main"));
        project.scenes = (0..3)
            .map(|id| Scene {
                id: SceneId(id),
                name: format!("scene {id}"),
                follow_actions: SceneFollowActions::default(),
            })
            .collect();
        let mut track = Track::new(TrackId(1), "track", 3);
        let mut clip = Clip::new(
            ClipId(1),
            "cue",
            BeatTime::from_beats(4).unwrap(),
            crate::model::LIVE_CLIP_PALETTE[0],
        )
        .unwrap();
        let cue = OscAddress::Relative("/cue".into());
        clip.events.push(
            track.hit(BeatTime::ZERO, cue, vec![OscArgument::Bool(true)], || {
                DeviceId(1)
            }),
        );
        track.slots[0].clip = Some(clip);
        track.slots[1].has_stop_button = false;
        project.tracks.push(track);
        project
    }

    #[test]
    fn round_trip_preserves_model_and_empty_slot_structure() {
        let file = path("round-trip");
        let project = sample();
        save(&file, &project).unwrap();
        let loaded = load(&file).unwrap();
        save(&file, &loaded).unwrap();
        assert_eq!(load(&file).unwrap(), project);
        assert_eq!(loaded.tracks[0].slots.len(), 3);
        assert!(loaded.tracks[0].slots[1].clip.is_none());
        assert!(!loaded.tracks[0].slots[1].has_stop_button);
        let _ = fs::remove_file(file);
    }

    #[test]
    fn event_lanes_round_trip_and_clips_without_lanes_save_as_before() {
        let file = path("lanes");
        let mut project = sample();
        save(&file, &project).unwrap();
        assert!(!fs::read_to_string(&file).unwrap().contains("lanes"));
        let empty = OscAddress::Relative("/empty".into());
        let definition = project.ensure_event(TrackId(1), empty, &[]).unwrap();
        project.tracks[0].slots[0].clip.as_mut().unwrap().lanes =
            vec![crate::model::Lane::Events(EventLane { definition })];
        save(&file, &project).unwrap();
        assert_eq!(load(&file).unwrap(), project);
        let _ = fs::remove_file(file);
    }

    #[test]
    fn partial_write_failure_keeps_old_project_and_cleans_up_temporary() {
        let file = path("partial-write");
        let original = sample();
        save(&file, &original).unwrap();
        let before = fs::read(&file).unwrap();

        let error = replace_file(&file, |temporary| {
            temporary.write_all(b"incomplete replacement")?;
            Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "injected write failure",
            ))
        })
        .unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::WriteZero);
        assert_eq!(fs::read(&file).unwrap(), before);
        assert_eq!(load(&file).unwrap(), original);
        assert!(!file.with_extension("ron.tmp").exists());
        // Cleanup permits the next save to succeed.
        let replacement = Project::default();
        save(&file, &replacement).unwrap();
        assert_eq!(load(&file).unwrap(), replacement);
        fs::remove_file(file).unwrap();
    }

    #[test]
    fn stale_temporary_file_is_replaced_and_save_succeeds() {
        let file = path("stale-temporary");
        let temporary = file.with_extension("ron.tmp");
        let original = sample();
        save(&file, &original).unwrap();
        fs::write(&temporary, b"incomplete save left by a crash").unwrap();

        let replacement = Project::default();
        save(&file, &replacement).unwrap();
        assert_eq!(load(&file).unwrap(), replacement);
        assert!(!temporary.exists());
        fs::remove_file(file).unwrap();
    }

    #[cfg(unix)]
    #[cfg(unix)]
    #[test]
    fn saving_through_a_symlink_keeps_the_link_and_updates_its_target() {
        let target = path("link-target");
        let link = path("link");
        let _ = fs::remove_file(&link);
        save(&target, &sample()).unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let mut changed = sample();
        changed.tempo = Tempo::new(97.0).unwrap();
        save(&link, &changed).unwrap();
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(load(&target).unwrap(), changed);
        fs::remove_file(&link).unwrap();
        fs::remove_file(&target).unwrap();
    }

    #[test]
    fn stale_temporary_symlink_is_removed_without_changing_its_target() {
        let file = path("stale-symlink");
        let temporary = file.with_extension("ron.tmp");
        let target = path("symlink-target");
        fs::write(&target, b"keep this file").unwrap();
        std::os::unix::fs::symlink(&target, &temporary).unwrap();

        let project = sample();
        save(&file, &project).unwrap();

        assert_eq!(load(&file).unwrap(), project);
        assert_eq!(fs::read(&target).unwrap(), b"keep this file");
        assert!(fs::symlink_metadata(temporary).is_err());
        fs::remove_file(target).unwrap();
        fs::remove_file(file).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn save_relative_filename_syncs_current_directory() {
        let file =
            std::path::PathBuf::from(format!(".qtable-relative-save-{}.ron", std::process::id()));
        let project = sample();
        let saved = save(&file, &project);
        let loaded = load(&file);
        let leftover = file.with_extension("ron.tmp").exists();
        let _ = fs::remove_file(&file);
        saved.unwrap();
        assert_eq!(loaded.unwrap(), project);
        assert!(!leftover);
    }

    #[test]
    fn failed_rename_cleans_up_temporary_and_keeps_destination() {
        let directory = path("rename-failure");
        fs::create_dir(&directory).unwrap();
        let sentinel = directory.join("keep.ron");
        fs::write(&sentinel, b"existing directory contents").unwrap();

        assert!(matches!(
            save(&directory, &Project::default()),
            Err(FileError::Io(_))
        ));
        assert_eq!(fs::read(&sentinel).unwrap(), b"existing directory contents");
        assert!(!directory.with_extension("ron.tmp").exists());
        fs::remove_file(sentinel).unwrap();
        fs::remove_dir(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn replacing_project_preserves_access_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let file = path("permissions");
        save(&file, &sample()).unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();

        save(&file, &Project::default()).unwrap();

        assert_eq!(
            fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(load(&file).unwrap(), Project::default());
        fs::remove_file(file).unwrap();
    }

    #[test]
    fn examples_load_and_validate() {
        for file in [
            "../../examples/generic-receiver.ron",
            "../../examples/bopos.ron",
            "../../examples/follow-demo.ron",
            "../../examples/grid-demo.ron",
        ] {
            let project =
                load(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(file)).unwrap();
            project.validate().unwrap();
        }
    }

    #[test]
    fn migrated_format_1_examples_load_and_validate() {
        for name in ["generic-receiver", "bopos", "follow-demo", "grid-demo"] {
            let file = format!("tests/fixtures/v1-example-{name}.ron");
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(file);
            assert!(fs::read_to_string(&path).unwrap().contains("version: 1,"));
            load(&path).unwrap().validate().unwrap();
        }
    }

    #[test]
    fn examples_are_current_format_with_named_devices_and_entries() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
        let mut seen = 0;
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "ron") {
                continue;
            }
            seen += 1;
            let text = fs::read_to_string(&path).unwrap();
            let shown = path.display();
            assert!(
                text.contains(&format!("version: {FORMAT_VERSION},")),
                "{shown} is not saved as format {FORMAT_VERSION}"
            );
            let project = load(&path).unwrap();
            for track in project.tracks.iter() {
                for device in &track.devices {
                    assert_ne!(device.name, "Target", "{shown}: track {:?}", track.name);
                    let DeviceKind::Target(target) = &device.kind;
                    for event in &target.events {
                        assert!(
                            !event.name.is_empty(),
                            "{shown}: unnamed event {:?} on {:?}",
                            event.key,
                            track.name
                        );
                    }
                }
            }
        }
        assert_eq!(seen, 4);
    }

    #[test]
    fn rejects_unknown_format_version() {
        let file = path("wrong-version");
        let unsupported = ProjectFile {
            version: FORMAT_VERSION + 1,
            project: Project::default(),
        };
        fs::write(&file, ron::ser::to_string(&unsupported).unwrap()).unwrap();
        assert!(matches!(
            load(&file),
            Err(FileError::UnsupportedVersion(version)) if version == FORMAT_VERSION + 1
        ));
        let _ = fs::remove_file(file);
    }

    #[test]
    fn unsupported_versions_are_refused_by_header_before_the_body_is_read() {
        // A reader older than a format sees only the version, even when the body holds
        // shapes it could not validate (nested groups under format 11, for a 10 reader).
        let file = path("header-first");
        let future = FORMAT_VERSION + 1;
        fs::write(
            &file,
            format!(
                "(version: {future}, project: (tracks: [Track(group: Nested(Unknown))], future: true))"
            ),
        )
        .unwrap();
        let error = load(&file).unwrap_err();
        assert!(matches!(error, FileError::UnsupportedVersion(version) if version == future));
        assert_eq!(
            error.to_string(),
            format!(
                "unsupported project format version {future}; this build supports version {FORMAT_VERSION}"
            )
        );
        let _ = fs::remove_file(file);
    }

    #[test]
    fn reads_handwritten_defaults_and_beat_positions() {
        let file = path("handwritten");
        fs::write(
            &file,
            r#"#![enable(implicit_some)]
(
    version: 1,
    project: Project(
        destinations: [Destination(id: DestinationId(1), name: "local", host: "127.0.0.1", port: 9000)],
        targets: [Target(id: TargetId(1), name: "main", destination: DestinationId(1))],
        scenes: [
            Scene(id: SceneId(1), name: "Only row"),
            Scene(id: SceneId(2), name: "Gap row"),
        ],
        tracks: [Track(id: TrackId(1), name: "Only track", target: TargetId(1), slots: [
            Slot(clip: Clip(id: ClipId(1), name: "Cue", length: 1.0, events: [
                Event(position: 0.5, address: Relative("/cue")),
            ])),
            Slot(),
        ])],
    ),
)"#,
        )
        .unwrap();

        let project = load(&file).unwrap();
        let track = &project.tracks[0];
        let clip = track.slots[0].clip.as_ref().unwrap();
        assert_eq!(project.tempo.bpm(), 120.0);
        assert_eq!(track.route, Route::single(project.outputs[0].id));
        assert_eq!(track.colour, LIVE_CLIP_PALETTE[0]);
        assert_eq!(track.delay_ms, 0.0);
        assert_eq!(clip.length(), BeatTime::from_beats(1).unwrap());
        assert!(clip.looping);
        assert_eq!(clip.colour, track.colour);
        assert_eq!(clip.events[0].position, BeatTime(480));
        assert!(clip.events[0].arguments.is_empty());
        assert_eq!(track.slots[1], Slot::default());
        assert_eq!(
            ron::from_str::<BeatTime>("0.333333333").unwrap(),
            BeatTime(320)
        );
        let _ = fs::remove_file(file);
    }
}
