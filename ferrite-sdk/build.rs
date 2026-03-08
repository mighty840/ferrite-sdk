fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();

    println!("cargo::rustc-check-cfg=cfg(has_fault_registers)");

    // ARMv7-M and ARMv7E-M targets have CFSR, HFSR, MMFAR, BFAR registers
    // and support Thumb-2 instructions (str high regs with immediate offset).
    if target.starts_with("thumbv7m") || target.starts_with("thumbv7em") {
        println!("cargo:rustc-cfg=has_fault_registers");
    }

    println!("cargo:rerun-if-changed=build.rs");
}
