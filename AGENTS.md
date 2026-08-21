# Contributor notes

Guidance for humans and coding agents working in this repository.

## What this is

A scanner-first Mac client for one LAN MFP (HP Color LaserJet Pro MFP M177fw).
Printing stays on the existing AirPrint/CUPS queue. See `REQUIREMENTS.md` for
the product contract and `docs/PROTOCOL.md` for on-the-wire behavior.

## Toolchain

- Rust **1.82** or newer (`edition = "2021"`). Always build with `--locked`.
- Newer crates.io releases of clap / icu / getrandom need edition 2024
  (Cargo 1.85+). Versions are pinned in `Cargo.toml` so 1.82 still works.
- Native GUI: `swiftc` (Xcode command-line tools) and AppKit. Do not introduce
  SwiftUI, browser UIs, or extra GUI crates.

```bash
cargo test --locked
cargo install --path . --locked
./scripts/install-gui.sh
```

`rust-toolchain.toml` pins **1.82.0**. CI (`.github/workflows/test.yml`) runs
`cargo test --locked`. The AppKit helper needs **swiftc** and **macOS 12+**.

Installs CLI tools to `~/.cargo/bin` and the app bundle to
`~/Applications/HP Color LaserJet Pro MFP M177fw Scanner.app`.

## Layout

| Path | Role |
| --- | --- |
| `src/scan.rs` | Single scan entry used by CLI, GUI, and the eSCL facade |
| `src/soap.rs` / `src/dime.rs` | HP SOAP on TCP 8289 + DIME JPEG |
| `src/wsd.rs` | Microsoft WSD Scan on TCP 3911 (`dib` / BMP) |
| `src/escl.rs` / `src/facade.rs` | Local AirScan surface |
| `src/fake.rs` | Protocol-accurate SOAP/DIME + WSD `/scanner` stand-in for tests |
| `gui/HP-M177-Scan.swift` | AppKit window (`ChromeButton` subviews; NSButton cells do not paint). **Scan All**; Source/Color/DPI/Format dropdowns; blinking wait status; Ready / Preview-ready copy. Hideable log; red error status; empty-preview scanner art; **Add Scanner to macOS** starts `hp-m177-bridge`. About/Help/Quit. Click-free: `--exec`, `--layout-check`, `--button-smoke` |
| `tests/` | Integration tests that launch shipped binaries |

## Rules of change

- Keep add/scan on **one** `scan()` / `add_by_address` path. Do not fork a
  second protocol stack for the GUI.
- Do not replace or reset an existing CUPS/AirPrint queue.
- Tests must drive shipped code. Do not hard-code expected image bytes, mock
  the unit under test, or reimplement the decoder inside the test.
- English only in docs, comments, and user-visible strings.
- No personal names, emails, machine-specific home paths, or private notes
  in files that are committed.
- Prefer a short timeout on HP SOAP (TCP 8289). Retry SOAP Error 13 a few
  times (device busy), then fall back to WSD 3911 rather than hanging the GUI.

## Useful environment variables

| Variable | Meaning |
| --- | --- |
| `HP_M177_HOME` | Directory for `devices.json` (tests set this) |
| `HP_M177_BIN` | `hp-m177` path used by the AppKit helper |
| `HP_M177_BRIDGE` | `hp-m177-bridge` path used by Add Scanner to macOS |
| `HP_M177_NATIVE_GUI` | Override for the compiled Swift binary |
| `HP_M177_FAKE` | Host for `hp-m177-gui --smoke` |

Do not point Cargo, Rustup, or the user home at a scratch directory.

## Protocol reminders

- Live firmware: `GET /eSCL/ScannerCapabilities` is 200; `POST /eSCL/ScanJobs`
  is 404. Jobs historically run on SOAP 8289 (CreateScanJob 8s, GetJobInfo
  deadline 20s, RetrieveImage 20s). When that service is wedged (timeout,
  Error 4, Error 13 after retries, empty image), WSD
  `POST http://<host>:3911/scanner` with format `dib` returns BMP via
  MTOM/XOP (create 8s, retrieve 90s).
- SOAP CreateScanJob tag layout must match this firmware (see `src/soap.rs`).
  A wrong wrapper returns gSOAP Error 4.
- On macOS, advertise `_uscan._tcp` with `dns-sd -R` so `dns-sd -B` can see it.
