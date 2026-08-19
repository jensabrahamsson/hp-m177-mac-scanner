# Live M177fw protocol notes

Recorded on 2026-08-18 against `DEV26BA77.local` (`192.168.50.14`),
AirPrint UUID `8adb6a9a-f28e-31fc-15de-50e0c4efdd92`.

## What is already working

- **Print:** `_ipp._tcp` → `DEV26BA77.local:631`, `rp=ipp/print`,
  `pdl=image/urf,image/jpeg,application/PCLm`. CUPS queue
  `HP_Color_LaserJet_Pro_MFP_M177fw` is idle and must not be replaced.
- **Bonjour scanner hint:** `_scanner._tcp` → **port 8289**
  (`flatbed=T feeder=T`). There is **no** `_uscan._tcp`.

## Open ports (relevant)

| Port | Role |
| --- | --- |
| 80 / 8080 / 443 | HTTP(S), LEDM, incomplete eSCL |
| 631 | IPP / AirPrint |
| 8289 | HP SOAP scan (gSOAP/2.7) — DIME JPEG when the service is healthy |
| 3911 | Microsoft WSD Scan (`POST /scanner`) — `dib` / BMP via MTOM when SOAP is wedged |
| 9100 | JetDirect raw print |

## eSCL (AirScan) on the device

`GET /eSCL/ScannerCapabilities` and `GET /eSCL/ScannerStatus` return 200
(old schema `2011/02/08` / `2011/05/03`). Capabilities list platen + ADF,
JPEG + PDF, 100/300/600 dpi.

`GET /eSCL/eSclManifest.xml` *declares* `POST /eSCL/ScanJobs`.

**`POST /eSCL/ScanJobs` returns 404** (empty body, `Server: Mrvl-R2_0`).
Native eSCL jobs are not implemented on this firmware. Image Capture cannot
talk to the printer directly even if we only advertised `_uscan._tcp`
pointing at the device.

Fixtures: `fixtures/live/escl-*.xml`.

## HP SOAP on 8289

`POST http://<host>:8289/` with `Content-Type: text/xml; charset=utf-8`.

`GetScannerElements` (HTTP **202**) returns platen + ADF, formats `jfif` and
`hpraw`, colors `BlackandWhite1`, `GrayScale8`, `RGB24`, `RGB48`. Optical
1200 dpi platen / 300 dpi ADF. Fixture:
`fixtures/live/soap-GetScannerElements.xml`.

Job cycle (same family as the M175nw / HPSimpleScan):

1. `CreateScanJobRequest` (`http://tempuri.org/wscn.xsd`) with
   `InputSource` Platen|ADF, `ColorProcessing` RGB24|GrayScale8,
   `Format` jfif, resolution in 1/1000 inch media units.
2. `GetJobInfo` until `JobState` is Processing/Completed.
3. `RetrieveImageRequest` → **DIME** (`application/dime`): SOAP record +
   `image/jpeg` (possibly chunked).

A first live `CreateScanJob` attempt using a slightly different tag layout
returned gSOAP **Error 4** (`SOAP_TAG_MISMATCH`). The client therefore emits
the HPSimpleScan-shaped `wscn:CreateScanJobRequest` ticket and retries the
short `CreateScanJob` operation name if the device rejects the first.

`scan()` uses an 8 second timeout on SOAP CreateScanJob. A transport timeout
falls through to WSD.

## WSD Scan on 3911

`POST http://<host>:3911/scanner` with SOAP 1.2 + WS-Addressing.

- `Content-Type: application/soap+xml; charset=utf-8; action="<Action>"`
- `User-Agent: WSDAPI`
- Namespace `http://schemas.microsoft.com/windows/2006/08/wdp/scan`
- `CreateScanJob` with `Format` **dib** (jfif is rejected:
  `ClientErrorDocumentFormatNotSupported`)
- `RetrieveImage` returns `multipart/related` XOP; the image part is
  `image/bmp` (32-bit top-down DIB, ~2528×3465 at 300 dpi)
- The client converts BMP → JPEG / PDF / TIFF

## Local eSCL facade

`hp-m177-bridge` implements modern eSCL (`2011/05/03`) toward the Mac and
calls the shared `scan()` backend toward the MFP (SOAP, WSD, or native eSCL
as recorded on the device — not SOAP-only). Tests drive this against
`hp-m177-fake`. Image Capture sees platen+ADF, color+gray, JPEG+PDF.

PDF output is always a **one-page** JPEG wrapper (`finalize()` keeps the
first retrieved page). TIFF is produced locally by the CLI/GUI; the eSCL
facade advertises JPEG and PDF only.

## DIME

See `src/dime.rs` (draft-nielsen-dime-02). Spec-faithful fixture:
`fixtures/dime-jpeg.bin`. Continuation (CF) chunks are concatenated by
`extract_image`; `fixtures/dime-jpeg-chunked.bin` covers that path.
