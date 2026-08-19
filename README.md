# hp-m177

Scan client for the **HP Color LaserJet Pro MFP M177fw** on a Mac, designed to
**coexist with the working AirPrint print queue**.

## Not an HP project

This is **not** an HP product, driver, or official tool. I have **no
affiliation, sponsorship, or endorsement** from HP, Hewlett-Packard, or HP
Inc. I am a private person who was frustrated that printing from this Mac
worked and scanning did not.

“HP”, “LaserJet”, and related names belong to their owners. This repository
only talks the network protocols the printer already exposes on the LAN.

## License

[MIT](LICENSE). Use it, fork it, break it, fix it. No warranty.

Apple Image Capture does not see this 2014 MFP as a network scanner: the
firmware advertises `_ipp._tcp` (print) and `_scanner._tcp` → port **8289**
(HP SOAP), but **not** `_uscan._tcp` (eSCL / AirScan). It also serves
`GET /eSCL/ScannerCapabilities` and `GET /eSCL/ScannerStatus`, yet
`POST /eSCL/ScanJobs` returns **404**. The job cycle that actually returns
pixels is HP’s SOAP API on **TCP 8289**, with the image in a **DIME** body.

This project:

1. Discovers the device on the LAN or adds it by host/IP.
2. Scans from the **platen** or **ADF**, in **color** or **grayscale**, to
   **JPEG** or **PDF** (PDF is a one-page wrapper around the device JPEG).
3. Ships a **CLI** and a **native Mac AppKit GUI** that share the same add/scan
   logic.
4. Optionally runs a **local eSCL + Bonjour `_uscan._tcp`** bridge so
   Image Capture, Preview, and System Settings treat it as a network scanner.
5. Can add an AirPrint/CUPS printer **only if no queue already exists**.

It does **not** replace the stock AirPrint driver, write a CUPS filter, or
install a kernel extension / ICA plugin.

## Install

Requires Rust **1.82 or newer** (`cargo --version`). Newer crates.io
releases of clap/icu/getrandom need edition 2024 (Cargo 1.85+); this
repo pins older versions so 1.82 works. Always install with `--locked`.

```bash
git pull
cargo install --path . --locked
```

That puts `hp-m177`, `hp-m177-bridge`, `hp-m177-gui`, and `hp-m177-fake` on
your `PATH` (typically `~/.cargo/bin`).

Native AppKit GUI (optional, needs Xcode CLT):

```bash
./scripts/build-gui.sh
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

## Scan (CLI)

```bash
hp-m177 scan --source platen --color color --dpi 300 --format jpeg --output scan.jpg
hp-m177 scan --source adf --color gray --dpi 300 --format pdf --output scan.pdf
```

## GUI

```bash
hp-m177-gui
```

The AppKit window (or the fallback form) can add the device and run the same
scan options. `hp-m177-gui --smoke --host 127.0.0.1:…` is a headless path used
by tests.

## Image Capture / Preview (Mac-native scanner)

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
cargo test
```

The suite starts a protocol-accurate fake MFP (SOAP + DIME + eSCL caps),
drives the library, launches the real CLI twice, and launches the real eSCL
listener twice.

## More documentation

- [docs/USAGE.md](docs/USAGE.md) — command reference
- [docs/PROTOCOL.md](docs/PROTOCOL.md) — what the live M177fw actually speaks
