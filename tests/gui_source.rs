//! Structural check: the native GUI is wired to the same add/scan functions.

#[test]
fn rust_gui_calls_shared_add_and_scan() {
    let gui = include_str!("../src/gui.rs");
    assert!(
        gui.contains("cli::add_by_address") || gui.contains("add_scanner"),
        "gui.rs must call add_by_address"
    );
    assert!(
        gui.contains("scan::scan") || gui.contains("scan("),
        "gui.rs must call scan"
    );
    let bin = include_str!("../src/bin/hp-m177-gui.rs");
    assert!(bin.contains("GuiApp"));
    assert!(bin.contains("add_scanner") || bin.contains("smoke"));
}

#[test]
fn appkit_gui_exists_and_invokes_cli() {
    let swift = include_str!("../gui/HP-M177-Scan.swift");
    assert!(swift.contains("NSWindow"), "AppKit window missing");
    assert!(
        swift.contains("ChromeButton") && swift.contains("RootView"),
        "buttons must be custom-drawn subviews (NSButton cells do not appear in this window)"
    );
    assert!(
        swift.contains("\"add\"") && swift.contains("\"scan\""),
        "Swift GUI must invoke hp-m177 add and scan"
    );
    assert!(swift.contains("HP-M177") || swift.contains("M177"));
    assert!(swift.contains("--exec"), "AppKit helper must be scriptable");
    assert!(
        swift.contains("PreviewView") || swift.contains("preview"),
        "GUI must have a preview surface"
    );
    assert!(
        swift.contains("NSMenu") && swift.contains("showAbout") && swift.contains("showHelp"),
        "app menu must include About and Help"
    );
    assert!(
        swift.contains("Quit") && swift.contains("Show Log"),
        "app menu must include Quit; View menu must hide the log"
    );
    assert!(
        swift.contains("--layout-check") && swift.contains("nonWhite"),
        "native helper must rasterize --layout-check so undrawn buttons fail"
    );
    assert!(
        swift.contains("scan-\\(ts)") && swift.contains("defaultDocumentsPath"),
        "AppKit default save field must be scan-<unix>.<ext> under Documents"
    );
    assert!(swift.contains("tiff") || swift.contains("TIFF"));
    assert!(swift.contains("lineart") || swift.contains("B/W") || swift.contains("bw"));
    assert!(
        swift.contains("--button-smoke") && swift.contains("ChromeButton") && swift.contains("runHpAsync"),
        "window buttons must be clickable and must not block the UI on SOAP"
    );
    assert!(
        swift.contains("imageFromScan") && swift.contains("hp-m177-preview-"),
        "Preview must load JPEG bytes from a unique temp file, not a leftover 8x8"
    );
    assert!(
        swift.contains("shortFailure") && swift.contains("Show Log for details"),
        "CLI dumps belong in the hideable log, not the status column"
    );
    assert!(
        swift.contains("statusIsError") && swift.contains("systemRed"),
        "failed status must draw in system red"
    );
    assert!(
        swift.contains("addToMacOS")
            && swift.contains("hp-m177-bridge")
            && swift.contains("Image Capture"),
        "GUI must start hp-m177-bridge so Image Capture / Preview can use the scanner"
    );
    assert!(
        swift.contains("EmptyPreview") && swift.contains("loadEmptyArt"),
        "empty preview pane must draw scanner artwork"
    );
    assert!(
        swift.contains("\"macos\"") && swift.contains("execAddToMacOS"),
        "AppKit --exec macos must start the AirScan bridge without requiring clicks"
    );
}

#[test]
fn app_icon_is_declared_and_present() {
    let icns = std::path::Path::new("gui/AppIcon.icns");
    assert!(icns.is_file(), "gui/AppIcon.icns missing");
    assert!(icns.metadata().unwrap().len() > 1024, "icns too small");
    let magic = std::fs::read(icns).unwrap();
    assert!(
        magic.starts_with(b"icns") || magic.windows(4).any(|w| w == b"icns"),
        "AppIcon.icns is not an Apple icon"
    );
    let install = include_str!("../scripts/install-gui.sh");
    assert!(install.contains("AppIcon.icns"));
    assert!(install.contains("CFBundleIconFile"));
    assert!(install.contains("AppIcon"));
    assert!(
        install.contains("setIcon") || install.contains("AppIcon.icns"),
        "install-gui.sh must install the icns"
    );
    assert!(
        install.contains("cargo install --path . --locked"),
        "install-gui.sh must install this tree's CLI"
    );
    assert!(
        install.contains("hp-m177-bridge") && install.contains("HP_M177_BRIDGE"),
        "install-gui.sh must bundle hp-m177-bridge for Add Scanner to macOS"
    );
    assert!(
        install.contains("EmptyPreview.png"),
        "install-gui.sh must copy empty-preview scanner art into the app"
    );
    let preview = std::path::Path::new("gui/EmptyPreview.png");
    assert!(preview.is_file(), "gui/EmptyPreview.png missing");
    let png = std::fs::read(preview).unwrap();
    assert!(
        png.starts_with(&[0x89, b'P', b'N', b'G']),
        "EmptyPreview.png is not a PNG"
    );
    assert!(png.len() > 10_000, "EmptyPreview.png too small");
}

#[test]
fn docs_match_shipped_install_scan_and_gui_flags() {
    let readme = include_str!("../README.md");
    let usage = include_str!("../docs/USAGE.md");
    let agents = include_str!("../AGENTS.md");
    let req = include_str!("../REQUIREMENTS.md");
    let protocol = include_str!("../docs/PROTOCOL.md");
    let swift = include_str!("../gui/HP-M177-Scan.swift");
    let cli = include_str!("../src/cli.rs");
    for (name, text) in [
        ("README.md", readme),
        ("docs/USAGE.md", usage),
        ("AGENTS.md", agents),
        ("REQUIREMENTS.md", req),
    ] {
        assert!(
            text.contains("cargo install --path . --locked"),
            "{name} must document cargo install --path . --locked"
        );
        assert!(
            text.contains("install-gui.sh"),
            "{name} must document scripts/install-gui.sh"
        );
    }
    assert!(readme.contains("cargo test --locked"));
    assert!(readme.contains("Discover") && (readme.contains("Add scanner") || readme.contains("Add Scanner")));
    assert!(readme.contains("Preview") && readme.contains("Scan"));
    assert!(
        readme.contains("Add Scanner to macOS") && usage.contains("Add Scanner to macOS"),
        "docs must describe adding the scanner to macOS for Image Capture / Preview"
    );
    assert!(
        swift.contains("Add Scanner to macOS") && usage.contains("hp-m177-bridge"),
        "GUI and usage must share the AirScan bridge path"
    );
    assert!(readme.contains("scan-<timestamp>") || readme.contains("Documents"));
    assert!(readme.contains("WSD"));
    assert!(usage.contains("--layout-check"));
    assert!(usage.contains("--button-smoke"));
    assert!(usage.contains("--adf-empty"));
    assert!(swift.contains("--exec") && swift.contains("--layout-check") && swift.contains("--button-smoke"));
    assert!(swift.contains("ChromeButton") && swift.contains("toggleLog"));
    assert!(protocol.contains("scan()") && protocol.contains("WSD"));
    assert!(protocol.contains("Error 13"));
    assert!(
        !cli.contains("DEV26BA77.local"),
        "add-printer must not hard-code a LAN hostname"
    );
    assert!(req.contains("--button-smoke"));
}
