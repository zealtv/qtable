//! Queued value batches through the command seam, output stages and loopback packets.
use super::*;
use crate::model::*;
use crate::scheduler::cluster_sim::*;

#[test]
fn queued_value_history_reaches_each_output_with_its_own_delay() {
    let mut track = ramp_track(3840);
    let target = track.devices[0].target_mut().unwrap();
    for parameter in &mut target.parameters {
        parameter.send_mode = SendMode::EveryFrame;
    }
    track.delay_ms = 500.0;
    track.route = Route::Outputs(vec![A, B]);
    let mut project = project(120.0, &[A, B], vec![track]);
    project.outputs[0].delay_ms = 300.0; // net +800 ms
    project.outputs[1].delay_ms = -600.0; // net -100 ms
    let mut sim = Sim::new(project, MS(20), lead_for(MS(50))).launched();
    sim.record_values = true;
    let key = ArgumentRef {
        device: DeviceRef::Chain(DeviceId(500)),
        parameter: ParameterKey("p0".into()),
        field: FieldKey("value".into()),
    }
    .on(TrackId(500));
    let mut snapshot = (*sim.planner.project).clone();
    sim.run_until(sim.start + MS(100));
    // Owner submissions continue during a long scheduler stall. Do not run a worker pass.
    for (posted, value) in [(100, 0.2), (200, 0.4), (300, 0.6)] {
        sim.now = sim.start + MS(posted);
        snapshot
            .set_parameter_values(&key.parameter, vec![(key.field.clone(), value)])
            .unwrap();
        sim.send(SchedulerCommand::UpdateParameters(
            crate::engine::ValueEdit::for_test(snapshot.clone(), vec![key.clone()]),
        ));
    }
    sim.now = sim.start + MS(500);
    sim.run_until(sim.start + MS(1200));
    assert_eq!(sim.log.len(), 3);
    assert!(sim.log.iter().all(|c| c.arrived >= sim.start + MS(500)));
    let sample = |output, tick, parameter: &str| {
        sim.sent_values(output, MS(tick))
            .unwrap()
            .iter()
            .find(|(address, _)| address == parameter)
            .unwrap()
            .1
    };
    for (tick, expected) in [(850, 0.025), (950, 0.2), (1050, 0.4), (1150, 0.6)] {
        assert!(
            (sample(A, tick, "/parameter/p0") - expected).abs() < 1e-5,
            "A at {tick}"
        );
        let untouched = (tick - 800) as f32 / 2000.0;
        assert!((sample(A, tick, "/parameter/p1") - untouched).abs() < 1e-5);
    }
    assert!((sample(B, 550, "/parameter/p0") - 0.6).abs() < 1e-5);
    assert!((sample(B, 550, "/parameter/p1") - 0.325).abs() < 1e-5);
}
