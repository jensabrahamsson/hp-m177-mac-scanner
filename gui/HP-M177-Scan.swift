import AppKit
import Foundation

/// Native AppKit GUI for the M177fw scanner.
///
/// Add and scan go through the `hp-m177` CLI (same functions the tests drive).
/// `HP_M177_BIN` overrides the binary path.
///
/// Headless:
///   HP-M177-Scan --exec add --host 192.168.50.14
///   HP-M177-Scan --exec scan --source platen --color color --dpi 300 --format jpeg --output ~/Documents/scan.jpg
///   HP-M177-Scan --button-smoke --host 127.0.0.1:PORT --output /tmp/gui.jpg

let args = CommandLine.arguments
if args.contains("--smoke") {
    exit(AppDelegate.smoke(args))
}

if let idx = args.firstIndex(of: "--exec"), args.index(after: idx) < args.endIndex {
    exit(AppDelegate.exec(Array(args[(idx + 1)...])))
}

let delegate = AppDelegate()
let app = NSApplication.shared
app.setActivationPolicy(.regular)
app.delegate = delegate
app.run()

/// Transparent click target so painted chrome receives mouse events even
/// when sibling controls sit in the same window.
final class HitTarget: NSView {
    var key: String = ""
    weak var owner: RootView?

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
    override var isFlipped: Bool { false }

    override func mouseDown(with event: NSEvent) {
        owner?.beginPress(key)
    }

    override func mouseUp(with event: NSEvent) {
        let p = convert(event.locationInWindow, from: nil)
        owner?.endPress(key, inside: bounds.contains(p))
    }
}

/// All chrome is painted in this view's draw(_:). NSButton cells previously
/// had frames but never appeared in the window backing store.
final class RootView: NSView {
    weak var app: AppDelegate!
    var frames: [String: NSRect] = [:]
    var press: String?
    var hits: [String: HitTarget] = [:]

    override var isFlipped: Bool { false }
    override var isOpaque: Bool { true }
    override var acceptsFirstResponder: Bool { true }

    override func layout() {
        super.layout()
        layoutChrome()
    }

    func layoutChrome() {
        let b = bounds
        guard b.width > 80, b.height > 80, app != nil else { return }
        let logH: CGFloat = 110
        let topH: CGFloat = 32
        let topY = b.height - 12 - topH
        let gap: CGFloat = 8
        let btnW = max((b.width - 32 - 3 * gap) / 4, 80)
        frames["discover"] = NSRect(x: 16, y: topY, width: btnW, height: topH)
        frames["add"] = NSRect(x: 16 + btnW + gap, y: topY, width: btnW, height: topH)
        frames["preview"] = NSRect(x: 16 + 2 * (btnW + gap), y: topY, width: btnW, height: topH)
        frames["scan"] = NSRect(x: 16 + 3 * (btnW + gap), y: topY, width: btnW, height: topH)

        let colX: CGFloat = 16
        let colW: CGFloat = 300
        var y = topY - 20
        func row(_ key: String, _ height: CGFloat, gapAfter: CGFloat = 8) {
            y -= height
            frames[key] = NSRect(x: colX, y: y, width: colW, height: height)
            y -= gapAfter
        }
        row("deviceHead", 18, gapAfter: 4)
        row("hostHead", 16, gapAfter: 4)
        row("hostField", 24)
        row("addPrinter", 32, gapAfter: 14)
        row("scanHead", 18, gapAfter: 6)
        row("source", 28)
        row("color", 28)
        row("dpi", 28)
        row("format", 28, gapAfter: 12)
        row("saveHead", 16, gapAfter: 4)
        row("outputField", 24)
        row("status", 40, gapAfter: 0)

        app.hostField.frame = frames["hostField"] ?? .zero
        app.outputField.frame = frames["outputField"] ?? .zero
        let previewY = logH + 12
        let previewTop = topY - 12
        app.preview.frame = NSRect(
            x: colX + colW + 16,
            y: previewY,
            width: max(b.width - (colX + colW + 16) - 16, 200),
            height: max(previewTop - previewY, 200)
        )
        app.scroll.frame = NSRect(x: 16, y: 12, width: b.width - 32, height: logH)

        for key in ["discover", "add", "preview", "scan", "addPrinter", "source", "color", "dpi", "format"] {
            if let r = frames[key] { ensureHit(key, r) }
        }
    }

    func ensureHit(_ key: String, _ r: NSRect) {
        let v: HitTarget
        if let existing = hits[key] {
            v = existing
        } else {
            v = HitTarget()
            v.key = key
            v.owner = self
            addSubview(v, positioned: .above, relativeTo: nil)
            hits[key] = v
        }
        v.frame = r
    }

    override func draw(_ dirtyRect: NSRect) {
        NSColor.windowBackgroundColor.setFill()
        bounds.fill()
        drawButton("Discover", key: "discover")
        drawButton("Add scanner", key: "add")
        drawButton("Preview", key: "preview")
        drawButton("Scan", key: "scan")
        drawLabel("Device", key: "deviceHead", bold: true)
        drawLabel("Host / IP", key: "hostHead", secondary: true)
        drawButton("Add printer if missing", key: "addPrinter")
        drawLabel("Scan", key: "scanHead", bold: true)
        drawCycle(caption: "Source", value: app.source, key: "source")
        drawCycle(caption: "Color", value: app.color, key: "color")
        drawCycle(caption: "DPI", value: app.dpi, key: "dpi")
        drawCycle(caption: "Format", value: app.format, key: "format")
        drawLabel("Save to Documents", key: "saveHead", secondary: true)
        drawLabel(app.statusText, key: "status", secondary: true)
    }

    func drawButton(_ title: String, key: String) {
        guard let r = frames[key] else { return }
        let path = NSBezierPath(roundedRect: r.insetBy(dx: 0.5, dy: 0.5), xRadius: 6, yRadius: 6)
        let pressed = press == key
        let disabled = app?.busy == true
        let fill: NSColor
        if disabled {
            fill = NSColor(srgbRed: 0.45, green: 0.55, blue: 0.70, alpha: 1)
        } else if pressed {
            fill = NSColor(srgbRed: 0.05, green: 0.28, blue: 0.70, alpha: 1)
        } else {
            fill = NSColor(srgbRed: 0.10, green: 0.45, blue: 0.95, alpha: 1)
        }
        fill.setFill()
        path.fill()
        NSColor.white.setStroke()
        path.lineWidth = 1
        path.stroke()
        let attrs: [NSAttributedString.Key: Any] = [
            .foregroundColor: NSColor.white,
            .font: NSFont.systemFont(ofSize: 13, weight: .semibold),
        ]
        let size = title.size(withAttributes: attrs)
        title.draw(
            at: NSPoint(x: r.midX - size.width / 2, y: r.midY - size.height / 2),
            withAttributes: attrs
        )
    }

    func drawLabel(_ text: String, key: String, bold: Bool = false, secondary: Bool = false) {
        guard let r = frames[key] else { return }
        let paragraph = NSMutableParagraphStyle()
        paragraph.lineBreakMode = .byWordWrapping
        let attrs: [NSAttributedString.Key: Any] = [
            .font: bold ? NSFont.boldSystemFont(ofSize: 13) : NSFont.systemFont(ofSize: 12),
            .foregroundColor: secondary ? NSColor.secondaryLabelColor : NSColor.labelColor,
            .paragraphStyle: paragraph,
        ]
        text.draw(in: r, withAttributes: attrs)
    }

    func drawCycle(caption: String, value: String, key: String) {
        guard let r = frames[key] else { return }
        let path = NSBezierPath(roundedRect: r.insetBy(dx: 0.5, dy: 0.5), xRadius: 5, yRadius: 5)
        NSColor(srgbRed: 0.95, green: 0.95, blue: 0.97, alpha: 1).setFill()
        path.fill()
        NSColor(srgbRed: 0.45, green: 0.45, blue: 0.50, alpha: 1).setStroke()
        path.lineWidth = 1
        path.stroke()
        let capAttrs: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 11),
            .foregroundColor: NSColor.secondaryLabelColor,
        ]
        let valAttrs: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 13, weight: .medium),
            .foregroundColor: NSColor(srgbRed: 0.10, green: 0.10, blue: 0.12, alpha: 1),
        ]
        caption.draw(at: NSPoint(x: r.minX + 8, y: r.midY - 7), withAttributes: capAttrs)
        let val = "\(value)  ▾"
        let size = val.size(withAttributes: valAttrs)
        val.draw(at: NSPoint(x: r.maxX - size.width - 8, y: r.midY - 8), withAttributes: valAttrs)
    }

    func beginPress(_ key: String) {
        guard app?.busy != true else { return }
        press = key
        needsDisplay = true
    }

    func endPress(_ key: String, inside: Bool) {
        press = nil
        needsDisplay = true
        guard inside else { return }
        activate(key)
    }

    func activate(_ key: String) {
        guard let app else { return }
        switch key {
        case "discover": app.discoverLan()
        case "add": app.addScanner()
        case "preview": app.runPreview()
        case "scan": app.runScan()
        case "addPrinter": app.addPrinter()
        case "source": app.cycle(&app.source, ["platen", "adf"])
        case "color": app.cycle(&app.color, ["color", "gray", "lineart"])
        case "dpi": app.cycle(&app.dpi, ["100", "300", "600"])
        case "format":
            app.cycle(&app.format, ["jpeg", "pdf", "tiff"])
            app.formatChanged()
        default: break
        }
        needsDisplay = true
    }
}

final class PreviewView: NSView {
    var image: NSImage? {
        didSet { needsDisplay = true }
    }
    var selection: NSRect? {
        didSet { needsDisplay = true }
    }
    var dragStart: NSPoint?

    override var isFlipped: Bool { false }
    override var acceptsFirstResponder: Bool { true }

    func fittedImageRect() -> NSRect {
        guard let image = image, image.size.width > 0, image.size.height > 0 else {
            return .zero
        }
        let bounds = self.bounds.insetBy(dx: 8, dy: 8)
        let sx = bounds.width / image.size.width
        let sy = bounds.height / image.size.height
        let scale = min(sx, sy)
        let w = image.size.width * scale
        let h = image.size.height * scale
        return NSRect(
            x: bounds.midX - w / 2,
            y: bounds.midY - h / 2,
            width: w,
            height: h
        )
    }

    func imagePoint(from viewPoint: NSPoint) -> NSPoint? {
        guard let image = image else { return nil }
        let r = fittedImageRect()
        if r.width <= 0 || r.height <= 0 || !r.contains(viewPoint) { return nil }
        let x = (viewPoint.x - r.minX) / r.width * image.size.width
        let y = (viewPoint.y - r.minY) / r.height * image.size.height
        return NSPoint(x: x, y: y)
    }

    override func draw(_ dirtyRect: NSRect) {
        NSColor.windowBackgroundColor.setFill()
        dirtyRect.fill()
        let frame = bounds.insetBy(dx: 1, dy: 1)
        let path = NSBezierPath(roundedRect: frame, xRadius: 8, yRadius: 8)
        NSColor.separatorColor.setStroke()
        path.lineWidth = 1
        path.stroke()
        if let image = image {
            image.draw(in: fittedImageRect(), from: .zero, operation: .copy, fraction: 1)
            if let sel = selection {
                let r = fittedImageRect()
                let sx = r.width / image.size.width
                let sy = r.height / image.size.height
                let vr = NSRect(
                    x: r.minX + sel.minX * sx,
                    y: r.minY + sel.minY * sy,
                    width: sel.width * sx,
                    height: sel.height * sy
                )
                NSColor.systemBlue.withAlphaComponent(0.18).setFill()
                vr.fill()
                NSColor.systemBlue.setStroke()
                let p = NSBezierPath(rect: vr)
                p.lineWidth = 2
                p.stroke()
            }
        } else {
            let msg = "Preview — Scan Preview, then drag a region"
            let attrs: [NSAttributedString.Key: Any] = [
                .foregroundColor: NSColor.secondaryLabelColor,
                .font: NSFont.systemFont(ofSize: 13),
            ]
            let size = msg.size(withAttributes: attrs)
            msg.draw(
                at: NSPoint(x: (bounds.width - size.width) / 2, y: (bounds.height - size.height) / 2),
                withAttributes: attrs
            )
        }
    }

    override func mouseDown(with event: NSEvent) {
        let p = convert(event.locationInWindow, from: nil)
        dragStart = imagePoint(from: p)
        selection = nil
    }

    override func mouseDragged(with event: NSEvent) {
        guard let start = dragStart,
              let cur = imagePoint(from: convert(event.locationInWindow, from: nil))
        else { return }
        selection = NSRect(
            x: min(start.x, cur.x),
            y: min(start.y, cur.y),
            width: abs(cur.x - start.x),
            height: abs(cur.y - start.y)
        )
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate {
    var window: NSWindow!
    var root: RootView!
    let hostField = NSTextField(string: "")
    let outputField = NSTextField(string: "")
    let logView = NSTextView()
    let preview = PreviewView()
    let scroll = NSScrollView()
    var source = "platen"
    var color = "color"
    var dpi = "100"
    var format = "jpeg"
    var statusText = "Add the scanner by IP or hostname, then Preview or Scan."
    var lastExit: Int32 = 0
    var busy = false

    func applicationDidFinishLaunching(_ notification: Notification) {
        window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 980, height: 640),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.title = "HP M177 Scanner"
        window.minSize = NSSize(width: 860, height: 560)
        window.delegate = self
        root = RootView(frame: NSRect(x: 0, y: 0, width: 980, height: 640))
        root.app = self
        window.contentView = root

        styleField(hostField)
        hostField.placeholderString = "192.168.50.14 or DEV26BA77.local"
        hostField.stringValue = "192.168.50.14"
        hostField.target = self
        hostField.action = #selector(addScanner)
        styleField(outputField)
        outputField.stringValue = Self.defaultDocumentsPath(ext: "jpg")
        outputField.placeholderString = "~/Documents/scan-<timestamp>.jpg"

        logView.isEditable = false
        logView.font = NSFont.monospacedSystemFont(ofSize: 11, weight: .regular)
        logView.backgroundColor = NSColor.textBackgroundColor
        logView.minSize = NSSize(width: 0, height: 0)
        logView.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        logView.isHorizontallyResizable = false
        logView.isVerticallyResizable = true
        logView.autoresizingMask = [.width]
        logView.textContainer?.widthTracksTextView = true
        scroll.documentView = logView
        scroll.hasVerticalScroller = true
        scroll.borderType = .bezelBorder

        root.addSubview(hostField)
        root.addSubview(outputField)
        root.addSubview(preview)
        root.addSubview(scroll)
        root.layoutChrome()
        appendLog("Using \(hpBinary())\n")

        NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self else { return event }
            if event.keyCode == 36 {
                let resp = self.window?.firstResponder
                if resp is NSTextView || resp is NSTextField {
                    return event
                }
                self.runScan()
                return nil
            }
            return event
        }

        if CommandLine.arguments.contains("--layout-check") {
            exit(runLayoutCheck())
        }
        if CommandLine.arguments.contains("--button-smoke") {
            exit(runButtonSmoke())
        }

        window.makeKeyAndOrderFront(nil)
        window.makeFirstResponder(hostField)
        NSApp.activate(ignoringOtherApps: true)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }

    func windowDidResize(_ notification: Notification) {
        root.needsLayout = true
        root.needsDisplay = true
        root.layoutChrome()
    }

    func cycle(_ value: inout String, _ items: [String]) {
        if let i = items.firstIndex(of: value) {
            value = items[(i + 1) % items.count]
        } else {
            value = items[0]
        }
    }

    func runLayoutCheck() -> Int32 {
        root.layoutChrome()
        window.layoutIfNeeded()
        root.layoutSubtreeIfNeeded()
        window.makeKeyAndOrderFront(nil)
        window.displayIfNeeded()
        RunLoop.current.run(until: Date(timeIntervalSinceNow: 0.25))
        root.layoutChrome()
        root.needsDisplay = true
        window.displayIfNeeded()

        let hf = hostField.frame
        let pv = preview.frame
        let scan = root.frames["scan"] ?? .zero
        let disc = root.frames["discover"] ?? .zero
        var painted = 0
        var png = ""
        let scale: CGFloat = 2
        let pxW = max(Int(root.bounds.width * scale), 1)
        let pxH = max(Int(root.bounds.height * scale), 1)
        if let rep = NSBitmapImageRep(
            bitmapDataPlanes: nil,
            pixelsWide: pxW,
            pixelsHigh: pxH,
            bitsPerSample: 8,
            samplesPerPixel: 4,
            hasAlpha: true,
            isPlanar: false,
            colorSpaceName: NSColorSpaceName.deviceRGB,
            bytesPerRow: 0,
            bitsPerPixel: 0
        ) {
            NSGraphicsContext.saveGraphicsState()
            if let gc = NSGraphicsContext(bitmapImageRep: rep) {
                NSGraphicsContext.current = gc
                gc.cgContext.translateBy(x: 0, y: CGFloat(pxH))
                gc.cgContext.scaleBy(x: scale, y: -scale)
                root.draw(root.bounds)
                for v in root.subviews where !(v is HitTarget) {
                    gc.cgContext.saveGState()
                    gc.cgContext.translateBy(x: v.frame.minX, y: v.frame.minY)
                    v.draw(v.bounds)
                    gc.cgContext.restoreGState()
                }
            }
            NSGraphicsContext.restoreGraphicsState()
            var y = 0
            while y < rep.pixelsHigh {
                var x = 0
                while x < rep.pixelsWide {
                    if let c = rep.colorAt(x: x, y: y)?.usingColorSpace(NSColorSpace.deviceRGB) {
                        let lum = 0.2126 * c.redComponent + 0.7152 * c.greenComponent + 0.0722 * c.blueComponent
                        let accent = c.blueComponent > 0.35 && c.blueComponent > c.redComponent + 0.05
                        let light = lum > 0.7
                        if accent || light { painted += 1 }
                    }
                    x += 3
                }
                y += 3
            }
            if let data = rep.representation(using: .png, properties: [:]) {
                let path = NSTemporaryDirectory() + "hp-m177-layout-check.png"
                try? data.write(to: URL(fileURLWithPath: path))
                png = path
            }
        }
        let report = "hostField=\(Int(hf.minX)),\(Int(hf.minY)) \(Int(hf.width))x\(Int(hf.height)) preview=\(Int(pv.minX)),\(Int(pv.minY)) \(Int(pv.width))x\(Int(pv.height)) scan=\(Int(scan.minX)),\(Int(scan.minY)) \(Int(scan.width))x\(Int(scan.height)) discover=\(Int(disc.minX)),\(Int(disc.minY)) \(Int(disc.width))x\(Int(disc.height)) window=\(Int(root.bounds.width))x\(Int(root.bounds.height)) nonWhite=\(painted) png=\(png)\n"
        FileHandle.standardOutput.write(Data(report.utf8))
        let topBand = root.bounds.height * 0.7
        let ok = hf.height >= 20 && hf.width >= 80 && hf.minX < 80
            && pv.minX >= 280
            && scan.height >= 20 && scan.width >= 40
            && disc.height >= 20 && disc.width >= 40
            && scan.minY > topBand
            && disc.minY > topBand
            && painted >= 30
            && root.hits["scan"] != nil
            && root.hits["discover"] != nil
        if !ok {
            fputs("layout-check failed (buttons missing or not drawn)\n", stderr)
        }
        return ok ? 0 : 1
    }

    /// Same path as clicking Add, Preview, then Scan in the window.
    func runButtonSmoke() -> Int32 {
        var host = ""
        var output = Self.defaultDocumentsPath(ext: "jpg")
        let argv = CommandLine.arguments
        var i = 0
        while i < argv.count {
            if argv[i] == "--host", i + 1 < argv.count { host = argv[i + 1] }
            if argv[i] == "--output", i + 1 < argv.count { output = argv[i + 1] }
            i += 1
        }
        if host.isEmpty {
            fputs("hp-m177-gui --button-smoke needs --host\n", stderr)
            return 2
        }
        root.layoutChrome()
        hostField.stringValue = host
        outputField.stringValue = output
        root.activate("add")
        if lastExit != 0 { return lastExit }
        root.activate("preview")
        if lastExit != 0 { return lastExit }
        if preview.image == nil {
            fputs("button-smoke: preview produced no image\n", stderr)
            return 1
        }
        root.activate("scan")
        if lastExit != 0 { return lastExit }
        if !FileManager.default.isReadableFile(atPath: output) {
            fputs("button-smoke: missing output \(output)\n", stderr)
            return 1
        }
        FileHandle.standardOutput.write(Data("gui-button-smoke-ok \(output)\n".utf8))
        return 0
    }

    @objc func formatChanged() {
        let mapped = format == "jpeg" ? "jpg" : format
        var path = outputField.stringValue
        if path.isEmpty { return }
        let url = URL(fileURLWithPath: path)
        let base = url.deletingPathExtension().lastPathComponent
        path = url.deletingLastPathComponent().appendingPathComponent("\(base).\(mapped)").path
        outputField.stringValue = path
    }

    static func exec(_ argv: [String]) -> Int32 {
        var host = ""
        var source = "platen"
        var color = "color"
        var dpi = "300"
        var format = "jpeg"
        var output = defaultDocumentsPath(ext: "jpg")
        var region = ""
        var i = 0
        let verb = argv.first ?? ""
        while i < argv.count {
            let a = argv[i]
            func next() -> String {
                i += 1
                return i < argv.count ? argv[i] : ""
            }
            switch a {
            case "--host": host = next()
            case "--source": source = next()
            case "--color": color = next()
            case "--dpi": dpi = next()
            case "--format": format = next()
            case "--output": output = next()
            case "--region": region = next()
            default: break
            }
            i += 1
        }
        let d = AppDelegate()
        switch verb {
        case "discover":
            return d.runHpStatus(["discover", "--timeout", "3"])
        case "add":
            if host.isEmpty { fputs("hp-m177-gui --exec add needs --host\n", stderr); return 2 }
            return d.runHpStatus(["add", host])
        case "scan":
            var args = [
                "scan", "--source", source, "--color", color,
                "--dpi", dpi, "--format", format, "--output", output,
            ]
            if !region.isEmpty { args += ["--region", region] }
            return d.runHpStatus(args)
        case "preview":
            let dest = output.isEmpty ? defaultDocumentsPath(ext: "jpg") : output
            return d.runHpStatus([
                "scan", "--source", source, "--color", color,
                "--dpi", "100", "--format", "jpeg", "--output", dest,
            ])
        case "add-printer":
            var args = ["add-printer"]
            if !host.isEmpty { args.append(host) }
            return d.runHpStatus(args)
        default:
            fputs("unknown --exec verb '\(verb)' (use add|scan|preview|discover|add-printer)\n", stderr)
            return 2
        }
    }

    static func defaultDocumentsPath(ext: String) -> String {
        let docs = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first
            ?? URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent("Documents")
        let ts = Int(Date().timeIntervalSince1970)
        return docs.appendingPathComponent("scan-\(ts).\(ext)").path
    }

    static func smoke(_ argv: [String]) -> Int32 {
        var host = ""
        var output = defaultDocumentsPath(ext: "jpg")
        var i = 0
        while i < argv.count {
            let a = argv[i]
            if a == "--host", i + 1 < argv.count { host = argv[i + 1] }
            if a == "--output", i + 1 < argv.count { output = argv[i + 1] }
            i += 1
        }
        if host.isEmpty {
            fputs("hp-m177-gui --smoke needs --host\n", stderr)
            return 2
        }
        let d = AppDelegate()
        let add = d.runHpStatus(["add", host])
        if add != 0 { return add }
        let scan = d.runHpStatus([
            "scan", "--source", "platen", "--color", "color",
            "--dpi", "300", "--format", "jpeg", "--output", output,
        ])
        if scan == 0 {
            FileHandle.standardOutput.write(Data("gui-native-smoke-ok \(output)\n".utf8))
        }
        return scan
    }

    @discardableResult
    func runHpStatus(_ args: [String]) -> Int32 {
        let (code, text) = spawnHp(args)
        lastExit = code
        if !text.isEmpty {
            FileHandle.standardOutput.write(Data(text.utf8))
        }
        return code
    }

    @objc func addScanner() {
        let host = hostField.stringValue.trimmingCharacters(in: .whitespaces)
        guard !host.isEmpty else {
            statusText = "Enter a host or IP first."
            root?.needsDisplay = true
            return
        }
        runHpAsync(["add", host])
    }

    @objc func discoverLan() {
        runHpAsync(["discover", "--timeout", "3"])
    }

    @objc func runPreview() {
        let dest = NSTemporaryDirectory() + "hp-m177-preview.jpg"
        runHpAsync([
            "scan",
            "--source", source,
            "--color", color,
            "--dpi", "100",
            "--format", "jpeg",
            "--output", dest,
        ]) { [weak self] code, _ in
            guard let self else { return }
            if code == 0, let img = NSImage(contentsOfFile: dest) {
                self.preview.image = img
                self.preview.selection = nil
                self.statusText = "Preview ready. Drag a rectangle, then Scan."
                self.root?.needsDisplay = true
            }
        }
    }

    @objc func runScan() {
        var args = [
            "scan",
            "--source", source,
            "--color", color,
            "--dpi", dpi,
            "--format", format,
        ]
        let out = outputField.stringValue.trimmingCharacters(in: .whitespaces)
        if !out.isEmpty {
            args += ["--output", out]
        }
        if let region = regionThousandths() {
            args += ["--region", region]
        }
        runHpAsync(args) { [weak self] code, _ in
            guard let self else { return }
            if code == 0, !out.isEmpty, let img = NSImage(contentsOfFile: out) {
                self.preview.image = img
            }
        }
    }

    func regionThousandths() -> String? {
        guard let sel = preview.selection, let image = preview.image,
              sel.width > 4, sel.height > 4,
              image.size.width > 0, image.size.height > 0
        else { return nil }
        let mediaW = 8500.0
        let mediaH = 11690.0
        let x = sel.minX / image.size.width * mediaW
        let yTop = (image.size.height - sel.maxY) / image.size.height * mediaH
        let w = sel.width / image.size.width * mediaW
        let h = sel.height / image.size.height * mediaH
        return "\(Int(x.rounded())),\(Int(yTop.rounded())),\(Int(w.rounded())),\(Int(h.rounded()))"
    }

    @objc func addPrinter() {
        var args = ["add-printer"]
        let host = hostField.stringValue.trimmingCharacters(in: .whitespaces)
        if !host.isEmpty { args.append(host) }
        runHpAsync(args)
    }

    /// Interactive buttons: do not block the run loop. `--exec` / smoke stay
    /// on spawnHp so they can wait for the process.
    func runHpAsync(_ args: [String], done: ((Int32, String) -> Void)? = nil) {
        if CommandLine.arguments.contains("--button-smoke") {
            let code = runHpStatus(args)
            done?(code, "")
            return
        }
        if busy { return }
        busy = true
        statusText = "Running hp-m177 \(args.joined(separator: " "))…"
        root?.needsDisplay = true
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self else { return }
            let (code, text) = self.spawnHp(args)
            DispatchQueue.main.async {
                self.busy = false
                self.lastExit = code
                self.appendLog("$ hp-m177 \(args.joined(separator: " "))\n\(text)\n")
                if code == 0 {
                    self.statusText = "Done."
                } else {
                    let tail = text.split(whereSeparator: \.isNewline).suffix(2).joined(separator: " ")
                    self.statusText = "Failed (exit \(code)). \(tail)"
                }
                self.root?.needsDisplay = true
                done?(code, text)
            }
        }
    }

    func spawnHp(_ args: [String]) -> (Int32, String) {
        let bin = hpBinary()
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: which(bin))
        proc.arguments = args
        let pipe = Pipe()
        proc.standardOutput = pipe
        proc.standardError = pipe
        do {
            try proc.run()
            proc.waitUntilExit()
            let data = pipe.fileHandleForReading.readDataToEndOfFile()
            let text = String(data: data, encoding: .utf8) ?? ""
            return (proc.terminationStatus, text)
        } catch {
            return (1, "Could not start hp-m177: \(error.localizedDescription)\n")
        }
    }

    func hpBinary() -> String {
        ProcessInfo.processInfo.environment["HP_M177_BIN"]
            ?? bundledBinary()
            ?? "hp-m177"
    }

    func appendLog(_ text: String) {
        guard window != nil else { return }
        logView.string += text
        logView.scrollToEndOfDocument(nil)
    }

    func bundledBinary() -> String? {
        let here = Bundle.main.bundlePath
        let candidates = [
            here + "/Contents/MacOS/hp-m177",
            (here as NSString).appendingPathComponent("hp-m177"),
            (here as NSString).deletingLastPathComponent + "/hp-m177",
            NSHomeDirectory() + "/.cargo/bin/hp-m177",
        ]
        return candidates.first { FileManager.default.isExecutableFile(atPath: $0) }
    }

    func which(_ name: String) -> String {
        if name.contains("/") { return name }
        let path = ProcessInfo.processInfo.environment["PATH"]
            ?? "/usr/bin:/bin:/usr/local/bin:/opt/homebrew/bin:\(NSHomeDirectory())/.cargo/bin"
        for dir in path.split(separator: ":") {
            let candidate = "\(dir)/\(name)"
            if FileManager.default.isExecutableFile(atPath: candidate) {
                return candidate
            }
        }
        return name
    }

    func styleField(_ field: NSTextField) {
        field.isEditable = true
        field.isBezeled = true
        field.bezelStyle = .squareBezel
        field.drawsBackground = true
        field.backgroundColor = NSColor.textBackgroundColor
        field.textColor = NSColor.labelColor
        field.font = NSFont.systemFont(ofSize: 13)
        field.translatesAutoresizingMaskIntoConstraints = true
        field.autoresizingMask = []
    }
}
