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
        "buttons must be layer-backed subviews (content-view draw is not shown on screen)"
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
        swift.contains("ChromeButton") && swift.contains("ChromeCycle"),
        "visible chrome must be NSView subclasses with draw(_:), like PreviewView"
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
    assert!(readme.contains("Discover") && readme.contains("Add scanner"));
    assert!(readme.contains("Preview") && readme.contains("Scan"));
    assert!(readme.contains("scan-<timestamp>") || readme.contains("Documents"));
    assert!(readme.contains("WSD"));
    assert!(usage.contains("--layout-check"));
    assert!(usage.contains("--button-smoke"));
    assert!(usage.contains("--adf-empty"));
    assert!(swift.contains("--exec") && swift.contains("--layout-check") && swift.contains("--button-smoke"));
    assert!(swift.contains("ChromeButton"));
    assert!(protocol.contains("scan()") && protocol.contains("WSD"));
    assert!(protocol.contains("Error 13"));
    assert!(
        !cli.contains("DEV26BA77.local"),
        "add-printer must not hard-code a LAN hostname"
    );
    assert!(req.contains("--button-smoke"));
}
