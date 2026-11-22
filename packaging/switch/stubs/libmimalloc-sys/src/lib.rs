//! The libmimalloc-sys calls horizon-threads makes, on the Switch: options are remembered and do
//! nothing, collecting does nothing, allocation is the system heap, and the statistics are empty.
#![allow(non_camel_case_types, clippy::missing_safety_doc)]
use std::ffi::{c_char, c_int, c_long, c_void};
use std::sync::atomic::{AtomicI64, Ordering};

pub type mi_option_t = c_int;

static OPTIONS: [AtomicI64; 64] = [const { AtomicI64::new(0) }; 64];

pub unsafe fn mi_option_get(option: mi_option_t) -> c_long {
    OPTIONS.get(option as usize).map_or(0, |o| o.load(Ordering::Relaxed) as c_long)
}

pub unsafe fn mi_option_set(option: mi_option_t, value: c_long) {
    if let Some(o) = OPTIONS.get(option as usize) {
        o.store(value as i64, Ordering::Relaxed);
    }
}

pub unsafe fn mi_collect(_force: bool) {}

unsafe extern "C" {
    fn malloc(size: usize) -> *mut c_void;
    fn free(p: *mut c_void);
}

pub unsafe fn mi_malloc(size: usize) -> *mut c_void {
    unsafe { malloc(size) }
}

pub unsafe fn mi_free(p: *mut c_void) {
    unsafe { free(p) }
}

#[allow(clippy::too_many_arguments)]
pub unsafe fn mi_process_info(
    elapsed_msecs: *mut usize,
    user_msecs: *mut usize,
    system_msecs: *mut usize,
    current_rss: *mut usize,
    peak_rss: *mut usize,
    current_commit: *mut usize,
    peak_commit: *mut usize,
    page_faults: *mut usize,
) {
    for p in [
        elapsed_msecs,
        user_msecs,
        system_msecs,
        current_rss,
        peak_rss,
        current_commit,
        peak_commit,
        page_faults,
    ] {
        if !p.is_null() {
            unsafe { *p = 0 };
        }
    }
}

/// No statistics: null, as mimalloc returns when it can't write them.
pub unsafe fn mi_stats_get_json(_buf_size: usize, _buf: *mut c_char) -> *mut c_char {
    std::ptr::null_mut()
}
