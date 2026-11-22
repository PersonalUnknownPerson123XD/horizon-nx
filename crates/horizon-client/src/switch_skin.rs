//! The Switch look of Horizon's own overlays (`horizon_theme::switch`): flat rounded plates, a
//! cyan (dark) or blue (light) focus frame that glides to the focused key and breathes, and a hint
//! row of round button glyphs. It follows the console's own theme setting on the Switch; on other
//! platforms it is off unless asked for, so nothing changes there.
//!
//! Choose it with `overlayTheme:` in `horizon.txt` (or `HORIZON_OVERLAY_THEME`): `horizon` (the
//! design system's own look), `switch` (this look, dark or light as the console is set), or
//! `switch-dark` / `switch-light`. The default is `switch` on the Switch and `horizon` elsewhere.
//!
//! Nothing here is Nintendo's art. The labels on the glyphs are plain letters in the system's
//! shared font, which `horizon_nx::shared_font` hands to [`crate::ui_theme::set_system_font`].

use crate::ui_theme::{self, Face, Ui, color};
use horizon_theme::{Palette, switch as theme};
use std::{cell::RefCell, sync::OnceLock, time::Instant};

/// Which look the overlays wear.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Look {
    /// The design system's own palette (`horizon_theme::DARK`).
    Horizon,
    Switch { dark: bool },
}

/// The look named by a `overlayTheme` value; `system_dark` is the console's own setting (dark when
/// unknown).
pub(crate) fn parse_look(value: Option<&str>, on_switch: bool, system_dark: bool) -> Look {
    match value.map(str::trim) {
        Some("horizon") => Look::Horizon,
        Some("switch-dark") => Look::Switch { dark: true },
        Some("switch-light") => Look::Switch { dark: false },
        Some("switch") => Look::Switch { dark: system_dark },
        _ if on_switch => Look::Switch { dark: system_dark },
        _ => Look::Horizon,
    }
}

/// The look in force, decided once.
pub(crate) fn look() -> Look {
    static LOOK: OnceLock<Look> = OnceLock::new();
    *LOOK.get_or_init(|| {
        let value = std::env::var("HORIZON_OVERLAY_THEME")
            .ok()
            .or_else(|| ui_theme::preference(std::path::Path::new("."), "overlayTheme"));
        #[cfg(target_os = "horizon")]
        let (on_switch, system_dark) = (
            true,
            !matches!(
                horizon_nx::system_theme(),
                Some(horizon_nx::SystemTheme::Light)
            ),
        );
        #[cfg(not(target_os = "horizon"))]
        let (on_switch, system_dark) = (false, true);
        parse_look(value.as_deref(), on_switch, system_dark)
    })
}

/// The Switch palette in force, if the overlays wear it.
pub(crate) fn palette() -> Option<Palette> {
    match look() {
        Look::Switch { dark } => Some(theme::palette(dark)),
        Look::Horizon => None,
    }
}

/// Seconds since the first call: the clock the glow breathes by.
pub(crate) fn seconds() -> f32 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f32()
}

/// A rectangle (x, y, w, h) gliding to wherever the focus goes.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Glide {
    from: [f32; 4],
    to: [f32; 4],
    since: f32,
    /// The last time it was asked for: a long gap means the thing was closed meanwhile.
    seen: f32,
    started: bool,
}
impl Glide {
    /// Where the frame is at time `now` (seconds) when the focus is on `target`: a new target
    /// starts a glide from wherever the frame had got to; the very first one appears in place.
    pub(crate) fn at(&mut self, target: [f32; 4], now: f32) -> [f32; 4] {
        if !self.started || now - self.seen > 0.5 {
            *self = Self {
                from: target,
                to: target,
                since: now,
                seen: now,
                started: true,
            };
            return target;
        }
        self.seen = now;
        if target != self.to {
            self.from = self.position(now);
            self.to = target;
            self.since = now;
        }
        self.position(now)
    }
    fn position(&self, now: f32) -> [f32; 4] {
        let k = theme::ease_out((now - self.since) / theme::FOCUS_MOVE_SECS);
        let mut out = [0.0; 4];
        for (i, v) in out.iter_mut().enumerate() {
            *v = self.from[i] + (self.to[i] - self.from[i]) * k;
        }
        out
    }
}
thread_local! {
    static FRAME: RefCell<Glide> = RefCell::new(Glide::default());
}

/// A colour with its alpha scaled.
fn faded(c: u32, k: f32) -> u32 {
    color::with_alpha(c, k)
}

/// The focus frame around `target` (x, y, w, h in dp): the accent, `FRAME` dp thick, standing off
/// the key by `FRAME_GAP`, with a soft glow of fading rings outside it.
pub(crate) fn focus_frame(ui: &mut Ui, p: &Palette, target: [f32; 4], radius: f32) {
    let now = seconds();
    let [x, y, w, h] = FRAME.with(|f| f.borrow_mut().at(target, now));
    let accent = color::hex(p.accent, 255);
    let step = 1.0 / ui.density;
    let glow = theme::glow(now);
    let base = theme::FRAME_GAP;
    let rings = (theme::FRAME * ui.density).round().max(1.0) as u32;
    for i in 0..rings {
        let o = base + i as f32 * step;
        ui.border(x - o, y - o, w + 2.0 * o, h + 2.0 * o, radius + o, accent);
    }
    // The glow: four more rings, each fainter.
    for i in 0..4 {
        let o = base + theme::FRAME + i as f32 * 1.5;
        let k = glow * 0.35 * (1.0 - i as f32 / 4.0);
        ui.border(
            x - o,
            y - o,
            w + 2.0 * o,
            h + 2.0 * o,
            radius + o,
            faded(accent, k),
        );
    }
}

/// A round button glyph (a pill for the longer ones) with `label` in it, left edge `x`, top `y`,
/// `size` dp tall; returns its width.
pub(crate) fn glyph(ui: &mut Ui, p: &Palette, x: f32, y: f32, size: f32, label: &str) -> f32 {
    let tw = ui.measure(Face::Medium, ui_theme::SMALL, label);
    let w = (tw + 12.0).max(size);
    ui.rounded(x, y, w, size, size / 2.0, color::hex(p.plate, 255));
    ui.border(x, y, w, size, size / 2.0, color::hex(p.muted, 255));
    ui.text_line(
        Face::Medium,
        ui_theme::SMALL,
        size,
        x + ((w - tw) / 2.0).round(),
        y,
        color::hex(p.ink, 255),
        label,
    );
    w
}

/// A row of "glyph  label" hints centred on `cx`, top `y`; returns its width.
pub(crate) fn hint_row(
    ui: &mut Ui,
    p: &Palette,
    cx: f32,
    y: f32,
    hints: &[(String, String)],
) -> f32 {
    const SIZE: f32 = 22.0;
    const GAP: f32 = 6.0;
    const BETWEEN: f32 = 20.0;
    let widths: Vec<(f32, f32)> = hints
        .iter()
        .map(|(g, t)| {
            let gw = (ui.measure(Face::Medium, ui_theme::SMALL, g) + 12.0).max(SIZE);
            (gw, ui.measure(Face::Regular, ui_theme::SMALL, t))
        })
        .collect();
    let total: f32 = widths.iter().map(|(g, t)| g + GAP + t).sum::<f32>()
        + BETWEEN * hints.len().saturating_sub(1) as f32;
    let mut x = (cx - total / 2.0).round();
    for ((g, t), (gw, tw)) in hints.iter().zip(widths) {
        glyph(ui, p, x, y, SIZE, g);
        ui.text_line(
            Face::Regular,
            ui_theme::SMALL,
            SIZE,
            x + gw + GAP,
            y,
            color::hex(p.ink_soft, 255),
            t,
        );
        x += gw + GAP + tw + BETWEEN;
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_look_follows_the_setting_then_the_platform() {
        assert_eq!(parse_look(None, false, true), Look::Horizon);
        assert_eq!(parse_look(None, true, false), Look::Switch { dark: false });
        assert_eq!(parse_look(Some("horizon"), true, true), Look::Horizon);
        assert_eq!(
            parse_look(Some("switch"), false, false),
            Look::Switch { dark: false }
        );
        assert_eq!(
            parse_look(Some(" switch-dark "), false, false),
            Look::Switch { dark: true }
        );
        assert_eq!(
            parse_look(Some("switch-light"), true, true),
            Look::Switch { dark: false }
        );
        assert_eq!(parse_look(Some("nonsense"), false, true), Look::Horizon);
    }

    #[test]
    fn the_frame_glides_from_where_it_was_and_settles() {
        let (a, b) = ([0.0, 0.0, 10.0, 10.0], [100.0, 40.0, 20.0, 10.0]);
        let mut g = Glide::default();
        assert_eq!(g.at(a, 5.0), a, "the first focus appears in place");
        assert_eq!(g.at(a, 6.0), a);
        assert_eq!(g.at(b, 6.0), a, "a new target starts from where the frame is");
        let mid = g.at(b, 6.0 + theme::FOCUS_MOVE_SECS / 4.0);
        assert!(mid[0] > 50.0 && mid[0] < 100.0, "ease-out is past half early: {mid:?}");
        // Retargeting mid-glide starts from the frame's current place, not from the old key.
        let c = [0.0, 40.0, 20.0, 10.0];
        let t = 6.0 + theme::FOCUS_MOVE_SECS / 4.0;
        let again = g.at(c, t);
        assert!((again[0] - mid[0]).abs() < 1e-3, "{again:?} vs {mid:?}");
        assert_eq!(g.at(c, t + 0.4), c);
        // Closed for a while, then reopened elsewhere: it appears in place.
        assert_eq!(g.at(b, t + 5.0), b);
    }
}
