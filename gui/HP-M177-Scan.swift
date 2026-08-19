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
    FileHandle.standardOutput.write(Data("gui-native-smoke-ok\n".utf8))
    exit(0)
}

if let idx = args.firstIndex(of: "--exec"), args.index(after: idx) < args.endIndex {
    exit(AppDelegate.exec(Array(args[(idx + 1)...])))
}

let delegate = AppDelegate()
let app = NSApplication.shared
app.setActivationPolicy(.regular)
app.delegate = delegate
app.run()

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

final class AppDelegate: NSObject, NSApplicationDelegate {
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
        window.minSize = NSSize(width: 820, height: 520)

        hostField.placeholderString = "192.168.50.14 or DEV26BA77.local"
        sourcePopup.addItems(withTitles: ["platen", "adf"])
        colorPopup.addItems(withTitles: ["color", "gray", "lineart"])
        dpiPopup.addItems(withTitles: ["100", "300", "600"])
        dpiPopup.selectItem(withTitle: "300")
        formatPopup.addItems(withTitles: ["jpeg", "pdf", "tiff"])
        formatPopup.target = self
        formatPopup.action = #selector(formatChanged)

        let docs = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first
            ?? URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent("Documents")
        outputField.stringValue = docs.appendingPathComponent("scan.jpg").path
        outputField.placeholderString = "~/Documents/scan.jpg"

        let add = NSButton(title: "Add scanner", target: self, action: #selector(addScanner))
        let discover = NSButton(title: "Discover", target: self, action: #selector(discover))
        let addPrinter = NSButton(title: "Add printer if missing", target: self, action: #selector(addPrinter))
        let previewBtn = NSButton(title: "Preview", target: self, action: #selector(runPreview))
        previewBtn.keyEquivalent = "p"
        let scan = NSButton(title: "Scan", target: self, action: #selector(runScan))
        scan.keyEquivalent = "\r"
        scan.bezelStyle = .rounded

        status.isEditable = false
        status.lineBreakMode = .byWordWrapping
        status.maximumNumberOfLines = 3
        logView.isEditable = false
        logView.font = NSFont.monospacedSystemFont(ofSize: 11, weight: .regular)
        logView.backgroundColor = NSColor.textBackgroundColor

        let controls = NSStackView(views: [
            heading("Device"),
            label("Host / IP"), hostField,
            row([discover, add, addPrinter]),
            heading("Scan"),
            row([label("Source"), sourcePopup, label("Color"), colorPopup]),
            row([label("DPI"), dpiPopup, label("Format"), formatPopup]),
            label("Save to (Documents by default)"), outputField,
            row([previewBtn, scan]),
            status,
        ])
        controls.orientation = .vertical
        controls.alignment = .leading
        controls.spacing = 8
        controls.translatesAutoresizingMaskIntoConstraints = false

        preview.translatesAutoresizingMaskIntoConstraints = false
        preview.wantsLayer = true

        let scroll = NSScrollView(frame: .zero)
        scroll.documentView = logView
        scroll.hasVerticalScroller = true
        scroll.translatesAutoresizingMaskIntoConstraints = false
        scroll.borderType = .bezelBorder

        content.addSubview(controls)
        content.addSubview(preview)
        content.addSubview(scroll)

        NSLayoutConstraint.activate([
            controls.topAnchor.constraint(equalTo: content.topAnchor, constant: 16),
            controls.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 16),
            controls.widthAnchor.constraint(equalToConstant: 340),
            hostField.widthAnchor.constraint(equalTo: controls.widthAnchor),
            outputField.widthAnchor.constraint(equalTo: controls.widthAnchor),

            preview.topAnchor.constraint(equalTo: content.topAnchor, constant: 16),
            preview.leadingAnchor.constraint(equalTo: controls.trailingAnchor, constant: 16),
            preview.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -16),
            preview.bottomAnchor.constraint(equalTo: scroll.topAnchor, constant: -12),

            scroll.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 16),
            scroll.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -16),
            scroll.bottomAnchor.constraint(equalTo: content.bottomAnchor, constant: -16),
            scroll.heightAnchor.constraint(equalToConstant: 120),
        ])

        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }

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
        return docs.appendingPathComponent("scan.\(ext)").path
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

    @objc func discover() {
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
        let sibling = (here as NSString).deletingLastPathComponent + "/hp-m177"
        let cargo = NSHomeDirectory() + "/.cargo/bin/hp-m177"
        if FileManager.default.isExecutableFile(atPath: sibling) { return sibling }
        if FileManager.default.isExecutableFile(atPath: cargo) { return cargo }
        return nil
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

    func row(_ views: [NSView]) -> NSStackView {
        let s = NSStackView(views: views)
        s.orientation = .horizontal
        s.spacing = 8
        return s
    }
}
