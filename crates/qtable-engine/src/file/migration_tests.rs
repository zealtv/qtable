use super::*;

/// Stop the older file chains before format 7 replaces hit paths with Target refs.
fn load_v6(path: &Path) -> Result<v6::Project, FileError> {
    #[derive(Deserialize)]
    struct Header {
        version: u32,
    }
    #[derive(Deserialize)]
    struct OlderFile<T> {
        project: T,
    }
    let source = fs::read_to_string(path).map_err(FileError::Io)?;
    let options =
        ron::Options::default().with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME);
    let header: Header = options.from_str(&source).map_err(FileError::Ron)?;
    Ok(match header.version {
        1 => {
            let file: OlderFile<legacy::Project> =
                options.from_str(&source).map_err(FileError::Ron)?;
            let v2 = legacy::migrate(file.project).map_err(FileError::Model)?;
            v4::migrate(v3::migrate(v2::migrate(v2)))
        }
        2 => {
            let file: OlderFile<v2::Project> = options.from_str(&source).map_err(FileError::Ron)?;
            v4::migrate(v3::migrate(v2::migrate(file.project)))
        }
        3 => {
            let file: OlderFile<v3::Project> = options.from_str(&source).map_err(FileError::Ron)?;
            v4::migrate(v3::migrate(file.project))
        }
        4 => {
            let file: OlderFile<v4::Project> = options.from_str(&source).map_err(FileError::Ron)?;
            v4::migrate(file.project)
        }
        5 => {
            let file: OlderFile<v5::Project> = options.from_str(&source).map_err(FileError::Ron)?;
            v5::migrate(file.project)
        }
        6 => {
            let file: OlderFile<v6::Project> = options.from_str(&source).map_err(FileError::Ron)?;
            file.project
        }
        version => return Err(FileError::UnsupportedVersion(version)),
    })
}

#[test]
fn every_legacy_fixture_hit_keeps_its_address_after_migration() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for directory in [root.join("tests/fixtures")] {
        let mut files: Vec<_> = fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "ron"))
            .collect();
        files.sort();
        assert!(!files.is_empty(), "{}", directory.display());
        let mut hits = 0;
        for path in files {
            let old = match load_v6(&path) {
                Ok(old) => old,
                Err(FileError::UnsupportedVersion(version)) if version >= 7 => continue,
                Err(error) => panic!("{}: {error}", path.display()),
            };
            let project = load(&path).unwrap();
            let destinations: Vec<_> = project.configured_destinations().collect();
            assert!(!destinations.is_empty(), "{}", path.display());
            let mut file_hits = 0;
            for track in old.tracks.iter().chain(&old.outputs).chain([&old.main]) {
                let migrated = project.track(track.id).unwrap();
                assert_eq!(
                    migrated.slots.len(),
                    track.slots.len(),
                    "{}",
                    path.display()
                );
                for (slot, (old, new)) in track.slots.iter().zip(&migrated.slots).enumerate() {
                    let context = format!("{} track {:?} slot {slot}", path.display(), track.id);
                    assert_eq!(old.clip.is_some(), new.clip.is_some(), "{context}");
                    let Some(clip) = &old.clip else {
                        continue;
                    };
                    let migrated = new.clip.as_ref().unwrap();
                    assert_eq!(migrated.id, clip.id, "{context}");
                    assert_eq!(migrated.events.len(), clip.events.len(), "{context}");
                    for (hit, (old, new)) in clip.events.iter().zip(&migrated.events).enumerate() {
                        assert_eq!(new.position, old.position, "{context} hit {hit}");
                        assert_eq!(new.arguments, old.arguments, "{context} hit {hit}");
                        let piece = project.event_path(track.id, &new.definition).unwrap();
                        // Compare every destination, including ones the track doesn't route to.
                        for output in &destinations {
                            let before = project
                                .resolve_event_address(track.id, output.id, &old.address)
                                .unwrap();
                            let after = project
                                .resolve_event_address(track.id, output.id, &piece)
                                .unwrap();
                            assert_eq!(after, before, "{context} hit {hit} output {:?}", output.id);
                        }
                        file_hits += 1;
                    }
                }
            }
            println!("{}: {file_hits} hits", path.display());
            hits += file_hits;
        }
        assert!(hits > 0, "{}", directory.display());
    }
}
