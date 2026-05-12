//! ANSI / VT escape parser driving a [`crate::Grid`].
//!
//! Wraps the `vte` crate -- its state machine handles the wire-level
//! parsing (CSI, OSC, DCS, UTF-8); we only need to implement [`vte::Perform`]
//! to translate parsed events into grid mutations.
//!
//! Coverage is intentionally minimal for v0.4: enough to render the
//! output of common shells (zsh, bash) and dev servers (Metro, Vite,
//! tsc) without garbled output. Advanced features (mouse tracking,
//! bracketed paste, OSC 8 hyperlinks) are deferred.

use crate::grid::{Cell, Color, Grid};

/// 16-colour ANSI palette. Roughly matches the atomio theme — calm
/// foregrounds, dim backgrounds. Bright variants nudged slightly so
/// emphasis reads on the dark canvas.
const ANSI_PALETTE: [Color; 16] = [
    0x000000, // 0  black
    0xff6b6b, // 1  red
    0x9bd87f, // 2  green
    0xffb454, // 3  yellow
    0x6cb3ff, // 4  blue
    0xd18cff, // 5  magenta
    0x5ed4d4, // 6  cyan
    0xc8cfd9, // 7  white
    0x4f5663, // 8  bright black
    0xff8a8a, // 9  bright red
    0xb8e89a, // 10 bright green
    0xffd58a, // 11 bright yellow
    0x8cc4ff, // 12 bright blue
    0xdcb0ff, // 13 bright magenta
    0x9cdcdc, // 14 bright cyan
    0xe8ebf0, // 15 bright white
];

/// ANSI parser. Wraps a `vte::Parser` plus a borrow of the grid it
/// drives. Construct one per terminal session; feed `advance` with
/// arriving PTY bytes.
pub struct Parser {
    inner: vte::Parser,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser {
    /// New parser with default state.
    pub fn new() -> Self {
        Self {
            inner: vte::Parser::new(),
        }
    }

    /// Feed bytes from the PTY into the grid. Bytes can come in
    /// arbitrary chunks; the parser is stateful and handles splits
    /// across CSI sequences correctly.
    pub fn advance(&mut self, grid: &mut Grid, bytes: &[u8]) {
        let mut performer = Performer { grid };
        self.inner.advance(&mut performer, bytes);
    }
}

struct Performer<'a> {
    grid: &'a mut Grid,
}

impl vte::Perform for Performer<'_> {
    fn print(&mut self, c: char) {
        self.grid.print(c);
    }

    fn execute(&mut self, byte: u8) {
        // C0 controls.
        match byte {
            0x08 => self.grid.backspace(),        // BS
            0x09 => self.grid.tab(),              // HT
            0x0a..=0x0c => self.grid.line_feed(), // LF / VT / FF
            0x0d => self.grid.carriage_return(),  // CR
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        _intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        let action = action as u8;
        // Flatten params: vte yields each parameter as `&[u16]` (for
        // sub-params separated by `:`). We only consume the first
        // value of each parameter, which is enough for everything we
        // currently handle.
        let mut iter = params.iter();
        let mut next_param = || iter.next().map(|p| p[0]);
        match action {
            // CSI <n> A — cursor up
            b'A' => {
                let n = next_param().unwrap_or(1).max(1);
                let (row, col) = self.grid.cursor();
                self.grid.move_cursor(row.saturating_sub(n), col);
            }
            // CSI <n> B — cursor down
            b'B' => {
                let n = next_param().unwrap_or(1).max(1);
                let (row, col) = self.grid.cursor();
                self.grid.move_cursor(row.saturating_add(n), col);
            }
            // CSI <n> C — cursor forward
            b'C' => {
                let n = next_param().unwrap_or(1).max(1);
                let (row, col) = self.grid.cursor();
                self.grid.move_cursor(row, col.saturating_add(n));
            }
            // CSI <n> D — cursor back
            b'D' => {
                let n = next_param().unwrap_or(1).max(1);
                let (row, col) = self.grid.cursor();
                self.grid.move_cursor(row, col.saturating_sub(n));
            }
            // CSI <row> ; <col> H — cursor position (1-indexed)
            b'H' | b'f' => {
                let row = next_param().unwrap_or(1).max(1) - 1;
                let col = next_param().unwrap_or(1).max(1) - 1;
                self.grid.move_cursor(row, col);
            }
            // CSI <n> J — erase in display
            b'J' => {
                let n = next_param().unwrap_or(0);
                if n == 2 {
                    self.grid.erase_screen();
                }
                // 0 (cursor -> end) and 1 (start -> cursor) are common
                // but the practical impact for shell output is small;
                // most shells follow up with full-screen redraws. Add
                // them when a real workload regresses.
            }
            // CSI <n> K — erase in line
            b'K' => {
                let n = next_param().unwrap_or(0);
                if n == 0 {
                    self.grid.erase_to_eol();
                }
            }
            // CSI ... m — Select Graphic Rendition (SGR)
            b'm' => self.apply_sgr(params),
            _ => {
                // Quietly ignore unsupported sequences. The vte parser
                // already swallowed them; printing would confuse the
                // grid.
            }
        }
    }
}

impl Performer<'_> {
    fn apply_sgr(&mut self, params: &vte::Params) {
        // SGR with no params means reset (CSI m == CSI 0 m).
        if params.is_empty() {
            self.grid.reset_pen();
            return;
        }
        let mut iter = params.iter();
        while let Some(p) = iter.next() {
            let code = p[0];
            match code {
                0 => self.grid.reset_pen(),
                1 => self.grid.pen_mut().bold = true,
                22 => self.grid.pen_mut().bold = false,
                30..=37 => {
                    self.grid.pen_mut().fg = ANSI_PALETTE[(code - 30) as usize];
                }
                38 => {
                    // 38;5;n  -> 256-colour foreground (clamped to 16)
                    // 38;2;r;g;b -> truecolour foreground.
                    if let Some(idx) = parse_extended(&mut iter) {
                        self.grid.pen_mut().fg = idx;
                    }
                }
                39 => self.grid.pen_mut().fg = Cell::blank().fg,
                40..=47 => {
                    self.grid.pen_mut().bg = ANSI_PALETTE[(code - 40) as usize];
                }
                48 => {
                    if let Some(idx) = parse_extended(&mut iter) {
                        self.grid.pen_mut().bg = idx;
                    }
                }
                49 => self.grid.pen_mut().bg = Cell::blank().bg,
                90..=97 => {
                    self.grid.pen_mut().fg = ANSI_PALETTE[(code - 90 + 8) as usize];
                }
                100..=107 => {
                    self.grid.pen_mut().bg = ANSI_PALETTE[(code - 100 + 8) as usize];
                }
                _ => {}
            }
        }
    }
}

/// Resolve a 38;<mode>;... or 48;<mode>;... extended colour sequence.
/// Supports mode 5 (256-colour, clamped to the 16-colour palette) and
/// mode 2 (truecolour `r;g;b`). Returns `None` when the sequence is
/// malformed; caller leaves the pen untouched.
fn parse_extended(iter: &mut vte::ParamsIter<'_>) -> Option<Color> {
    let mode = iter.next()?[0];
    match mode {
        2 => {
            let r = iter.next()?[0] as u32 & 0xff;
            let g = iter.next()?[0] as u32 & 0xff;
            let b = iter.next()?[0] as u32 & 0xff;
            Some((r << 16) | (g << 8) | b)
        }
        5 => {
            let idx = iter.next()?[0] as usize;
            if idx < ANSI_PALETTE.len() {
                Some(ANSI_PALETTE[idx])
            } else {
                // 256-colour cube / grey ramp: bucket coarsely into
                // the 16-colour palette so output isn't lost.
                Some(ANSI_PALETTE[idx & 0x0f])
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_row(grid: &Grid, row: u16) -> String {
        let cols = grid.cols() as usize;
        let start = row as usize * cols;
        grid.cells()[start..start + cols]
            .iter()
            .map(|c| c.ch)
            .collect()
    }

    #[test]
    fn plain_text_lands_on_grid() {
        let mut g = Grid::new(8, 2);
        let mut p = Parser::new();
        p.advance(&mut g, b"hello");
        assert_eq!(render_row(&g, 0), "hello   ");
    }

    #[test]
    fn crlf_moves_cursor_to_next_line() {
        let mut g = Grid::new(8, 2);
        let mut p = Parser::new();
        p.advance(&mut g, b"hi\r\nyo");
        assert_eq!(render_row(&g, 0), "hi      ");
        assert_eq!(render_row(&g, 1), "yo      ");
    }

    #[test]
    fn sgr_red_changes_fg() {
        let mut g = Grid::new(4, 1);
        let mut p = Parser::new();
        p.advance(&mut g, b"\x1b[31mR\x1b[0m");
        let red = ANSI_PALETTE[1];
        assert_eq!(g.cells()[0].ch, 'R');
        assert_eq!(g.cells()[0].fg, red);
        // Reset back to default after closing SGR.
        p.advance(&mut g, b"X");
        assert_eq!(g.cells()[1].fg, Cell::blank().fg);
    }

    #[test]
    fn sgr_bold_sets_attribute() {
        let mut g = Grid::new(4, 1);
        let mut p = Parser::new();
        p.advance(&mut g, b"\x1b[1mB\x1b[0m");
        assert!(g.cells()[0].bold);
    }

    #[test]
    fn cursor_movement_csi() {
        let mut g = Grid::new(8, 4);
        let mut p = Parser::new();
        // Move to (2, 4) (1-indexed CSI -> 0-indexed grid).
        p.advance(&mut g, b"\x1b[3;5HX");
        assert_eq!(g.cells()[2 * 8 + 4].ch, 'X');
    }

    #[test]
    fn erase_screen_csi() {
        let mut g = Grid::new(4, 2);
        let mut p = Parser::new();
        p.advance(&mut g, b"abc");
        p.advance(&mut g, b"\x1b[2J");
        assert!(g.cells().iter().all(|c| c.ch == ' '));
    }

    #[test]
    fn truecolor_sgr_38_2() {
        let mut g = Grid::new(4, 1);
        let mut p = Parser::new();
        p.advance(&mut g, b"\x1b[38;2;10;20;30mX");
        let expected = (10u32 << 16) | (20u32 << 8) | 30u32;
        assert_eq!(g.cells()[0].fg, expected);
    }

    #[test]
    fn partial_csi_then_completion() {
        let mut g = Grid::new(4, 1);
        let mut p = Parser::new();
        // Split a CSI across two advance calls — vte should hold state.
        p.advance(&mut g, b"\x1b[3");
        p.advance(&mut g, b"1mR");
        assert_eq!(g.cells()[0].fg, ANSI_PALETTE[1]);
    }
}
