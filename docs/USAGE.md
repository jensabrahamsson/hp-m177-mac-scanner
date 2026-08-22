# Usage

This project is independent of HP. See the README disclaimer.

## Install (what actually works)

```bash
# 1. CLI tools → ~/.cargo/bin
cargo install --path . --locked
export PATH="$HOME/.cargo/bin:$PATH"

# 2. Native AppKit app → ~/Applications + ~/.cargo/bin
#    (runs cargo install --path . --locked --force, then swiftc)
./scripts/install-gui.sh
```

`--locked` is required on Rust 1.82. Without it, Cargo may pull clap 4.6 /
icu 2.3, which need edition 2024 (Cargo 1.85+).

## Configuration

| Variable | Meaning |
| --- | --- |
| `HP_M177_HOME` | Directory for `devices.json` (default: `~/Library/Application Support/hp-m177`) |
| `HP_M177_BIN` | Path to `hp-m177` used by the AppKit GUI (install script sets this in the `.app`) |
| `HP_M177_BRIDGE` | Path to `hp-m177-bridge` used by **Add Scanner to macOS** |
| `HP_M177_NATIVE_GUI` | Optional override for the compiled Swift binary |
| `HP_M177_FAKE` | Host used by `hp-m177-gui --smoke` |

`hp-m177-gui` looks for the native window in this order:
`HP_M177_NATIVE_GUI`, the same directory as itself,
`~/.cargo/bin/hp-m177-native-gui`, then
`~/Applications/HP Color LaserJet Pro MFP M177fw Scanner.app`.

## `hp-m177`

```
hp-m177 discover [--timeout 3]
hp-m177 add <host> [--soap-port 8289] [--escl-port 80]
hp-m177 list
hp-m177 probe <host> [--soap-port] [--escl-port]
hp-m177 scan [--source platen|adf] [--color color|gray|lineart] [--dpi 300]
             [--format jpeg|pdf|tiff] [--output PATH] [--device ID]
             [--region x,y,w,h]
hp-m177 add-printer [host]
hp-m177 bridge [--port 8087] [--bind ADDR] [--no-advertise]
```

`add` probes eSCL capabilities, HP SOAP on `--soap-port`, and WSD on 3911.
This firmware’s native eSCL ScanJobs are 404. SOAP 8289 is used when it
answers; otherwise the saved job protocol is WSD (`dib`). `scan` talks SOAP
(CreateScanJob → GetJobInfo → RetrieveImage DIME) and falls back to WSD
on timeout, Error 4, Error 13 after a few busy retries, or an empty image.
PDF and TIFF are built locally. Default `--output` is
`~/Documents/scan-<timestamp>.<ext>`. CLI default DPI is **300**; the
AppKit window defaults to **100**.

## `hp-m177-bridge`

The standalone `hp-m177-bridge` binary binds **`0.0.0.0`** by default (LAN
Macs can import). The CLI subcommand `hp-m177 bridge` binds **`127.0.0.1`**
by default (this Mac only). They are the same eSCL facade; only the listen
address differs.

```
hp-m177-bridge --port 8087
hp-m177 bridge --port 8087 --bind 0.0.0.0:8087
```

Leave that process running while Image Capture / Preview is open.

## GUI automation API

The AppKit window and these commands share `GuiApp::add_scanner` / `GuiApp::scan`:

```
hp-m177-gui add <printer-ip>
hp-m177-gui list
hp-m177-gui scan --source platen --color color --dpi 300 --format jpeg --output ~/Documents/scan.jpg
hp-m177-gui exec scan --format tiff --output ~/Documents/scan.tiff
```

The native helper (same code as the buttons, including Preview) also accepts:

```
hp-m177-native-gui --exec add --host <printer-ip>
hp-m177-native-gui --exec scan --source platen --color color --dpi 300 --format jpeg --output ~/Documents/scan.jpg
hp-m177-native-gui --exec scan-all --output ~/Documents/scan.jpg
hp-m177-native-gui --exec preview --host <printer-ip>
hp-m177-native-gui --exec discover
hp-m177-native-gui --exec add-printer
hp-m177-native-gui --exec macos
```

Exit status is 0 on success. stdout is the CLI/API log.

## GUI

After `./scripts/install-gui.sh`:

```
open "$HOME/Applications/HP Color LaserJet Pro MFP M177fw Scanner.app"
hp-m177-gui
```

Spotlight name: **HP Color LaserJet Pro MFP M177fw Scanner**.

The window title is **HP Color LaserJet Pro MFP M177fw Scanner** (version
**0.3.0**). Use **Discover**, **Add Scanner**, **Preview**, **Scan All**, and
**Scan** (or the **Scan** menu). **Scan All** is a full-page scan with no
preview. Source / color / DPI / format are dropdown menus. Files go to
`~/Documents/scan-<unix>.<ext>` (GUI default **100 dpi**). After Preview,
drag a rectangle to crop; Scan clears the overlay. **Add Scanner to macOS**
starts bundled `hp-m177-bridge` on port 8087, advertises `_uscan._tcp` as
**HP Color LaserJet Pro MFP M177fw Scanner**, and opens Image Capture so
Preview and other apps can scan. Image Capture’s destination popup can
send the scan to this app. Failed status lines are
red; waiting lines such as **Looking for scanners…** blink until the
command returns. CLI dumps stay in the hideable log. The empty preview pane shows a
flatbed scanner. **View → Show Log** (⌘L) or the Show Log control toggles
`hp-m177` command output (off by default). **About** shows the version;
**Help** explains the flow; **Quit** (⌘Q) exits. **Add Printer if Missing**
(Scan menu) only creates an AirPrint queue when none exists. The app icon
is a flatbed scanner (open lid / glass), not a printer.

Developer flags:

```
hp-m177-gui --headless
hp-m177-gui --smoke --host HOST --output FILE
hp-m177-gui --layout-check
hp-m177-native-gui --smoke --host HOST --output FILE
hp-m177-native-gui --button-smoke --host HOST --output FILE
```

`--layout-check` prints Host/Preview/Scan frames, rasterizes the action bar,
and exits 0 only when Scan/Discover actually drew pixels (not just frames).

`hp-m177-gui --smoke` calls `GuiApp::add_scanner` and `GuiApp::scan`.
`hp-m177-native-gui --smoke` is the AppKit helper: real `hp-m177 add` then
`hp-m177 scan`. `--button-smoke` builds the window and fires the same Add /
Preview / Scan handlers the blue buttons use.

To rebuild only the Swift binary without installing the `.app`:

```
./scripts/build-gui.sh
```

## `hp-m177-fake`

Developer stand-in for the MFP. Prints the bound address, then serves SOAP,
WSD `/scanner` (`dib` / MTOM BMP), and eSCL capabilities until killed.

```
hp-m177-fake
hp-m177-fake --adf-empty
```

`--adf-empty` reports no paper in the ADF (RetrieveImage returns the empty-ADF
fault).

## Adding the scanner in System Settings / Image Capture

From the GUI: **Add Scanner**, then **Add Scanner to macOS**. Or:

1. `hp-m177 add <printer-ip>`
2. Leave `hp-m177-bridge` running (`hp-m177-bridge --port 8087`, or the GUI
   button, or `hp-m177-native-gui --exec macos`).
3. **Image Capture**, **Preview → File → Import from Scanner…**, or
   **System Settings → Printers & Scanners** should list a network scanner
   named **HP Color LaserJet Pro MFP M177fw Scanner** once Bonjour `_uscan._tcp` is visible.
4. Do not remove the existing **HP Color LaserJet Pro MFP M177fw** print
   queue.

If Bonjour is filtered, add the scanner as a URL:
`http://127.0.0.1:8087` (AirScan / eSCL).

The bridge stays up after the GUI quits. Stop it with `killall hp-m177-bridge`.
