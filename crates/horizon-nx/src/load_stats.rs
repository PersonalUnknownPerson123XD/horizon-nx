//! Loading diagnostics, opt-in: `HORIZON_NX_LOAD_STATS=1` (env.txt) times the SD card, so a log
//! tells a load step that waits on the card from one that works the CPU.
//!
//! On, the SD card's newlib device (`sdmc:`, its slot in `devoptab_list`) is replaced by a copy
//! whose opens, closes, reads, writes, `stat`s and listings count and time themselves before
//! calling the card's own; [`report`] gives a `[SD]` line on what happened since its last call.
//! Off (the default), nothing is installed: the device table is libnx's, and no read, write or
//! frame does any extra work.

use crate::logging::ffi::{DEVOPTAB_WORDS, STD_MAX, devoptab_list};
use core::ffi::{c_char, c_int, c_void};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

type R = *mut c_void;
/// newlib `struct stat`: only passed through.
type Stat = c_void;

/// The env variable that turns it on.
pub const VARIABLE: &str = "HORIZON_NX_LOAD_STATS";

// newlib `devoptab_t` fields (`sys/iosupport.h`), as indexes of its pointer-sized words.
const OPEN_R: usize = 2;
const CLOSE_R: usize = 3;
const WRITE_R: usize = 4;
const READ_R: usize = 5;
const STAT_R: usize = 8;
const DIROPEN_R: usize = 15;
const DIRNEXT_R: usize = 17;
const LSTAT_R: usize = 26;

type OpenR = unsafe extern "C" fn(R, *mut c_void, *const c_char, c_int, c_int) -> c_int;
type CloseR = unsafe extern "C" fn(R, *mut c_void) -> c_int;
type WriteR = unsafe extern "C" fn(R, *mut c_void, *const c_char, usize) -> isize;
type ReadR = unsafe extern "C" fn(R, *mut c_void, *mut c_char, usize) -> isize;
type StatR = unsafe extern "C" fn(R, *const c_char, *mut Stat) -> c_int;
type DirOpenR = unsafe extern "C" fn(R, *mut c_void, *const c_char) -> *mut c_void;
type DirNextR = unsafe extern "C" fn(R, *mut c_void, *mut c_char, *mut Stat) -> c_int;

unsafe extern "C" {
    /// libsysbase: the index in `devoptab_list` of the device a path names.
    fn FindDevice(name: *const c_char) -> c_int;
}

/// The card's own device, which every timed call ends up in.
static ORIGINAL: OnceLock<[usize; DEVOPTAB_WORDS]> = OnceLock::new();
/// The timed copy that takes its slot.
static mut TIMED: [usize; DEVOPTAB_WORDS] = [0; DEVOPTAB_WORDS];
static ON: AtomicBool = AtomicBool::new(false);

fn original<T: Copy>(field: usize) -> T {
    let word = ORIGINAL.get().expect("load_stats: not installed")[field];
    // SAFETY: `field` is one of the function pointers `install_from_env` checked, and `T` is that
    // field's type.
    unsafe { core::mem::transmute_copy(&word) }
}

/// Whether the SD card is timed (`HORIZON_NX_LOAD_STATS=1`).
pub fn enabled() -> bool {
    ON.load(Relaxed)
}

/// Times the SD card when `HORIZON_NX_LOAD_STATS=1`. Call it once, early in `init`, after
/// env.txt is applied and before other threads run.
pub(crate) fn install_from_env() {
    if std::env::var(VARIABLE).as_deref() != Ok("1") {
        return;
    }
    let result = unsafe { install() };
    let line = match result {
        Ok(()) => format!("[Boot] {VARIABLE}=1: the SD card is timed ([SD] lines)"),
        Err(why) => format!("[Boot] {VARIABLE}=1, but the SD card isn't timed: {why}"),
    };
    crate::logging::write_log_line(format_args!("{line}"));
}

unsafe fn install() -> Result<(), &'static str> {
    unsafe {
        let index = FindDevice(c"sdmc:".as_ptr());
        if index < 0 || index as usize >= STD_MAX {
            return Err("no SD card device in newlib's table");
        }
        let slot = (&raw mut devoptab_list)
            .cast::<*const usize>()
            .add(index as usize);
        let sd = slot.read();
        if sd.is_null() {
            return Err("no SD card device in newlib's table");
        }
        let mut words = [0usize; DEVOPTAB_WORDS];
        core::ptr::copy_nonoverlapping(sd, words.as_mut_ptr(), DEVOPTAB_WORDS);
        for field in [
            OPEN_R, CLOSE_R, READ_R, WRITE_R, STAT_R, DIROPEN_R, DIRNEXT_R,
        ] {
            if words[field] == 0 {
                return Err("the SD card device lacks a call it times");
            }
        }
        if ORIGINAL.set(words).is_err() {
            return Err("already installed");
        }
        let mut timed = words;
        timed[OPEN_R] = open_r as OpenR as usize;
        timed[CLOSE_R] = close_r as CloseR as usize;
        timed[WRITE_R] = write_r as WriteR as usize;
        timed[READ_R] = read_r as ReadR as usize;
        timed[STAT_R] = stat_r as StatR as usize;
        timed[DIROPEN_R] = diropen_r as DirOpenR as usize;
        timed[DIRNEXT_R] = dirnext_r as DirNextR as usize;
        if words[LSTAT_R] != 0 {
            timed[LSTAT_R] = lstat_r as StatR as usize;
        }
        let table = (&raw mut TIMED).cast::<usize>();
        core::ptr::copy_nonoverlapping(timed.as_ptr(), table, DEVOPTAB_WORDS);
        boot();
        ON.store(true, Relaxed);
        slot.write(table.cast_const());
        Ok(())
    }
}

unsafe extern "C" fn open_r(
    r: R,
    file: *mut c_void,
    path: *const c_char,
    flags: c_int,
    mode: c_int,
) -> c_int {
    let started = Instant::now();
    let rc = unsafe { original::<OpenR>(OPEN_R)(r, file, path, flags, mode) };
    OPEN.add(started, u64::from(rc == -1));
    rc
}

unsafe extern "C" fn close_r(r: R, file: *mut c_void) -> c_int {
    let started = Instant::now();
    let rc = unsafe { original::<CloseR>(CLOSE_R)(r, file) };
    CLOSE.add(started, 0);
    rc
}

unsafe extern "C" fn read_r(r: R, file: *mut c_void, buf: *mut c_char, len: usize) -> isize {
    let started = Instant::now();
    let n = unsafe { original::<ReadR>(READ_R)(r, file, buf, len) };
    READ.add(started, n.max(0) as u64);
    n
}

unsafe extern "C" fn write_r(r: R, file: *mut c_void, buf: *const c_char, len: usize) -> isize {
    let started = Instant::now();
    let n = unsafe { original::<WriteR>(WRITE_R)(r, file, buf, len) };
    WRITE.add(started, n.max(0) as u64);
    n
}

unsafe extern "C" fn stat_r(r: R, path: *const c_char, st: *mut Stat) -> c_int {
    let started = Instant::now();
    let rc = unsafe { original::<StatR>(STAT_R)(r, path, st) };
    STAT.add(started, u64::from(rc == -1));
    rc
}

unsafe extern "C" fn lstat_r(r: R, path: *const c_char, st: *mut Stat) -> c_int {
    let started = Instant::now();
    let rc = unsafe { original::<StatR>(LSTAT_R)(r, path, st) };
    STAT.add(started, u64::from(rc == -1));
    rc
}

unsafe extern "C" fn diropen_r(r: R, dir: *mut c_void, path: *const c_char) -> *mut c_void {
    let started = Instant::now();
    let opened = unsafe { original::<DirOpenR>(DIROPEN_R)(r, dir, path) };
    LIST.add(started, 0);
    opened
}

unsafe extern "C" fn dirnext_r(r: R, dir: *mut c_void, name: *mut c_char, st: *mut Stat) -> c_int {
    let started = Instant::now();
    let rc = unsafe { original::<DirNextR>(DIRNEXT_R)(r, dir, name, st) };
    // Time into the listing's total; one more entry when one came.
    LIST.add_time(started, u64::from(rc == 0));
    rc
}

/// One kind of call: how many, how long in all (ns), and a third number (failures, bytes).
struct Counter {
    count: AtomicU64,
    nanos: AtomicU64,
    extra: AtomicU64,
}

impl Counter {
    const fn new() -> Self {
        Counter {
            count: AtomicU64::new(0),
            nanos: AtomicU64::new(0),
            extra: AtomicU64::new(0),
        }
    }

    /// Counts one call that started at `since`, adding `extra`.
    fn add(&self, since: Instant, extra: u64) {
        self.count.fetch_add(1, Relaxed);
        self.nanos
            .fetch_add(since.elapsed().as_nanos() as u64, Relaxed);
        if extra != 0 {
            self.extra.fetch_add(extra, Relaxed);
        }
    }

    /// Adds time and `extra` without counting a call (a listing's next entry).
    fn add_time(&self, since: Instant, extra: u64) {
        self.nanos
            .fetch_add(since.elapsed().as_nanos() as u64, Relaxed);
        self.extra.fetch_add(extra, Relaxed);
    }

    fn get(&self) -> [u64; 3] {
        [
            self.count.load(Relaxed),
            self.nanos.load(Relaxed),
            self.extra.load(Relaxed),
        ]
    }
}

/// Opens (extra: failed ones, mostly files that aren't there).
static OPEN: Counter = Counter::new();
/// `stat` and `lstat` (extra: failed ones).
static STAT: Counter = Counter::new();
/// Folder listings opened (extra: entries read).
static LIST: Counter = Counter::new();
/// Reads (extra: bytes).
static READ: Counter = Counter::new();
/// Closes.
static CLOSE: Counter = Counter::new();
/// Writes (extra: bytes).
static WRITE: Counter = Counter::new();

const ALL: [&Counter; 6] = [&OPEN, &STAT, &LIST, &READ, &CLOSE, &WRITE];

/// The totals at the last [`report`], and when it was.
static LAST: Mutex<Option<(Instant, [[u64; 3]; 6])>> = Mutex::new(None);
static BOOT: OnceLock<Instant> = OnceLock::new();

fn boot() -> Instant {
    *BOOT.get_or_init(Instant::now)
}

fn ms(nanos: u64) -> f64 {
    nanos as f64 / 1e6
}

/// One line on the SD card since the last call (since it was installed, for the first): counts
/// and the time the calls took, added over all threads (so it can exceed the wall time when
/// threads wait on the card together). `None` when it isn't timed or nothing happened.
pub fn report() -> Option<String> {
    if !enabled() {
        return None;
    }
    let now: [[u64; 3]; 6] = ALL.map(Counter::get);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    let (since, before) = last.unwrap_or((boot(), [[0; 3]; 6]));
    *last = Some((Instant::now(), now));
    let d: Vec<[u64; 3]> = now
        .iter()
        .zip(before)
        .map(|(n, b)| [n[0] - b[0], n[1] - b[1], n[2] - b[2]])
        .collect();
    if d.iter().all(|c| c[0] == 0) {
        return None;
    }
    let [open, stat, list, read, close, write] = [d[0], d[1], d[2], d[3], d[4], d[5]];
    let waited = d.iter().map(|c| c[1]).sum::<u64>();
    let mib = |bytes: u64| bytes as f64 / 1048576.0;
    Some(format!(
        "[SD] over {:.1} s: {:.1} s in SD card calls; {} opens ({} missing) {:.0} ms, {} closes {:.0} ms, {} stats ({} missing) {:.0} ms, {} listings ({} entries) {:.0} ms, {} reads of {:.1} MiB {:.0} ms ({:.1} MiB/s while reading), {} writes of {:.1} MiB {:.0} ms",
        since.elapsed().as_secs_f64(),
        ms(waited) / 1000.0,
        open[0],
        open[2],
        ms(open[1]),
        close[0],
        ms(close[1]),
        stat[0],
        stat[2],
        ms(stat[1]),
        list[0],
        list[2],
        ms(list[1]),
        read[0],
        mib(read[2]),
        ms(read[1]),
        if read[1] > 0 {
            mib(read[2]) / (read[1] as f64 / 1e9)
        } else {
            0.0
        },
        write[0],
        mib(write[2]),
        ms(write[1]),
    ))
}
