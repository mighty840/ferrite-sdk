# Integrations Overview

ferrite-sdk provides first-class integration crates for the two most popular embedded Rust frameworks, plus a C FFI library for use with C/C++ firmware.

## Rust integrations

| Integration | Crate | Upload model |
|---|---|---|
| [Embassy](./embassy) | `ferrite-embassy` | Async task with periodic timer and/or external trigger |
| [RTIC](./rtic) | `ferrite-rtic` | Blocking upload from a software task, with a shared resource wrapper |
| [Bare-metal](./baremetal) | `ferrite-sdk` (core) | Call `UploadManager::upload()` directly from your main loop |

All three use the same core SDK. The integration crates are thin wrappers that adapt the upload mechanism to each framework's concurrency model.

## C/C++ integrations

| Integration | Artifact | Description |
|---|---|---|
| [Zephyr](./zephyr-c) | `libferrite_ffi.a` + `ferrite_sdk.h` | Static library linked into a Zephyr CMake project |
| [FreeRTOS](./freertos-c) | `libferrite_ffi.a` + `ferrite_sdk.h` | Static library called from a FreeRTOS task |

The C FFI exposes the full SDK API through `extern "C"` functions: init, record reboot reason, record metrics, retrieve fault records, and run a blocking upload session via function-pointer callbacks.

## Choosing an integration

- **Embassy** is the recommended path for new Rust firmware projects. The async upload task integrates naturally with Embassy's executor and avoids blocking the main task.
- **RTIC** is the right choice if you are already using RTIC for your task scheduling. The `RticTransportResource` wrapper handles the request/poll pattern that RTIC's priority-based scheduling expects.
- **Bare-metal** (no framework) works when you have a simple superloop and can afford to block during upload.
- **Zephyr or FreeRTOS** are for teams with existing C firmware that want to add observability without a full rewrite. The FFI library is compiled once for your target and linked as a static library.
