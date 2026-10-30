# qemu-fault

A crash that survives the reset, in QEMU, without hardware.

The program runs on QEMU's `lm3s6965evb` machine (Cortex-M3). On the first
run it executes an undefined instruction (`udf`) in `handle_message`, which
causes a HardFault. ferrite's fault handler saves the registers and the fault
status into retained RAM and resets the chip. On the second run,
`ferrite_sdk::fault::last_fault()` finds that record and the program prints it.
`addr2line` then turns the program counter into a function and a line.

The crash is planted on purpose. A real device would upload the record with
`UploadManager` and carry on; this one prints it and exits so QEMU stops.

## Run it

Needs Rust with the `thumbv7m-none-eabi` target, `qemu-system-arm` and the
Arm GNU binutils (`arm-none-eabi-addr2line`).

```
rustup target add thumbv7m-none-eabi
cargo build --release
qemu-system-arm -cpu cortex-m3 -machine lm3s6965evb -nographic \
  -semihosting-config enable=on,target=native \
  -kernel target/thumbv7m-none-eabi/release/qemu-fault
```

`cargo run --release` does the same through the runner in `.cargo/config.toml`.
With rustc 1.97.0 and QEMU 7.2.22 it prints:

```
Timer with period zero, disabling
boot: no crash recorded, handling a message
boot: the previous run crashed
  pc   = 0x00000158
  cfsr = 0x00010000
```

The first line comes from QEMU, not the program. `cfsr` bit 16 is UNDEFINSTR,
an undefined instruction. Look the program counter up in the same ELF file:

```
arm-none-eabi-addr2line -f -C -s -e target/thumbv7m-none-eabi/release/qemu-fault 0x00000158
```

```
qemu_fault::handle_message
main.rs:39
```

Another compiler version may place the code at a different address; use the
`pc` your run prints.

`memory.x`, `device.x`, `build.rs` and `.cargo/config.toml` are the same as in
`tests/qemu`. `memory.x` reserves the retained RAM region at the top of RAM.
