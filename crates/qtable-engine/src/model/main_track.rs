//! Main is a destination Track kept outside the clip and output lists.
use super::*;

/// Main's fixed presentation; it cannot be renamed or recoloured.
pub struct Main;
impl Main {
    pub const NAME: &'static str = "Main";
    pub const COLOUR: Colour = Colour {
        red: 70,
        green: 70,
        blue: 70,
    };
}
impl Track {
    /// New projects have a localhost destination.
    pub fn new_main(id: TrackId) -> Self {
        let mut main = Self::new_output(id, Main::NAME, "127.0.0.1", 9000);
        main.kind = TrackKind::Main;
        main.colour = Main::COLOUR;
        main.route = Route::empty();
        main
    }
    /// Migrated projects deliberately stay unset: nothing sends until a host is typed.
    pub fn new_unset_main(id: TrackId) -> Self {
        let mut main = Self::new_main(id);
        main.output = None;
        main
    }
}
impl Project {
    /// Main's name and colour belong to the build, not the file: a saved value from an
    /// older build is replaced rather than rejected.
    pub fn normalise_main(&mut self) {
        self.main.name = Main::NAME.into();
        self.main.colour = Main::COLOUR;
    }
    pub(super) fn validate_main(&self) -> ModelResult {
        let main = &self.main;
        require(main.kind == TrackKind::Main, "Main must have kind Main")?;
        require(
            main.name == Main::NAME && main.colour == Main::COLOUR,
            "Main has fixed name and colour",
        )?;
        require(main.route == Route::empty(), "Main has no route")?;
        require(
            main.mixer == MixerState::default(),
            "Main has no Activator or Solo",
        )?;
        require(
            main.group.is_none() && !main.fold_state && main.slots.is_empty(),
            "Main holds no clips and joins no group",
        )?;
        if let Some(output) = &main.output {
            super::output_checks::validate_destination(&output.destination)?;
            output.stage.validate()?;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn main_is_a_stored_destination_without_switches_or_routes() {
        let p = Project::default();
        let main = p.track(p.main.id).unwrap();
        assert_eq!(main.kind, TrackKind::Main);
        assert_eq!(main.destination().unwrap().host, "127.0.0.1");
        assert_eq!(main.destination().unwrap().port, 9000);
        assert!(main.capabilities().destination && !main.capabilities().switched);
        assert!(p.track_facts(p.main.id).unwrap().route.is_none());
        assert!(p.ancestors(main.id).next().is_none());
        p.validate().unwrap();
    }
    #[test]
    fn normalise_main_replaces_a_stale_name_and_colour() {
        let mut p = Project::default();
        p.main.name = "Old".into();
        p.main.colour = Colour {
            red: 128,
            green: 128,
            blue: 128,
        };
        assert!(p.validate().is_err());
        p.normalise_main();
        p.validate().unwrap();
    }
    #[test]
    fn main_rejects_routes_switches_and_duplicate_ids() {
        let mut p = Project::default();
        p.main.route = Route::single(TrackId(9));
        assert!(p.validate().is_err());
        p.main.route = Route::empty();
        p.main.mixer.solo = true;
        assert!(p.validate().is_err());
        p.main.mixer = MixerState::default();
        p.tracks.push(Track::new(p.main.id, "duplicate", 0));
        assert!(p.validate().is_err());
    }
}
