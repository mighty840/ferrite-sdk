# Update-reach reports

Shipping a fix isn't the same as having it on devices. Update-reach reports answer three questions from the fleet's own reports:

- **Which builds is the fleet running?** Devices per firmware version and build, optionally per group or tag.
- **Did this campaign's update reach the devices?** Which devices report the target build, which haven't, which are offline, and how long patching took.
- **What has this device been running, and since when?** Every build a device reported, with the time it was first seen.

This is the evidence a security-update record needs (EU Cyber Resilience Act, Annex I Part II): what was released, to which devices, and when it was installed. Export it as CSV or JSON.

## How the data is collected

- Every device reports its `firmware_version` and `build_id` with each upload. When a device reports a build it wasn't running before, the server records it in the device's firmware history with a timestamp.
- A campaign remembers when it was first activated. **Time to patch** is the time from activation to the device's first report of the target build.
- A device counts as **on target** when its latest report is the target build, whether it got there through the campaign or not. Devices that already ran the target before the campaign started are marked `patched_before_campaign` and left out of time-to-patch.
- A device is **unreachable** when it isn't on target and hasn't been seen within the online window (`ALERT_OFFLINE_MINUTES`, default 10).
- Devices that rolled back an update report it ([Signed OTA updates](/guide/ota)) and show as `failed`.

## Dashboard

- **OTA → Fleet builds:** devices and online devices per build, with CSV/JSON export.
- **Campaign page → Update reach:** devices on the target build, unreachable devices, median and slowest time to patch, the build each device is running, and CSV/JSON export.

## CLI

```bash
ferrite report builds                    # devices per build
ferrite report builds --group 3 --tag line1
ferrite report campaign 12               # summary line + per-device table
ferrite report history stm32h563-fleet-01
ferrite --format csv report campaign 12 > campaign-12.csv
ferrite --format json report campaign 12 # full report
```

## API

All three endpoints need user authentication and accept `?format=csv` for a CSV download. Spreadsheet formulas in device-supplied values are neutralized.

| Path | Returns |
|---|---|
| `GET /reports/fleet/builds?group_id=&tag=` | `{ total_devices, online_window_minutes, builds: [{ firmware_version, build_id, devices, online, last_seen }] }` |
| `GET /reports/campaigns/:id` | `{ online_window_minutes, reach: { targeted, on_target, installed, failed, pending, unreachable, percent_on_target, activated_at, target_build_id, time_to_patch: { count, min_secs, median_secs, max_secs }, devices: [...] } }` |
| `GET /reports/devices/:device_id/firmware-history` | `{ device_id, history: [{ build_id, firmware_version, observed_at }] }` |

Firmware history starts when a server running this version first sees a device; earlier builds aren't known.
