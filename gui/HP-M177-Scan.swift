import AppKit
import Foundation

/// Native AppKit GUI for the M177fw scanner.
///
/// Add and scan go through the `hp-m177` CLI, which calls the same
/// `hp_m177::add_by_address` and `hp_m177::scan` functions the test suite
/// drives. Set `HP_M177_BIN` to override the binary path.

if CommandLine.arguments.contains("--smoke") {
    FileHandle.standardOutput.write(Data("gui-native-smoke-ok\n".utf8))
    exit(0)
}

let delegate = AppDelegate()
let app = NSApplication.shared
app.setActivationPolicy(.regular)
app.delegate = delegate
app.run()

final class AppDelegate: NSObject, NSApplicationDelegate {
    var window: NSWindow!
    let hostField = NSTextField(string: "")
    let sourcePopup = NSPopUpButton()
    let colorPopup = NSPopUpButton()
    let dpiPopup = NSPopUpButton()
    let formatPopup = NSPopUpButton()
    let outputField = NSTextField(string: "scan.jpg")
    let status = NSTextField(labelWithString: "Add the scanner by IP or hostname, then Scan.")
    let logView = NSTextView()

    func applicationDidFinishLaunching(_ notification: Notification) {
        let content = NSView(frame: NSRect(x: 0, y: 0, width: 640, height: 420))
        window = NSWindow(
            contentRect: content.frame,
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.title = "HP M177fw Scanner"
        window.contentView = content

        hostField.placeholderString = "192.168.50.14 or DEV26BA77.local"
        sourcePopup.addItems(withTitles: ["platen", "adf"])
        colorPopup.addItems(withTitles: ["color", "gray"])
        dpiPopup.addItems(withTitles: ["100", "300", "600"])
        dpiPopup.selectItem(withTitle: "300")
        formatPopup.addItems(withTitles: ["jpeg", "pdf"])

        let add = NSButton(title: "Add scanner", target: self, action: #selector(addScanner))
        let discover = NSButton(title: "Discover", target: self, action: #selector(discover))
        let scan = NSButton(title: "Scan", target: self, action: #selector(runScan))
        let addPrinter = NSButton(title: "Add printer (if missing)", target: self, action: #selector(addPrinter))

        status.isEditable = false
        status.lineBreakMode = .byWordWrapping
        logView.isEditable = false
        logView.font = NSFont.monospacedSystemFont(ofSize: 11, weight: .regular)

        let stack = NSStackView(views: [
            label("Host / IP"), hostField,
            row([discover, add, addPrinter]),
            row([label("Source"), sourcePopup, label("Color"), colorPopup]),
            row([label("DPI"), dpiPopup, label("Format"), formatPopup]),
            label("Output path"), outputField,
            scan, status,
        ])
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 8
        stack.translatesAutoresizingMaskIntoConstraints = false
        content.addSubview(stack)
        let scroll = NSScrollView(frame: .zero)
        scroll.documentView = logView
        scroll.hasVerticalScroller = true
        scroll.translatesAutoresizingMaskIntoConstraints = false
        content.addSubview(scroll)
        NSLayoutConstraint.activate([
            stack.topAnchor.constraint(equalTo: content.topAnchor, constant: 16),
            stack.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 16),
            stack.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -16),
            scroll.topAnchor.constraint(equalTo: stack.bottomAnchor, constant: 12),
            scroll.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 16),
            scroll.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -16),
            scroll.bottomAnchor.constraint(equalTo: content.bottomAnchor, constant: -16),
            scroll.heightAnchor.constraint(greaterThanOrEqualToConstant: 140),
            hostField.widthAnchor.constraint(greaterThanOrEqualToConstant: 400),
        ])

        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }

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
        runHp(args)
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
            logView.string += "$ \(bin) \(args.joined(separator: " "))\n\(text)\n"
            status.stringValue = proc.terminationStatus == 0 ? "Done." : "Failed (exit \(proc.terminationStatus))."
        } catch {
            status.stringValue = "Could not start hp-m177: \(error.localizedDescription)"
        }
    }

    func bundledBinary() -> String? {
        let here = Bundle.main.bundlePath
        let sibling = (here as NSString).deletingLastPathComponent + "/hp-m177"
        return FileManager.default.isExecutableFile(atPath: sibling) ? sibling : nil
    }

    func which(_ name: String) -> String {
        if name.contains("/") { return name }
        let path = ProcessInfo.processInfo.environment["PATH"] ?? "/usr/bin:/bin:/usr/local/bin"
        for dir in path.split(separator: ":") {
            let candidate = "\(dir)/\(name)"
            if FileManager.default.isExecutableFile(atPath: candidate) {
                return candidate
            }
        }
        return name
    }

    func label(_ text: String) -> NSTextField {
        let l = NSTextField(labelWithString: text)
        l.font = NSFont.boldSystemFont(ofSize: 12)
        return l
    }

    func row(_ views: [NSView]) -> NSStackView {
        let s = NSStackView(views: views)
        s.orientation = .horizontal
        s.spacing = 8
        return s
    }
}
