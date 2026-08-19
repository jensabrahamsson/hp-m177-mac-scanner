# Improvement plan (2026-08-19)

Review of the M177fw Mac scanner client. Keep CLI, AppKit GUI, and the
local eSCL facade on the **same** `add_by_address` / `scan()` path. Do
**not** replace the AirPrint/CUPS queue, fork a GUI-only protocol stack,
or hard-code expected image bytes in tests.

Status of the review: **partial**. Live firmware was not re-probed here;
Image Capture through `hp-m177-bridge` was not observed end to end.

## What is already in place

- One job path: GUI buttons exec `hp-m177`; `hp-m177-gui add|scan` and
  `--exec` call the same functions as the CLI.
- SOAP on TCP 8289 (DIME JPEG) with a WSD `:3911` `dib`/BMP fallback after
  some SOAP failures.
- Local `_uscan._tcp` bridge whose `POST /eSCL/ScanJobs` calls `scan()`.
- JPEG / PDF / TIFF conversion; platen preview + rubber-band region in the
  AppKit window; Documents as the default save folder.

## Highest priority

1. **GUI automation at CLI quality.** Drive shipped `hp-m177-gui` (or
   `GuiApp::scan` / `scan()`) **twice** against the fake: JPEG platen/color,
   then PDF ADF/gray — the same bar `tests/cli.rs` already has. Do not treat
   `--layout-check` or the Swift `--smoke` stub as that coverage.
2. **Listening WSD stand-in.** Extend `FakeDevice` (or a sibling) so add/scan
   can hit `POST http://<host>:3911/scanner` with `dib`/MTOM BMP. In-process
   `FnTransport` Error-4 injection is not a listening WSD server.
3. **AppKit `--exec scan` (and a real `--smoke` path)** against the fake,
   not only the Rust `hp-m177-gui scan` subcommand.

## High — GUI must not hang when SOAP has no pixels

| Gap | Change |
| --- | --- |
| 8s timeout is only on SOAP `CreateScanJob` | Bound `GetJobInfo` and `RetrieveImage` as well, or fail over to WSD sooner |
| WSD create 40s + retrieve 180s × 8 | Cap WSD so a wedged SOAP path cannot block the GUI for minutes |
| Empty-image Protocol and retrieve HTTP remapped to Protocol | Treat “SOAP answered but no pixels” as fallback-worthy |
| `post_xml_timeout` turns HTTP 400/500 into `Ok(detail)` | Restore a real HTTP-error path for CreateScanJob |
| Unparseable `GetJobInfo` (including faults) is “not ready” | Stop spinning the 90s wait on faults; align `JobId` typing (`xsd:String` vs `xsd:int`) and add a live GetJobInfo fixture |

## Medium — fidelity of images and tickets

- Reassemble DIME continuation records (`CF` bit is parsed but unused).
  Add a chunked fixture; current `dime-jpeg.bin` is two unchunked records.
- Capture live `RetrieveImage` DIME and WSD MTOM BMP into `fixtures/` and
  decode them with the shipped decoders.
- SOAP ADF can retrieve many pages; `finalize()` keeps only the first
  (matches “one-page PDF wrapper” — document that, or wrap multiple pages).
- Forward `ScanRegion` through the eSCL facade (`parse_scan_settings`
  currently forces `region: None`).
- Timestamp the AppKit default save field (`scan.jpg` vs CLI
  `scan-<unix>.<ext>`).
- Advertise TIFF on the eSCL facade only if TIFF stays in the Image Capture
  contract; today capabilities list JPEG and PDF only.
- Stop hard-coding ADF-empty `ScannerStatus` in the facade.

## Lower — docs and toolchain

- README Tests must be `cargo test --locked` (1.82 / edition 2021 pins).
- Document `hp-m177-fake --adf-empty`.
- Add `rust-toolchain` and CI that run `cargo test --locked`.
- Pin or record `swiftc` / macOS floor for the AppKit helper.
- Correct `PROTOCOL.md`: the bridge calls `scan()`, not SOAP-only.
- Strip personal names and LAN hosts from committed files if that rule is
  meant to apply to README / LICENSE / PROTOCOL as well as AGENTS.md.

## Out of scope (unchanged)

- Replacing or resetting the existing AirPrint/CUPS print queue.
- A second protocol stack inside the GUI.
- OCR, duplex ADF, other printer models, signed ICA/TWAIN plugins.

## Open questions (not verified in this review)

- Whether live SOAP DIME JPEG is actually CF-chunked.
- Whether live WSD BMP uses `BI_BITFIELDS` (client rejects `compression != 0`).
- Whether firmware rejects GetJobInfo’s `jobId` / `xsd:String`.
- Whether a healthy CreateScanJob can exceed 8s (ADF / 600 dpi).
- Whether Image Capture completes a job through `hp-m177-bridge`.
