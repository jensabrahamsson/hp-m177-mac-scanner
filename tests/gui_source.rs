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
    assert!(swift.contains("NSButton"), "AppKit controls missing");
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
        swift.contains("pinVertical"),
        "control column must use a non-collapsing vertical layout"
    );
    assert!(swift.contains("tiff") || swift.contains("TIFF"));
    assert!(swift.contains("lineart") || swift.contains("B/W") || swift.contains("bw"));
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
}
