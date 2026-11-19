//! The saved bytes of every track kind stay as they were when `StoredKind` and `TrackKind`
//! became one enum.
//!
//! Each fixture (the frozen files of every older format, the examples and the GUI's fixtures)
//! is loaded, migrated to the current model and saved. The result must equal the golden bytes
//! in `tests/fixtures/saved-bytes/`, which the code before the merge wrote (2026-11-15). A
//! second save after a reload must repeat them. To rewrite the goldens after a deliberate
//! format change, run with `QTABLE_BLESS=1`.
use qtable_engine::file;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every `.ron` file the repository keeps as a project, with the name its golden takes.
fn sources() -> Vec<(String, PathBuf)> {
    let roots = [
        ("engine", manifest().join("tests/fixtures")),
        ("example", manifest().join("../../examples")),
        ("gui", manifest().join("../qtable-gui/tests/fixtures")),
    ];
    let mut found = Vec::new();
    for (prefix, root) in roots {
        for entry in fs::read_dir(&root).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "ron") {
                let stem = path.file_stem().unwrap().to_string_lossy();
                found.push((format!("{prefix}-{stem}.ron"), path));
            }
        }
    }
    found.sort();
    found
}

fn saved_bytes(project: &qtable_engine::model::Project, scratch: &Path) -> Vec<u8> {
    file::save(scratch, project).unwrap();
    fs::read(scratch).unwrap()
}

#[test]
fn saved_bytes_match_the_goldens_for_every_fixture() {
    let goldens = manifest().join("tests/fixtures/saved-bytes");
    let scratch_dir =
        std::env::temp_dir().join(format!("qtable-kind-bytes-{}", std::process::id()));
    fs::create_dir_all(&scratch_dir).unwrap();
    let bless = std::env::var_os("QTABLE_BLESS").is_some();
    if bless {
        fs::create_dir_all(&goldens).unwrap();
    }
    let sources = sources();
    assert!(sources.len() >= 20, "fixtures went missing: {sources:?}");
    let mut kinds = [false; 3];
    for (name, path) in sources {
        let project = file::load(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        let scratch = scratch_dir.join(&name);
        let first = saved_bytes(&project, &scratch);
        let again = saved_bytes(&file::load(&scratch).unwrap(), &scratch);
        assert_eq!(first, again, "{name}: a reload changed the saved bytes");
        let text = String::from_utf8(first.clone()).unwrap();
        for (seen, kind) in kinds.iter_mut().zip(["Group", "Output", "Main"]) {
            *seen |= text.contains(&format!("kind: {kind},"));
        }
        assert!(
            !text.contains("kind: Track"),
            "{name}: a plain track must omit its kind"
        );
        let golden = goldens.join(&name);
        if bless {
            fs::write(&golden, &first).unwrap();
        } else {
            let expected = fs::read(&golden).unwrap_or_else(|e| panic!("{name}: no golden: {e}"));
            assert!(
                first == expected,
                "{name}: saved bytes differ from the golden"
            );
        }
    }
    assert_eq!(
        kinds, [true; 3],
        "the fixtures must cover Group, Output and Main"
    );
    fs::remove_dir_all(&scratch_dir).ok();
}
