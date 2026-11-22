//! (The runtime, the software keyboard and the link arguments are `horizon-nx`'s; the pads are
//! gilrs-core's Horizon backend. What is left here is what the winit backend reads: the applet's
//! focus and docking state, and the touch screen.)
//!
//! Switch input for Horizon: libnx's HID turned into the plain `Frame` the game's pad code reads
//! (`horizon-client/src/game/pad/hardware.rs`). The mapping, hotplug and touch logic are pure and
//! tested on any host; only `hid` talks to libnx and exists on `target_os = "horizon"`.
#[cfg(any(target_os = "horizon", feature = "ffi"))]
mod hid;
mod npad;
mod touch;

#[cfg(any(target_os = "horizon", feature = "ffi"))]
pub use hid::{Hid, Libnx, TouchScreen};
pub use npad::{Frame, Key, Raw, Tracker, bits, style};
pub use touch::{Pointer, Touch, Tracker as TouchTracker};
