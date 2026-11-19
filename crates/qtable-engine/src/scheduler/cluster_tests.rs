//! complete-clusters-before-frames, gate 1 (proposal revision 3): tests 6–14 on the simulated
//! worker (`cluster_sim`) and the planner.
use std::time::Duration;

use super::cluster_sim::*;

/// The leads swept (µs): every 10 µs where either parity's tick falls within ~1.2 ms before a
/// cluster or inside one, every 250 µs elsewhere.
fn leads() -> Vec<u64> {
    let period = PERIOD.as_micros() as u64;
    let dense = |at: u64| at <= 1_200 || (7_500..=9_533).contains(&at) || at >= 16_000;
    let mut leads: Vec<_> = (0..period).step_by(10).filter(|&at| dense(at)).collect();
    leads.extend((0..period).step_by(250).filter(|&at| !dense(at)));
    leads.sort();
    leads
}

/// Run `arm` at 120 BPM with the grid at `lead` µs before even sixteenths, through four
/// sixteenths after the start.
fn swept(arm: Arm, lead: u64) -> Sim {
    let mut sim = Sim::new(arm.project(120.0), MS(20), US(lead)).launched();
    let until = sim.start + SIXTEENTH * 4 + MS(5);
    sim.run_until(until);
    sim
}

/// Test 6: at every lead, both parities, no grid frame starts between a cluster's first and last
/// remaining event, and no pass at or after T + D holds one back.
fn sweep(arm: Arm) {
    let mut failures = Vec::new();
    for lead in leads() {
        let sim = swept(arm, lead);
        let splits = sim.splits();
        let overheld = sim.overheld();
        if !splits.is_empty() || !overheld.is_empty() {
            failures.push(format!("lead {lead} µs: {splits:?} {overheld:?}"));
        }
        assert!(sim.sent.len() >= 16, "lead {lead}: {} sent", sim.sent.len());
    }
    assert!(
        failures.is_empty(),
        "{arm:?}: {} leads failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn phase_sweep_arm_c() {
    sweep(Arm::C);
}

#[test]
fn phase_sweep_arm_g() {
    sweep(Arm::G);
}

#[test]
fn phase_sweep_arm_j() {
    sweep(Arm::J);
}

/// Test 7: 30 s at a tempo whose sixteenths drift across the frame grid. The same assertions as
/// the sweep; the leads the clusters actually had are reported (`--nocapture`), not assumed.
fn drift(arm: Arm, bpm: f64) {
    let mut sim = Sim::new(arm.project(bpm), MS(20), Duration::ZERO).launched();
    let until = sim.start + Duration::from_secs(30);
    sim.run_until(until);
    // Each cluster's lead: how far the tick before its sixteenth (its first hit less 0.137 ms)
    // sits before it, in 50 µs bins.
    let period = PERIOD.as_nanos();
    let mut bins = vec![0u32; period.div_ceil(50_000) as usize];
    let clusters = sim.clusters();
    for cluster in &clusters {
        let sixteenth = cluster[0].due - US(137);
        let lead = (sixteenth - sim.start).as_nanos() % period;
        bins[(lead / 50_000) as usize] += 1;
    }
    let window: u32 = bins[581 / 50..=863 / 50].iter().sum();
    println!(
        "{arm:?} at {bpm} BPM: {} clusters; {} of {} 50 µs lead bins covered; {window} clusters \
         with leads in 550–900 µs; bins covered (µs): {:?}",
        clusters.len(),
        bins.iter().filter(|&&n| n > 0).count(),
        bins.len(),
        bins.iter()
            .enumerate()
            .filter(|(_, n)| **n > 0)
            .map(|(i, _)| i * 50)
            .collect::<Vec<_>>()
    );
    let splits = sim.splits();
    let overheld = sim.overheld();
    assert!(
        splits.is_empty() && overheld.is_empty(),
        "{arm:?} at {bpm} BPM: {} splits, {} passes past the cap:\n{}\n{overheld:?}",
        splits.len(),
        overheld.len(),
        splits.join("\n")
    );
}

#[test]
fn drift_at_123_bpm() {
    for arm in [Arm::C, Arm::G, Arm::J] {
        drift(arm, 123.0);
    }
}

#[test]
fn drift_at_97_bpm() {
    for arm in [Arm::C, Arm::G, Arm::J] {
        drift(arm, 97.0);
    }
}
