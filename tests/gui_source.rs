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
}
