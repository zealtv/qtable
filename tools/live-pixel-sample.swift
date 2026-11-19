// Sample pixel colours in a window region over time (e.g. a blinking Live launch icon).
// Usage: swiftc -O -o /tmp/sample tools/live-pixel-sample.swift && /tmp/sample <window-id>
// Edit `centres` and the crop/box to the region you need. Find window ids with CGWindowListCopyWindowInfo.
import CoreGraphics
import Foundation
let wid = CGWindowID(UInt32(CommandLine.arguments[1])!)
let centres = [87, 105, 123, 141]  // track 1 rows 1-4, points
let start = Date()
while Date().timeIntervalSince(start) < 6.0 {
    let rect = CGRect(x: 0, y: 0, width: 40, height: 170)
    guard let base = CGWindowListCreateImage(.null, .optionIncludingWindow, wid, [.boundsIgnoreFraming, .nominalResolution]),
          let img = base.cropping(to: rect) else { exit(1) }
    let w = img.width, h = img.height
    var data = [UInt8](repeating: 0, count: w * h * 4)
    let ctx = CGContext(data: &data, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4, space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    ctx.draw(img, in: CGRect(x: 0, y: 0, width: w, height: h))
    var out: [String] = []
    for c in centres {
        var green = 0, dark = 0
        for yy in (c - 7)...(c + 7) { for x in 6..<30 {
            let y = yy
            if y < 0 || y >= h { continue }
            let i = (y * w + x) * 4
            let r = Int(data[i]), g = Int(data[i+1]), b = Int(data[i+2])
            if g > 150 && g - r > 80 && g - b > 30 { green += 1 }
            if r + g + b < 60 { dark += 1 }
        } }
        out.append("\(green)/\(dark)")
    }
    print(String(format: "%.3f ", Date().timeIntervalSince(start)) + out.joined(separator: " "))
    usleep(25000)
}
