# fault-record

Everything ferrite's HardFault handler saves, printed after the reset, in QEMU
without hardware.

The program crashes once, then on the next boot prints the whole
`FaultRecord` from retained RAM:

- the eight registers the processor pushes onto the stack when the fault
  happens (R0 to R3, R12, LR, PC, xPSR);
- what ferrite's handler adds: R4 to R11, the SP of the code that crashed,
  the fault status registers (CFSR, HFSR, MMFAR, BFAR) and the 16 words of
  stack above that SP.

Two faults are built in:

- default: the `udf` crash from `examples/qemu-fault`, an undefined
  instruction (CFSR bit 16, UNDEFINSTR);
- `--features guard`: a read from 32 bytes at `0x2000_8000` that the MPU
  makes inaccessible (CFSR bits 1 and 7, DACCVIOL and MMARVALID), which puts
  the faulting address in MMFAR.

## Run it

Needs Rust with the `thumbv7m-none-eabi` target, `qemu-system-arm` and the
Arm GNU binutils (`arm-none-eabi-addr2line`).

```
rustup target add thumbv7m-none-eabi
cargo build --release
qemu-system-arm -cpu cortex-m3 -machine lm3s6965evb -nographic \
  -semihosting-config enable=on,target=native \
  -kernel target/thumbv7m-none-eabi/release/fault-record
```

With rustc 1.97.0 and QEMU 7.2.22 it prints:

```
Timer with period zero, disabling
boot: no crash recorded, handling a message
pushed by the processor:
  r0  = 0x00000000  r1 = 0xffffffff  r2 = 0x0000002c  r3   = 0x00001e6d
  r12 = 0x00000001  lr = 0x0000035d  pc = 0x00000158  xpsr = 0x61000000
saved by ferrite's handler:
  r4  = 0x00000000  r5 = 0x2000fb38  r6  = 0x00000000  r7  = 0x2000fa98
  r8  = 0x00000000  r9 = 0x00000000  r10 = 0x00000000  r11 = 0x00000000
  sp  = 0x2000fa98
  cfsr = 0x00010000  hfsr = 0x40000000
  mmfar = 0x00000000  bfar = 0x00000000
  stack from sp:
    0x2000fbf0 0x0000035d 0x00000002 0x00000000
    0x00000000 0x00000000 0x00000000 0x00000000
    0x00000000 0x00000000 0x00000000 0x00000000
    0x00000000 0x00000000 0x00000000 0x00000000
```

The first line comes from QEMU. `sp` is the SP of `handle_message` when it
crashed: the exception frame the processor pushed ends right below it. The
first two stack words are what `handle_message` pushed on entry, `main`'s
frame pointer and the return address `0x0000035d`, the same value as `lr`.

```
arm-none-eabi-addr2line -f -C -s -e target/thumbv7m-none-eabi/release/fault-record 0x00000158
```

```
fault_record::handle_message
main.rs:59
```

With `cargo build --release --features guard` and the same `qemu-system-arm`
command:

```
Timer with period zero, disabling
boot: no crash recorded, handling a message
pushed by the processor:
  r0  = 0xe000ed94  r1 = 0x00000001  r2 = 0x20008000  r3   = 0x00000009
  r12 = 0x00000001  lr = 0x000003b1  pc = 0x0000018c  xpsr = 0x21000000
saved by ferrite's handler:
  r4  = 0x00000000  r5 = 0x2000fb38  r6  = 0x00000000  r7  = 0x2000fa98
  r8  = 0x00000000  r9 = 0x00000000  r10 = 0x00000000  r11 = 0x00000000
  sp  = 0x2000fa88
  cfsr = 0x00000082  hfsr = 0x40000000
  mmfar = 0x20008004  bfar = 0x00000000
  stack from sp:
    0x00000004 0x00000003 0x00000000 0x00000000
    0x2000fbf0 0x000003b1 0x00000002 0x00000000
    0x00000000 0x00000000 0x00000000 0x00000000
    0x00000000 0x00000000 0x00000000 0x00000000
```

`mmfar = 0x20008004` is the address the program tried to read, and `pc`
(`0x0000018c`) resolves to `read_guarded` at `main.rs:81`.

Another compiler version may place the code at other addresses; use the
values your run prints. Before #140, the handler saved some of its own
registers and its own stack instead of the crashed code's (#139).

`memory.x`, `device.x`, `build.rs` and `.cargo/config.toml` are the same as in
`examples/qemu-fault`.
