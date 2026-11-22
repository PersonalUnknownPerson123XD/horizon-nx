//! The controller's motion sensor (libnx "six-axis sensor", `switch/services/hid.h`), for gyro
//! aiming: the handheld Joy-Con, a Pro Controller, or the right Joy-Con of a pair.
//!
//! [`motion`] starts the sensor of whichever controller plays (the same choice as rumble's,
//! `third_party/gilrs-core/src/platform/horizon/ff.rs`) and returns its latest reading. A
//! controller change (detaching the Joy-Con, a Pro Controller) stops the old sensor and starts
//! the new one.

use std::sync::Mutex;

/// libnx `HidVector`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Vector {
    x: f32,
    y: f32,
    z: f32,
}

/// libnx `HidSixAxisSensorState`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct State {
    delta_time: u64,
    sampling_number: u64,
    acceleration: Vector,
    angular_velocity: Vector,
    angle: Vector,
    direction: [[f32; 3]; 3],
    attributes: u32,
    reserved: u32,
}

unsafe extern "C" {
    fn hidGetNpadStyleSet(id: u32) -> u32;
    fn hidGetSixAxisSensorHandles(
        handles: *mut u32,
        total_handles: i32,
        id: u32,
        style: u32,
    ) -> u32;
    fn hidStartSixAxisSensor(handle: u32) -> u32;
    fn hidStopSixAxisSensor(handle: u32) -> u32;
    fn hidGetSixAxisSensorStates(handle: u32, states: *mut State, count: usize) -> usize;
}

/// libnx `HidNpadIdType_No1` and `HidNpadIdType_Handheld`.
const PLAYER_1: u32 = 0;
const HANDHELD: u32 = 0x20;
/// libnx `HidNpadStyleTag`s.
const STYLE_FULL_KEY: u32 = 1 << 0;
const STYLE_HANDHELD: u32 = 1 << 1;
const STYLE_JOY_DUAL: u32 = 1 << 2;

/// A motion reading, in the controller's axes (x to its right, y to its top, z out of its face).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Motion {
    /// Turning speed around each axis, in turns per second.
    pub rotation: [f32; 3],
    /// Acceleration in g; at rest it is gravity's reaction, pointing up.
    pub acceleration: [f32; 3],
}

/// The started sensor: the npad id and style it was made for, and its handle.
struct Sensor {
    target: (u32, u32),
    handle: u32,
}

static SENSOR: Mutex<Option<Sensor>> = Mutex::new(None);

/// The playing controller: the handheld Joy-Con when attached, else player 1's Pro Controller
/// or Joy-Con pair.
fn active_target() -> Option<(u32, u32)> {
    // SAFETY: plain libnx queries.
    let handheld = unsafe { hidGetNpadStyleSet(HANDHELD) };
    if handheld & STYLE_HANDHELD != 0 {
        return Some((HANDHELD, STYLE_HANDHELD));
    }
    let player = unsafe { hidGetNpadStyleSet(PLAYER_1) };
    if player & STYLE_FULL_KEY != 0 {
        Some((PLAYER_1, STYLE_FULL_KEY))
    } else if player & STYLE_JOY_DUAL != 0 {
        Some((PLAYER_1, STYLE_JOY_DUAL))
    } else {
        None
    }
}

fn start(target: (u32, u32)) -> Option<Sensor> {
    let (id, style) = target;
    // A Joy-Con pair has a sensor in each; the right one aims, as the right stick does.
    let count = if style == STYLE_JOY_DUAL { 2 } else { 1 };
    let mut handles = [0u32; 2];
    // SAFETY: `handles` holds `count` handles.
    let rc = unsafe { hidGetSixAxisSensorHandles(handles.as_mut_ptr(), count, id, style) };
    if rc != 0 {
        return None;
    }
    let handle = handles[count as usize - 1];
    // SAFETY: a handle libnx just gave.
    if unsafe { hidStartSixAxisSensor(handle) } != 0 {
        return None;
    }
    Some(Sensor { target, handle })
}

/// The playing controller's latest motion reading, starting its sensor first; `None` with no
/// controller or when the sensor can't start.
pub fn motion() -> Option<Motion> {
    let mut sensor = SENSOR.lock().ok()?;
    let target = active_target();
    if sensor.as_ref().map(|s| s.target) != target {
        if let Some(old) = sensor.take() {
            // SAFETY: the handle this module started.
            unsafe { hidStopSixAxisSensor(old.handle) };
        }
        *sensor = target.and_then(start);
    }
    let handle = sensor.as_ref()?.handle;
    let mut state = State::default();
    // SAFETY: room for one state.
    if unsafe { hidGetSixAxisSensorStates(handle, &mut state, 1) } == 0 {
        return None;
    }
    let v = |v: Vector| [v.x, v.y, v.z];
    Some(Motion {
        rotation: v(state.angular_velocity),
        acceleration: v(state.acceleration),
    })
}

/// Stop the sensor (gyro aiming turned off): it draws power while running.
pub fn stop_motion() {
    if let Ok(mut sensor) = SENSOR.lock()
        && let Some(old) = sensor.take()
    {
        // SAFETY: the handle this module started.
        unsafe { hidStopSixAxisSensor(old.handle) };
    }
}
