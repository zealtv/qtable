// Rasterises an SVG to transparent PNGs and raw RGBA files, one per size.
// Usage: swift render.swift <in.svg> <out-dir> <size>...
// Writes <out-dir>/<size>.png and <out-dir>/<size>.rgba (straight alpha, row-major, 4 bytes a pixel).
// Used by build.sh; macOS only (AppKit renders the SVG).
import AppKit

let args = CommandLine.arguments
guard args.count >= 4, let svg = NSImage(contentsOfFile: args[1]), svg.size.width > 0 else {
    FileHandle.standardError.write("usage: render.swift in.svg out-dir size...\n".data(using: .utf8)!)
    exit(1)
}
let outDir = args[2]
let cs = CGColorSpace(name: CGColorSpace.sRGB)!

for arg in args[3...] {
    guard let size = Int(arg), size > 0 else { exit(1) }
    let ctx = CGContext(data: nil, width: size, height: size, bitsPerComponent: 8, bytesPerRow: size * 4,
                        space: cs, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    ctx.interpolationQuality = .high
    NSGraphicsContext.current = NSGraphicsContext(cgContext: ctx, flipped: false)
    svg.draw(in: NSRect(x: 0, y: 0, width: size, height: size))
    let image = ctx.makeImage()!
    try! NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:])!
        .write(to: URL(fileURLWithPath: "\(outDir)/\(size).png"))

    // Raw RGBA with straight (not premultiplied) alpha, as winit's window icon wants it.
    let px = ctx.data!.assumingMemoryBound(to: UInt8.self)
    var raw = [UInt8](repeating: 0, count: size * size * 4)
    for i in 0..<(size * size) {
        let a = Int(px[i * 4 + 3])
        for c in 0..<3 { raw[i * 4 + c] = a == 0 ? 0 : UInt8(min(255, (Int(px[i * 4 + c]) * 255 + a / 2) / a)) }
        raw[i * 4 + 3] = UInt8(a)
    }
    try! Data(raw).write(to: URL(fileURLWithPath: "\(outDir)/\(size).rgba"))
}
