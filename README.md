# HP Color LaserJet Pro MFP M177fw — Mac scanner client

Scanner client for the **HP Color LaserJet Pro MFP M177fw** (CZ165A) on a Mac.
The native app is named **HP Color LaserJet Pro MFP M177fw Scanner**. It is
meant to **sit beside** the working AirPrint print queue, not replace it.

Repository / crate name: `hp-m177`.

## Not an HP project

This is **not** an HP product, driver, or official tool. I have **no
affiliation, sponsorship, or endorsement** from HP, Hewlett-Packard, or HP
Inc. I am a private person who was frustrated that printing from this Mac
worked and scanning did not.

“HP”, “LaserJet”, and related names belong to their owners. This repository
only talks the network protocols the printer already exposes on the LAN.

The product contract is `REQUIREMENTS.md`. Contributor/build notes are `AGENTS.md`.

## Firmware upgrade

HP firmware for this MFP can add the AirScan/eSCL bits that current macOS
expects, so Image Capture might then see the **HP Color LaserJet Pro MFP
M177fw** without this project. That was considered. A failed or mismatched
flash on a 2014 printer can brick it, so this client talks to the SOAP / WSD
/ eSCL surfaces the device already exposes and leaves the firmware alone.

## License

[MIT](LICENSE). Use it, fork it, break it, fix it. No warranty.

## What it does

Apple Image Capture does not see this 2014 MFP as a network scanner: the
firmware advertises `_ipp._tcp` (print) and `_scanner._tcp` → port **8289**
(HP SOAP), but **not** `_uscan._tcp` (eSCL / AirScan). It also serves
`GET /eSCL/ScannerCapabilities` and `GET /eSCL/ScannerStatus`, yet
`POST /eSCL/ScanJobs` returns **404**. Jobs use HP SOAP on **TCP 8289**
(DIME JPEG) when that service answers. If SOAP times out, returns Error 4
or Error 13 after a few retries, or yields no pixels, `scan()` falls back
to **WSD Scan** on **TCP 3911** (`dib` / BMP).

This project:

1. Discovers the device on the LAN or adds it by host/IP.
2. Scans from the **platen** or **ADF**, in **color**, **grayscale**, or
   **black-and-white**, to **JPEG**, **PDF**, or **TIFF**.
3. Ships a **CLI** and a **native Mac AppKit GUI** that share the same add/scan
   logic. The GUI can **preview** the platen and drag a scan region.
4. Optionally runs a **local eSCL + Bonjour `_uscan._tcp`** bridge so
   Image Capture, Preview, and System Settings treat it as a network scanner.
5. Can add an AirPrint/CUPS printer **only if no queue already exists**.
6. When HP SOAP on TCP 8289 is wedged, jobs fall back to **WSD Scan** on
   TCP 3911 (`dib` / BMP → the requested file format).

It does **not** replace the stock AirPrint driver, write a CUPS filter, or
install a kernel extension / ICA plugin.

## Install the CLI

Requires Rust **1.82 or newer** (`cargo --version`). Newer crates.io
releases of clap/icu/getrandom need edition 2024 (Cargo 1.85+); this
repo pins older versions so 1.82 works. Always use `--locked`.

```bash
git clone https://github.com/jensabrahamsson/hp-m177-mac-scanner.git
cd hp-m177-mac-scanner
cargo install --path . --locked
```

That installs `hp-m177`, `hp-m177-bridge`, `hp-m177-gui`, and `hp-m177-fake`
to `~/.cargo/bin`. If the shell cannot find them:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

Check: `hp-m177 --help`

## Install the GUI

Needs the Xcode command-line tools (`swiftc`). The script runs
`cargo install --path . --locked --force` so the `.app` bundles **this
tree’s** `hp-m177`, then compiles the AppKit helper.

```bash
./scripts/install-gui.sh
```

That installs:

| Location | What |
| --- | --- |
| `~/Applications/HP Color LaserJet Pro MFP M177fw Scanner.app` | Spotlight / Dock / Finder (scanner icns, not a printer) |
| `~/.cargo/bin/hp-m177-native-gui` | same binary; `hp-m177-gui` finds it automatically |

Open it from Spotlight (**HP Color LaserJet Pro MFP M177fw Scanner**), or:

```bash
open "$HOME/Applications/HP Color LaserJet Pro MFP M177fw Scanner.app"
# or
hp-m177-gui
```

The window title is **HP Color LaserJet Pro MFP M177fw Scanner** version
**0.2.0**. **Discover**, **Add Scanner**, **Preview**, **Scan All**, and
**Scan** sit along the top (drawn as window subviews; stock `NSButton` cells
do not appear here). **Scan All** scans the whole page with no preview.
**Scan** uses a crop rectangle if you dragged one after Preview. Source /
color / DPI / format are dropdown menus. Files default to
`~/Documents/scan-<timestamp>.<ext>` at **100 dpi**. Drag a rectangle on
Preview to crop; Scan clears the overlay. **Add Scanner to macOS** starts the
bundled `hp-m177-bridge` and advertises `_uscan._tcp` so Image Capture lists
**HP Color LaserJet Pro MFP M177fw Scanner**. Preview and other apps can
use the same scanner.
**View → Show Log** (or the Show Log control) reveals `hp-m177` output,
hidden by default. Waiting status lines blink; failed status lines are red. The empty preview pane shows
a flatbed scanner. The app menu has **About**, **Version**, **Quit**, and
**Help**. The Dock icon is a lid-open flatbed scanner.

The same path is scriptable (no clicks):

```bash
hp-m177-gui add <printer-ip>
hp-m177-gui scan --source platen --color color --dpi 300 --format jpeg --output ~/Documents/scan.jpg
hp-m177-gui exec scan --source platen --format tiff --output ~/Documents/scan.tiff
```

## Add the scanner

By address (IP or `.local` hostname):

```bash
hp-m177 add <printer-ip>
# or
hp-m177 add printer.local
```

By discovery (Bonjour `_ipp._tcp` / `_scanner._tcp` / `_uscan._tcp`):

```bash
hp-m177 discover
hp-m177 add <host-from-discover>
```

Device records live in
`~/Library/Application Support/hp-m177/devices.json`
(override with `HP_M177_HOME`).

If `add` says there is no usable scan protocol, the printer is off the LAN
or both eSCL capabilities and SOAP are unreachable. Printing can still work
via AirPrint.

If SOAP on port **8289** does not answer, `add` records WSD on port **3911**
and `scan` uses that path automatically (BMP/`dib` converted to JPEG, PDF, or
TIFF). Power-cycling the MFP can restore SOAP; it is not required for a scan.

## Scan (CLI)

Default output is `~/Documents/scan-<timestamp>.<ext>`.

```bash
hp-m177 scan --source platen --color color --dpi 300 --format jpeg
hp-m177 scan --source adf --color gray --dpi 300 --format pdf --output ~/Documents/scan.pdf
hp-m177 scan --source platen --color lineart --format tiff
hp-m177 scan --region 500,500,4000,6000 --format jpeg
```

`--color` is `color`, `gray`, or `lineart`. `--format` is `jpeg`, `pdf`, or
`tiff`. `--region` is `x,y,width,height` in 1/1000 inch.

## Image Capture / Preview

Keep the AirPrint printer as-is. The MFP does not advertise AirScan itself,
so this Mac must run a local eSCL bridge.

From the GUI: **Add Scanner**, then **Add Scanner to macOS**. That starts
`hp-m177-bridge` (port **8087**), advertises `_uscan._tcp`, and opens
Image Capture. The bridge stays running after you quit the GUI.

From a terminal:

```bash
hp-m177 add <printer-ip>
hp-m177-bridge --port 8087
```

While the bridge is running it:

- Serves `http://127.0.0.1:8087/eSCL/ScannerCapabilities` (platen + ADF,
  RGB24 + Grayscale8, JPEG + PDF)
- Advertises `_uscan._tcp` with `rs=eSCL`, `is=platen,adf`,
  `cs=color,grayscale`, `pdl=image/jpeg,application/pdf`

Open **Image Capture**, **Preview → File → Import from Scanner…**, or
**System Settings → Printers & Scanners**. The scanner appears as
**HP Color LaserJet Pro MFP M177fw Scanner** (our app, via the local
AirScan bridge). Image Capture can also send the finished scan to this
app (it registers as an Automatic Task and opens JPEG / PDF / TIFF). If
Bonjour is filtered, add the URL `http://127.0.0.1:8087`. Stop sharing
with `killall hp-m177-bridge`.

## Optional: add the printer

Only if you do **not** already have a working queue:

```bash
hp-m177 add-printer <printer-ip>
```

This runs `lpadmin … -m everywhere` (IPP Everywhere / AirPrint). If a queue
whose URI or name looks like the M177fw already exists, it is left untouched.

## Tests

```bash
cargo test --locked
```

The suite starts a protocol-accurate fake MFP (SOAP + DIME + WSD `/scanner`
+ eSCL caps), drives the library, launches the real CLI twice (JPEG platen
color, then PDF ADF gray), and launches the real eSCL listener (including
the `hp-m177-bridge` binary) twice. AppKit `--layout-check` and
`--button-smoke` cover the native window chrome and button handlers.

## More documentation

- [docs/USAGE.md](docs/USAGE.md) — command reference
- [docs/PROTOCOL.md](docs/PROTOCOL.md) — what the live M177fw actually speaks
