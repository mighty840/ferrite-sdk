{
  description = "Ferrite SDK — firmware observability dev environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        # Stable Rust with all embedded targets
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
          targets = [
            "thumbv6m-none-eabi"        # Cortex-M0/M0+ (RP2040)
            "thumbv7m-none-eabi"        # Cortex-M3
            "thumbv7em-none-eabi"       # Cortex-M4/M7 no FPU (STM32WL55)
            "thumbv7em-none-eabihf"     # Cortex-M4F (STM32L4A6, nRF52840)
            "thumbv8m.main-none-eabi"   # Cortex-M33 no FPU (nRF5340)
            "thumbv8m.main-none-eabihf" # Cortex-M33 FPU (STM32H563)
            "wasm32-unknown-unknown"    # Dashboard (Dioxus WASM)
          ];
        };

        # ESP32-C3 needs nightly with rust-src
        rustNightly = pkgs.rust-bin.nightly."2025-04-15".default.override {
          extensions = [ "rust-src" ];
          targets = [ "riscv32imc-unknown-none-elf" ];
        };
      in {
        devShells.default = pkgs.mkShell {
          name = "ferrite-dev";

          nativeBuildInputs = [
            # ── Rust toolchains ──
            rustToolchain

            # ── Embedded tooling ──
            pkgs.probe-rs-tools          # Flash & debug (STM32, nRF)
            pkgs.cargo-binutils          # objcopy, nm, size
            pkgs.cbindgen                # C FFI header generation
            pkgs.gcc-arm-embedded        # arm-none-eabi-gcc for C FFI example
            pkgs.stlink                  # st-flash for STM32WL55

            # ── Server dependencies ──
            pkgs.pkg-config
            pkgs.openssl

            # ── Dashboard ──
            pkgs.dioxus-cli              # dx serve / dx build

            # ── Cross-compilation (RPi gateway) ──
            pkgs.cross                   # Docker-based cross-compile

            # ── Testing ──
            pkgs.qemu                    # QEMU for lm3s6965evb tests

            # ── Docs site ──
            pkgs.nodejs_22

            # ── General dev tools ──
            pkgs.sshpass                 # RPi deployment
            pkgs.just                    # Task runner
            pkgs.cargo-watch             # Auto-rebuild on save
          ];

          buildInputs = [
            pkgs.openssl
          ] ++ pkgs.lib.optionals pkgs.stdenv.isLinux [
            pkgs.udev                    # libudev for gateway USB
            pkgs.dbus                    # libdbus for BLE
            pkgs.bluez                   # BLE stack
          ];

          shellHook = ''
            echo ""
            echo "  ╔══════════════════════════════════════════╗"
            echo "  ║  ferrite-sdk dev environment             ║"
            echo "  ╚══════════════════════════════════════════╝"
            echo ""
            echo "  Rust:       $(rustc --version)"
            echo "  Targets:    thumbv{6m,7m,7em,8m.main}-none-eabi[hf], wasm32, riscv32imc"
            echo "  Tools:      probe-rs, dx, cbindgen, cross, qemu"
            echo ""
            echo "  Quick start:"
            echo "    cargo test -p ferrite-sdk --no-default-features    # SDK tests"
            echo "    cargo test -p ferrite-server                       # Server tests"
            echo "    cargo run -p ferrite-server                        # Run server"
            echo "    cd ferrite-dashboard && dx serve                   # Dev dashboard"
            echo ""

            # Make sure linker finds system libs
            export PKG_CONFIG_PATH="${pkgs.openssl.dev}/lib/pkgconfig:$PKG_CONFIG_PATH"
          '';

          # For esp32c3 work, switch toolchain:
          # nix develop .#esp
          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
        };

        # Separate shell for ESP32-C3 (needs nightly)
        devShells.esp = pkgs.mkShell {
          name = "ferrite-esp";

          nativeBuildInputs = [
            rustNightly
            pkgs.python3         # esptool.py
            pkgs.esptool
          ];

          shellHook = ''
            echo ""
            echo "  ╔══════════════════════════════════════════╗"
            echo "  ║  ferrite ESP32-C3 environment (nightly)  ║"
            echo "  ╚══════════════════════════════════════════╝"
            echo ""
            echo "  Rust:   $(rustc --version)"
            echo "  Target: riscv32imc-unknown-none-elf"
            echo ""
          '';
        };
      }
    );
}
