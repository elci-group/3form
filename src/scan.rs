// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT

//! Real-time cascading progress scan bars.
//!
//! A scan bar is a pure function of a *phase* in `[0, 1)`. A [`Cascade`]
//! derives that phase from elapsed wall-clock time and a *slot* (a row in a
//! listing, a tool in a status line…), never from a tick counter. Motion
//! therefore stays smooth however often or irregularly the caller repaints.
//!
//! Each successive slot lags the one before it by [`Cascade::step`] of a
//! cycle. Stacked vertically, the sweeps form a diagonal wave that cascades
//! down the rows instead of every indicator pulsing in lockstep.
//!
//! Two shapes are provided:
//! - [`scan_bar`]: indeterminate. A comet with a fading tail sweeps across
//!   a thin track.
//! - [`progress_scan_bar`]: determinate. A smooth (1/8-cell) fill for
//!   `current / total`, with the comet sweeping the unfilled remainder while
//!   work is still running.
//!
//! Plain and coloured renderings always have identical display width, so
//! layouts that measure the plain form stay aligned.
//!
//! ```
//! use form3::scan::{scan_bar, Cascade};
//! use std::time::Duration;
//!
//! let cascade = Cascade::default();
//! let elapsed = Duration::from_millis(400);
//! for row in 0..3 {
//!     // Each row trails the one above it.
//!     let bar = scan_bar(cascade.phase(elapsed, row), 6, None);
//!     assert_eq!(form3::width::display_width(&bar), 6);
//! }
//! ```

use crate::ansi::{fg, reset, sgr, AnsiColor, Attr};
use std::time::Duration;

/// Unlit track cell.
const TRACK: char = '─';
/// Comet head, then its fading tail.
const COMET: [char; 4] = ['█', '▓', '▒', '░'];
/// Left-aligned partial blocks for 1/8 … 7/8 of a cell.
const EIGHTHS: [char; 7] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉'];
const FULL: char = '█';

/// Maps wall-clock time and a slot to a scan phase, with successive slots
/// trailing each other to form a cascade.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cascade {
    /// Time for one full sweep of a bar.
    pub period: Duration,
    /// Fraction of a cycle each successive slot lags the one before it.
    pub step: f32,
}

impl Default for Cascade {
    fn default() -> Self {
        Self {
            period: Duration::from_millis(1100),
            step: 0.08,
        }
    }
}

impl Cascade {
    pub const fn new(period: Duration, step: f32) -> Self {
        Self { period, step }
    }

    /// Sweep phase in `[0, 1)` for `slot` at `elapsed` into the animation.
    /// Higher slots trail lower ones.
    pub fn phase(&self, elapsed: Duration, slot: usize) -> f32 {
        let period = self.period.as_secs_f32().max(f32::EPSILON);
        (elapsed.as_secs_f32() / period - slot as f32 * self.step).rem_euclid(1.0)
    }

    /// `phase` pushed `steps` further back in the cascade — e.g. to continue
    /// a vertical cascade rightwards across several bars on one row.
    pub fn lag(&self, phase: f32, steps: f32) -> f32 {
        (phase - steps * self.step).rem_euclid(1.0)
    }
}

/// Indeterminate scan bar `width` cells wide at `phase` (`[0, 1)`).
///
/// The head travels far enough past the right edge for the whole tail to
/// leave the bar before the next sweep enters from the left, so the wrap is
/// seamless. `colour` paints the comet; the track is drawn dim. `None`
/// renders plain text.
pub fn scan_bar(phase: f32, width: usize, colour: Option<&AnsiColor>) -> String {
    paint(&comet_cells(phase, width), colour)
}

/// Determinate progress bar for `current / total`, `width` cells wide.
///
/// While `running` and not yet full, the comet sweeps the unfilled remainder
/// at `phase`, so a bar stuck at the same count still visibly shows work in
/// progress. With `total == 0` (size not yet known) the whole bar scans
/// while running, and is an empty track otherwise.
pub fn progress_scan_bar(
    current: u64,
    total: u64,
    running: bool,
    phase: f32,
    width: usize,
    colour: Option<&AnsiColor>,
) -> String {
    if total == 0 {
        return if running {
            scan_bar(phase, width, colour)
        } else {
            paint(&vec![Cell::Track; width], colour)
        };
    }

    let fraction = (current.min(total) as f64 / total as f64).clamp(0.0, 1.0);
    let eighths = (fraction * (width * 8) as f64).round() as usize;
    let full = eighths / 8;
    let partial = eighths % 8;

    let mut cells: Vec<Cell> = vec![Cell::Lit(FULL); full];
    if partial > 0 {
        cells.push(Cell::Lit(EIGHTHS[partial - 1]));
    }
    let rest = width - cells.len();
    if running {
        cells.extend(comet_cells(phase, rest));
    } else {
        cells.extend(std::iter::repeat_n(Cell::Track, rest));
    }
    paint(&cells, colour)
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Cell {
    /// Unlit track, drawn dim.
    Track,
    /// Lit glyph, drawn in the bar's colour.
    Lit(char),
}

impl Cell {
    fn glyph(self) -> char {
        match self {
            Cell::Track => TRACK,
            Cell::Lit(ch) => ch,
        }
    }
}

fn comet_cells(phase: f32, width: usize) -> Vec<Cell> {
    let travel = (width + COMET.len()) as f32;
    let head = (phase.rem_euclid(1.0) * travel).floor() as isize;
    (0..width as isize)
        .map(|i| match usize::try_from(head - i) {
            Ok(d) if d < COMET.len() => Cell::Lit(COMET[d]),
            _ => Cell::Track,
        })
        .collect()
}

/// Render cells, grouping runs of the same style so a coloured bar emits as
/// few escape sequences as possible.
fn paint(cells: &[Cell], colour: Option<&AnsiColor>) -> String {
    let Some(colour) = colour else {
        return cells.iter().map(|c| c.glyph()).collect();
    };

    let lit_code = fg(colour);
    let track_code = sgr(Attr::Dim);
    let mut out = String::new();
    let mut lit_run: Option<bool> = None;
    for cell in cells {
        let lit = matches!(cell, Cell::Lit(_));
        if lit_run != Some(lit) {
            if lit_run.is_some() {
                out.push_str(reset());
            }
            out.push_str(if lit { &lit_code } else { &track_code });
            lit_run = Some(lit);
        }
        out.push(cell.glyph());
    }
    if lit_run.is_some() {
        out.push_str(reset());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ansi::Color;
    use crate::width::display_width;

    fn at(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    #[test]
    fn phase_moves_with_wall_clock_time() {
        let c = Cascade::default();
        assert!(c.phase(at(0), 0) < c.phase(at(300), 0));
        // A full period later the bar is back where it started.
        assert!((c.phase(at(200), 0) - c.phase(at(200) + c.period, 0)).abs() < 1e-4);
    }

    #[test]
    fn successive_slots_cascade_behind_each_other() {
        let c = Cascade::default();
        let t = at(500);
        let lag = (c.phase(t, 0) - c.phase(t, 1)).rem_euclid(1.0);
        assert!((lag - c.step).abs() < 1e-4);
        assert!((c.lag(c.phase(t, 0), 1.0) - c.phase(t, 1)).abs() < 1e-4);
        // Neighbouring slots are visibly offset at the same instant.
        assert_ne!(
            scan_bar(c.phase(t, 0), 6, None),
            scan_bar(c.phase(t, 4), 6, None)
        );
    }

    #[test]
    fn comet_sweeps_left_to_right() {
        let head = |s: &str| s.chars().position(|c| c == '█');
        assert!(head(&scan_bar(0.05, 6, None)).unwrap() < head(&scan_bar(0.45, 6, None)).unwrap());
    }

    #[test]
    fn scan_bar_has_fixed_width_plain_and_coloured() {
        let colour = AnsiColor::Standard(Color::Yellow, false);
        for step in 0..40 {
            let phase = step as f32 / 40.0;
            assert_eq!(display_width(&scan_bar(phase, 6, None)), 6);
            assert_eq!(display_width(&scan_bar(phase, 6, Some(&colour))), 6);
        }
    }

    #[test]
    fn coloured_bar_dims_track_and_colours_comet() {
        let colour = AnsiColor::Standard(Color::Magenta, false);
        let bar = scan_bar(0.2, 8, Some(&colour));
        assert!(bar.contains(&fg(&colour)));
        assert!(bar.contains(&sgr(Attr::Dim)));
        assert!(bar.ends_with(reset()));
    }

    #[test]
    fn progress_fill_tracks_fraction() {
        assert_eq!(progress_scan_bar(0, 4, false, 0.0, 8, None), "────────");
        assert_eq!(progress_scan_bar(2, 4, false, 0.0, 8, None), "████────");
        assert_eq!(progress_scan_bar(4, 4, true, 0.3, 8, None), "████████");
        // Sub-cell precision: 1/16 of 8 cells is half a cell.
        assert_eq!(progress_scan_bar(1, 16, false, 0.0, 8, None), "▌───────");
    }

    #[test]
    fn running_progress_scans_the_remainder() {
        let a = progress_scan_bar(2, 4, true, 0.1, 8, None);
        let b = progress_scan_bar(2, 4, true, 0.5, 8, None);
        assert!(a.starts_with("████") && b.starts_with("████"));
        assert_ne!(a, b, "remainder should animate while running");
        assert_eq!(display_width(&a), 8);
    }

    #[test]
    fn unknown_total_scans_whole_bar_while_running() {
        assert_ne!(
            progress_scan_bar(0, 0, true, 0.2, 8, None),
            progress_scan_bar(0, 0, true, 0.6, 8, None)
        );
        assert_eq!(progress_scan_bar(0, 0, false, 0.2, 8, None), "────────");
    }
}
