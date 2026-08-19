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

/// Top-down coordinates so controls stack from the top of the column.
class FlippedView: NSView {
    override var isFlipped: Bool { true }
}

/// Frame-based column. Auto Layout previously reported frames but drew nothing.
final class ControlColumn: FlippedView {
    var items: [NSView] = []

    override func layout() {
        super.layout()
        var y: CGFloat = 16
        let x: CGFloat = 16
        let w = max(bounds.width - 32, 80)
        wantsLayer = true
        layer?.backgroundColor = NSColor.controlBackgroundColor.cgColor
        for v in items {
            let h: CGFloat
            if let tf = v as? NSTextField, tf.isEditable {
                h = 24
            } else if v is RowStrip {
                h = 32
            } else if v is NSButton || v is NSPopUpButton {
                h = 32
            } else if v is NSTextField {
                h = 18
            } else {
                h = 24
            }
            v.frame = NSRect(x: x, y: y, width: w, height: h)
            y += h + 8
        }
    }
}

final class RowStrip: NSView {
    let views: [NSView]
    init(_ views: [NSView]) {
        self.views = views
        super.init(frame: .zero)
        for v in views {
            v.translatesAutoresizingMaskIntoConstraints = true
            addSubview(v)
        }
    }
    required init?(coder: NSCoder) { fatalError() }
    override func layout() {
        super.layout()
        let gap: CGFloat = 8
        let n = max(CGFloat(views.count), 1)
        let w = max((bounds.width - gap * (n - 1)) / n, 40)
        for (i, v) in views.enumerated() {
            v.frame = NSRect(x: CGFloat(i) * (w + gap), y: 0, width: w, height: bounds.height)
        }
    }
}

final class PreviewView: NSView {
    var image: NSImage? {
        didSet { needsDisplay = true }
    }
    /// Selection in image coordinates (origin bottom-left, same as NSImage).
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
    let hostField = NSTextField(string: "")
    let sourcePopup = NSPopUpButton()
    let colorPopup = NSPopUpButton()
    let dpiPopup = NSPopUpButton()
    let formatPopup = NSPopUpButton()
    let outputField = NSTextField(string: "")
    let status = NSTextField(labelWithString: "Add the scanner by IP or hostname, then Preview or Scan.")
    let logView = NSTextView()
    let preview = PreviewView()
    let left = ControlColumn()
    let scroll = NSScrollView()
    var addButton: NSButton!
    var discoverButton: NSButton!
    var previewButton: NSButton!
    var scanButton: NSButton!
    var addPrinterButton: NSButton!
    var lastExit: Int32 = 0

    func applicationDidFinishLaunching(_ notification: Notification) {
        let content = NSView(frame: NSRect(x: 0, y: 0, width: 980, height: 640))
        window = NSWindow(
            contentRect: content.frame,
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.title = "M177fw Scanner"
        window.contentView = content
        window.minSize = NSSize(width: 860, height: 560)
        window.delegate = self

        styleField(hostField)
        hostField.placeholderString = "192.168.50.14 or DEV26BA77.local"
        hostField.stringValue = "192.168.50.14"
        stylePopup(sourcePopup, titles: ["platen", "adf"])
        stylePopup(colorPopup, titles: ["color", "gray", "lineart"])
        stylePopup(dpiPopup, titles: ["100", "300", "600"])
        dpiPopup.selectItem(withTitle: "300")
        stylePopup(formatPopup, titles: ["jpeg", "pdf", "tiff"])
        formatPopup.target = self
        formatPopup.action = #selector(formatChanged)

        styleField(outputField)
        outputField.stringValue = Self.defaultDocumentsPath(ext: "jpg")
        outputField.placeholderString = "~/Documents/scan-<timestamp>.jpg"

        addButton = pushButton("Add scanner", #selector(addScanner))
        discoverButton = pushButton("Discover", #selector(discoverLan))
        addPrinterButton = pushButton("Add printer if missing", #selector(addPrinter))
        previewButton = pushButton("Preview", #selector(runPreview))
        previewButton.keyEquivalent = "p"
        scanButton = pushButton("Scan", #selector(runScan))
        scanButton.keyEquivalent = "\r"

        status.isEditable = false
        status.isBezeled = false
        status.drawsBackground = false
        status.lineBreakMode = .byWordWrapping
        status.maximumNumberOfLines = 4
        status.font = NSFont.systemFont(ofSize: 12)
        status.textColor = NSColor.secondaryLabelColor
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
        scroll.autoresizingMask = [.width]

        left.items = [
            heading("Device"),
            label("Host / IP"),
            hostField,
            RowStrip([discoverButton, addButton]),
            addPrinterButton,
            heading("Scan"),
            RowStrip([label("Source"), sourcePopup]),
            RowStrip([label("Color"), colorPopup]),
            RowStrip([label("DPI"), dpiPopup]),
            RowStrip([label("Format"), formatPopup]),
            label("Save to Documents"),
            outputField,
            RowStrip([previewButton, scanButton]),
            status,
        ]
        for v in left.items {
            v.translatesAutoresizingMaskIntoConstraints = true
            left.addSubview(v)
        }

        preview.wantsLayer = true
        content.addSubview(left)
        content.addSubview(preview)
        content.addSubview(scroll)
        applyChrome()

        if CommandLine.arguments.contains("--layout-check") {
            left.layoutSubtreeIfNeeded()
            let hf = hostField.convert(hostField.bounds, to: content)
            let pv = preview.convert(preview.bounds, to: content)
            let scan = scanButton.convert(scanButton.bounds, to: content)
            let disc = discoverButton.convert(discoverButton.bounds, to: content)
            let report = "hostField=\(Int(hf.minX)),\(Int(hf.minY)) \(Int(hf.width))x\(Int(hf.height)) preview=\(Int(pv.minX)),\(Int(pv.minY)) \(Int(pv.width))x\(Int(pv.height)) scan=\(Int(scan.minX)),\(Int(scan.minY)) \(Int(scan.width))x\(Int(scan.height)) discover=\(Int(disc.minX)),\(Int(disc.minY)) \(Int(disc.width))x\(Int(disc.height)) window=\(Int(content.bounds.width))x\(Int(content.bounds.height))\n"
            FileHandle.standardOutput.write(Data(report.utf8))
            let ok = hf.height >= 20 && hf.width >= 80 && hf.minX < 80
                && pv.minX >= 300
                && scan.height >= 20 && scan.width >= 40
                && disc.height >= 20 && disc.width >= 40
            exit(ok ? 0 : 1)
        }

        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }

    func windowDidResize(_ notification: Notification) {
        applyChrome()
    }

    func applyChrome() {
        guard let content = window.contentView else { return }
        let b = content.bounds
        let logH: CGFloat = 110
        let leftW: CGFloat = 340
        let pad: CGFloat = 16
        scroll.frame = NSRect(x: pad, y: pad, width: b.width - pad * 2, height: logH)
        let topY = pad + logH + 12
        let topH = max(b.height - topY - 8, 200)
        left.frame = NSRect(x: 0, y: topY, width: leftW, height: topH)
        preview.frame = NSRect(x: leftW + 8, y: topY, width: max(b.width - leftW - 8 - pad, 200), height: topH)
        left.needsLayout = true
        left.layoutSubtreeIfNeeded()
    }

    @objc func formatChanged() {
        let ext = formatPopup.titleOfSelectedItem ?? "jpeg"
        let mapped = ext == "jpeg" ? "jpg" : ext
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

    /// Real add+scan (not a stub). Same CLI path as the buttons.
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
        runHp(args)
        return lastExit
    }

    @objc func addScanner() {
        let host = hostField.stringValue.trimmingCharacters(in: .whitespaces)
        guard !host.isEmpty else {
            status.stringValue = "Enter a host or IP first."
            return
        }
        runHp(["add", host])
    }

    @objc func discoverLan() {
        runHp(["discover", "--timeout", "3"])
    }

    @objc func runPreview() {
        let dest = NSTemporaryDirectory() + "hp-m177-preview.jpg"
        runHp([
            "scan",
            "--source", sourcePopup.titleOfSelectedItem ?? "platen",
            "--color", colorPopup.titleOfSelectedItem ?? "color",
            "--dpi", "100",
            "--format", "jpeg",
            "--output", dest,
        ])
        if lastExit == 0, let img = NSImage(contentsOfFile: dest) {
            preview.image = img
            preview.selection = nil
            status.stringValue = "Preview ready. Drag a rectangle, then Scan."
        }
    }

    @objc func runScan() {
        var args = [
            "scan",
            "--source", sourcePopup.titleOfSelectedItem ?? "platen",
            "--color", colorPopup.titleOfSelectedItem ?? "color",
            "--dpi", dpiPopup.titleOfSelectedItem ?? "300",
            "--format", formatPopup.titleOfSelectedItem ?? "jpeg",
        ]
        let out = outputField.stringValue.trimmingCharacters(in: .whitespaces)
        if !out.isEmpty {
            args += ["--output", out]
        }
        if let region = regionThousandths() {
            args += ["--region", region]
        }
        runHp(args)
        if lastExit == 0, let img = NSImage(contentsOfFile: out) {
            preview.image = img
        }
    }

    func regionThousandths() -> String? {
        guard let sel = preview.selection, let image = preview.image,
              sel.width > 4, sel.height > 4,
              image.size.width > 0, image.size.height > 0
        else { return nil }
        // Image is a full-platen preview; firmware units are 1/1000 inch.
        let mediaW = 8500.0
        let mediaH = 11690.0
        let x = sel.minX / image.size.width * mediaW
        // NSImage origin is bottom-left; firmware ScanRegionYOffset is top-left.
        let yTop = (image.size.height - sel.maxY) / image.size.height * mediaH
        let w = sel.width / image.size.width * mediaW
        let h = sel.height / image.size.height * mediaH
        return "\(Int(x.rounded())),\(Int(yTop.rounded())),\(Int(w.rounded())),\(Int(h.rounded()))"
    }

    @objc func addPrinter() {
        var args = ["add-printer"]
        let host = hostField.stringValue.trimmingCharacters(in: .whitespaces)
        if !host.isEmpty { args.append(host) }
        runHp(args)
    }

    func runHp(_ args: [String]) {
        let bin = ProcessInfo.processInfo.environment["HP_M177_BIN"]
            ?? bundledBinary()
            ?? "hp-m177"
        status.stringValue = "Running \(bin) \(args.joined(separator: " "))…"
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
            if window != nil {
                logView.string += "$ \(bin) \(args.joined(separator: " "))\n\(text)\n"
            }
            lastExit = proc.terminationStatus
            FileHandle.standardOutput.write(data)
            status.stringValue = lastExit == 0 ? "Done." : "Failed (exit \(lastExit))."
        } catch {
            lastExit = 1
            status.stringValue = "Could not start hp-m177: \(error.localizedDescription)"
            fputs("\(status.stringValue)\n", stderr)
        }
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

    func heading(_ text: String) -> NSTextField {
        let l = NSTextField(labelWithString: text)
        l.font = NSFont.boldSystemFont(ofSize: 13)
        l.textColor = NSColor.labelColor
        return l
    }

    func label(_ text: String) -> NSTextField {
        let l = NSTextField(labelWithString: text)
        l.font = NSFont.systemFont(ofSize: 12)
        l.textColor = NSColor.secondaryLabelColor
        return l
    }

    func pushButton(_ title: String, _ sel: Selector) -> NSButton {
        let b = NSButton(title: title, target: self, action: sel)
        b.bezelStyle = .rounded
        b.setButtonType(.momentaryPushIn)
        b.translatesAutoresizingMaskIntoConstraints = true
        return b
    }

    func styleField(_ field: NSTextField) {
        field.isEditable = true
        field.isBezeled = true
        field.bezelStyle = .squareBezel
        field.font = NSFont.systemFont(ofSize: 13)
        field.setContentHuggingPriority(.defaultLow, for: .horizontal)
    }

    func stylePopup(_ popup: NSPopUpButton, titles: [String]) {
        popup.addItems(withTitles: titles)
        popup.bezelStyle = .rounded
        popup.setContentHuggingPriority(.defaultHigh, for: .horizontal)
    }

}
