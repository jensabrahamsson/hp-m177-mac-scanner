import AppKit
import Foundation

/// Native AppKit GUI for the M177fw scanner.
///
/// Add and scan go through the `hp-m177` CLI (same functions the tests drive).
/// `HP_M177_BIN` overrides the binary path.
///
/// Headless:
///   HP-M177-Scan --exec add --host <printer-ip>
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

final class PreviewView: NSView {
    var image: NSImage? {
        didSet {
            needsDisplay = true
            display()
        }
    }
    var message: String = "Preview — Scan Preview, then drag a region" {
        didSet {
            needsDisplay = true
            display()
        }
    }
    var selection: NSRect? {
        didSet { needsDisplay = true }
    }
    var dragStart: NSPoint?

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = false
        translatesAutoresizingMaskIntoConstraints = true
        autoresizingMask = []
    }

    required init?(coder: NSCoder) { fatalError() }
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
            let attrs: [NSAttributedString.Key: Any] = [
                .foregroundColor: NSColor.secondaryLabelColor,
                .font: NSFont.systemFont(ofSize: 13),
            ]
            let size = message.size(withAttributes: attrs)
            message.draw(
                at: NSPoint(x: max((bounds.width - size.width) / 2, 12), y: (bounds.height - size.height) / 2),
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

final class RootView: NSView {
    weak var app: AppDelegate!
    var frames: [String: NSRect] = [:]

    override var isOpaque: Bool { true }
    override var wantsUpdateLayer: Bool { true }
    override func updateLayer() {
        layer?.backgroundColor = NSColor.windowBackgroundColor.cgColor
    }

    override func layout() {
        super.layout()
        layoutChrome()
    }

    func layoutChrome() {
        guard let app, bounds.width > 80, bounds.height > 80 else { return }
        let logH: CGFloat = app.logVisible ? 120 : 0
        let topH: CGFloat = 32
        let topY = bounds.height - 16 - topH
        let gap: CGFloat = 8
        let btnW = max((bounds.width - 32 - 3 * gap) / 4, 80)
        frames["discover"] = NSRect(x: 16, y: topY, width: btnW, height: topH)
        frames["add"] = NSRect(x: 16 + btnW + gap, y: topY, width: btnW, height: topH)
        frames["preview"] = NSRect(x: 16 + 2 * (btnW + gap), y: topY, width: btnW, height: topH)
        frames["scan"] = NSRect(x: 16 + 3 * (btnW + gap), y: topY, width: btnW, height: topH)
        app.discoverButton.frame = frames["discover"] ?? .zero
        app.addButton.frame = frames["add"] ?? .zero
        app.previewButton.frame = frames["preview"] ?? .zero
        app.scanButton.frame = frames["scan"] ?? .zero

        let colX: CGFloat = 20
        let colW: CGFloat = 280
        var y = topY - 28
        func place(_ v: NSView, _ h: CGFloat, gapAfter: CGFloat = 6) {
            y -= h
            v.frame = NSRect(x: colX, y: y, width: colW, height: h)
            y -= gapAfter
        }
        place(app.deviceHeading, 18, gapAfter: 4)
        place(app.hostLabel, 16, gapAfter: 4)
        place(app.hostField, 22)
        place(app.addPrinterButton, 28, gapAfter: 16)
        place(app.scanHeading, 18, gapAfter: 6)
        place(app.sourcePopup, 26)
        place(app.colorPopup, 26)
        place(app.dpiPopup, 26)
        place(app.formatPopup, 26, gapAfter: 12)
        place(app.saveLabel, 16, gapAfter: 4)
        place(app.outputField, 22)
        place(app.logToggle, 18, gapAfter: 8)
        place(app.statusField, 36, gapAfter: 0)

        let previewBottom: CGFloat = app.logVisible ? 16 + logH + 8 : 16
        app.preview.frame = NSRect(
            x: colX + colW + 20,
            y: previewBottom,
            width: max(bounds.width - (colX + colW + 20) - 16, 200),
            height: max(topY - 12 - previewBottom, 200)
        )
        app.scroll.isHidden = !app.logVisible
        if app.logVisible {
            app.scroll.frame = NSRect(x: 16, y: 12, width: bounds.width - 32, height: logH)
        }
        app.discoverButton.isEnabled = !app.busy
        app.addButton.isEnabled = !app.busy
        app.previewButton.isEnabled = !app.busy
        app.scanButton.isEnabled = !app.busy
        app.addPrinterButton.isEnabled = !app.busy
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate {
    var window: NSWindow!
    var root: RootView!
    var discoverButton: NSButton!
    var addButton: NSButton!
    var previewButton: NSButton!
    var scanButton: NSButton!
    var addPrinterButton: NSButton!
    var logToggle: NSButton!
    var logMenuItem: NSMenuItem!
    let hostField = NSTextField(string: "")
    let outputField = NSTextField(string: "")
    let statusField = NSTextField(labelWithString: "")
    let deviceHeading = NSTextField(labelWithString: "Device")
    let hostLabel = NSTextField(labelWithString: "Host / IP")
    let scanHeading = NSTextField(labelWithString: "Scan")
    let saveLabel = NSTextField(labelWithString: "Save to Documents")
    let sourcePopup = NSPopUpButton()
    let colorPopup = NSPopUpButton()
    let dpiPopup = NSPopUpButton()
    let formatPopup = NSPopUpButton()
    let logView = NSTextView()
    let preview = PreviewView()
    let scroll = NSScrollView()
    var logVisible = false
    var lastExit: Int32 = 0
    var busy = false

    var source: String { sourcePopup.titleOfSelectedItem ?? "platen" }
    var color: String { colorPopup.titleOfSelectedItem ?? "color" }
    var dpi: String { dpiPopup.titleOfSelectedItem ?? "100" }
    var format: String { formatPopup.titleOfSelectedItem ?? "jpeg" }

    func applicationWillFinishLaunching(_ notification: Notification) {
        buildMainMenu()
    }

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
        root.wantsLayer = true
        window.contentView = root

        discoverButton = macButton("Discover", #selector(discoverLan))
        addButton = macButton("Add Scanner", #selector(addScanner))
        previewButton = macButton("Preview", #selector(runPreview))
        previewButton.keyEquivalent = "p"
        scanButton = macButton("Scan", #selector(runScan))
        scanButton.keyEquivalent = "\r"
        addPrinterButton = macButton("Add Printer if Missing", #selector(addPrinter))
        logToggle = NSButton(checkboxWithTitle: "Show Log", target: self, action: #selector(toggleLog(_:)))
        logToggle.state = .off

        styleHeading(deviceHeading)
        styleHeading(scanHeading)
        styleCaption(hostLabel)
        styleCaption(saveLabel)
        statusField.font = NSFont.systemFont(ofSize: 11)
        statusField.textColor = NSColor.secondaryLabelColor
        statusField.lineBreakMode = .byWordWrapping
        statusField.maximumNumberOfLines = 3
        statusField.stringValue = "Add the scanner, then Preview or Scan."

        styleField(hostField)
        hostField.placeholderString = "IPv4 or hostname.local"
        hostField.stringValue = Self.savedHost() ?? ""
        hostField.target = self
        hostField.action = #selector(addScanner)
        styleField(outputField)
        outputField.stringValue = Self.defaultDocumentsPath(ext: "jpg")
        outputField.placeholderString = "~/Documents/scan-<timestamp>.jpg"

        fillPopup(sourcePopup, ["platen", "adf"], "platen")
        fillPopup(colorPopup, ["color", "gray", "lineart"], "color")
        fillPopup(dpiPopup, ["100", "300", "600"], "100")
        fillPopup(formatPopup, ["jpeg", "pdf", "tiff"], "jpeg")
        formatPopup.target = self
        formatPopup.action = #selector(formatChanged)

        logView.isEditable = false
        logView.font = NSFont.monospacedSystemFont(ofSize: 11, weight: .regular)
        logView.backgroundColor = NSColor.textBackgroundColor
        logView.textColor = NSColor.textColor
        logView.minSize = NSSize(width: 0, height: 0)
        logView.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        logView.isHorizontallyResizable = false
        logView.isVerticallyResizable = true
        logView.autoresizingMask = [.width]
        logView.textContainer?.widthTracksTextView = true
        scroll.documentView = logView
        scroll.hasVerticalScroller = true
        scroll.borderType = .bezelBorder
        scroll.isHidden = true

        let views: [NSView] = [
            discoverButton!, addButton!, previewButton!, scanButton!, addPrinterButton!,
            deviceHeading, hostLabel, hostField, scanHeading, saveLabel, statusField,
            sourcePopup, colorPopup, dpiPopup, formatPopup, logToggle!,
            preview, scroll, outputField,
        ]
        for v in views {
            v.translatesAutoresizingMaskIntoConstraints = true
            root.addSubview(v)
        }
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
        root.layoutChrome()
    }

    func buildMainMenu() {
        let appName = "HP M177 Scanner"
        let main = NSMenu()

        let appMenu = NSMenu()
        appMenu.addItem(menuItem("About \(appName)", #selector(showAbout(_:))))
        appMenu.addItem(NSMenuItem.separator())
        let ver = NSMenuItem(title: "Version \(versionString())", action: nil, keyEquivalent: "")
        ver.isEnabled = false
        appMenu.addItem(ver)
        appMenu.addItem(NSMenuItem.separator())
        appMenu.addItem(withTitle: "Hide \(appName)", action: #selector(NSApplication.hide(_:)), keyEquivalent: "h")
        let hideOthers = NSMenuItem(title: "Hide Others", action: #selector(NSApplication.hideOtherApplications(_:)), keyEquivalent: "h")
        hideOthers.keyEquivalentModifierMask = [.command, .option]
        appMenu.addItem(hideOthers)
        appMenu.addItem(withTitle: "Show All", action: #selector(NSApplication.unhideAllApplications(_:)), keyEquivalent: "")
        appMenu.addItem(NSMenuItem.separator())
        appMenu.addItem(withTitle: "Quit \(appName)", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        let appItem = NSMenuItem()
        appItem.submenu = appMenu
        main.addItem(appItem)

        let scanMenu = NSMenu(title: "Scan")
        scanMenu.addItem(menuItem("Discover", #selector(discoverLan), "d"))
        scanMenu.addItem(menuItem("Add Scanner", #selector(addScanner), "a"))
        scanMenu.addItem(menuItem("Preview", #selector(runPreview), "p"))
        scanMenu.addItem(menuItem("Scan", #selector(runScan), "s"))
        scanMenu.addItem(NSMenuItem.separator())
        scanMenu.addItem(menuItem("Add Printer if Missing", #selector(addPrinter)))
        let scanItem = NSMenuItem(title: "Scan", action: nil, keyEquivalent: "")
        scanItem.submenu = scanMenu
        main.addItem(scanItem)

        let viewMenu = NSMenu(title: "View")
        logMenuItem = menuItem("Show Log", #selector(toggleLog(_:)), "l")
        viewMenu.addItem(logMenuItem)
        let viewItem = NSMenuItem(title: "View", action: nil, keyEquivalent: "")
        viewItem.submenu = viewMenu
        main.addItem(viewItem)

        let windowMenu = NSMenu(title: "Window")
        windowMenu.addItem(withTitle: "Minimize", action: #selector(NSWindow.performMiniaturize(_:)), keyEquivalent: "m")
        windowMenu.addItem(withTitle: "Zoom", action: #selector(NSWindow.performZoom(_:)), keyEquivalent: "")
        let windowItem = NSMenuItem(title: "Window", action: nil, keyEquivalent: "")
        windowItem.submenu = windowMenu
        main.addItem(windowItem)

        let helpMenu = NSMenu(title: "Help")
        helpMenu.addItem(menuItem("\(appName) Help", #selector(showHelp(_:)), "?"))
        let helpItem = NSMenuItem(title: "Help", action: nil, keyEquivalent: "")
        helpItem.submenu = helpMenu
        main.addItem(helpItem)

        NSApp.mainMenu = main
        NSApp.windowsMenu = windowMenu
        NSApp.helpMenu = helpMenu
    }

    func menuItem(_ title: String, _ sel: Selector, _ key: String = "") -> NSMenuItem {
        let i = NSMenuItem(title: title, action: sel, keyEquivalent: key)
        i.target = self
        return i
    }

    func versionString() -> String {
        let short = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.1.0"
        let build = Bundle.main.infoDictionary?["CFBundleVersion"] as? String ?? "9"
        return "\(short) (\(build))"
    }

    @objc func showAbout(_ sender: Any?) {
        let credits = NSAttributedString(
            string: "Scan client for the HP Color LaserJet Pro MFP M177fw.\nNot an HP product. No affiliation with HP Inc.",
            attributes: [.font: NSFont.systemFont(ofSize: 11)]
        )
        NSApp.orderFrontStandardAboutPanel(options: [
            .applicationName: "HP M177 Scanner",
            .version: versionString(),
            .credits: credits,
        ])
    }

    @objc func showHelp(_ sender: Any?) {
        let alert = NSAlert()
        alert.messageText = "HP M177 Scanner"
        alert.informativeText = """
        1. Enter the printer’s IP or hostname and choose Add Scanner.
        2. Preview the glass. Drag a rectangle to crop.
        3. Scan writes ~/Documents/scan-<timestamp>.<ext>.

        Scan → Discover / Add Scanner / Preview / Scan
        View → Show Log hides hp-m177 command output (off by default).

        Default preview/scan DPI is 100. 300 dpi can take a minute.

        This is not an HP product.
        """
        alert.addButton(withTitle: "OK")
        alert.runModal()
    }

    @objc func toggleLog(_ sender: Any?) {
        logVisible.toggle()
        logToggle.state = logVisible ? .on : .off
        logMenuItem.state = logVisible ? .on : .off
        logMenuItem.title = logVisible ? "Hide Log" : "Show Log"
        root.layoutChrome()
    }

    func macButton(_ title: String, _ sel: Selector) -> NSButton {
        let b = NSButton(title: title, target: self, action: sel)
        b.bezelStyle = .rounded
        b.setButtonType(.momentaryPushIn)
        b.translatesAutoresizingMaskIntoConstraints = true
        return b
    }

    func fillPopup(_ popup: NSPopUpButton, _ titles: [String], _ selected: String) {
        popup.removeAllItems()
        popup.addItems(withTitles: titles)
        popup.selectItem(withTitle: selected)
        popup.bezelStyle = .rounded
        popup.translatesAutoresizingMaskIntoConstraints = true
    }

    func styleHeading(_ field: NSTextField) {
        field.font = NSFont.boldSystemFont(ofSize: 13)
        field.textColor = NSColor.labelColor
        field.isBezeled = false
        field.drawsBackground = false
        field.isEditable = false
    }

    func styleCaption(_ field: NSTextField) {
        field.font = NSFont.systemFont(ofSize: 11)
        field.textColor = NSColor.secondaryLabelColor
        field.isBezeled = false
        field.drawsBackground = false
        field.isEditable = false
    }

    func styleField(_ field: NSTextField) {
        field.isEditable = true
        field.isBezeled = true
        field.bezelStyle = .roundedBezel
        field.drawsBackground = true
        field.backgroundColor = NSColor.textBackgroundColor
        field.textColor = NSColor.textColor
        field.font = NSFont.systemFont(ofSize: 13)
        field.translatesAutoresizingMaskIntoConstraints = true
    }

    func setStatus(_ text: String) {
        statusField.stringValue = text
    }

    func activate(_ key: String) {
        switch key {
        case "discover": discoverLan()
        case "add": addScanner()
        case "preview": runPreview()
        case "scan": runScan()
        case "addPrinter": addPrinter()
        default: break
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
        window.displayIfNeeded()

        let hf = hostField.frame
        let pv = preview.frame
        let scan = scanButton.frame
        let disc = discoverButton.frame
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
                NSColor.windowBackgroundColor.setFill()
                NSBezierPath(rect: root.bounds).fill()
                for v in root.subviews {
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
            && NSApp.mainMenu != nil
        if !ok {
            fputs("layout-check failed (buttons missing or not drawn)\n", stderr)
        }
        return ok ? 0 : 1
    }

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
        activate("add")
        if lastExit != 0 { return lastExit }
        activate("preview")
        if lastExit != 0 { return lastExit }
        if preview.image == nil {
            fputs("button-smoke: preview produced no image\n", stderr)
            return 1
        }
        activate("scan")
        if lastExit != 0 { return lastExit }
        if preview.selection != nil {
            fputs("button-smoke: crop overlay still set after scan\n", stderr)
            return 1
        }
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

    static func savedHost() -> String? {
        let dir = ProcessInfo.processInfo.environment["HP_M177_HOME"]
            ?? (NSHomeDirectory() + "/Library/Application Support/hp-m177")
        let url = URL(fileURLWithPath: dir).appendingPathComponent("devices.json")
        guard let data = try? Data(contentsOf: url),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let devices = obj["devices"] as? [[String: Any]]
        else { return nil }
        let def = obj["default_id"] as? String
        let rec = devices.first { $0["id"] as? String == def } ?? devices.first
        return rec?["host"] as? String
    }

    static func imageFromScan(_ path: String) -> NSImage? {
        guard let data = try? Data(contentsOf: URL(fileURLWithPath: path)), data.count > 128 else {
            return nil
        }
        if let img = NSImage(data: data), img.size.width >= 1, img.size.height >= 1 {
            img.isTemplate = false
            return img
        }
        guard let rep = NSBitmapImageRep(data: data) else { return nil }
        let img = NSImage(size: NSSize(width: CGFloat(rep.pixelsWide), height: CGFloat(rep.pixelsHigh)))
        img.addRepresentation(rep)
        return img
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
            setStatus("Enter a host or IP first.")
            return
        }
        runHpAsync(["add", host])
    }

    @objc func discoverLan() {
        runHpAsync(["discover", "--timeout", "3"])
    }

    @objc func runPreview() {
        let dest = NSTemporaryDirectory() + "hp-m177-preview-\(Int(Date().timeIntervalSince1970)).jpg"
        preview.message = "Scanning preview…"
        preview.image = nil
        preview.selection = nil
        runHpAsync([
            "scan",
            "--source", source,
            "--color", color,
            "--dpi", "100",
            "--format", "jpeg",
            "--output", dest,
        ]) { [weak self] code, text in
            guard let self else { return }
            if code == 0, let img = Self.imageFromScan(dest) {
                self.preview.image = img
                self.preview.selection = nil
                self.setStatus("Preview ready. Drag a rectangle, then Scan.")
            } else {
                let tail = text.split(whereSeparator: \.isNewline).suffix(2).joined(separator: " ")
                self.preview.image = nil
                self.preview.selection = nil
                self.preview.message = "Preview failed (exit \(code)). \(tail)"
                self.setStatus(self.preview.message)
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
        runHpAsync(args) { [weak self] code, text in
            guard let self else { return }
            self.preview.selection = nil
            if code == 0, !out.isEmpty, let img = Self.imageFromScan(out) {
                self.preview.image = img
                self.preview.selection = nil
                self.setStatus("Done.")
            } else if code != 0 {
                let tail = text.split(whereSeparator: \.isNewline).suffix(2).joined(separator: " ")
                self.preview.message = "Scan failed (exit \(code)). \(tail)"
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

    func runHpAsync(_ args: [String], done: ((Int32, String) -> Void)? = nil) {
        if CommandLine.arguments.contains("--button-smoke") {
            let code = runHpStatus(args)
            done?(code, "")
            return
        }
        if busy { return }
        busy = true
        setStatus("Running hp-m177 \(args.joined(separator: " "))…")
        root.layoutChrome()
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self else { return }
            let (code, text) = self.spawnHp(args)
            DispatchQueue.main.async {
                self.busy = false
                self.lastExit = code
                self.appendLog("$ hp-m177 \(args.joined(separator: " "))\n\(text)\n")
                if code != 0 {
                    let tail = text.split(whereSeparator: \.isNewline).suffix(2).joined(separator: " ")
                    self.setStatus("Failed (exit \(code)). \(tail)")
                } else if done == nil {
                    self.setStatus("Done.")
                }
                self.root.layoutChrome()
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
        if !logVisible, text.contains("Failed") || text.contains("error") {
            // Keep log hidden; status line already shows the failure.
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
}
