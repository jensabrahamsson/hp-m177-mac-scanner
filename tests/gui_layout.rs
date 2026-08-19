//! Drive the real AppKit helper's `--layout-check` so a collapsed control
//! column cannot ship.

use std::path::PathBuf;
use std::process::Command;

fn native_gui() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/hp-m177-native-gui")
}

fn ensure_built() {
    let gui = native_gui();
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("gui/HP-M177-Scan.swift");
    let stale = !gui.is_file()
        || gui.metadata().unwrap().modified().unwrap() < src.metadata().unwrap().modified().unwrap();
    if stale {
        let status = Command::new("sh")
            .arg("scripts/build-gui.sh")
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .status()
            .expect("build-gui.sh");
        assert!(status.success(), "swiftc failed");
    }
}

#[test]
fn native_helper_layout_check_exposes_host_and_scan() {
    ensure_built();
    let out = Command::new(native_gui())
        .arg("--layout-check")
        .output()
        .expect("spawn --layout-check");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "layout-check failed: {stdout}{stderr}"
    );
    assert!(
        stdout.contains("hostField="),
        "layout-check must print hostField frame: {stdout}"
    );
    assert!(
        stdout.contains("scan="),
        "layout-check must print Scan button frame: {stdout}"
    );
    assert!(
        stdout.contains("nonWhite="),
        "layout-check must rasterize pixels, not only frames: {stdout}"
    );
    // Parse host width/height from `hostField=x,y WxH`.
    let host = stdout
        .split("hostField=")
        .nth(1)
        .and_then(|s| s.split_whitespace().nth(1))
        .expect("host size token");
    let mut wh = host.split('x');
    let w: i32 = wh.next().unwrap().parse().unwrap();
    let h: i32 = wh.next().unwrap().parse().unwrap();
    assert!(w >= 200, "host field width {w}");
    assert!(h >= 20, "host field height {h}");
    let scan = stdout
        .split("scan=")
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .expect("scan origin token");
    let scan_y: i32 = scan.split(',').nth(1).unwrap().parse().unwrap();
    let win_h: i32 = stdout
        .split("window=")
        .nth(1)
        .and_then(|s| s.split('x').nth(1))
        .and_then(|s| s.split_whitespace().next())
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        scan_y > win_h * 7 / 10,
        "Scan button must sit in the top action bar (y={scan_y} window={win_h}): {stdout}"
    );
    let non_white: i32 = stdout
        .split("nonWhite=")
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        non_white >= 30,
        "Scan/Discover must actually draw pixels (nonWhite={non_white}): {stdout}"
    );
}
