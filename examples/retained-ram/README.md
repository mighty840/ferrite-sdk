# retained-ram

A boot counter in ferrite's retained RAM block, in QEMU, without hardware.
It shows what survives a reset, what doesn't survive a power cut, and how
the block's magic number tells ferrite's data from whatever else RAM holds.

`ferrite_sdk::init()` keeps a boot counter in the header of the retained
block. The program reads the header's magic number before `init()`, prints
the boot number, whether a fault record is present, and then resets itself
with `SCB::sys_reset()` until boot 3.

## Run it

Needs Rust with the `thumbv7m-none-eabi` target and `qemu-system-arm`.

```
rustup target add thumbv7m-none-eabi
cargo build --release
qemu-system-arm -cpu cortex-m3 -machine lm3s6965evb -nographic \
  -semihosting-config enable=on,target=native \
  -kernel target/thumbv7m-none-eabi/release/retained-ram
```

With rustc 1.97.0 and QEMU 7.2.22 it prints:

```
Timer with period zero, disabling
boot 1 (magic before init: 0x00000000)
  no fault record
  software reset
boot 2 (magic before init: 0xabcd1234)
  no fault record
  software reset
boot 3 (magic before init: 0xabcd1234)
  no fault record
```

The first line comes from QEMU. The counter survives the two software resets.
Running QEMU again starts from boot 1: a new QEMU process starts with RAM
full of zeros, as a stand-in for a power cut. Real SRAM after a power cut
holds unpredictable values rather than zeros, which is why the block carries
a magic number.

## Constructed noise

The `noise` feature fills the block with pseudo-random bytes before `init()`,
as a constructed stand-in for power-up SRAM, with the fault record's `valid`
flag set so the noise looks like a crash record. It doesn't reset, because the
noise would be written again on every boot.

```
cargo build --release --features noise
qemu-system-arm -cpu cortex-m3 -machine lm3s6965evb -nographic \
  -semihosting-config enable=on,target=native \
  -kernel target/thumbv7m-none-eabi/release/retained-ram
```

```
Timer with period zero, disabling
boot 1 (magic before init: 0x26acab3a)
  no fault record
```

The magic number doesn't match, so `init()` clears the whole block and the
counter starts again. Before #137, `init()` reset only the header and this run
reported the noise as a fault record (#136).

`memory.x`, `device.x`, `build.rs` and `.cargo/config.toml` are the same as in
`examples/qemu-fault`.
