import AppKit
import CoreGraphics
import Foundation

guard CommandLine.arguments.count == 2,
      let pid = Int32(CommandLine.arguments[1]) else {
    fputs("usage: window_probe.swift <pid>\n", stderr)
    exit(2)
}

let options: CGWindowListOption = [.optionOnScreenOnly, .excludeDesktopElements]
let windowInfo = CGWindowListCopyWindowInfo(options, kCGNullWindowID) as? [[String: Any]] ?? []

var windows: [[String: Any]] = []
for window in windowInfo {
    guard let ownerPID = window[kCGWindowOwnerPID as String] as? Int32,
          ownerPID == pid else {
        continue
    }

    let bounds: [String: Any]
    if let rawBounds = window[kCGWindowBounds as String] as? [String: Any] {
        bounds = [
            "x": rawBounds["X"] as? Double ?? 0,
            "y": rawBounds["Y"] as? Double ?? 0,
            "width": rawBounds["Width"] as? Double ?? 0,
            "height": rawBounds["Height"] as? Double ?? 0
        ]
    } else {
        bounds = ["x": 0, "y": 0, "width": 0, "height": 0]
    }

    windows.append([
        "window_id": window[kCGWindowNumber as String] as? Int32 ?? 0,
        "name": window[kCGWindowName as String] as? String ?? "",
        "owner_pid": ownerPID,
        "layer": window[kCGWindowLayer as String] as? Int ?? 0,
        "bounds": bounds,
        "alpha": window[kCGWindowAlpha as String] as? Double ?? 0,
        "on_screen": window[kCGWindowIsOnscreen as String] as? Bool ?? false
    ])
}

let frontmost = NSWorkspace.shared.frontmostApplication
let result: [String: Any] = [
    "pid": pid,
    "windows": windows,
    "frontmost": [
        "bundle_id": frontmost?.bundleIdentifier ?? "",
        "pid": frontmost?.processIdentifier ?? 0
    ]
]

let json = try JSONSerialization.data(withJSONObject: result, options: [.prettyPrinted, .sortedKeys])
FileHandle.standardOutput.write(json)
FileHandle.standardOutput.write(Data("\n".utf8))
