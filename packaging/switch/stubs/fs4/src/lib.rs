//! The Switch build's stand-in for fs4 (packaging/switch/cargo-patch.toml): fs4 goes through
//! rustix's file-system calls, which Horizon's newlib only has in part. The game asks one thing,
//! the free space of the SD card under a folder.

use std::{ffi::CString, io, os::unix::ffi::OsStrExt, path::Path};

/// The bytes free for this program on the volume holding `path`.
pub fn available_space<P: AsRef<Path>>(path: P) -> io::Result<u64> {
    let path = CString::new(path.as_ref().as_os_str().as_bytes())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::zeroed();
    if unsafe { libc::statvfs(path.as_ptr(), stat.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let stat = unsafe { stat.assume_init() };
    Ok((stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64))
}
