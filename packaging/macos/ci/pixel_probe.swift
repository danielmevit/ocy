import AppKit
import CoreGraphics
import Foundation

guard CommandLine.arguments.count == 4,
      let x = Double(CommandLine.arguments[2]),
      let y = Double(CommandLine.arguments[3]) else {
    fputs("usage: pixel_probe.swift <png> <x-point> <y-point>\n", stderr)
    exit(2)
}

let url = URL(fileURLWithPath: CommandLine.arguments[1])
guard let data = try? Data(contentsOf: url),
      let source = CGImageSourceCreateWithData(data as CFData, nil),
      let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
    fputs("cannot read PNG\n", stderr)
    exit(3)
}

let scale = NSScreen.main?.backingScaleFactor ?? 1.0
let pixelX = Int(round(x * scale))
let pixelY = Int(round(y * scale))
guard pixelX >= 0, pixelY >= 0,
      pixelX < image.width, pixelY < image.height else {
    fputs("pixel outside PNG\n", stderr)
    exit(4)
}

let colorSpace = CGColorSpace(name: CGColorSpace.sRGB)!
guard let context = CGContext(
    data: nil,
    width: image.width,
    height: image.height,
    bitsPerComponent: 8,
    bytesPerRow: 0,
    space: colorSpace,
    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
) else {
    fputs("cannot create bitmap context\n", stderr)
    exit(5)
}
context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
guard let pixel = context.data?.assumingMemoryBound(to: UInt8.self) else {
    fputs("cannot read pixel buffer\n", stderr)
    exit(6)
}
let offset = (pixelY * context.bytesPerRow) + (pixelX * 4)
print("\(pixel[offset]) \(pixel[offset + 1]) \(pixel[offset + 2]) \(pixel[offset + 3])")
