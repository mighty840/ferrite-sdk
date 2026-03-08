# Dashboard Overview

The iotai dashboard is a web frontend for visualizing device telemetry collected by the iotai-server. It is built with Dioxus (Rust WASM framework) and communicates with the server's REST API.

## Features

- **Device list** -- see all registered devices with firmware version and last-seen time
- **Fault viewer** -- browse crash dumps with symbolicated addresses, register values, and stack snapshots
- **Metrics charts** -- visualize counter, gauge, and histogram metrics over time
- **Reboot history** -- view reboot events with reason codes and boot sequence numbers
- **Real-time updates** -- polls the server for new data on a configurable interval

## Running the dashboard

The dashboard is a static WASM application. Build it with:

```bash
cd iotai-dashboard
dx build --release
```

Serve the output directory with any static file server, or use the built-in Dioxus dev server:

```bash
dx serve
```

The dashboard expects the iotai-server API at the same origin or at a URL configured in the application settings. Since the server enables CORS, the dashboard can also be served from a different origin.

## Connecting to the server

By default, the dashboard connects to `http://localhost:4000`. To configure a different server URL, set it in the dashboard settings page or pass it as a query parameter:

```
http://localhost:8080/?server=https://iotai.example.com
```
