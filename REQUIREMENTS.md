# Requirements

Product contract for this repository: a Mac scanner client for LAN MFPs that
speak eSCL ScanJobs, HP SOAP/DIME, or WSD Scan (`dib`). **Validated
hardware:** **HP Color LaserJet Pro MFP M177fw** (CZ165A). Other models are
best-effort via the same probe; they are not first-class targets.

This is **not** an HP product. There is no affiliation, sponsorship, or
endorsement by HP Inc. or Hewlett-Packard. “HP”, “LaserJet”, and related
names belong to their owners.

## In scope

1. **Discover and add** the MFP by IPv4 address, `.local` hostname, or LAN
   Bonjour browse (`_ipp._tcp`, `_scanner._tcp`, `_uscan._tcp`). Persist the
   device in a local store.
2. **Scan** from **platen** and **ADF**.
3. **Color processing:** color, grayscale, and black-and-white (line art).
4. **Output files:** **JPEG**, **PDF**, and **TIFF**. PDF is a one-page wrapper
   around a JPEG. TIFF is uncompressed RGB or gray from the scanned raster.
   The device may return JPEG (HP SOAP/DIME) or BMP/`dib` (WSD); the client
   converts to the requested format.
5. **CLI** (`hp-m177`) and a **native Mac AppKit GUI** share the same add and
   scan implementation.
6. **GUI-only preview:** a low-resolution preview of the platen, then an
   optional rubber-band region for the final scan. Region is expressed to the
   device in 1/1000 inch (`ScanRegion`). **Scan All** scans the whole page
   with no preview. Source / color / DPI / format are dropdown menus.
7. **Default save folder** on macOS is the user’s **Documents** directory
   (`~/Documents/scan-<timestamp>.<ext>`), overridable with `--output`.
8. **Local eSCL + Bonjour `_uscan._tcp`** bridge (`hp-m177-bridge`) so Image
   Capture / Preview / other apps can treat this Mac as an AirScan scanner.
   The GUI **Add Scanner to macOS** action starts the bundled bridge (port
   8087) and advertises `_uscan._tcp` as `{probed make/model} Scanner` (on the
   validated M177fw that is **HP Color LaserJet Pro MFP M177fw Scanner**).
   The bridge calls the same `scan()` backend. Native device
   `POST /eSCL/ScanJobs` is 404 on M177fw firmware; the bridge is the Image
   Capture path.
9. **Optional AirPrint/CUPS add-printer** only when no queue already exists.
   Never replace the working print queue.
10. **Job APIs:** probe the host: native eSCL ScanJobs if they exist; otherwise
    HP SOAP on TCP **8289** (DIME JPEG, M177fw ticket as the default profile);
    if SOAP does not return pixels, Microsoft **WSD Scan** on TCP **3911**
    (`/scanner`, document format `dib`).
11. **GUI automation** with no clicks: `hp-m177-gui add|scan|list`, the
    AppKit helper `--exec add|scan|scan-all|preview|discover|add-printer|macos`, `--layout-check`,
    and `--button-smoke` (same Add / Preview / Scan handlers as the window).
12. **Custom Mac app icon** of a **flatbed scanner** (open lid, glass platen),
    not a printer with an output tray (`gui/AppIcon.icns`,
    `CFBundleIconFile=AppIcon`). App name, window title, and Bonjour
    instance default to **HP Color LaserJet Pro MFP M177fw Scanner** on the
    validated model (Bonjour follows the probed make/model).
    The app registers JPEG/PDF/TIFF document types and an Image Capture
    Automatic Task so scans can be sent to it.
13. **MIT** license. English docs and code comments.

## Out of scope

- Replacing the AirPrint/CUPS print stack, CUPS filters, PPDs, or kernel
  extensions.
- Fax, HP Smart / cloud, USB-only scan, duplex ADF, OCR / searchable PDF.
- Other printer models as first-class targets.
- Signed ICA/TWAIN plugins.
- First-class Windows or Linux apps.

## Quality bar

- Install: `cargo install --path . --locked` (CLI tools) and
  `./scripts/install-gui.sh` (AppKit `.app` + bundled CLI).
- In-repo tests: `cargo test --locked` drives the **shipped** add/scan
  functions against a protocol-accurate fake (SOAP/DIME) and a WSD stand-in
  for the `dib` path.
- CLI and GUI automation each scan twice against the fake (JPEG platen color;
  PDF ADF gray).
- Live LAN scans are evidence, not a substitute for those tests.
