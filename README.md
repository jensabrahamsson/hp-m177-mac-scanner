# hp-m177

Scan client for the **HP Color LaserJet Pro MFP M177fw** on a Mac. It is
meant to **sit beside** the working AirPrint print queue, not replace it.

## Not an HP project

This is **not** an HP product, driver, or official tool. I have **no
affiliation, sponsorship, or endorsement** from HP, Hewlett-Packard, or HP
Inc. I am a private person who was frustrated that printing from this Mac
worked and scanning did not.

“HP”, “LaserJet”, and related names belong to their owners. This repository
only talks the network protocols the printer already exposes on the LAN.

The product contract is `REQUIREMENTS.md`. Contributor/build notes are `AGENTS.md`.

## License

[MIT](LICENSE). Use it, fork it, break it, fix it. No warranty.

## What it does

Apple Image Capture does not see this 2014 MFP as a network scanner: the
firmware advertises `_ipp._tcp` (print) and `_scanner._tcp` → port **8289**
(HP SOAP), but **not** `_uscan._tcp` (eSCL / AirScan). It also serves
`GET /eSCL/ScannerCapabilities` and `GET /eSCL/ScannerStatus`, yet
`POST /eSCL/ScanJobs` returns **404**. The job cycle that actually returns
pixels is HP’s SOAP API on **TCP 8289**, with the image in a **DIME** body.

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

Needs the Xcode command-line tools (`swiftc`) and a working `hp-m177` on
`PATH` (the step above).

```bash
./scripts/install-gui.sh
```

That builds the AppKit app and installs:

| Location | What |
| --- | --- |
| `~/Applications/HP M177 Scanner.app` | Spotlight / Dock / Finder (scanner icns, not a printer) |
| `~/.cargo/bin/hp-m177-native-gui` | same binary; `hp-m177-gui` finds it automatically |

Open it from Spotlight (**HP M177 Scanner**), or:

```bash
open "$HOME/Applications/HP M177 Scanner.app"
# or
hp-m177-gui
```

The window title is **HP M177 Scanner**. A top row of blue buttons is
**Discover**, **Add scanner**, **Preview**, and **Scan**. The left column
has Host/IP, source / color / DPI / format (click to cycle), and the
Documents save path. The right pane is the preview (drag a region after
Preview). Files default to **Documents**. The Dock icon is a lid-open
flatbed scanner.

The same path is scriptable (no clicks):

```bash
hp-m177-gui add 192.168.50.14
hp-m177-gui scan --source platen --color color --dpi 300 --format jpeg --output ~/Documents/scan.jpg
hp-m177-gui exec scan --source platen --format tiff --output ~/Documents/scan.tiff
```

## Add the scanner

By address (IP or `.local` hostname):

```bash
hp-m177 add 192.168.50.14
# or
hp-m177 add DEV26BA77.local
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

Keep the AirPrint printer as-is. In a terminal:

```bash
hp-m177 add 192.168.50.14
hp-m177-bridge --port 8087
```

While the bridge is running it:

- Serves `http://127.0.0.1:8087/eSCL/ScannerCapabilities` (platen + ADF,
  RGB24 + Grayscale8, JPEG + PDF)
- Advertises `_uscan._tcp` with `rs=eSCL`, `is=platen,adf`,
  `cs=color,grayscale`, `pdl=image/jpeg,application/pdf`

Open **Image Capture**, **Preview → File → Import from iPhone or…**, or
**System Settings → Printers & Scanners**. The scanner appears as
`HP M177fw (hp-m177)`.

## Optional: add the printer

Only if you do **not** already have a working queue:

```bash
hp-m177 add-printer 192.168.50.14
```

This runs `lpadmin … -m everywhere` (IPP Everywhere / AirPrint). If a queue
whose URI or name looks like the M177fw already exists, it is left untouched.

## Tests

```bash
cargo test --locked
```

The suite starts a protocol-accurate fake MFP (SOAP + DIME + eSCL caps),
drives the library, launches the real CLI twice, and launches the real eSCL
listener (including the `hp-m177-bridge` binary) twice.

## More documentation

- [docs/USAGE.md](docs/USAGE.md) — command reference
- [docs/PROTOCOL.md](docs/PROTOCOL.md) — what the live M177fw actually speaks
