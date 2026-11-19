use qtable_engine::edit::{self, ProjectEdit, TargetEdit};
use qtable_engine::engine::{Command, Engine};
use qtable_engine::file;
use qtable_engine::model::*;
use std::net::UdpSocket;
use std::time::Duration;

fn receive_hit(mut project: Project) -> Vec<u8> {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    project.outputs[0].output.as_mut().unwrap().destination.port =
        socket.local_addr().unwrap().port();
    let mut engine = Engine::new(project).unwrap();
    engine
        .execute(Command::Edit(ProjectEdit::RenameTrack {
            track: TrackId(1),
            name: "Renamed in engine".into(),
        }))
        .unwrap();
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        })
        .unwrap();
    let mut bytes = vec![0; 65_535];
    let count = socket.recv(&mut bytes).unwrap();
    bytes.truncate(count);
    bytes
}

#[test]
fn legacy_seventeen_argument_hits_survive_edits_and_save_reload_on_the_wire() {
    let expected = rosc::encoder::encode(&rosc::OscPacket::Message(rosc::OscMessage {
        addr: "/cue".into(),
        args: (0..17).map(rosc::OscType::Int).collect(),
    }))
    .unwrap();
    for version in [2, 6] {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("tests/fixtures/v{version}-seventeen-arguments.ron"));
        let mut project = file::load(fixture).unwrap();
        let hit = &project.tracks[0].slots[0].clip.as_ref().unwrap().events[0];
        let reference = hit.definition.clone();
        assert_eq!(
            hit.arguments,
            (0..17).map(OscArgument::Int).collect::<Vec<_>>()
        );
        let (_, event) = project.tracks[0].event_def(&reference).unwrap();
        assert_eq!(event.fields.len(), 17);
        assert_eq!(receive_hit(project.clone()), expected);
        edit::apply(
            &mut project,
            ProjectEdit::RenameTrack {
                track: TrackId(1),
                name: "Renamed".into(),
            },
        )
        .unwrap();
        edit::apply(
            &mut project,
            ProjectEdit::EditTarget {
                device: reference.device,
                edit: TargetEdit::SetFieldName {
                    entry: reference.key.clone(),
                    field: FieldKey("f17".into()),
                    name: "Last argument".into(),
                },
            },
        )
        .unwrap();
        let before = project.clone();
        assert!(
            edit::apply(
                &mut project,
                ProjectEdit::EditTarget {
                    device: reference.device,
                    edit: TargetEdit::AddField {
                        entry: reference.key,
                        value_type: FieldType::Int,
                    },
                },
            )
            .is_err()
        );
        assert_eq!(project, before);
        let path = std::env::temp_dir().join(format!(
            "qtable-legacy-arity-v{version}-{}.ron",
            std::process::id()
        ));
        file::save(&path, &project).unwrap();
        let loaded = file::load(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(loaded, project);
        assert_eq!(receive_hit(loaded), expected);
    }
}
