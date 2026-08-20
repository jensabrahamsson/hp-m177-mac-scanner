import AppKit
import Darwin
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

let productName = "HP Color LaserJet Pro MFP M177fw"
let appName = "HP Color LaserJet Pro MFP M177fw Scanner"

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

/// Same drawing path as PreviewView, which the user can see. The window
/// content view's draw(_:) is not composited on layer-backed macOS.
func paintPrepare(_ v: NSView) {
    v.wantsLayer = true
    v.layerContentsRedrawPolicy = .onSetNeedsDisplay
    v.translatesAutoresizingMaskIntoConstraints = true
    v.autoresizingMask = []
}

final class ChromeButton: NSView {
    enum Kind { case accent, plain }
    var title: String
    var key: String
    var kind: Kind
    weak var owner: RootView?
    var pressed = false

    init(title: String, key: String, kind: Kind = .accent) {
        self.title = title
        self.key = key
        self.kind = kind
        super.init(frame: .zero)
        paintPrepare(self)
    }

    required init?(coder: NSCoder) { fatalError() }
    override var isOpaque: Bool { true }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func draw(_ dirtyRect: NSRect) {
        let disabled = owner?.app?.busy == true
        let path = NSBezierPath(roundedRect: bounds.insetBy(dx: 0.5, dy: 0.5), xRadius: 6, yRadius: 6)
        let fill: NSColor
        let stroke: NSColor
        let fg: NSColor
        if kind == .plain {
            fill = pressed ? NSColor(srgbRed: 0.86, green: 0.86, blue: 0.88, alpha: 1)
                : NSColor(srgbRed: 0.94, green: 0.94, blue: 0.96, alpha: 1)
            stroke = NSColor(srgbRed: 0.72, green: 0.72, blue: 0.75, alpha: 1)
            fg = NSColor(srgbRed: 0.12, green: 0.12, blue: 0.14, alpha: 1)
        } else if disabled {
            fill = NSColor(srgbRed: 0.55, green: 0.62, blue: 0.78, alpha: 1)
            stroke = fill
            fg = .white
        } else if pressed {
            fill = NSColor.controlAccentColor.blended(withFraction: 0.2, of: .black) ?? NSColor.controlAccentColor
            stroke = fill
            fg = .white
        } else {
            fill = NSColor.controlAccentColor
            stroke = fill
            fg = .white
        }
        fill.setFill()
        path.fill()
        stroke.setStroke()
        path.lineWidth = 1
        path.stroke()
        let attrs: [NSAttributedString.Key: Any] = [
            .foregroundColor: fg,
            .font: NSFont.systemFont(ofSize: 13, weight: .medium),
        ]
        let size = title.size(withAttributes: attrs)
        title.draw(
            at: NSPoint(x: (bounds.width - size.width) / 2, y: (bounds.height - size.height) / 2),
            withAttributes: attrs
        )
    }

    override func mouseDown(with event: NSEvent) {
        guard owner?.app?.busy != true else { return }
        pressed = true
        needsDisplay = true
    }

    override func mouseUp(with event: NSEvent) {
        let inside = bounds.contains(convert(event.locationInWindow, from: nil))
        pressed = false
        needsDisplay = true
        if inside { owner?.activate(key) }
    }
}

final class ChromeLabel: NSView {
    var text: String
    var bold: Bool
    var secondary: Bool
    var error: Bool

    init(_ text: String, bold: Bool = false, secondary: Bool = false, error: Bool = false) {
        self.text = text
        self.bold = bold
        self.secondary = secondary
        self.error = error
        super.init(frame: .zero)
        paintPrepare(self)
    }

    required init?(coder: NSCoder) { fatalError() }

    override func draw(_ dirtyRect: NSRect) {
        NSColor.windowBackgroundColor.setFill()
        bounds.fill()
        let paragraph = NSMutableParagraphStyle()
        paragraph.lineBreakMode = .byWordWrapping
        let fg: NSColor
        if error {
            fg = NSColor.systemRed
        } else if secondary {
            fg = NSColor(srgbRed: 0.35, green: 0.35, blue: 0.38, alpha: 1)
        } else {
            fg = NSColor(srgbRed: 0.10, green: 0.10, blue: 0.12, alpha: 1)
        }
        let attrs: [NSAttributedString.Key: Any] = [
            .font: bold ? NSFont.boldSystemFont(ofSize: 13) : NSFont.systemFont(ofSize: 12),
            .foregroundColor: fg,
            .paragraphStyle: paragraph,
        ]
        text.draw(in: bounds, withAttributes: attrs)
    }
}

final class ChromeCycle: NSView {
    var caption: String
    var key: String
    var value: String
    weak var owner: RootView?

    init(caption: String, key: String, value: String) {
        self.caption = caption
        self.key = key
        self.value = value
        super.init(frame: .zero)
        paintPrepare(self)
    }

    required init?(coder: NSCoder) { fatalError() }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func draw(_ dirtyRect: NSRect) {
        let path = NSBezierPath(roundedRect: bounds.insetBy(dx: 0.5, dy: 0.5), xRadius: 5, yRadius: 5)
        NSColor(srgbRed: 0.95, green: 0.95, blue: 0.97, alpha: 1).setFill()
        path.fill()
        NSColor(srgbRed: 0.45, green: 0.45, blue: 0.50, alpha: 1).setStroke()
        path.lineWidth = 1
        path.stroke()
        let capAttrs: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 11),
            .foregroundColor: NSColor(srgbRed: 0.35, green: 0.35, blue: 0.38, alpha: 1),
        ]
        let valAttrs: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 13, weight: .medium),
            .foregroundColor: NSColor(srgbRed: 0.10, green: 0.10, blue: 0.12, alpha: 1),
        ]
        caption.draw(at: NSPoint(x: 8, y: bounds.height / 2 - 7), withAttributes: capAttrs)
        let val = "\(value)  ▾"
        let size = val.size(withAttributes: valAttrs)
        val.draw(at: NSPoint(x: bounds.width - size.width - 8, y: bounds.height / 2 - 8), withAttributes: valAttrs)
    }

    override func mouseUp(with event: NSEvent) {
        guard bounds.contains(convert(event.locationInWindow, from: nil)) else { return }
        owner?.activate(key)
    }
}

final class RootView: NSView {
    weak var app: AppDelegate!
    var frames: [String: NSRect] = [:]
    var chrome: [String: NSView] = [:]

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
        let b = bounds
        guard b.width > 80, b.height > 80, app != nil else { return }
        let logH: CGFloat = app.logVisible ? 110 : 0
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
        row("deviceHead", 32, gapAfter: 4)
        row("hostHead", 16, gapAfter: 4)
        row("hostField", 24)
        row("addToMacOS", 32, gapAfter: 14)
        row("scanHead", 18, gapAfter: 6)
        row("source", 28)
        row("color", 28)
        row("dpi", 28)
        row("format", 28, gapAfter: 12)
        row("saveHead", 16, gapAfter: 4)
        row("outputField", 24)
        row("logToggle", 28, gapAfter: 8)
        row("status", 40, gapAfter: 0)

        placeButton("discover", "Discover", kind: .plain)
        placeButton("add", "Add Scanner", kind: .plain)
        placeButton("preview", "Preview")
        placeButton("scan", "Scan")
        placeLabel("deviceHead", productName, bold: true)
        placeLabel("hostHead", "Host / IP", secondary: true)
        placeButton("addToMacOS", "Add Scanner to macOS", kind: .plain)
        placeLabel("scanHead", "Scan", bold: true)
        placeCycle("source", "Source", app.source)
        placeCycle("color", "Color", app.color)
        placeCycle("dpi", "DPI", app.dpi)
        placeCycle("format", "Format", app.format)
        placeLabel("saveHead", "Save to Documents", secondary: true)
        placeButton("logToggle", app.logVisible ? "Hide Log" : "Show Log", kind: .plain)
        placeLabel("status", app.statusText, secondary: true, error: app.statusIsError)

        app.hostField.frame = frames["hostField"] ?? .zero
        app.outputField.frame = frames["outputField"] ?? .zero
        let previewY: CGFloat = app.logVisible ? logH + 12 : 16
        let previewTop = topY - 12
        app.preview.frame = NSRect(
            x: colX + colW + 16,
            y: previewY,
            width: max(b.width - (colX + colW + 16) - 16, 200),
            height: max(previewTop - previewY, 200)
        )
        app.scroll.isHidden = !app.logVisible
        if app.logVisible {
            app.scroll.frame = NSRect(x: 16, y: 12, width: b.width - 32, height: logH)
        }
        refresh()
    }

    func placeButton(_ key: String, _ title: String, kind: ChromeButton.Kind = .accent) {
        let v: ChromeButton
        if let existing = chrome[key] as? ChromeButton {
            v = existing
            v.title = title
            v.kind = kind
        } else {
            v = ChromeButton(title: title, key: key, kind: kind)
            v.owner = self
            addSubview(v)
            chrome[key] = v
        }
        v.frame = frames[key] ?? .zero
        v.needsDisplay = true
    }

    func placeLabel(_ key: String, _ text: String, bold: Bool = false, secondary: Bool = false, error: Bool = false) {
        let v: ChromeLabel
        if let existing = chrome[key] as? ChromeLabel {
            v = existing
            v.text = text
            v.bold = bold
            v.secondary = secondary
            v.error = error
        } else {
            v = ChromeLabel(text, bold: bold, secondary: secondary, error: error)
            addSubview(v)
            chrome[key] = v
        }
        v.frame = frames[key] ?? .zero
        v.needsDisplay = true
    }

    func placeCycle(_ key: String, _ caption: String, _ value: String) {
        let v: ChromeCycle
        if let existing = chrome[key] as? ChromeCycle {
            v = existing
            v.value = value
        } else {
            v = ChromeCycle(caption: caption, key: key, value: value)
            v.owner = self
            addSubview(v)
            chrome[key] = v
        }
        v.frame = frames[key] ?? .zero
        v.needsDisplay = true
    }

    func refresh() {
        if let status = chrome["status"] as? ChromeLabel {
            status.text = app.statusText
            status.error = app.statusIsError
            status.needsDisplay = true
        }
        if let c = chrome["source"] as? ChromeCycle { c.value = app.source; c.needsDisplay = true }
        if let c = chrome["color"] as? ChromeCycle { c.value = app.color; c.needsDisplay = true }
        if let c = chrome["dpi"] as? ChromeCycle { c.value = app.dpi; c.needsDisplay = true }
        if let c = chrome["format"] as? ChromeCycle { c.value = app.format; c.needsDisplay = true }
        for key in ["discover", "add", "preview", "scan", "addToMacOS"] {
            chrome[key]?.needsDisplay = true
        }
    }

    func activate(_ key: String) {
        guard let app else { return }
        switch key {
        case "discover": app.discoverLan()
        case "add": app.addScanner()
        case "preview": app.runPreview()
        case "scan": app.runScan()
        case "addToMacOS": app.addToMacOS()
        case "addPrinter": app.addPrinter()
        case "source": app.cycle(&app.source, ["platen", "adf"])
        case "color": app.cycle(&app.color, ["color", "gray", "lineart"])
        case "dpi": app.cycle(&app.dpi, ["100", "300", "600"])
        case "format":
            app.cycle(&app.format, ["jpeg", "pdf", "tiff"])
            app.formatChanged()
        case "logToggle":
            app.toggleLog()
        default: break
        }
        refresh()
    }
}

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
    var emptyArt: NSImage?

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = false
        translatesAutoresizingMaskIntoConstraints = true
        autoresizingMask = []
        emptyArt = PreviewView.loadEmptyArt()
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
            if let art = emptyArt, art.size.width > 1, art.size.height > 1 {
                let box = bounds.insetBy(dx: 28, dy: 40)
                let sx = box.width / art.size.width
                let sy = box.height / art.size.height
                let scale = min(sx, sy)
                let w = art.size.width * scale
                let h = art.size.height * scale
                let r = NSRect(
                    x: bounds.midX - w / 2,
                    y: bounds.midY - h / 2 + 10,
                    width: w,
                    height: h
                )
                art.draw(in: r, from: .zero, operation: .sourceOver, fraction: 1)
            }
            let attrs: [NSAttributedString.Key: Any] = [
                .foregroundColor: NSColor.secondaryLabelColor,
                .font: NSFont.systemFont(ofSize: 13),
            ]
            let size = message.size(withAttributes: attrs)
            message.draw(
                at: NSPoint(x: max((bounds.width - size.width) / 2, 12), y: 18),
                withAttributes: attrs
            )
        }
    }

    static func loadEmptyArt() -> NSImage? {
        var paths: [String] = []
        if let r = Bundle.main.resourcePath {
            paths.append(r + "/EmptyPreview.png")
        }
        let here = Bundle.main.bundlePath
        paths.append(here + "/Contents/Resources/EmptyPreview.png")
        paths.append((here as NSString).deletingLastPathComponent + "/gui/EmptyPreview.png")
        paths.append((here as NSString).deletingLastPathComponent + "/EmptyPreview.png")
        paths.append(FileManager.default.currentDirectoryPath + "/gui/EmptyPreview.png")
        paths.append(NSHomeDirectory() + "/Applications/\(appName).app/Contents/Resources/EmptyPreview.png")
        for p in paths {
            if let img = NSImage(contentsOfFile: p), img.size.width > 16 {
                img.isTemplate = false
                return img
            }
        }
        return nil
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
    var statusText = "Add the \(productName) by IP or hostname, then Preview or Scan."
    var statusIsError = false
    var lastExit: Int32 = 0
    var busy = false
    var logVisible = false
    var logMenuItem: NSMenuItem!
    var bridgeProcess: Process?
    let bridgePort: UInt16 = 8087
    var pendingOpen: String?

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
        window.title = appName
        window.minSize = NSSize(width: 860, height: 560)
        window.delegate = self
        root = RootView(frame: NSRect(x: 0, y: 0, width: 980, height: 640))
        root.app = self
        root.wantsLayer = true
        window.contentView = root

        styleField(hostField)
        hostField.placeholderString = "IPv4 or hostname.local"
        hostField.stringValue = Self.savedHost() ?? ""
        hostField.target = self
        hostField.action = #selector(addScanner)
        styleField(outputField)
        outputField.stringValue = Self.defaultDocumentsPath(ext: "jpg")
        outputField.placeholderString = "~/Documents/scan-<timestamp>.jpg"

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

        root.addSubview(preview)
        root.addSubview(scroll)
        root.addSubview(hostField)
        root.addSubview(outputField)
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
        consumePendingOpen()
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }

    func application(_ sender: NSApplication, openFile filename: String) -> Bool {
        pendingOpen = filename
        if window != nil { consumePendingOpen() }
        return true
    }

    func application(_ application: NSApplication, open urls: [URL]) {
        pendingOpen = urls.first?.path
        if window != nil { consumePendingOpen() }
    }

    func consumePendingOpen() {
        guard let path = pendingOpen else { return }
        pendingOpen = nil
        loadOpenedScan(path)
    }

    func loadOpenedScan(_ path: String) {
        window?.makeKeyAndOrderFront(nil)
        if let img = Self.imageFromScan(path) {
            preview.image = img
            preview.selection = nil
            statusIsError = false
            statusText = "Opened \(URL(fileURLWithPath: path).lastPathComponent)"
            if !path.isEmpty { outputField.stringValue = path }
            root?.refresh()
            appendLog("Opened \(path)\n")
        } else if path.lowercased().hasSuffix(".pdf") {
            NSWorkspace.shared.open(URL(fileURLWithPath: path))
            statusIsError = false
            statusText = "Opened PDF in Preview."
            root?.refresh()
        } else {
            statusIsError = true
            statusText = "Could not open \(URL(fileURLWithPath: path).lastPathComponent)."
            root?.refresh()
        }
    }

    func buildMainMenu() {
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
        appItem.title = appName
        appItem.submenu = appMenu
        main.addItem(appItem)

        let scanMenu = NSMenu(title: "Scan")
        scanMenu.addItem(menuItem("Discover", #selector(discoverLan), "d"))
        scanMenu.addItem(menuItem("Add Scanner", #selector(addScanner), "a"))
        scanMenu.addItem(menuItem("Preview", #selector(runPreview), "p"))
        scanMenu.addItem(menuItem("Scan", #selector(runScan), "s"))
        scanMenu.addItem(NSMenuItem.separator())
        scanMenu.addItem(menuItem("Add Scanner to macOS", #selector(addToMacOS)))
        scanMenu.addItem(menuItem("Add Printer if Missing", #selector(addPrinter)))
        let scanItem = NSMenuItem(title: "Scan", action: nil, keyEquivalent: "")
        scanItem.submenu = scanMenu
        main.addItem(scanItem)

        let viewMenu = NSMenu(title: "View")
        logMenuItem = menuItem("Show Log", #selector(toggleLog), "l")
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
        let build = Bundle.main.infoDictionary?["CFBundleVersion"] as? String ?? "10"
        return "\(short) (\(build))"
    }

    @objc func showAbout(_ sender: Any?) {
        let credits = NSAttributedString(
            string: "Scan client for the HP Color LaserJet Pro MFP M177fw.\nNot an HP product. No affiliation with HP Inc.",
            attributes: [.font: NSFont.systemFont(ofSize: 11)]
        )
        NSApp.orderFrontStandardAboutPanel(options: [
            .applicationName: appName,
            .version: versionString(),
            .credits: credits,
        ])
    }

    @objc func showHelp(_ sender: Any?) {
        let alert = NSAlert()
        alert.messageText = appName
        alert.informativeText = """
        1. Enter the printer IP and click Add Scanner.
        2. Preview the glass. Drag a rectangle to crop.
        3. Scan writes ~/Documents/scan-<timestamp>.<ext>.
        4. Add Scanner to macOS starts a local AirScan bridge. Image Capture lists \(appName). You can also send the scan to this app from Image Capture’s destination menu. Leave the bridge running.

        View → Show Log (⌘L) reveals hp-m177 command output.

        This is not an HP product.
        """
        alert.addButton(withTitle: "OK")
        alert.runModal()
    }

    @objc func toggleLog() {
        logVisible.toggle()
        logMenuItem?.title = logVisible ? "Hide Log" : "Show Log"
        logMenuItem?.state = logVisible ? .on : .off
        root.layoutChrome()
    }

    func windowDidResize(_ notification: Notification) {
        root.needsLayout = true
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
        let hasScan = root.chrome["scan"] is ChromeButton
        let hasDisc = root.chrome["discover"] is ChromeButton
        let ok = hf.height >= 20 && hf.width >= 80 && hf.minX < 80
            && pv.minX >= 280
            && scan.height >= 20 && scan.width >= 40
            && disc.height >= 20 && disc.width >= 40
            && scan.minY > topBand
            && disc.minY > topBand
            && painted >= 30
            && hasScan && hasDisc
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
        case "macos", "add-to-macos":
            return d.execAddToMacOS(openCapture: false)
        default:
            fputs("unknown --exec verb '\(verb)' (use add|scan|preview|discover|add-printer|macos)\n", stderr)
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

    static func runningStatus(for args: [String]) -> String {
        switch args.first {
        case "add": return "Adding scanner…"
        case "discover": return "Looking for scanners…"
        case "add-printer": return "Checking print queue…"
        case "scan":
            if args.contains("100") { return "Scanning preview…" }
            return "Scanning…"
        default: return "Working…"
        }
    }

    static func shortFailure(for args: [String]) -> String {
        switch args.first {
        case "add": return "Could not add the scanner. Open Show Log for details."
        case "discover": return "Discover failed. Open Show Log for details."
        case "add-printer": return "Could not add a print queue. Open Show Log for details."
        case "scan": return "Scan failed. Open Show Log for details."
        default: return "Failed. Open Show Log for details."
        }
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
            statusIsError = true
            statusText = "Enter a host or IP first."
            root?.refresh()
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
                self.statusIsError = false
                self.statusText = "Preview ready. Drag a rectangle, then Scan."
            } else {
                self.preview.image = nil
                self.preview.message = "Preview failed. Open Show Log for details."
                self.statusIsError = true
                self.statusText = "Preview failed. Open Show Log for details."
            }
            self.root?.refresh()
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
            } else if code != 0 {
                self.statusIsError = true
                self.preview.message = "Scan failed. Open Show Log for details."
            } else {
                self.statusIsError = false
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

    @objc func addToMacOS() {
        let host = (window != nil ? hostField.stringValue : "").trimmingCharacters(in: .whitespaces)
        if Self.savedHost() == nil {
            guard !host.isEmpty else {
                statusIsError = true
                statusText = "Enter a host or IP, click Add Scanner, then add it to macOS."
                root?.refresh()
                return
            }
            runHpAsync(["add", host]) { [weak self] code, _ in
                guard let self else { return }
                if code == 0 {
                    self.beginBridge(openCapture: true)
                }
            }
            return
        }
        beginBridge(openCapture: true)
    }

    func execAddToMacOS(openCapture: Bool) -> Int32 {
        let (ok, msg) = startBridge(openCapture: openCapture)
        FileHandle.standardOutput.write(Data((msg + "\n").utf8))
        lastExit = ok ? 0 : 1
        return lastExit
    }

    func beginBridge(openCapture: Bool) {
        if CommandLine.arguments.contains("--button-smoke") {
            _ = startBridge(openCapture: false)
            return
        }
        if busy { return }
        busy = true
        statusIsError = false
        statusText = "Adding scanner to macOS…"
        root?.refresh()
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self else { return }
            let (ok, msg) = self.startBridge(openCapture: openCapture)
            DispatchQueue.main.async {
                self.busy = false
                self.lastExit = ok ? 0 : 1
                self.appendLog("\(msg)\n")
                self.statusIsError = !ok
                self.statusText = ok
                    ? "Available in Image Capture as \(appName)."
                    : msg
                self.root?.refresh()
            }
        }
    }

    func startBridge(openCapture: Bool) -> (Bool, String) {
        if bridgeCapsOk() {
            if openCapture {
                DispatchQueue.main.async { self.openImageCapture() }
            }
            return (true, "eSCL already listening on http://127.0.0.1:\(bridgePort)/eSCL/ScannerCapabilities")
        }
        guard let bin = bridgeBinary() else {
            return (false, "hp-m177-bridge is missing. Run scripts/install-gui.sh.")
        }
        if let running = bridgeProcess, running.isRunning {
            _ = waitForBridge(seconds: 4)
            if bridgeCapsOk() {
                if openCapture {
                    DispatchQueue.main.async { self.openImageCapture() }
                }
                return (true, "eSCL listening on http://127.0.0.1:\(bridgePort)/eSCL/ScannerCapabilities")
            }
        }
        let logPath = NSTemporaryDirectory() + "hp-m177-bridge.log"
        if !FileManager.default.fileExists(atPath: logPath) {
            FileManager.default.createFile(atPath: logPath, contents: Data(), attributes: nil)
        }
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: bin)
        proc.arguments = ["--port", "\(bridgePort)"]
        do {
            let log = try FileHandle(forWritingTo: URL(fileURLWithPath: logPath))
            log.seekToEndOfFile()
            proc.standardOutput = log
            proc.standardError = log
            proc.standardInput = FileHandle.nullDevice
            try proc.run()
            _ = setpgid(proc.processIdentifier, proc.processIdentifier)
            bridgeProcess = proc
        } catch {
            if bridgeCapsOk() {
                if openCapture {
                    DispatchQueue.main.async { self.openImageCapture() }
                }
                return (true, "eSCL already listening on http://127.0.0.1:\(bridgePort)/eSCL/ScannerCapabilities")
            }
            return (false, "Could not start hp-m177-bridge: \(error.localizedDescription)")
        }
        let ready = waitForBridge(seconds: 5)
        if !ready {
            return (false, "hp-m177-bridge started but eSCL did not answer on port \(bridgePort). Open Show Log for details.")
        }
        if openCapture {
            DispatchQueue.main.async { self.openImageCapture() }
        }
        return (true, "eSCL listening on http://127.0.0.1:\(bridgePort)/eSCL/ScannerCapabilities (Advertised _uscan._tcp as \(appName))")
    }

    func waitForBridge(seconds: Double) -> Bool {
        let deadline = Date().addingTimeInterval(seconds)
        while Date() < deadline {
            if bridgeCapsOk() { return true }
            Thread.sleep(forTimeInterval: 0.12)
        }
        return bridgeCapsOk()
    }

    func bridgeCapsOk() -> Bool {
        guard let url = URL(string: "http://127.0.0.1:\(bridgePort)/eSCL/ScannerCapabilities") else {
            return false
        }
        var req = URLRequest(url: url)
        req.timeoutInterval = 0.6
        req.httpMethod = "GET"
        let sem = DispatchSemaphore(value: 0)
        var ok = false
        URLSession.shared.dataTask(with: req) { _, resp, _ in
            if let http = resp as? HTTPURLResponse, (200...299).contains(http.statusCode) {
                ok = true
            }
            sem.signal()
        }.resume()
        _ = sem.wait(timeout: .now() + 0.9)
        return ok
    }

    func openImageCapture() {
        let paths = [
            "/System/Applications/Image Capture.app",
            "/Applications/Image Capture.app",
        ]
        for p in paths {
            if FileManager.default.fileExists(atPath: p) {
                NSWorkspace.shared.open(URL(fileURLWithPath: p))
                return
            }
        }
        if let url = URL(string: "x-apple.systempreferences:com.apple.Print-Scan-Settings.extension") {
            NSWorkspace.shared.open(url)
        }
    }

    func applicationWillTerminate(_ notification: Notification) {
        // Leave hp-m177-bridge running so Image Capture / Preview keep seeing the scanner.
        if let p = bridgeProcess, p.isRunning {
            _ = setpgid(p.processIdentifier, p.processIdentifier)
        }
        bridgeProcess = nil
    }

    func runHpAsync(_ args: [String], done: ((Int32, String) -> Void)? = nil) {
        if CommandLine.arguments.contains("--button-smoke") {
            let code = runHpStatus(args)
            done?(code, "")
            return
        }
        if busy { return }
        busy = true
        statusIsError = false
        statusText = Self.runningStatus(for: args)
        root?.refresh()
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self else { return }
            let (code, text) = self.spawnHp(args)
            DispatchQueue.main.async {
                self.busy = false
                self.lastExit = code
                self.appendLog("$ hp-m177 \(args.joined(separator: " "))\n\(text)\n")
                if code != 0 {
                    self.statusIsError = true
                    self.statusText = Self.shortFailure(for: args)
                } else if done == nil {
                    self.statusIsError = false
                    self.statusText = "Done."
                }
                self.root?.refresh()
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

    func bridgeBinary() -> String? {
        if let env = ProcessInfo.processInfo.environment["HP_M177_BRIDGE"],
           FileManager.default.isExecutableFile(atPath: env) {
            return env
        }
        let here = Bundle.main.bundlePath
        let candidates = [
            here + "/Contents/MacOS/hp-m177-bridge",
            (here as NSString).appendingPathComponent("hp-m177-bridge"),
            (here as NSString).deletingLastPathComponent + "/hp-m177-bridge",
            NSHomeDirectory() + "/.cargo/bin/hp-m177-bridge",
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
        field.backgroundColor = NSColor.white
        field.textColor = NSColor.black
        field.font = NSFont.systemFont(ofSize: 13)
        field.translatesAutoresizingMaskIntoConstraints = true
        field.autoresizingMask = []
        paintPrepare(field)
    }
}
