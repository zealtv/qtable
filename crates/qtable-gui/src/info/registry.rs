//! Plain contextual help. Add a registry entry when adding a control.

pub(crate) struct Entry {
    pub(crate) title: &'static str,
    pub(crate) text: &'static str,
}

macro_rules! entries {
    ($($id:ident: $title:literal, $text:literal;)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        // The registry also reserves entries for the concurrent inspector rebuild.
        #[allow(dead_code)]
        pub(crate) enum Id { $($id,)* }

        impl Id {
            #[cfg(test)]
            pub(crate) const ALL: &'static [Self] = &[$(Self::$id,)*];

            pub(crate) fn entry(self) -> Entry {
                match self {
                    $(Self::$id => Entry { title: $title, text: $text },)*
                }
            }
        }
    };
}

entries! {
    ReEnableAutomation: "Re-Enable Automation", "Return overridden arguments to their clip envelopes. Manual changes override one argument until re-enabled or a clip containing its envelope launches.";
    ShowEnvelope: "Show Envelope", "Open this control's envelope at full height in the selected clip's Detail View. The first point you draw adds the envelope if the clip has none.";
    ShowEnvelopeNoClip: "Show Envelope", "Unavailable: no clip is selected. Select a clip on this control's track; a playing clip does not count.";
    ShowEnvelopeOtherTrack: "Show Envelope", "Unavailable: the selected clip is on another track. Select a clip on this control's track; a playing clip does not count.";
    TargetPath: "Target Path", "The Target's own path piece, between the track's path and each relative entry path. The parent is mid-grey; older ancestors are dim. Two Targets of one receiver differ only here.";
    TargetAddEvent: "Add Event", "Add an event: a momentary message with typed arguments that clips play. It starts with a free path to change.";
    TargetAddParameter: "Add Parameter", "Add a parameter: one OSC message with held arguments. Define it here and edit its values on the generated face. It starts with one Float argument from 0 to 1.";
    TargetDeleteEntry: "Delete Entry", "Delete the selected event or parameter. An event takes the lanes and hits that play it with it.";
    TargetEvent: "Event", "Click the title, while the Target is expanded, to select this event for editing. Delete removes it with the lanes and hits that play it. Up and Down move the selection.";
    TargetFold: "Target View", "Open the editor beside the controls (expanded), or close it and keep only the controls (collapsed).";
    TargetValue: "Argument Value", "Drag to change this argument, or type a number and press Return for exact entry. Escape cancels. Parameters send the whole message; event controls edit defaults for new hits and triggers.";
    TargetFire: "Trigger Event", "Sends once now, as a clip hit would.";
    TargetParameter: "Parameter", "Click the title, while the Target is expanded, to select this parameter for editing. Delete removes it. Up and Down move the selection.";
    TargetEnum: "Argument Enum", "Choose an option. Parameters send the whole message; event controls edit defaults for new hits and triggers.";
    TargetNumber: "Argument Number", "Type a number and press Return to apply. Escape cancels. Parameters send the whole message; event controls edit defaults for new hits and triggers.";
    TargetText: "Argument Text", "Type text and press Return to apply the event argument default for new hits and triggers. Escape cancels.";
    TargetToggle: "Argument Toggle", "Click to switch Off or On. Numeric Toggles send 0/1; Bool sends OSC true or false.";
    TargetName: "Entry Name", "An optional name. Empty shows the path. Clips and lanes keep pointing at the entry when it is renamed.";
    TargetEntryPath: "Entry Path", "This entry's OSC path. Its parent is mid-grey; older ancestors are dim. Absolute skips them and dims the bracket. Changing it changes what every clip sends.";
    TargetFieldAdd: "Add Argument", "Append a Float argument. Parameters hold 1–16 arguments; events hold 0–16. New event hits use the argument default.";
    TargetFieldRemove: "Remove Argument", "Remove this argument. Event hits lose the matching value. A parameter must keep at least one argument.";
    TargetFieldTab: "Argument Tab", "Edit this argument. The face outlines its control. Move or remove the selected tab's argument with ◀ ▶ and ×; + adds an argument.";
    TargetFieldMove: "Move Argument", "Move this argument one place left or right, in OSC order. Its key and any existing hit values follow it.";
    TargetFieldName: "Argument Name", "An optional argument label; unnamed arguments read Arg 1, Arg 2, .... Renaming preserves the argument key.";
    TargetFieldType: "Argument Type", "The OSC wire type: Float, Int, Bool (true/false), or String. Type limits the available controls. Existing event hits convert with it.";
    TargetFieldControl: "Argument Control", "The control generated for this argument. Numeric Toggles send 0/1; Bool sends true/false; Enum sends an option value. Changing Control keeps the wire type.";
    TargetFieldRange: "Event Argument Range", "Enable an optional numeric range. Without one only Number box is available. Choose Number box before removing a range.";
    TargetOptionLabel: "Option Label", "The name shown in the Enum. For String arguments this is also the value sent.";
    TargetOptionValue: "Option Value", "The typed OSC value sent for this Enum option. Numeric labels and values may differ.";
    TargetOptionAdd: "Add Option", "Append an Enum option, up to 64. Numeric options start at their zero-based position.";
    TargetOptionRemove: "Remove Option", "Remove this option. Parameters keep at least two options; events keep at least one. A removed default becomes the first option.";
    TargetMin: "Minimum", "The lowest value the receiver accepts. Disabled for Bool, numeric Toggle and rangeless event arguments.";
    TargetMax: "Maximum", "The highest value the receiver accepts. Disabled for Bool, numeric Toggle and rangeless event arguments.";
    TargetDefault: "Default", "The argument default. New event hits start with it; existing hits keep their values. Parameter current values are edited on the generated face.";
    TargetSend: "Send Mode", "Continuous sends changes on output frames; On Release commits at drag release. Every Frame repeats while playing. Stopped edits send once.";
    TargetThreshold: "Threshold", "Minimum change in the sent value; a smaller one sends after a quiet frame. Only for a continuous number (Slider, Dial or Number box) on a parameter, else hidden. Dimmed in Every Frame.";
    TargetHint: "Target", "Events are momentary messages that clips play; parameters hold ordered values. The expanded panel defines their arguments and controls.";
    DeviceName: "Device Name", "Rename this device with Command-R. Return keeps the name; Escape cancels.";
    DeviceActivator: "Device Activator", "Turn this device's events and parameters on or off. Its clips keep their place in time while it is off.";
    CopyDevice: "Copy Devices", "Copy selected devices in chain order. The device clipboard can be pasted into another set.";
    PasteDevice: "Paste Devices", "Paste copied devices after the selected devices, or at the end of their category when nothing is selected.";
    DuplicateDevice: "Duplicate Devices", "Duplicate selected devices after the last selected device, with fresh ids and the same definitions.";
    DeleteDevice: "Delete Devices", "Remove selected devices and the clip lanes and hits that play their events.";

    MappingInput: "Mapping Input Range", "Set ordered input endpoints. Values outside the range clamp to an endpoint. Descending endpoints reverse the input direction.";
    MappingOutput: "Mapping Output Range", "Set ordered output endpoints before bias. Descending endpoints reverse the output direction; equal endpoints produce a constant value.";
    MappingExponent: "Mapping Exponent", "Set a positive scaling exponent. 1 is linear; values above 1 bend toward the start, and values below 1 bend toward the end.";
    MappingBias: "Mapping Bias", "Add this offset after scaling, in output units. Both output endpoints shift by this amount.";
    SceneLaunchSection: "Scene Launch", "Show or hide this scene's follow actions. Choose actions and chances, then set the time in bars.beats.sixteenths.";
    DetailOutputSection: "Output", "Show or hide the selected output's destination.";
    DetailLaunchSection: "Launch", "Show or hide Main's launch quantisation and global follow-action switch.";
    KeyMapSwitch: "Key Map Mode", "Command-K. Mappable controls turn red. Click one, then type a letter, digit or punctuation mark; a and A differ. Named keys and Command, Option or Control chords stay shortcuts.";
    MapSwitch: "MIDI Map Mode", "Command-M. Controls a controller can drive turn blue. Click one, then move a control on your controller to map it. Mapped controls don't operate meanwhile.";
    MappingList: "Mapping List", "Mappings for the active MIDI or Key mode, in the order applied. Click a row, or use Up and Down, to edit it below; Delete removes it. One source can drive several controls.";
    MappingSource: "Source", "The controller message driving this mapping: MIDI channel, then CC, note or pitch bend, or a keyboard key. Note 60 is C3, as in Live.";
    MappingPath: "Mapping Path", "Where the mapped control sits: its track, then its device.";
    MappingName: "Mapping Name", "The mapped control's name. A clip slot follows its slot when scenes are inserted or moved.";
    MappingMin: "Mapping Min", "The control's value at the controller's low end; Min above Max inverts it. An Enum splits the range into equal parts. An Activator is on between Min and Max.";
    MappingMax: "Mapping Max", "The control's value at the controller's high end; Max below Min inverts it. An Enum splits the range into equal parts. An Activator is on between Min and Max.";
    MappingMode: "Mapping Mode", "A CC: Absolute follows its position; Relative modes read endless encoders. A note or key: Latch flips or steps on each press; Gate holds Max while down and Min on release.";
    MappingTakeover: "Mapping Takeover", "For an absolute CC or pitch bend. None follows at once. Pick-Up waits until the controller passes the value. Value Scaling converges smoothly, so nothing jumps.";
    MappingSensitivity: "Encoder Sensitivity", "How far one step of a relative encoder moves, from 1% to 1000%. 100% crosses a range in 256 steps, or one unit per step. Dimmed while the Mode is Absolute.";
    MappingInvert: "Invert", "Swap Min and Max, so the control runs the other way. Lit while Min is above Max. For a switch under a note or key, it reverses On and Off in Gate mode.";
    InfoView: "Info View", "Point to a control to see what it does. Press ? to show or hide this view.";
    Browser: "Browser", "Show or hide the Browser with Command-Option-B. Option-5 makes it the active region.";
    Detail: "Detail View", "Follow the selected clip, scene, track or output. Show or hide these controls with Command-Option-L. Option-3 makes it the active region.";
    OutputArea: "Outputs", "The columns right of the tracks are the set's outputs, like Live's return tracks. Show or hide them with Command-Option-R; insert one with Command-Option-T or a right-click.";
    OutputBand: "Output", "Route toggles on tracks and groups (a multi-select above four outputs); the destination on output tracks and Main. Check or uncheck this row with Command-Option-I or the mixer menu.";
    MixerBand: "Mixer", "Each column's Activator, Solo and activity meter. Check or uncheck this row in the mixer menu.";
    PathBand: "Path", "Each column's Path, edited in place. Check or uncheck this row in the mixer menu.";
    ThroughputBand: "Throughput", "Destination load meters and messages per second. Check or uncheck this row in the mixer menu.";
    StripSwitch: "Mixer Sections", "Shows or hides every checked strip row at once (Command-Option-M). The arrow beside it opens the menu of rows.";
    SectionsMenuArrow: "Mixer Sections Menu", "Choose which strip rows the mixer switch shows: Output, Path, Mixer and Throughput, and the Outputs columns.";
    RouteMenu: "Route List", "Open the list to tick the destinations this track sends to: Main and outputs combine, Group follows the group.";
    OutputTitle: "Output", "The output's name, in its colour. Click to select it; right-click for its track menu.";
    OutputName: "Output Name", "Rename this output. Right-click its column, choose Rename or press ⌘R, then Return to apply.";
    OutputColour: "Output Colour", "Choose a colour for this output.";
    DeleteOutput: "Delete Output", "Remove this output from the set and from track and group routes. Delete or Backspace deletes selected headers.";
    OutputEnable: "Output On", "When lit, this output sends. Its letter follows its position among outputs. Switch it off to silence this receiver while tracks keep playing.";
    OutputSolo: "Output Solo", "Solo tracks routed to this output, keeping their enabled routes audible. Cmd-click adds or removes a solo. Muted tracks stay silent.";
    OutputLatency: "Output Latency", "Shift every send to this output by this many milliseconds, added to the track's delay. Press Return to apply.";
    OutputActivity: "Output Activity", "The MIDI-style orange dots light on OSC sends and fade over about a third of a second.";
    TrackActivity: "Track Activity", "Lights orange on OSC sends and fades over about a third of a second. A muted or soloed-out track stays dark while its clips play on.";
    TrackActivator: "Track Activator", "Switch the track off to mute it: its clips keep playing but send no OSC. The number is the track's position; F1 to F8 toggle the first eight.";
    TrackSolo: "Solo", "Solo this track: every other track stops sending. Command-click adds or removes a track; S solos the selected tracks. Unsolo brings back earlier states.";
    GroupActivator: "Group Activator", "Switch the group off to mute all its tracks. Their own Activators are kept for when the group is switched back on.";
    GroupSolo: "Group Solo", "Solo the group's tracks. Half lit when only some of its tracks are soloed; soloing the group then plays just those.";
    DismissStatus: "Dismiss Message", "Clear the message in the status bar.";
    New: "New Set", "Create a new set with one track and eight scenes.";
    Open: "Open Set", "Open a saved qtable set.";
    Save: "Save Set", "Save changes to the current set.";
    SaveAs: "Save Set As", "Save this set under a new name.";
    Settings: "Settings", "Open the Settings pane: settings kept for this computer, in tabs. Click again to close it.";
    SettingsClose: "Close Settings", "Close the Settings pane. A click outside it closes it too.";
    SettingsMidi: "MIDI Settings", "MIDI input ports on this computer. Switch Remote on for a port to let its controller drive qtable.";
    MidiInIndicator: "MIDI In", "Flashes when MIDI arrives from a port with Remote on. Switch ports in Settings, MIDI.";
    MainTrack: "Main Track", "Select Main to view its output, mixer, launch quantisation and global follow actions. Scene cells select their scene; clicking the title keeps a ghost scene cell.";
    Tempo: "Tempo", "Set the transport speed in beats per minute. Press Return to apply.";
    Meter: "Time Signature", "The number of beats in a bar and the note value of each beat.";
    GlobalQuantisation: "Global Launch Quantisation", "Choose when clips and stop buttons take effect. None launches immediately.";
    Position: "Transport Position", "The current transport position in bars, beats and sixteenths.";
    DrawMode: "Draw Mode", "Paint steps on envelopes and hits on event lanes instead of selecting. B toggles it; hold B for a temporary toggle.";
    Play: "Play", "Start the transport at 1.1.1 and restart clips armed at the last global Stop.";
    Stop: "Stop", "Stop transport and freeze playing clip positions. Press again to rewind; Play restarts the armed clips from the beginning.";
    ClipLaunch: "Launch Clip", "Launch this clip at its quantisation boundary. A flashing launch button is waiting to start.";
    SlotStop: "Clip Stop Button", "Stop this track at the next global quantisation boundary.";
    TrackStop: "Track Stop Button", "Stop this track's clips at the next global quantisation boundary.";
    StopAll: "Stop All Clips", "Stop every track's clips at the next global quantisation boundary. The transport keeps running.";
    FollowActionsToggle: "Follow Actions Toggle", "Turn every clip and scene Follow Action on or off together. Dim means all Follow Actions are off.";
    ClipSlot: "Clip Slot", "Select to edit; drag the body to move. Option-drag (Ctrl on Windows) copies; drops replace clips. In the active grid (Option-1), arrows select, Return launches, Command-arrows move tracks/scenes.";
    EmptySlot: "Empty Clip Slot", "Select this slot, then use Insert Clip (⇧⌘M, the right-click menu, or a double-click) to add a clip. Empty slots separate Follow Action groups.";
    MapSceneLaunch: "Scene Launch", "Assign this to a MIDI controller or keyboard key to launch the highlighted scene.";
    MapSceneCancel: "Cancel Scene Launch", "Assign this to a MIDI controller or keyboard key to cancel the launch of a triggered scene.";
    MapSceneUp: "Scene Up", "Assign this to a MIDI controller or keyboard key to move the highlighted scene selection up.";
    MapSceneDown: "Scene Down", "Assign this to a MIDI controller or keyboard key to move the highlighted scene selection down.";
    MapSceneSelect: "Scene Select", "Assign this control to a MIDI controller, preferably an endless encoder, to scroll through the scenes. The highlighted scene will always remain in view.";
    SceneLaunch: "Launch Scene", "Launch the clips in this row. Empty slots with stop buttons stop their tracks.";
    SceneSelect: "Scene", "Select this scene and highlight its row to edit its name and Follow Actions. Empty output slots and the space between columns also select scenes. Command-Up or Down moves it while the grid is active.";
    TrackHeader: "Track", "Select this track and highlight its whole column. Shift-click for a range to group. Drag it to move it, into a group or out of one; Command-Left or Right moves it while the grid is active.";
    GroupHeader: "Group Track", "Select this group's tracks. Use the fold button to show or hide them; the group track stays. Drop a dragged track here to add it to the group.";
    GroupFold: "Fold Group", "Show or hide this group's tracks.";
    GroupStop: "Stop Group", "Stop every track in this group at the next global quantisation boundary.";
    GroupLaunch: "Launch Group Slot", "Launch this row across the group's tracks only; other tracks keep playing. Empty slots with stop buttons stop their tracks.";
    GroupSlot: "Group Slot", "Striped where the group's tracks have clips in this row. Click to select one of them.";
    AddTrack: "Add Track", "Add a track to the right of the existing tracks (⌘T, or Insert Track in a track's right-click menu).";
    AddScene: "Add Scene", "Add a scene below the existing scenes (⌘I, or Insert Scene in a scene's right-click menu).";
    GroupTracks: "Group Tracks", "Put one or more selected adjacent plain tracks in a group (⌘G, or a track's right-click menu). Members route to Group.";
    CreateClip: "Create Clip", "Create an empty OSC clip in the selected slot (⇧⌘M).";
    DeleteClip: "Delete Clip", "Remove the clip from the selected slot. Delete or Backspace does the same while the grid has focus.";
    ToggleSlotStop: "Slot Stop Button", "Add or remove the selected empty slot's stop button. Without it, scene launches leave this track playing. ⌘E or the right-click menu toggles it.";
    TrackName: "Track Name", "Rename the selected track. Press Return to apply.";
    TrackColour: "Track Colour", "Choose a colour for the selected track.";
    ClipName: "Clip Name", "Rename the selected clip. Command-R starts renaming in the grid; Return applies the name.";
    DuplicateClip: "Duplicate Clip", "Copy this clip directly below it, adding a scene if needed. Command-D does the same in the grid.";
    CutClip: "Cut Clip", "Copy this clip to the clipboard and remove it from its slot. Command-X does the same in the grid.";
    CopyClip: "Copy Clip", "Copy this clip to the clipboard. Command-C does the same in the grid.";
    PasteClip: "Paste Clip", "Put the copied clip in this slot, replacing any clip there. Command-V does the same in the grid. Unavailable until a clip is copied.";
    CopyLanes: "Copy Lanes", "Copy selected lane headers, with their hits or envelope points. Shift-click extends the selection.";
    PasteLanes: "Paste Lanes", "Insert copied lanes before this selection. Matching lanes merge; content outside the clip is trimmed. The status bar reports discarded content.";
    DeleteLanes: "Delete Lanes", "Remove selected lanes and their hits or points from this clip. Definitions stay on the track.";
    CopyTrack: "Copy Track", "Copy the selected tracks, a group with its tracks, or outputs. Command-C does the same in the grid. Main cannot be copied.";
    PasteTrack: "Paste Track", "Put the copied columns left of the selected one, or at the end of its own region. Command-V does the same in the grid. Unavailable until tracks are copied.";
    DuplicateTrack: "Duplicate Track", "Copy the selected tracks, a group with its tracks, or outputs right after the source, and select the copy. Command-D does the same in the grid. The clipboard is untouched. Main cannot be duplicated.";
    DeleteTrack: "Delete Track", "Remove this track and its clips, or a group with its members. Delete or Backspace deletes selected headers together. A set needs at least one track.";
    MoveClip: "Move Clip", "Choose a destination slot in the grid to move the selected clip there.";
    ClipColour: "Clip Colour", "Choose a colour for the selected clip.";
    AddOutput: "Insert Output", "Add an output: a UDP receiver address and port. Command-Option-T does the same.";
    RemoveOutput: "Remove Output", "Remove this output from the set and from track and group routes. Delete or Backspace deletes selected headers.";
    OutputHost: "Output Host", "Enter the receiver's IP address or host name.";
    OutputPort: "Output Port", "Enter the UDP port on which the receiver listens.";
    OutputMode: "UDP Mode", "Choose Unicast for one receiver, or Broadcast for receivers on the local network.";
    DetailMixerSection: "Mixer", "Set normalized volume and its independent OSC controller. Clip OSC arguments remain unchanged.";
    Volume: "Volume", "Normalized volume, from 0 at the bottom to 1 at the top. The slot lights on OSC sends; it is an activity display, not audio level.";
    MixerVolume: "Value", "Normalized 0 to 1. Mapping sets the outgoing range. Source Activator and Solo do not gate this controller.";
    MixerVolumeOutput: "Enable", "Enable volume OSC using the path and mapping below. Sends on live edits and transport start; destination Activator gates sending. Off dims and disables its controls.";
    MixerVolumeSend: "Send Mode", "Send Mode: Continuous sends changes on output frames; On Release commits at drag release. Every Frame repeats while playing. Stopped edits send once. Changing mode sends nothing.";
    MixerThreshold: "Threshold", "Minimum change in the mapped OSC value. A smaller change sends after one quiet frame. Every Frame ignores Threshold and keeps its value.";
    DetailThroughputSection: "Throughput", "Show or hide this receiver's counts over the preceding second.";
    OutputLoad: "Load", "Load: messages per second sent to this receiver; orange while changes wait for room in the Budget. With Budget off, the bar compares with the busiest receiver in the set.";
    ThroughputMessages: "Messages/s", "OSC messages sent to this receiver in the preceding second, including clip events and parameter values.";
    ThroughputPackets: "Packets/s", "UDP datagrams sent in the preceding second. Bundles can carry several messages per packet.";
    ThroughputBytes: "Bytes/s", "OSC payload bytes sent in the preceding second; kB is 1000 bytes. UDP and IP headers are not counted.";
    ThroughputMerged: "Merged/s", "Parameter values overwritten by newer values before sending, in the preceding second.";
    ThroughputDeferred: "Deferred/s", "Changes left waiting for Budget room at frame ends in the preceding second; a change can count on several frames.";
    ThroughputSkipped: "Skipped/s", "Repeats skipped because the Budget had no room, in the preceding second.";
    ThroughputOversize: "Oversize", "Messages too large for an empty bundle, sent plain in the preceding second.";
    ThroughputFramesLate: "Frame late", "Frames dispatched more than one frame period late, in the preceding second.";
    OutputFrameRate: "Frame Rate", "Output frames per second, from 1 to 60 Hz. Parameter changes wait for the next frame; clip events keep their exact timing.";
    OutputBudget: "Budget", "Messages per second, clip events included. Changes take priority over repeats; events are never held back. Off keeps the typed value.";
    OutputRefresh: "Refresh", "Keep-alive interval in seconds. Resends enabled parameters across frames, while playing or stopped. Off keeps the typed value.";
    OutputBundle: "Bundle", "Pack each frame into immediate OSC bundles, up to 1472 bytes per datagram. Enable only for receivers that accept bundles.";
    VolumePath: "Path", "The Volume slider's path. Its parent is mid-grey; older ancestors are dim. A missing leading slash is added.";
    Path: "Path", "The path this column adds to its events. Its closest parent is mid-grey; older ancestors are dim. A missing leading slash is added.";
    TrackRoute: "Track Route", "The track's resolved route. Change it with the colour-matched toggles in the I/O band.";
    TrackDelay: "Track Delay", "Shift this track's OSC sends in milliseconds. Negative values send early; positive values send late.";
    GroupDelay: "Group Delay", "Shift every send from this group's tracks in milliseconds, added to each track's own Track Delay.";
    MainDelay: "Main Delay", "Shift every send from every track in milliseconds, added to the track and group delays.";
    TrackDelayUnit: "Milliseconds", "The unit of the Track Delay field beside it.";
    GroupRoute: "Group Route", "The group's route summary. Change it with the colour-matched toggles in the I/O band.";
    GroupName: "Group Name", "Rename this group. Press Return to apply.";
    GroupColour: "Group Colour", "Cycle through the colours for this group.";
    Ungroup: "Ungroup Tracks", "Remove the group and keep its tracks (⇧⌘G with the group header selected).";
    SceneName: "Scene Name", "Rename this scene. Select it and press ⌘R to type over its name in Main; Return applies.";
    ClipLength: "Loop Length", "Set the loop brace length, independently of Start and End.";
    ClipStart: "Clip Start", "Set where playback begins when the clip launches.";
    ClipEnd: "Clip End", "Set where playback stops with Loop off. This marker is ignored while looping.";
    ClipMarkerSet: "Set Marker", "Set this marker to the playing clip’s content position, rounded to global quantisation. Length sets the loop end.";
    EnvelopeLink: "Envelope Region", "Unlink to copy the clip markers and give this envelope its own loop. Relink keeps its points. Edit unlinked envelopes in One lane.";
    ClipPosition: "Loop Position", "Move the loop brace without changing its length. Positions count from 1.1.1.";
    ClipSection: "Clip", "Show or hide the clip's length and loop settings.";
    ColourChooser: "Clip Colour", "Open or close the clip colour chooser.";
    Loop: "Loop", "Repeat the clip. Switching Loop off lets the current pass finish.";
    LaunchSection: "Launch", "Show or hide the clip's launch and Follow Action settings.";
    ClipQuantisation: "Launch Quantisation", "Choose this clip's launch boundary. Global uses the Control Bar setting; None launches immediately.";
    LaunchMode: "Launch Mode", "Trigger starts the clip when its launch button is pressed.";
    Legato: "Legato", "When on, this clip takes over the play position of the clip that was playing in the track, instead of starting from its beginning.";
    FollowEnabled: "Follow Action", "Enable or disable the actions that run after this clip or scene is launched.";
    FollowA: "Follow Action A", "Choose the first possible action. Jump chooses a specific scene.";
    FollowB: "Follow Action B", "Choose the second possible action. The chances decide which action runs.";
    FollowChance: "Follow Action Chance", "Set the relative chances of Action A and Action B. Zero excludes an action; two zeros do nothing.";
    FollowTime: "Follow Action Time", "When Unlinked, the Follow Action is triggered after the clip has played for this time, in bars, beats and sixteenths.";
    SceneFollowTime: "Scene Follow Action Time", "Set bars.beats.sixteenths from the scene's launch to its Follow Action. A pasted 5 means five bars.";
    FollowLink: "Follow Action Linked", "Linked fires the Follow Action at the end of the clip, or after the loops set by the Multiplier. Unlinked fires it after the Follow Action Time.";
    FollowMultiplier: "Follow Action Multiplier", "This sets the number of loops that the clip will play before the Follow Action is triggered.";
    CutContent: "Cut", "Copy the selected time range to the content clipboard and clear it from the lanes it covers. ⌘X.";
    CopyContent: "Copy", "Copy the picked points or hits, or the selected time range, to the content clipboard. ⌘C.";
    PasteContent: "Paste", "Paste the copied content at the clicked point or hit, the insert marker, or the start of the selected range. ⌘V. Unavailable until something is copied.";
    DuplicateContent: "Duplicate", "Copy the selected time range and paste it straight after itself. ⌘D.";
    DeleteContent: "Delete", "Delete the picked points or hits, or clear the selected time range. Delete. Text fields keep their own editing keys.";
    ClearEnvelope: "Clear Envelope", "Remove this argument's envelope from the clip, so it stops automating. Unavailable while the clip has none.";
    InsertShape: "Insert Shape", "Fill the selected range, or one grid step from here, with this shape; it joins the envelope at both ends. Ramp In and Ramp Out meet the level after or before.";
    SimplifyEnvelope: "Simplify Envelope", "Remove points a straight line does without, inside the selected range or across the whole envelope. Unavailable below three points.";
    GoToDevice: "Go to Device", "Select the Target device on this track that defines the event or parameter, and show its definition in the Track tab. For Volume, show the Mixer.";
    EventTimeline: "Lanes", "Click places the paste marker. Hits: double-click adds, drag moves, Alt-drag copies. Points: a click on a line adds, on a point selects, again deletes; drag moves. Alt bends, ⌘ leaves the grid.";
    RulerZoom: "Time Ruler", "Drag down to zoom in around the pointer, up to zoom out; drag sideways to scroll. Double-click zooms to the selection, then the whole clip. ⌘-scroll zooms; scroll sideways to move.";
    EventGrid: "Event Grid", "Choose the time grid for adding and dragging events. T means triplets; Off allows individual ticks.";
    PointPosition: "Point Position", "Where the selected point sits in the clip, as bars.beats.sixteenths. A position past a neighbouring point removes that point; past the loop end it lands on the end.";
    PointValue: "Point Value", "The selected point's value, in the argument's own control. Whole numbers for a stepped argument; a level for a toggle or enum.";
    PointCurve: "Point Curve", "Bend the segment leaving this point, from -1 to 1. Positive starts slowly, negative starts quickly. Off for toggles and enums, and for the last point.";
    RemoveEvent: "Remove Hit", "Remove this hit from its lane.";
    EditorTabEvents: "Events Tab", "Show this clip's event lanes, one row per event, as Live's Notes tab. Select a hit to edit it below.";
    EditorTabEnvelopes: "Envelopes Tab", "Edit one envelope at full height. Choose it with the Device and Control choosers; with no choice yet, the clip's first envelope shows.";
    EditorTabOverview: "Overview Tab", "Show every lane of this clip stacked, events and envelopes.";
    LaneDeviceChooser: "Device", "Choose a device on this track to edit one of its arguments at full height. Mixer comes last.";
    LaneControlChooser: "Control", "Choose an argument of the chosen device. A red dot marks a lane in this clip. An argument's envelope starts with the first point you draw.";
    LaneChooserEntry: "Chooser Entry", "Show this entry. Only show adjusted envelopes lists just this clip's lanes.";
    OneLane: "One Lane", "One argument's envelope at full height. Click the background to place the paste marker; drag to select time. With no envelope yet, click the line to start one. ⌘ leaves the grid.";
    EnvelopeLine: "Envelope", "Click a line to add a point, a point to select it, again to delete it. Drag a point, or near a segment to move it; Alt-drag bends. Shift refines, ⌘ leaves the grid.";
    StartMarker: "Start Marker", "Where the clip starts when launched: drag it along the grid, ⌘ off the grid. Alt-Left/Right moves Start and End together. On an unlinked lane, the lane's own Start.";
    EndMarker: "End Marker", "Where the clip stops when its Loop is off; with Loop on it draws grey. Drag it along the grid, ⌘ off the grid. On an unlinked lane, the lane's own End.";
    LoopBrace: "Loop Brace", "The part that repeats while Loop is on. Drag it to move the loop and keep its length, along the grid or with ⌘ off it. On an unlinked lane it loops that lane alone.";
    LoopStart: "Loop Start", "Drag to change where the loop starts, along the grid or with ⌘ off it.";
    LoopEnd: "Loop End", "Drag to change where the loop ends, along the grid or with ⌘ off it.";
    ArgumentKind: "Argument Type", "Choose Integer, Float, String or Boolean for this OSC argument.";
    ArgumentValue: "Argument Value", "Set the value sent with this OSC argument.";
    AddArgument: "Add Argument", "Append an argument to this hit.";
    RemoveArgument: "Remove Argument", "Remove this argument from the hit.";
}
