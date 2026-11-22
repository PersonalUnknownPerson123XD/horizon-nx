//! What the console's own HOME menu looks like, read at run time so Horizon's overlays can match
//! it without shipping any of Nintendo's assets: the system's colour theme (Settings > Themes:
//! basic white or basic black) and its shared standard font, which the `pl` service maps into
//! every application's address space.
//!
//! Both are read once. Anything that goes wrong answers `None`, and the caller keeps its own
//! defaults (the dark palette and the bundled font).

use std::ffi::c_void;
use std::sync::OnceLock;

/// libnx `PlFontData`.
#[repr(C)]
struct PlFontData {
    kind: u32,
    size: u32,
    address: *mut c_void,
}

unsafe extern "C" {
    fn plInitialize(service_type: u32) -> u32;
    fn plGetSharedFontByType(font: *mut PlFontData, kind: u32) -> u32;
    fn setsysInitialize() -> u32;
    fn setsysExit();
    fn setsysGetColorSetId(out: *mut i32) -> u32;
}

/// libnx `PlServiceType_User` and `PlSharedFontType_Standard`.
const PL_USER: u32 = 0;
const FONT_STANDARD: u32 = 0;

/// The console's colour theme (libnx `ColorSetId`: 0 basic white, 1 basic black).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemTheme {
    Light,
    Dark,
}

/// The theme the player chose in the console's settings, if the settings service answers.
pub fn system_theme() -> Option<SystemTheme> {
    static THEME: OnceLock<Option<SystemTheme>> = OnceLock::new();
    *THEME.get_or_init(|| {
        let mut id = -1;
        // SAFETY: plain libnx calls; `id` outlives them.
        let ok = unsafe {
            let started = setsysInitialize() == 0;
            let got = started && setsysGetColorSetId(&mut id) == 0;
            if started {
                setsysExit();
            }
            got
        };
        match (ok, id) {
            (true, 0) => Some(SystemTheme::Light),
            (true, 1) => Some(SystemTheme::Dark),
            _ => None,
        }
    })
}

/// True for the bytes of a TrueType or OpenType font file (what `pl` hands out is decoded already).
fn is_font(bytes: &[u8]) -> bool {
    matches!(bytes.get(..4), Some([0, 1, 0, 0] | b"OTTO" | b"true" | b"ttcf"))
}

/// The console's standard shared font (Latin, Cyrillic and more), as the TrueType file the system
/// keeps in shared memory for the life of the process; `None` when `pl` doesn't answer or the data
/// isn't a font.
pub fn shared_font() -> Option<&'static [u8]> {
    static FONT: OnceLock<Option<&'static [u8]>> = OnceLock::new();
    *FONT.get_or_init(|| {
        let mut font = PlFontData {
            kind: 0,
            size: 0,
            address: std::ptr::null_mut(),
        };
        // SAFETY: libnx fills `font`; the shared memory stays mapped until `plExit`, which is never
        // called, so the slice is valid for the rest of the run.
        let bytes = unsafe {
            if plInitialize(PL_USER) != 0 || plGetSharedFontByType(&mut font, FONT_STANDARD) != 0 {
                return None;
            }
            if font.address.is_null() || font.size < 1024 {
                return None;
            }
            std::slice::from_raw_parts(font.address.cast::<u8>(), font.size as usize)
        };
        is_font(bytes).then_some(bytes)
    })
}
