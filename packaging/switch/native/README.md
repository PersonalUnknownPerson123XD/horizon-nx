# Building the NRO without Docker or Podman

`packaging/switch/build.sh` needs a container engine. These three scripts do the same steps on a plain
Linux box (they were used to build `out/horizon.nro` where only a user shell and `sudo apt` exist).
They take the same revisions as `build.sh` (`bevy-horizon` `3eac779`, `mesa-switch` `7789c1d`).

1. devkitA64, libnx and elf2nro/nacptool: `crane export devkitpro/devkita64:latest - | tar x` into a
   folder, linked as `/opt/devkitpro` (pkg.devkitpro.org answers 403 to some machines). Then
   `packaging/switch/portlibs-fallback.sh` for zlib, zstd and expat, and empty `libdl.a`, `librt.a`,
   `libutil.a`, `libgcc_s.a` in `portlibs/switch/lib` for NAK's Rust sanity link.
2. Rust `nightly-2026-10-04` with `rust-src` and the `aarch64-unknown-linux-gnu` target, plus
   `cargo install bindgen-cli cbindgen`; apt: `meson ninja-build flex bison llvm-19-dev
   libclc-19-dev libllvmspirvlib-19-dev libclang-19-dev spirv-tools glslang-tools python3-mako`.
3. `nvk.sh <mesa-switch checkout>` (NVK archive, no bevy-horizon patches: `7789c1d` already has
   them), `localize.sh` (`libvulkan_bh.a`), `nro.sh` (the game), then `nacptool` + `elf2nro`.
   In mesa-switch's `rustc-switch-wrapper.sh` add `*meson-private/sanity.rs*` to the sanity-check
   `case`, or meson's Rust probe tries to link for the Switch.
