//! CLI commands, launch grids and named/indexed selectors.
use qtable_engine::model::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Selector {
    Name(String),
    Index(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum ParsedCommand {
    Launch(Selector, usize),
    Scene(Selector),
    StopTrack(Selector),
    StopGroup(Selector),
    StopAll,
    StartTransport,
    StopTransport,
    Tempo(f64),
    Quantisation(GlobalQuantisation),
    Follow(bool),
    Status,
    Help,
    Quit,
}

pub(super) fn parse_command(line: &str) -> Result<ParsedCommand, String> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(ParsedCommand::Status);
    }
    let mut parts = line.split_whitespace();
    let Some(verb) = parts.next() else {
        return Ok(ParsedCommand::Status);
    };
    let verb = verb.to_ascii_lowercase();
    let rest: Vec<_> = parts.collect();
    match verb.as_str() {
        "help" | "h" | "?" if rest.is_empty() => Ok(ParsedCommand::Help),
        "quit" | "q" | "exit" if rest.is_empty() => Ok(ParsedCommand::Quit),
        "start-transport" | "play" if rest.is_empty() => Ok(ParsedCommand::StartTransport),
        "stop-transport" | "halt" if rest.is_empty() => Ok(ParsedCommand::StopTransport),
        "stop-all" | "stopall" | "sa" if rest.is_empty() => Ok(ParsedCommand::StopAll),
        "status" if rest.is_empty() => Ok(ParsedCommand::Status),
        "tempo" if rest.len() == 1 => rest[0]
            .parse::<f64>()
            .map(ParsedCommand::Tempo)
            .map_err(|_| "tempo expects a positive BPM number".to_owned()),
        "tempo" => Err("usage: tempo <bpm>".to_owned()),
        "follow-actions" | "follow" if rest == ["on"] => Ok(ParsedCommand::Follow(true)),
        "follow-actions" | "follow" if rest == ["off"] => Ok(ParsedCommand::Follow(false)),
        "follow-actions" | "follow" => Err("usage: follow-actions on|off".into()),
        "quantisation" | "quant" if rest.len() == 1 => {
            parse_quantisation(rest[0]).map(ParsedCommand::Quantisation)
        }
        "quantisation" | "quant" => Err(
            "usage: quantisation <none|1bar|2bars|4bars|8bars|1/2[T]|1/4[T]|1/8[T]|1/16[T]|1/32>"
                .to_owned(),
        ),
        "launch" | "l" if rest.len() >= 2 => {
            let slot = rest
                .last()
                .ok_or_else(|| "slot is required".to_owned())?
                .parse::<usize>()
                .map_err(|_| "slot must be a 1-based positive number".to_owned())?;
            if slot == 0 {
                return Err("slot numbers start at 1".to_owned());
            }
            let selector = parse_selector(&rest[..rest.len() - 1])?;
            Ok(ParsedCommand::Launch(selector, slot))
        }
        "launch" | "l" => Err("usage: launch <track name|index> <slot>".to_owned()),
        "scene" if !rest.is_empty() => Ok(ParsedCommand::Scene(parse_scene_selector(&rest)?)),
        "scene" => Err("usage: scene <name|index>".to_owned()),
        "stopgroup" | "sg" if !rest.is_empty() => Ok(ParsedCommand::StopGroup(
            parse_selector(&rest).map_err(|_| "group indices start at 1".to_owned())?,
        )),
        "stopgroup" | "sg" => Err("usage: stopgroup <name|index>".to_owned()),
        "stop" | "s" if !rest.is_empty() => Ok(ParsedCommand::StopTrack(parse_selector(&rest)?)),
        "stop" | "s" => Err("usage: stop <track name|index>".to_owned()),
        _ => Err(format!("unknown command: {verb:?}; type help")),
    }
}

pub(super) fn parse_quantisation(value: &str) -> Result<GlobalQuantisation, String> {
    let value = value.to_ascii_lowercase();
    let grid = match value.as_str() {
        "none" => return Ok(GlobalQuantisation::None),
        "1bar" | "1bars" => QuantisationValue::Bars(1),
        "2bar" | "2bars" => QuantisationValue::Bars(2),
        "4bar" | "4bars" => QuantisationValue::Bars(4),
        "8bar" | "8bars" => QuantisationValue::Bars(8),
        _ => {
            let (note, triplet) = value
                .strip_suffix('t')
                .map_or((value.as_str(), false), |note| (note, true));
            let note = match note {
                "1/2" => NoteValue::Half,
                "1/4" => NoteValue::Quarter,
                "1/8" => NoteValue::Eighth,
                "1/16" => NoteValue::Sixteenth,
                "1/32" => NoteValue::ThirtySecond,
                _ => return Err(format!("unsupported quantisation: {value}")),
            };
            if triplet {
                QuantisationValue::Triplet(note)
            } else {
                QuantisationValue::Note(note)
            }
        }
    };
    grid.validate().map_err(|error| error.to_string())?;
    Ok(GlobalQuantisation::Value(grid))
}

pub(super) fn quantisation_label(value: GlobalQuantisation) -> String {
    match value {
        GlobalQuantisation::None => "none".into(),
        GlobalQuantisation::Value(QuantisationValue::Bars(bars)) => format!("{bars}bar"),
        GlobalQuantisation::Value(grid) => {
            let (note, suffix) = match grid {
                QuantisationValue::Note(note) => (note, ""),
                QuantisationValue::Triplet(note) => (note, "T"),
                QuantisationValue::Bars(_) => unreachable!(),
            };
            format!("1/{}{suffix}", 4 * TICKS_PER_BEAT / note.duration().ticks())
        }
    }
}

pub(super) fn parse_selector(words: &[&str]) -> Result<Selector, String> {
    let value = words.join(" ");
    if let Ok(index) = value.parse::<usize>() {
        return if index > 0 {
            Ok(Selector::Index(index))
        } else {
            Err("track indices start at 1".to_owned())
        };
    }
    Ok(Selector::Name(value))
}

pub(super) fn parse_scene_selector(words: &[&str]) -> Result<Selector, String> {
    parse_selector(words).map_err(|_| "scene indices start at 1".to_owned())
}

pub(super) fn resolve<T, I>(
    selector: &Selector,
    items: &[T],
    kind: &str,
    name: impl Fn(&T) -> &str,
    id: impl Fn(&T) -> I,
) -> Result<I, String> {
    let item = match selector {
        Selector::Index(index) => index.checked_sub(1).and_then(|i| items.get(i)),
        Selector::Name(value) => items
            .iter()
            .find(|item| name(item).eq_ignore_ascii_case(value)),
    };
    item.map(id)
        .ok_or_else(|| format!("unknown {kind}: {}", selector_label(selector)))
}

pub(super) fn resolve_scene(selector: &Selector, scenes: &[Scene]) -> Result<SceneId, String> {
    resolve(
        selector,
        scenes,
        "scene",
        |scene| &scene.name,
        |scene| scene.id,
    )
}
pub(super) fn resolve_track(selector: &Selector, tracks: &[&Track]) -> Result<TrackId, String> {
    resolve(
        selector,
        tracks,
        "track",
        |track| &track.name,
        |track| track.id,
    )
}
pub(super) fn resolve_group(selector: &Selector, groups: &[&Track]) -> Result<TrackId, String> {
    resolve(
        selector,
        groups,
        "group",
        |group| &group.name,
        |group| group.id,
    )
}

pub(super) fn selector_label(selector: &Selector) -> String {
    match selector {
        Selector::Name(name) => format!("{name:?}"),
        Selector::Index(index) => index.to_string(),
    }
}
