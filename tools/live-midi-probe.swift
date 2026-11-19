// A virtual MIDI destination "qtable-probe" that prints each CC as: wall-time channel cc value.
// Build: swiftc -O tools/live-midi-probe.swift -o <scratch>/midiprobe; run it, then route a Live track's MIDI To to qtable-probe. Optional arg: seconds to run (default 1800).
import CoreMIDI
import Foundation
setvbuf(stdout, nil, _IOLBF, 0)
var client = MIDIClientRef()
MIDIClientCreateWithBlock("qtable-probe" as CFString, &client, nil)
var dest = MIDIEndpointRef()
let status = MIDIDestinationCreateWithBlock(client, "qtable-probe" as CFString, &dest) { list, _ in
    let t = Date().timeIntervalSince1970
    var packet = list.pointee.packet
    for _ in 0..<list.pointee.numPackets {
        let bytes = Mirror(reflecting: packet.data).children.map { $0.value as! UInt8 }
        var i = 0
        while i + 2 < Int(packet.length) {
            if bytes[i] & 0xF0 == 0xB0 {
                print(String(format: "%.4f %d %d %d", t, bytes[i] & 0x0F, bytes[i + 1], bytes[i + 2]))
                i += 3
            } else { i += 1 }
        }
        packet = MIDIPacketNext(&packet).pointee
    }
}
print("ready", status)
RunLoop.main.run(until: Date().addingTimeInterval(Double(CommandLine.arguments.dropFirst().first ?? "1800") ?? 1800))
