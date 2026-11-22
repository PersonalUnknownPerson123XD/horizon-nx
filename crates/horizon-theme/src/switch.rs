//! The Switch look of Horizon's own overlays (the on-screen keyboard, the button hints): a flat,
//! rounded, two-appearance design in the spirit of the console's HOME menu, drawn with our own
//! colours, shapes and motion. Nothing here is Nintendo's art: no glyph bitmaps, icons or sounds,
//! and the font is the one the console hands every application at run time (`horizon-nx`'s
//! `shared_font`), not a file in this repository.
//!
//! The same [`Palette`] as the rest of the design system, so every text colour is held to the same
//! WCAG AA contrast tests; `accent` is the focus frame's cyan (dark) or blue (light).

use crate::Palette;

/// The dark appearance: charcoal plates, white text, a cyan focus frame.
pub const SWITCH_DARK: Palette = Palette {
    page: 0x2b2b2b,
    plate_soft: 0x353535,
    plate: 0x404040,
    surface: 0x323232,
    rule: 0x4d4d4d,
    ink: 0xf5f5f5,
    ink_soft: 0xd6d6d6,
    muted: 0xb4b4b4,
    accent: 0x00c3e3,
    on_accent: 0x0b1b20,
    danger: 0xff8c96,
    success: 0x4fd88a,
    dark: true,
};

/// The light appearance: pale grey plates, charcoal text, a blue focus frame.
pub const SWITCH_LIGHT: Palette = Palette {
    page: 0xebebeb,
    plate_soft: 0xf5f5f5,
    plate: 0xdcdcdc,
    surface: 0xffffff,
    rule: 0xcfcfcf,
    ink: 0x2d2d2d,
    ink_soft: 0x464646,
    muted: 0x5a5a5a,
    accent: 0x0062a8,
    on_accent: 0xffffff,
    danger: 0xb3142a,
    success: 0x136a38,
    dark: false,
};

/// Keys, tiles and chips: generous rounding.
pub const RADIUS_KEY: f32 = 8.0;
/// A panel's corners.
pub const RADIUS_PANEL: f32 = 14.0;
/// The focus frame's thickness in dp, and how far it stands off the thing it frames.
pub const FRAME: f32 = 3.0;
pub const FRAME_GAP: f32 = 2.0;
/// The frame glides to the newly focused key in this many seconds (ease-out).
pub const FOCUS_MOVE_SECS: f32 = 0.14;
/// One breath of the frame's glow, in seconds.
pub const GLOW_PERIOD_SECS: f32 = 1.6;

/// Ease-out cubic: fast at first, settling. `t` in 0..=1.
pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// The glow's strength at `secs` into the run, 0.35..=1.0, a slow sine breath.
pub fn glow(secs: f32) -> f32 {
    let phase = secs / GLOW_PERIOD_SECS * std::f32::consts::TAU;
    0.675 + 0.325 * phase.sin()
}

/// The palette for a console theme; `dark` is the system's setting.
pub const fn palette(dark: bool) -> Palette {
    if dark { SWITCH_DARK } else { SWITCH_LIGHT }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contrast;

    #[test]
    fn text_meets_wcag_aa_on_every_plate() {
        for p in [SWITCH_DARK, SWITCH_LIGHT] {
            for bg in [p.page, p.plate_soft, p.plate, p.surface] {
                for fg in [p.ink, p.ink_soft, p.muted, p.accent, p.danger, p.success] {
                    let ratio = contrast(fg, bg);
                    assert!(ratio >= 4.5, "{fg:06x} on {bg:06x}: {ratio:.2}");
                }
            }
            assert!(contrast(p.on_accent, p.accent) >= 4.5);
        }
    }

    #[test]
    fn the_focus_frame_reads_against_the_page() {
        for p in [SWITCH_DARK, SWITCH_LIGHT] {
            assert!(contrast(p.accent, p.page) >= 3.0, "WCAG 1.4.11");
        }
    }

    #[test]
    fn motion_is_bounded_and_settles() {
        assert_eq!(ease_out(0.0), 0.0);
        assert_eq!(ease_out(1.0), 1.0);
        assert!(ease_out(0.3) > 0.6);
        for i in 0..200 {
            let g = glow(i as f32 * 0.05);
            assert!((0.35..=1.0).contains(&g), "{g}");
        }
    }
}
