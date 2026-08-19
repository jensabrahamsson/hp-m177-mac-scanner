# Usage

This project is independent of HP. See the README disclaimer.

## Configuration

| Variable | Meaning |
| --- | --- |
| `HP_M177_HOME` | Directory for `devices.json` (default: `~/Library/Application Support/hp-m177`) |
| `HP_M177_BIN` | Path to `hp-m177` used by the AppKit GUI |
| `HP_M177_NATIVE_GUI` | Optional path to the compiled Swift GUI |
| `HP_M177_FAKE` | Host used by `hp-m177-gui --smoke` |

## `hp-m177`

```
hp-m177 discover [--timeout 3]
hp-m177 add <host> [--soap-port 8289] [--escl-port 80]
hp-m177 list
hp-m177 probe <host> [--soap-port] [--escl-port]
hp-m177 scan [--source platen|adf] [--color color|gray] [--dpi 300]
             [--format jpeg|pdf] [--output PATH] [--device ID]
hp-m177 add-printer [host]
hp-m177 bridge [--port 8087] [--bind ADDR] [--no-advertise]
```

`add` probes eSCL capabilities (informational) and the SOAP scanner on
`--soap-port`. This firmware’s working job API is SOAP; that is what gets
saved. `scan` then talks SOAP: CreateScanJob → GetJobInfo → RetrieveImage
(DIME). If you asked for PDF, the JPEG is wrapped locally.

## `hp-m177-bridge`

Same eSCL listener as `hp-m177 bridge`. Bind `0.0.0.0` if other Macs on the
LAN should import through this machine.

```
hp-m177-bridge --port 8087
```

## `hp-m177-gui`

```
hp-m177-gui                  # AppKit helper if present, else interactive form
hp-m177-gui --headless       # start and exit (no window)
hp-m177-gui --smoke --host HOST --output FILE
```

`--smoke` calls `GuiApp::add_scanner` and `GuiApp::scan` (the same functions
the automated tests call).

Build the AppKit window:

```
./scripts/build-gui.sh
export HP_M177_NATIVE_GUI=./target/hp-m177-native-gui
hp-m177-gui
```

## `hp-m177-fake`

Developer stand-in for the MFP. Prints the bound address, then serves SOAP
and eSCL capabilities until killed.

## Adding the scanner in System Settings

1. `hp-m177 add <printer-ip>`
2. Leave `hp-m177-bridge` running.
3. **System Settings → Printers & Scanners** should list a network scanner
   named `HP M177fw (hp-m177)` once Bonjour `_uscan._tcp` is visible.
4. Do not remove the existing **HP Color LaserJet Pro MFP M177fw** print
   queue.

If Bonjour is filtered, add the scanner as a URL:
`http://127.0.0.1:8087` (AirScan / eSCL).
