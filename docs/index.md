---
layout: home

hero:
  name: iotai-sdk
  text: Firmware Observability for Cortex-M
  tagline: Capture crashes, metrics, and logs on embedded devices. Upload over any transport. Zero alloc. Zero panic.
  image:
    src: /logo.svg
    alt: iotai-sdk
  actions:
    - theme: brand
      text: Get Started
      link: /guide/quickstart
    - theme: alt
      text: View on GitHub
      link: https://github.com/your-org/iotai-sdk

features:
  - icon: 💥
    title: HardFault Capture
    details: Automatically captures all Cortex-M registers, CFSR/HFSR fault status, and a 64-byte stack snapshot on every HardFault. Stored in retained RAM, uploaded after reboot.
  - icon: 📊
    title: Metrics & Histograms
    details: Record counters, gauges, and histograms with a fixed-capacity ring buffer. No heap allocation. Oldest entries are evicted when the buffer fills.
  - icon: 📝
    title: Structured Trace Logs
    details: Captures defmt log output into a circular trace buffer. Frames are uploaded as binary fragments and decoded server-side.
  - icon: 🔄
    title: Reboot Reason Tracking
    details: Detects and records why the device rebooted — power-on, watchdog, fault, pin reset, brownout, software request, and more.
  - icon: 🔌
    title: Transport Agnostic
    details: Implement a single trait (Rust) or callback (C) to send chunks over UART, BLE, LoRa, HTTP, USB CDC, or anything else.
  - icon: 🎯
    title: Tiny Footprint
    details: "Default configuration: ~1.7 KB RAM, ~6 KB flash. No alloc, no std, no panics in production paths."
---
