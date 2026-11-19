//! Compare against format 6's wire shape, independently of the new model storage.
use qtable_engine::file;
use std::fs;

fn mixer_section(source: &str) -> &str {
    let start = source.find("mixer_device: ").unwrap() + "mixer_device: ".len();
    let mut depth = 0;
    for (index, ch) in source[start..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return &source[start..=start + index];
                }
            }
            _ => {}
        }
    }
    panic!("unclosed mixer section")
}

// Ignore layout whitespace, preserving every byte inside string values.
fn normalized(text: &str) -> String {
    let (mut quoted, mut escaped) = (false, false);
    text.chars()
        .filter(|&ch| {
            let keep = quoted || !ch.is_whitespace();
            if escaped {
                escaped = false;
            } else if quoted && ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = !quoted;
            }
            keep
        })
        .collect()
}

#[test]
fn format_6_nondefault_mixer_saves_with_identical_wire_serialization() {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v6-mixer.ron");
    let source = fs::read_to_string(fixture).unwrap();
    let project = file::load(fixture).unwrap();
    assert_eq!(
        project.main.mixer_device.volume.output,
        qtable_engine::model::ParameterOutput {
            enabled: true,
            send_mode: qtable_engine::model::SendMode::OnRelease,
            address: "/gain".into(),
            mapping: project.main.mixer_device.volume.output.mapping,
            ..Default::default()
        }
    );
    assert_eq!(file::FORMAT_VERSION, 12);
    assert_eq!(project.main.mixer_device.volume.value, 0.375);
    let path = std::env::temp_dir().join(format!(
        "qtable-parameter-format-{}.ron",
        std::process::id()
    ));
    file::save(&path, &project).unwrap();
    let saved = fs::read_to_string(&path).unwrap();
    assert!(!saved.contains("stream_rate"));
    assert_eq!(
        normalized(mixer_section(&saved)),
        normalized(mixer_section(&source))
    );
    assert_eq!(file::load(&path).unwrap(), project);
    file::save(&path, &project).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), saved);
    fs::remove_file(path).unwrap();
}
