import AppKit

guard NSScreen.main != nil else {
    fputs("no main screen\n", stderr)
    exit(2)
}

let app = NSApplication.shared
app.setActivationPolicy(.regular)

let frame = NSScreen.main!.frame
let window = NSWindow(
    contentRect: frame,
    styleMask: .borderless,
    backing: .buffered,
    defer: false
)
window.isOpaque = true
window.backgroundColor = NSColor(srgbRed: 232.0 / 255.0,
                                 green: 93.0 / 255.0,
                                 blue: 154.0 / 255.0,
                                 alpha: 1.0)
window.level = .normal
window.ignoresMouseEvents = true
window.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]
window.setFrame(frame, display: true)
window.orderFrontRegardless()

app.run()
