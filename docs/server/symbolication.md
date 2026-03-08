# Symbolication

Symbolication resolves raw program counter (PC) addresses from fault records into human-readable source locations like `app::main at src/main.rs:42`.

## How it works

1. **Upload your ELF file** to the server via `POST /ingest/elf` with the firmware version in the `X-Firmware-Version` header.
2. When a **FaultRecord chunk arrives**, the server extracts the PC address and runs `arm-none-eabi-addr2line` against the matching ELF file.
3. The resolved symbol string is stored in the `fault_events.symbol` column alongside the raw address.

## Uploading ELF files

Upload your debug ELF after each build:

```bash
curl -X POST http://localhost:4000/ingest/elf \
  -H "X-Firmware-Version: $(cargo metadata --format-version 1 | jq -r '.packages[] | select(.name=="my-firmware") | .version')" \
  --data-binary @target/thumbv7em-none-eabihf/release/my-firmware
```

The server saves the file as `{version}.elf` in the `--elf-dir` directory and registers it for symbolication.

::: tip
For the best symbolication results, upload the **release** ELF with debug info. Ensure your `Cargo.toml` includes:

```toml
[profile.release]
debug = true    # Keep DWARF debug info
strip = false   # Do not strip symbols
```
:::

## addr2line configuration

The server uses `arm-none-eabi-addr2line` with the following flags:

```bash
arm-none-eabi-addr2line -e firmware.elf -f -C -p 0x08002000
```

- `-e`: Specify the ELF file
- `-f`: Show function names
- `-C`: Demangle C++ and Rust symbol names
- `-p`: Pretty-print (function + file:line on one line)

If the tool is not on your PATH, specify the full path:

```bash
ferrite-server --addr2line /opt/arm-toolchain/bin/arm-none-eabi-addr2line
```

## Limitations

- Symbolication only works for PC addresses. LR (link register) symbolication is planned.
- If no ELF file matches the device's firmware version, the server falls back to the most recently uploaded ELF.
- ELF files must be uploaded before or at the same time as fault records for symbolication to work. Faults received before an ELF is available will have `symbol: null`.
- The server does not re-symbolicate historical faults when a new ELF is uploaded.

## CI integration

Add an ELF upload step to your CI pipeline:

```yaml
# GitHub Actions example
- name: Upload ELF for symbolication
  run: |
    curl -X POST https://ferrite.example.com/ingest/elf \
      -H "X-Firmware-Version: ${{ env.FIRMWARE_VERSION }}" \
      --data-binary @target/thumbv7em-none-eabihf/release/my-firmware
```
