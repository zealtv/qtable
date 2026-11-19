//! Manual measurement through the public engine edit API.
//! Submission timing includes client-side work and is not comparable to the original
//! scheduler-only command/sync metrics.
//! Usage: parameter_update_cost [rate_hz=60] [fields=1|8]. Eight fields use one batch.
use qtable_engine::{
    edit::ProjectEdit,
    engine::{Command, Engine},
    model::*,
    scheduler::SchedulerEvent,
};
use qtable_test_support::TestReceiver;
use std::time::{Duration, Instant};

fn project(port: u16) -> (Project, FieldRef) {
    let mut project = Project {
        outputs: vec![Track::new_output(
            TrackId(1000),
            "loopback",
            "127.0.0.1",
            port,
        )],
        scenes: vec![Scene {
            id: SceneId(1),
            name: "all".into(),
            follow_actions: Default::default(),
        }],
        global_quantisation: GlobalQuantisation::None,
        ..Project::default()
    };
    for i in 1..=16 {
        let mut track = Track::new(TrackId(i), "automated", 1);
        track.route = Route::single(TrackId(1000));
        track.mixer_device.volume.output.enabled = true;
        track.mixer_device.volume.output.address = format!("/volume/{i}");
        let mut clip = Clip::new(
            ClipId(i),
            "ramp and hits",
            BeatTime(3840),
            LIVE_CLIP_PALETTE[0],
        )
        .unwrap();
        for j in 0..16 {
            let device = track.first_target_mut_or_insert(|| DeviceId(i));
            let target = device.target_mut().unwrap();
            target.parameters.push(ParameterDef {
                key: EntryKey(format!("p{j}")),
                name: String::new(),
                path: OscAddress::Absolute(format!("/t{i}/p{j}")),
                fields: vec![FieldDef::new(
                    FieldKey("value".into()),
                    FieldType::Float,
                    true,
                )],
                send_mode: SendMode::Continuous,
            });
            clip.lanes.push(Lane::Envelope(AutomationEnvelope {
                region: None,
                target: ArgumentRef {
                    device: DeviceRef::Chain(DeviceId(i)),
                    parameter: ParameterKey(format!("p{j}")),
                    field: FieldKey("value".into()),
                },
                points: vec![
                    Breakpoint {
                        time: BeatTime(0),
                        value: 0.0,
                        curve: 0.0,
                    },
                    Breakpoint {
                        time: BeatTime(3840),
                        value: 1.0,
                        curve: 0.0,
                    },
                ],
            }));
        }
        if i == 1 {
            for n in 0..32 {
                clip.events.push(track.hit(
                    BeatTime(n * 120),
                    OscAddress::Absolute("/hit".into()),
                    vec![],
                    || DeviceId(i),
                ));
            }
            clip.lanes.push(Lane::Envelope(AutomationEnvelope {
                region: None,
                target: ArgumentRef::volume(),
                points: vec![
                    Breakpoint {
                        time: BeatTime(0),
                        value: 0.0,
                        curve: 0.0,
                    },
                    Breakpoint {
                        time: BeatTime(3840),
                        value: 1.0,
                        curve: 0.0,
                    },
                ],
            }));
        }
        track.slots[0].clip = Some(clip);
        project.tracks.push(track);
    }
    let key = ArgumentRef::volume().on(TrackId(1));
    (project, key)
}

fn stats(name: &str, mut values: Vec<f64>) {
    values.sort_by(f64::total_cmp);
    if values.is_empty() {
        println!("{name},0,0,0,0,0");
        return;
    }
    println!(
        "{name},{},{:.3},{:.3},{:.3},{:.3}",
        values.len(),
        values.iter().sum::<f64>() / values.len() as f64,
        values[values.len() / 2],
        values[(values.len() * 99 / 100).min(values.len() - 1)],
        values[values.len() - 1]
    );
}

fn main() {
    let rate: usize = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "60".into())
        .parse()
        .unwrap();
    let field_count: usize = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "1".into())
        .parse()
        .unwrap();
    assert!(rate > 0 && matches!(field_count, 1 | 8));
    let receiver = TestReceiver::bind().unwrap();
    let (project, field) = project(receiver.addr().port());
    let fields: Vec<_> = std::iter::once(field.clone())
        .chain((0..7).map(|j| FieldRef {
            parameter: ParameterRef {
                track: TrackId(1),
                device: DeviceRef::Chain(DeviceId(1)),
                key: ParameterKey(format!("p{j}")),
            },
            field: FieldKey("value".into()),
        }))
        .take(field_count)
        .collect();
    let mut engine = Engine::with_diagnostics(project, true).unwrap();
    engine
        .execute(Command::LaunchScene { scene: SceneId(1) })
        .unwrap();
    let start = Instant::now() + Duration::from_millis(500);
    let finish = start + Duration::from_secs(2);
    let end = finish + Duration::from_millis(300);
    let mut posted = 0;
    let mut edits = Vec::new();
    let mut hits = Vec::new();
    let mut frames = Vec::new();
    while Instant::now() < end {
        let now = Instant::now();
        if posted < rate * 2 && now >= start + Duration::from_secs_f64(posted as f64 / rate as f64)
        {
            let began = Instant::now();
            let value = (posted % 100) as f64 / 100.0;
            let edit = if field_count == 1 {
                ProjectEdit::SetParameter {
                    target: field.parameter.clone(),
                    values: vec![(field.field.clone(), value)],
                }
            } else {
                ProjectEdit::SetParameters(
                    fields
                        .iter()
                        .map(|field| (field.parameter.clone(), vec![(field.field.clone(), value)]))
                        .collect(),
                )
            };
            engine.execute(Command::Edit(edit)).unwrap();
            edits.push(began.elapsed().as_secs_f64() * 1e6);
            posted += 1;
        }
        let next = if posted < rate * 2 {
            start + Duration::from_secs_f64(posted as f64 / rate as f64)
        } else {
            end
        };
        if let Ok(event) = engine.scheduler_events().recv_timeout(
            next.saturating_duration_since(Instant::now())
                .min(Duration::from_millis(1)),
        ) {
            match event {
                SchedulerEvent::Sent { due, lateness } if (start..finish).contains(&due) => {
                    hits.push(lateness.as_secs_f64() * 1e6)
                }
                SchedulerEvent::FrameSent { due, lateness, .. }
                    if (start..finish).contains(&due) =>
                {
                    frames.push(lateness.as_secs_f64() * 1e6)
                }
                SchedulerEvent::Error(error) => panic!("{error}"),
                SchedulerEvent::EventsDropped { count } => panic!("lost {count} diagnostics"),
                _ => {}
            }
        }
    }
    println!(
        "rate_hz={rate},fields={field_count},posted={posted},submitted_per_second={:.3}",
        posted as f64 / 2.0
    );
    println!("metric,count,mean_us,p50_us,p99_us,max_us");
    // Includes client-side cloning and validation; unlike the original phase-mark
    // measurement, this does not report scheduler application or sync durations.
    stats("engine_edit_submission", edits);
    stats("hit_pre_send_lateness", hits);
    stats("frame_completion_lateness", frames);
    engine.execute(Command::Stop).unwrap();
}
