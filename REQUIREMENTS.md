# Requirements

Product contract for this repository: a Mac scan client for the
**HP Color LaserJet Pro MFP M177fw** (CZ165A) on a local network.

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
   device in 1/1000 inch (`ScanRegion`).
7. **Default save folder** on macOS is the user’s **Documents** directory
   (`~/Documents/scan-<timestamp>.<ext>`), overridable with `--output`.
8. **Local eSCL + Bonjour `_uscan._tcp`** bridge (`hp-m177-bridge`) so Image
   Capture / Preview can treat this Mac as an AirScan scanner. The bridge
   calls the same `scan()` backend. Native device `POST /eSCL/ScanJobs` is
   404 on this firmware; the bridge is the Image Capture path.
9. **Optional AirPrint/CUPS add-printer** only when no queue already exists.
   Never replace the working print queue.
10. **Job APIs:** prefer native eSCL ScanJobs if they exist; otherwise HP SOAP
    on TCP **8289** (DIME JPEG); if SOAP does not return pixels, Microsoft
    **WSD Scan** on TCP **3911** (`/scanner`, document format `dib`).
11. **GUI automation** with no clicks: `hp-m177-gui add|scan|list` and the
    AppKit helper `--exec add|scan|discover`.
12. **Custom Mac app icon** (`gui/AppIcon.icns`, `CFBundleIconFile=AppIcon`).
13. **MIT** license. English docs and code comments.

## Out of scope

- Replacing the AirPrint/CUPS print stack, CUPS filters, PPDs, or kernel
  extensions.
- Fax, HP Smart / cloud, USB-only scan, duplex ADF, OCR / searchable PDF.
- Other printer models as first-class targets.
- Signed ICA/TWAIN plugins.
- First-class Windows or Linux apps.

## Quality bar

- In-repo tests drive the **shipped** add/scan functions against a
  protocol-accurate fake (SOAP/DIME) and a WSD stand-in for the `dib` path.
- CLI and GUI automation each scan twice against the fake (JPEG platen color;
  PDF ADF gray).
- Live LAN scans are evidence, not a substitute for those tests.
