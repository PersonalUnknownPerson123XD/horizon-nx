# Building Horizon for the Nintendo Switch (homebrew, `horizon.nro`)

The game's Switch code (`crates/horizon-nx`, the `target_os = "horizon"` parts of the client,
gilrs-core's Horizon backend) is the port's own: release 0.1.0 was built from it in the port's
workshop. This recipe rebuilds that workshop from a checkout. It produced NROs that ran on a console
(Atmosphère, November 2026) in the fork KoHaku743/horizon-nx, built on release 0.1.0's tree.

## Build

```sh
packaging/switch/build.sh all     # image, NVK (slow, once), then the NRO
```

Needs Docker or Podman on Linux. It clones [bevy-horizon](https://github.com/JohnUzoka/bevy-horizon)
(the target spec, the toolchain image, the crates it patches for Horizon, the NVK build scripts)
and [mesa-switch](https://github.com/JohnUzoka/mesa-switch) at the revision whose NVK release 0.1.0
linked (`7789c1d`) beside this repository. The NRO is
`target/aarch64-nintendo-horizon/release/horizon.nro`.

The graphics driver decides the frame rate: built from the same source, release 0.1.0's NVK runs at
60 fps where danfromtico's public mesa-switch (`606441c`) with bevy-horizon's patches runs at about
30 (console, 2026-11-20); the public one also hangs the GPU with NAK's `plat` and `ofold` passes on.
`MESA_SWITCH_URL` and `MESA_SWITCH_REV` pick another driver.

`.github/workflows/switch-nro.yml` runs the same script on pushes to `switch` and on pull requests
that touch the Switch build, and uploads `switch/horizon/horizon.nro` and `switch/horizon/client.txt`
as an artifact laid out like the SD card. It keeps the toolchain image and the NVK build between
runs (`pkg.devkitpro.org` refuses some CI machines; `portlibs-fallback.sh` builds zlib, zstd and
expat when the image lacks them).

Prepared game files are never part of the build: `horizon-prepare switch-export` copies a
player's own prepared folder for the SD card (`crates/horizon-prepare/src/switch_export.rs`).

## What the Switch build patches

The window, event-loop and networking copies the table names (`third_party/winit`, `mio`,
`socket2`, `wgpu`, `wgpu-core`, `getrandom-0.3`, and `crates/horizon-switch`) are not part of the
pull request that added this recipe; `packaging/switch/cargo-patch.toml` expects them.

Cargo `[patch]` tables passed with `--config`, so desktop builds are unchanged:

| Crate | From | Why |
|---|---|---|
| libc, getrandom 0.2 and 0.4, memmap2, rustix, ring, cpal, raw-window-handle | bevy-horizon's `patches/` | newlib's types on Horizon, libnx's audio renderer, the Horizon window handle |
| wgpu, wgpu-core, wgpu-core-deps-windows-linux-android, getrandom 0.3 | `third_party/`, from upstream PR #4532 | the Vulkan backend on `target_os = "horizon"` |
| winit | `third_party/winit`, from PR #4532, adapted | a Horizon backend: one full-screen window, the touch screen as a pointer. Adapted to this port: the game drains the applet's messages itself, and the loop hands HOME and sleep to `horizon_nx::suspend_until_out_of_background` |
| mio | `third_party/mio` (1.2.5) | the Switch's `poll(2)` types, `Waker` on Horizon, and a waker that sends a UDP datagram to itself (libnx has no pipe) |
| socket2 | `third_party/socket2` (0.5.10) | without the socket options libnx lacks; `sendmsg`/`recvmsg` report unsupported |
| arboard, if-addrs, fs4 | `packaging/switch/stubs/` | stand-ins with the few calls the game makes: no clipboard, the console's one address, free space through `statvfs` |

`crates/horizon-switch` (from PR #4532, without its runtime: `horizon-nx` is the runtime) is what
the winit backend reads: focus, docking, the touch screen.

## Not known to match the workshop

The workshop's own patches for winit, mio, socket2, arboard, if-addrs and fs4 were never
published; the ones here were written to the same crate versions `Cargo.lock` names, against what
the game's code expects of them. Expect differences on a console, first of all in: suspending and
resuming (HOME, sleep), the touch screen, hosting a LAN world and Play with Friends (mio's waker,
socket2), docked resolution changes.
