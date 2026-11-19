use qtable_engine::model::*;
pub fn project(port: u16, delays: &[f64]) -> Project {
    let mut project = Project {
        tempo: Tempo::new(600.0).unwrap(),

        outputs: vec![Track::new_output(TrackId(201), "test", "127.0.0.1", port)],
        scenes: vec![
            Scene {
                id: SceneId(1),
                name: "one".into(),
                follow_actions: SceneFollowActions::default(),
            },
            Scene {
                id: SceneId(2),
                name: "two".into(),
                follow_actions: SceneFollowActions::default(),
            },
        ],
        ..Project::default()
    };
    for (i, delay) in delays.iter().enumerate() {
        let mut track = Track::new(TrackId(i as u64 + 1), "track", 2);
        track.route = Route::single(TrackId(201));
        track.delay_ms = *delay;
        for slot in 0..2 {
            let mut clip = Clip::new(
                ClipId((i * 2 + slot + 1) as u64),
                "clip",
                BeatTime(960),
                qtable_engine::model::LIVE_CLIP_PALETTE[0],
            )
            .unwrap();
            // Deliberately unsorted; equal timestamps retain insertion order.
            clip.events = vec![480, 0, 480]
                .into_iter()
                .enumerate()
                .map(|(j, tick)| {
                    let path = OscAddress::Relative(format!("/t{}/s{slot}/e{j}", i + 1));
                    track.hit(BeatTime(tick), path, vec![], || DeviceId(i as u64 + 1))
                })
                .collect();
            track.slots[slot].clip = Some(clip);
        }
        project.tracks.push(track);
    }
    project
}
