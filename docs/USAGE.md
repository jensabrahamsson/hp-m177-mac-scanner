# Usage

This project is independent of HP. See the README disclaimer.

## Install (what actually works)

```bash
# 1. CLI tools → ~/.cargo/bin
cargo install --path . --locked
export PATH="$HOME/.cargo/bin:$PATH"

# 2. Native AppKit app → ~/Applications + ~/.cargo/bin
./scripts/install-gui.sh
```

`--locked` is required on Rust 1.82. Without it, Cargo may pull clap 4.6 /
icu 2.3, which need edition 2024 (Cargo 1.85+).

## Configuration

| Variable | Meaning |
| --- | --- |
| `HP_M177_HOME` | Directory for `devices.json` (default: `~/Library/Application Support/hp-m177`) |
| `HP_M177_BIN` | Path to `hp-m177` used by the AppKit GUI (install script sets this in the `.app`) |
| `HP_M177_NATIVE_GUI` | Optional override for the compiled Swift binary |
| `HP_M177_FAKE` | Host used by `hp-m177-gui --smoke` |

`hp-m177-gui` looks for the native window in this order:
`HP_M177_NATIVE_GUI`, the same directory as itself,
`~/.cargo/bin/hp-m177-native-gui`, then
`~/Applications/HP M177 Scanner.app`.

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
if SOAP times out. PDF and TIFF are built locally. Default `--output` is
`~/Documents/scan-<timestamp>.<ext>`.

## `hp-m177-bridge`

Same eSCL listener as `hp-m177 bridge`. Bind `0.0.0.0` if other Macs on the
LAN should import through this machine.

```
hp-m177-bridge --port 8087
```

Leave that process running while Image Capture / Preview is open.

## GUI automation API

The AppKit window and these commands share `GuiApp::add_scanner` / `GuiApp::scan`:

```
hp-m177-gui add 192.168.50.14
hp-m177-gui list
hp-m177-gui scan --source platen --color color --dpi 300 --format jpeg --output ~/Documents/scan.jpg
hp-m177-gui exec scan --format tiff --output ~/Documents/scan.tiff
```

The native helper (same code as the buttons, including Preview) also accepts:

```
hp-m177-native-gui --exec add --host 192.168.50.14
hp-m177-native-gui --exec scan --source platen --color color --dpi 300 --format jpeg --output ~/Documents/scan.jpg
hp-m177-native-gui --exec preview --host 192.168.50.14
```

Exit status is 0 on success. stdout is the CLI/API log.

## GUI

After `./scripts/install-gui.sh`:

```
open "$HOME/Applications/HP M177 Scanner.app"
hp-m177-gui
```

Spotlight name: **HP M177 Scanner**.

In the window the **left column** has Host/IP, Discover, Add scanner,
source / color / DPI / format, save path (Documents by default), Preview,
and Scan. After Preview, drag a rectangle on the right-hand glass image to
crop. **Add printer (if missing)** only creates an AirPrint queue when none
exists. The app icon is a flatbed scanner (open lid / glass), not a printer.

Developer flags (no window):

```
hp-m177-gui --headless
hp-m177-gui --smoke --host HOST --output FILE
hp-m177-gui --layout-check
```

`--layout-check` prints Host/Preview/Scan frames and exits 0 only when the
control column is not collapsed.

`--smoke` calls `GuiApp::add_scanner` and `GuiApp::scan` (the same functions
the automated tests call).

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

## Adding the scanner in System Settings

1. `hp-m177 add <printer-ip>`
2. Leave `hp-m177-bridge` running.
3. **System Settings → Printers & Scanners** should list a network scanner
   named `HP M177fw (hp-m177)` once Bonjour `_uscan._tcp` is visible.
4. Do not remove the existing **HP Color LaserJet Pro MFP M177fw** print
   queue.

If Bonjour is filtered, add the scanner as a URL:
`http://127.0.0.1:8087` (AirScan / eSCL).
