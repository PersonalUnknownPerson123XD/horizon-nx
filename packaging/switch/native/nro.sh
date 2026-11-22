#!/usr/bin/env bash
set -euo pipefail
ROOT=/workspace/horizon-nx; BH=/workspace/bevy-horizon
export PATH=$HOME/.cargo/bin:/opt/devkitpro/devkitA64/bin:/opt/devkitpro/tools/bin:$PATH DEVKITPRO=/opt/devkitpro
export RUSTUP_TOOLCHAIN=nightly-2026-10-04
export HORIZON_NX_NVK_LIB=$BH/nvk/lib
CONFIG="--config $BH/.cargo/config.toml --config $ROOT/packaging/switch/cargo-bevy-horizon.toml --config $ROOT/packaging/switch/cargo-patch.toml"
cd $ROOT
cp Cargo.lock Cargo.lock.switch-saved
trap 'mv -f $ROOT/Cargo.lock.switch-saved $ROOT/Cargo.lock' EXIT
STD_LIB="$(rustc --print sysroot)/lib/rustlib/src/rust/library"
cargo update $CONFIG -p libc --manifest-path "$STD_LIB/Cargo.toml"
cargo build --release -j ${JOBS:-6} $CONFIG -Z build-std=core,alloc,std,panic_abort -Z json-target-spec \
  --target $BH/aarch64-nintendo-horizon.json -p horizon-client --bin horizon-client --no-default-features \
  --features "${FEATURES:-audio,controllers,nx-allocator}"
