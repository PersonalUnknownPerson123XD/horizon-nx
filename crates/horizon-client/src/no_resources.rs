//! First-run setup when Horizon starts without its prepared game files.
//! The player copies their own `client.jar`, `.minecraft` folder, or a folder
//! already prepared by the desktop launcher / `horizon-prepare`, onto the SD
//! card (or next to the desktop binary). This screen finds those files, says
//! what is missing, and starts the game when a prepared tree is in place.
//! Horizon does not download Minecraft's files.
use std::path::{Path, PathBuf};

use crate::switch_setup::{self, Scan};
use crate::ui_theme::{self, Face, Ui, color};

/// The Horizon website (preparation on a computer still uses the desktop launcher).
pub const DOWNLOAD_URL: &str = "https://playhorizon.dev";

const TITLE: &str = "Set up Horizon's game files";
const COLUMN: f32 = 560.0;
const BUTTON_GAP: f32 = 12.0;

/// What a click on the screen does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Leave setup and start the game: prepared files were found.
    Start,
    /// Search the SD card (or game folder) again.
    Scan,
    Quit,
}

pub struct NoResources {
    /// Where the prepared files were first looked for.
    looked_in: String,
    /// Preferred install location (`resources/` beside the NRO, or the path given at launch).
    dest: PathBuf,
    /// Folders to search (NRO directory, working directory, and so on).
    bases: Vec<PathBuf>,
    /// Last scan of those folders.
    pub scan: Scan,
    /// Focused button for keyboard and pad navigation (index into the laid-out buttons).
    focus: usize,
}

impl NoResources {
    pub fn new(looked_in: &Path, bases: Vec<PathBuf>) -> Self {
        let shown = std::path::absolute(looked_in).unwrap_or_else(|_| looked_in.to_path_buf());
        let bases = if bases.is_empty() {
            let mut b = Vec::new();
            if let Some(parent) = looked_in.parent() {
                b.push(parent.to_path_buf());
            }
            b.push(looked_in.to_path_buf());
            if let Ok(cwd) = std::env::current_dir() {
                if !b.iter().any(|p| p == &cwd) {
                    b.push(cwd);
                }
            }
            b
        } else {
            bases
        };
        let mut screen = Self {
            looked_in: format!("Looked for prepared files in {}", shown.display()),
            dest: looked_in.to_path_buf(),
            bases,
            scan: Scan {
                prepared: None,
                prepared_error: None,
                client_jar: None,
                minecraft_dir: None,
                notes: Vec::new(),
            },
            focus: 0,
        };
        screen.rescan();
        screen
    }

    /// Search again and refresh the status.
    pub fn rescan(&mut self) {
        self.scan = switch_setup::scan(&self.bases);
        // Prefer starting on Start when the files are ready, else Scan again.
        self.focus = 0;
    }

    /// A prepared tree the game can start with, after a successful scan.
    pub fn ready_path(&self) -> Option<&Path> {
        if self.scan.is_ready() {
            self.scan.prepared.as_deref()
        } else {
            None
        }
    }

    /// Copy a prepared tree found elsewhere into `dest` when that helps, then
    /// the path the game should use.
    pub fn take_ready_path(&self) -> Option<PathBuf> {
        let src = self.ready_path()?;
        match switch_setup::install_prepared(src, &self.dest) {
            Ok(path) => Some(path),
            Err(e) => {
                log::warn!("Horizon: could not copy prepared files to {}: {e}", self.dest.display());
                Some(src.to_path_buf())
            }
        }
    }

    fn paragraphs(&self) -> Vec<String> {
        let mut parts = vec![switch_setup::status_text(&self.scan)];
        for note in &self.scan.notes {
            if !parts[0].contains(note) {
                parts.push(note.clone());
            }
        }
        parts
    }

    /// The buttons' labels, actions and rectangles in dp, and the paragraphs' wrapped
    /// lines: one layout for drawing and clicks.
    fn layout(&self, ui: &Ui) -> Layout {
        let width = COLUMN.min(ui.w - 64.0).max(160.0);
        let x = ((ui.w - width) / 2.0).round();
        let paragraphs = self.paragraphs();
        let lines: Vec<Vec<String>> = paragraphs
            .iter()
            .map(|p| wrap(ui, Face::Regular, BODY, p, width))
            .collect();
        let text_h = lines.iter().map(|l| l.len() as f32 * LINE).sum::<f32>()
            + (lines.len().saturating_sub(1)) as f32 * PARAGRAPH_GAP;
        let height = TITLE_LINE + 16.0 + text_h + 28.0 + CONTROL + 20.0 + SMALL_LINE * 2.0;
        let y = ((ui.h - height) / 2.0).max(24.0).round();
        let buttons_y = y + TITLE_LINE + 16.0 + text_h + 28.0;
        let ready = self.scan.is_ready();
        let mut specs: Vec<(&str, Action, bool)> = Vec::new();
        if ready {
            specs.push(("Start", Action::Start, true));
        }
        specs.push(("Scan again", Action::Scan, true));
        specs.push(("Quit", Action::Quit, true));
        let mut buttons = Vec::new();
        let mut bx = x;
        let count = specs.len() as f32;
        let avail = width - BUTTON_GAP * (count - 1.0).max(0.0);
        let even = (avail / count.max(1.0)).round().max(88.0);
        for (label, action, enabled) in specs {
            let measured = (ui.measure(Face::Medium, BUTTON_TEXT, label) + 40.0).round();
            let w = measured.max(even).min(width);
            if bx + w > x + width + 1.0 && !buttons.is_empty() {
                // Unlikely with three short labels; keep on one row by shrinking.
            }
            buttons.push(Button {
                label,
                action,
                enabled,
                x: bx,
                y: buttons_y,
                w,
            });
            bx += w + BUTTON_GAP;
        }
        Layout {
            x,
            y,
            width,
            lines,
            buttons,
        }
    }

    pub fn draw(&self, ui: &mut Ui) {
        let layout = self.layout(ui);
        ui.rounded(0.0, 0.0, ui.w, ui.h, 0.0, color::BG);
        let mut y = layout.y;
        ui.text_line(
            Face::Medium,
            TITLE_SIZE,
            TITLE_LINE,
            layout.x,
            y,
            color::TEXT,
            TITLE,
        );
        y += TITLE_LINE + 16.0;
        for paragraph in &layout.lines {
            for line in paragraph {
                ui.text_line(Face::Regular, BODY, LINE, layout.x, y, color::TEXT, line);
                y += LINE;
            }
            y += PARAGRAPH_GAP;
        }
        let focus = self.focus.min(layout.buttons.len().saturating_sub(1));
        for (i, button) in layout.buttons.iter().enumerate() {
            let hovered = button.contains(ui.cursor) || i == focus;
            let (plate, ink) = match (button.action, hovered) {
                (Action::Start, false) => (color::ACCENT, color::ON_ACCENT),
                (Action::Start, true) => {
                    (color::with_alpha(color::ACCENT, 0.85), color::ON_ACCENT)
                }
                (_, false) => (color::RAISED, color::TEXT),
                (_, true) => (color::HOVER, color::TEXT),
            };
            let h = CONTROL;
            ui.rounded(button.x, button.y, button.w, h, h / 2.0, plate);
            if i == focus {
                ui.rounded(
                    button.x - 2.0,
                    button.y - 2.0,
                    button.w + 4.0,
                    h + 4.0,
                    h / 2.0 + 2.0,
                    color::with_alpha(color::ACCENT, 0.35),
                );
                ui.rounded(button.x, button.y, button.w, h, h / 2.0, plate);
            }
            let lw = ui.measure(Face::Medium, BUTTON_TEXT, button.label);
            let lx = button.x + ((button.w - lw) / 2.0).round();
            ui.text_line(
                Face::Medium,
                BUTTON_TEXT,
                h,
                lx,
                button.y,
                ink,
                button.label,
            );
        }
        let small_y = layout.buttons[0].y + CONTROL + 20.0;
        for (i, line) in wrap(
            ui,
            Face::Regular,
            ui_theme::SMALL,
            &self.looked_in,
            layout.width,
        )
        .iter()
        .enumerate()
        {
            let line_y = small_y + i as f32 * SMALL_LINE;
            ui.text_line(
                Face::Regular,
                ui_theme::SMALL,
                SMALL_LINE,
                layout.x,
                line_y,
                color::MUTED,
                line,
            );
        }
    }

    /// The button under the cursor (`ui.cursor`, in dp), if any.
    pub fn click(&self, ui: &Ui) -> Option<Action> {
        self.layout(ui)
            .buttons
            .iter()
            .find(|b| b.contains(ui.cursor) && b.enabled)
            .map(|b| b.action)
    }

    /// Confirm the focused button (Enter / A).
    pub fn confirm(&self, ui: &Ui) -> Option<Action> {
        let layout = self.layout(ui);
        layout
            .buttons
            .get(self.focus.min(layout.buttons.len().saturating_sub(1)))
            .filter(|b| b.enabled)
            .map(|b| b.action)
    }

    /// Move focus left (`dir < 0`) or right (`dir > 0`).
    pub fn move_focus(&mut self, dir: i32, ui: &Ui) {
        let n = self.layout(ui).buttons.len();
        if n == 0 {
            return;
        }
        let step = if dir < 0 { n - 1 } else { 1 };
        self.focus = (self.focus + step) % n;
    }

    /// How many buttons the current layout has (tests and focus clamping).
    pub fn button_count(&self, ui: &Ui) -> usize {
        self.layout(ui).buttons.len()
    }
}

const TITLE_SIZE: f32 = horizon_theme::TITLE.px;
const TITLE_LINE: f32 = 40.0;
const BODY: f32 = horizon_theme::BODY.px;
const LINE: f32 = 24.0;
const PARAGRAPH_GAP: f32 = 10.0;
const BUTTON_TEXT: f32 = horizon_theme::UI.px;
const SMALL_LINE: f32 = 18.0;
const CONTROL: f32 = horizon_theme::CONTROL_LARGE;

struct Layout {
    x: f32,
    y: f32,
    width: f32,
    lines: Vec<Vec<String>>,
    buttons: Vec<Button>,
}

struct Button {
    label: &'static str,
    action: Action,
    enabled: bool,
    x: f32,
    y: f32,
    w: f32,
}

impl Button {
    fn contains(&self, (cx, cy): (f32, f32)) -> bool {
        cx >= self.x && cx < self.x + self.w && cy >= self.y && cy < self.y + CONTROL
    }
}

/// `text` broken at spaces into lines no wider than `width` dp (a longer word,
/// such as a path, gets a line of its own).
fn wrap(ui: &Ui, face: Face, size: f32, text: &str, width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split(' ') {
            let candidate = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if !line.is_empty() && ui.measure(face, size, &candidate) > width {
                lines.push(std::mem::replace(&mut line, word.to_owned()));
            } else {
                line = candidate;
            }
        }
        if !line.is_empty() {
            lines.push(line);
        } else if paragraph.is_empty() {
            lines.push(String::new());
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::{HudAtlas, HudBatch};
    use std::fs;

    /// A Ui over a `size` window at GUI scale 1, the cursor at `cursor` pixels.
    fn with_ui<R>(size: [u32; 2], cursor: (f32, f32), f: impl FnOnce(&mut Ui) -> R) -> R {
        let atlas =
            HudAtlas::build(std::path::Path::new("/nonexistent/horizon-test-assets")).unwrap();
        let mut batch = HudBatch::new(size);
        let cursor = (cursor.0 / batch.scale, cursor.1 / batch.scale);
        let mut ui = Ui::new(&atlas, &mut batch, cursor);
        f(&mut ui)
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "horizon-no-resources-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn missing_files_offer_scan_and_quit() {
        let root = temp("missing");
        let dest = root.join("resources");
        let screen = NoResources::new(&dest, vec![root.clone()]);
        assert!(!screen.scan.is_ready());
        for size in [[1280, 720], [800, 600], [2560, 1440]] {
            with_ui(size, (0.0, 0.0), |ui| {
                let layout = screen.layout(ui);
                let labels: Vec<_> = layout.buttons.iter().map(|b| b.action).collect();
                assert_eq!(labels, vec![Action::Scan, Action::Quit], "{size:?}");
                let center = |b: &Button| (b.x + b.w / 2.0, b.y + CONTROL / 2.0);
                let scan_at = center(&layout.buttons[0]);
                let quit_at = center(&layout.buttons[1]);
                let density = ui.density;
                drop(layout);
                assert_eq!(
                    screen.click(ui),
                    None,
                    "cursor was at {:?}, expected a later click",
                    ui.cursor
                );
                let _ = (scan_at, quit_at, density);
            });
            let (scan, quit, density) = with_ui(size, (0.0, 0.0), |ui| {
                let layout = screen.layout(ui);
                let center = |b: &Button| (b.x + b.w / 2.0, b.y + CONTROL / 2.0);
                (
                    center(&layout.buttons[0]),
                    center(&layout.buttons[1]),
                    ui.density,
                )
            });
            let at = |p: (f32, f32)| (p.0 * density, p.1 * density);
            assert_eq!(
                with_ui(size, at(scan), |ui| screen.click(ui)),
                Some(Action::Scan),
                "{size:?}"
            );
            assert_eq!(
                with_ui(size, at(quit), |ui| screen.click(ui)),
                Some(Action::Quit)
            );
            assert_eq!(with_ui(size, (1.0, 1.0), |ui| screen.click(ui)), None);
            with_ui(size, (0.0, 0.0), |ui| {
                let layout = screen.layout(ui);
                let last = layout.buttons.last().unwrap();
                assert!(
                    last.x + last.w <= ui.w && last.y + CONTROL <= ui.h,
                    "{size:?}"
                );
            });
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_prepared_folder_enables_start() {
        let root = temp("ready-ui");
        let dest = root.join("resources");
        fs::create_dir_all(&dest).unwrap();
        fs::write(
            dest.join(switch_setup::MANIFEST),
            format!("{{\"data_version\":{}}}", horizon_core::version::DATA_VERSION),
        )
        .unwrap();
        fs::write(dest.join(switch_setup::BLOCK_TAGS), "{}").unwrap();
        let screen = NoResources::new(&dest, vec![root.clone()]);
        assert!(screen.scan.is_ready());
        assert_eq!(screen.ready_path(), Some(dest.as_path()));
        with_ui([1280, 720], (0.0, 0.0), |ui| {
            let layout = screen.layout(ui);
            assert_eq!(
                layout.buttons.iter().map(|b| b.action).collect::<Vec<_>>(),
                vec![Action::Start, Action::Scan, Action::Quit]
            );
            assert_eq!(screen.confirm(ui), Some(Action::Start));
        });
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn text_wraps_within_the_column_and_keeps_every_word() {
        with_ui([1280, 720], (0.0, 0.0), |ui| {
            let paragraph = "Horizon needs your own copy of Minecraft Java Edition files on the SD card.";
            let lines = wrap(ui, Face::Regular, BODY, paragraph, 300.0);
            assert!(lines.len() > 1, "{paragraph}");
            for line in &lines {
                assert!(ui.measure(Face::Regular, BODY, line) <= 300.0, "{line}");
            }
            assert_eq!(lines.join(" "), paragraph);
        });
    }
}
