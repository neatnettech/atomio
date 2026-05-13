//! Terminal grid + cell model.
//!
//! Stores the visible viewport as a flat `Vec<Cell>` (row-major,
//! `cols * rows` long) plus a scrollback ring of complete rows that
//! the user has scrolled past or that the cursor pushed off the top.
//!
//! Coordinate system: `(row, col)` both 0-indexed, top-left origin.

/// Maximum scrollback rows retained. Picked to cover a few screenfuls
/// of long stack traces without ballooning memory. Each row is at
/// most `cols * sizeof(Cell)` bytes.
pub const DEFAULT_SCROLLBACK: usize = 5_000;

/// Cell colour. 24-bit RGB packed as `0x00RRGGBB`. The high byte is
/// reserved; consumers should mask if they care.
pub type Color = u32;

/// Default foreground / background. Match the atomio theme tokens
/// loosely so terminal output blends with the rest of the UI: TX_1
/// for fg, BG_1 for bg.
pub const DEFAULT_FG: Color = 0xe8ebf0;
pub const DEFAULT_BG: Color = 0x11141a;

/// One screen cell. Optimised for size; large grids stay cache-friendly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    /// Character rendered. Spaces fill empty cells, never `'\0'`.
    pub ch: char,
    /// Foreground colour.
    pub fg: Color,
    /// Background colour.
    pub bg: Color,
    /// Bold attribute (SGR 1).
    pub bold: bool,
}

impl Cell {
    /// Default empty cell: space, default fg/bg, not bold.
    pub const fn blank() -> Self {
        Self {
            ch: ' ',
            fg: DEFAULT_FG,
            bg: DEFAULT_BG,
            bold: false,
        }
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self::blank()
    }
}

/// Mutable grid + scrollback. Held inside [`crate::Terminal`] behind
/// a mutex so the read thread and the UI thread can both reach it.
#[derive(Debug, Clone)]
pub struct Grid {
    cols: u16,
    rows: u16,
    cells: Vec<Cell>,
    scrollback: Vec<Vec<Cell>>,
    /// Cursor position. Row + col both 0-indexed; clamped on insert.
    cursor_row: u16,
    cursor_col: u16,
    /// Current SGR pen state.
    pen: Cell,
}

impl Grid {
    /// New grid sized `cols x rows`, all cells blank.
    pub fn new(cols: u16, rows: u16) -> Self {
        assert!(cols > 0 && rows > 0, "grid dimensions must be positive");
        Self {
            cols,
            rows,
            cells: vec![Cell::blank(); cols as usize * rows as usize],
            scrollback: Vec::new(),
            cursor_row: 0,
            cursor_col: 0,
            pen: Cell::blank(),
        }
    }

    /// Grid columns.
    pub fn cols(&self) -> u16 {
        self.cols
    }

    /// Grid rows.
    pub fn rows(&self) -> u16 {
        self.rows
    }

    /// Cursor position `(row, col)`, both 0-indexed.
    pub fn cursor(&self) -> (u16, u16) {
        (self.cursor_row, self.cursor_col)
    }

    /// Move cursor to absolute `(row, col)`, clamped to the viewport.
    pub fn move_cursor(&mut self, row: u16, col: u16) {
        self.cursor_row = row.min(self.rows.saturating_sub(1));
        self.cursor_col = col.min(self.cols.saturating_sub(1));
    }

    /// Read-only access to viewport cells. Length is `cols * rows`,
    /// row-major.
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// One row of scrollback (oldest first). Mostly useful for tests
    /// and for a future scrollback rendering pass.
    pub fn scrollback(&self) -> &[Vec<Cell>] {
        &self.scrollback
    }

    /// Current pen state. Mutators on `Parser` adjust this; cell writes
    /// snapshot it.
    pub fn pen_mut(&mut self) -> &mut Cell {
        &mut self.pen
    }

    /// Reset pen to defaults (SGR 0).
    pub fn reset_pen(&mut self) {
        self.pen = Cell::blank();
    }

    /// Print one printable character at the cursor, advance the
    /// cursor by one column, scroll on overflow.
    pub fn print(&mut self, ch: char) {
        // Wrap into next line if cursor was already at right edge.
        if self.cursor_col >= self.cols {
            self.cursor_col = 0;
            self.line_feed();
        }
        let idx = self.idx(self.cursor_row, self.cursor_col);
        let mut cell = self.pen;
        cell.ch = ch;
        self.cells[idx] = cell;
        self.cursor_col += 1;
    }

    /// Carriage return: cursor to column 0 of current row.
    pub fn carriage_return(&mut self) {
        self.cursor_col = 0;
    }

    /// Line feed: move cursor down one row, scrolling the top row
    /// into scrollback if the cursor was already on the last row.
    pub fn line_feed(&mut self) {
        if self.cursor_row + 1 >= self.rows {
            self.scroll_up();
        } else {
            self.cursor_row += 1;
        }
    }

    /// Backspace: cursor one column to the left, clamp at 0.
    pub fn backspace(&mut self) {
        self.cursor_col = self.cursor_col.saturating_sub(1);
    }

    /// Horizontal tab: advance cursor to the next multiple of 8.
    pub fn tab(&mut self) {
        let next = ((self.cursor_col / 8) + 1) * 8;
        self.cursor_col = next.min(self.cols.saturating_sub(1));
    }

    /// Erase the visible viewport (CSI 2 J). Scrollback retained.
    pub fn erase_screen(&mut self) {
        for cell in &mut self.cells {
            *cell = Cell::blank();
        }
    }

    /// Erase from cursor to end of current line (CSI 0 K).
    pub fn erase_to_eol(&mut self) {
        let start = self.idx(self.cursor_row, self.cursor_col);
        let end = self.idx(self.cursor_row, self.cols - 1) + 1;
        for cell in &mut self.cells[start..end] {
            *cell = Cell::blank();
        }
    }

    /// Resize viewport. Cells are reallocated; cursor + pen preserved.
    /// Scrollback is untouched. On grow, new cells are blank; on
    /// shrink, the right/bottom edge is dropped.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        if cols == self.cols && rows == self.rows {
            return;
        }
        let mut new_cells = vec![Cell::blank(); cols as usize * rows as usize];
        let copy_rows = self.rows.min(rows);
        let copy_cols = self.cols.min(cols);
        for r in 0..copy_rows {
            for c in 0..copy_cols {
                let src = self.idx(r, c);
                let dst = r as usize * cols as usize + c as usize;
                new_cells[dst] = self.cells[src];
            }
        }
        self.cells = new_cells;
        self.cols = cols;
        self.rows = rows;
        self.cursor_row = self.cursor_row.min(rows.saturating_sub(1));
        self.cursor_col = self.cursor_col.min(cols.saturating_sub(1));
    }

    /// Take a cheap snapshot for the UI to render without holding the
    /// mutex during paint. Cells are cloned; scrollback is left alone.
    pub fn snapshot(&self) -> GridSnapshot {
        GridSnapshot {
            cols: self.cols,
            rows: self.rows,
            cells: self.cells.clone(),
            cursor: (self.cursor_row, self.cursor_col),
            scrollback_len: self.scrollback.len(),
        }
    }

    fn idx(&self, row: u16, col: u16) -> usize {
        row as usize * self.cols as usize + col as usize
    }

    fn scroll_up(&mut self) {
        let cols = self.cols as usize;
        // Push the top row into scrollback (capped).
        let top: Vec<Cell> = self.cells[..cols].to_vec();
        self.scrollback.push(top);
        if self.scrollback.len() > DEFAULT_SCROLLBACK {
            // Drop oldest. Vec::remove(0) is O(n); a VecDeque would be
            // better but the cap is only hit on heavy churn and the
            // copy is cache-friendly.
            self.scrollback.remove(0);
        }
        // Shift remaining rows up.
        self.cells.copy_within(cols.., 0);
        // Clear the new bottom row.
        let total = self.cells.len();
        for cell in &mut self.cells[total - cols..] {
            *cell = Cell::blank();
        }
    }
}

/// Cheap clone of a grid for the UI render path. Made on the parser
/// thread (under the mutex) and passed to the gpui thread without
/// taking the lock during paint.
#[derive(Debug, Clone)]
pub struct GridSnapshot {
    /// Columns wide.
    pub cols: u16,
    /// Rows tall.
    pub rows: u16,
    /// `cols * rows` cells, row-major.
    pub cells: Vec<Cell>,
    /// Cursor `(row, col)`.
    pub cursor: (u16, u16),
    /// How many rows live in scrollback at snapshot time. UI can show
    /// a "+N rows above" affordance.
    pub scrollback_len: usize,
}

impl GridSnapshot {
    /// Visible viewport as text, one `String` per row, trailing spaces
    /// trimmed. Used by callers that need to grep terminal output for
    /// markers (e.g. atomio's Metro detection) without re-implementing
    /// the row-major iteration. Blank rows come through as empty
    /// strings so caller-side line numbers line up with the grid.
    pub fn text_lines(&self) -> Vec<String> {
        let cols = self.cols as usize;
        if cols == 0 || self.rows == 0 {
            return Vec::new();
        }
        (0..self.rows as usize)
            .map(|r| {
                let start = r * cols;
                let row = &self.cells[start..start + cols];
                let mut line: String = row.iter().map(|c| c.ch).collect();
                let trimmed_len = line.trim_end().len();
                line.truncate(trimmed_len);
                line
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_grid_is_all_blanks() {
        let g = Grid::new(4, 2);
        assert_eq!(g.cells().len(), 8);
        assert!(g.cells().iter().all(|c| c.ch == ' '));
        assert_eq!(g.cursor(), (0, 0));
    }

    #[test]
    fn snapshot_text_lines_trims_trailing_spaces() {
        let mut g = Grid::new(10, 2);
        for ch in "hi".chars() {
            g.print(ch);
        }
        g.line_feed();
        g.carriage_return();
        for ch in "metro".chars() {
            g.print(ch);
        }
        let lines = g.snapshot().text_lines();
        // Row 0 had "hi" followed by 8 spaces -> trims to "hi".
        // Row 1 had "metro" followed by 5 spaces -> trims to "metro".
        assert_eq!(lines, vec!["hi".to_string(), "metro".to_string()]);
    }

    #[test]
    fn snapshot_text_lines_preserves_blank_rows() {
        let mut g = Grid::new(4, 3);
        for ch in "top".chars() {
            g.print(ch);
        }
        // Skip row 1 entirely; print on row 2.
        g.line_feed();
        g.carriage_return();
        g.line_feed();
        g.carriage_return();
        for ch in "bot".chars() {
            g.print(ch);
        }
        let lines = g.snapshot().text_lines();
        // Row 1 stays empty in the output so caller-side row numbers
        // line up with the grid's row indices.
        assert_eq!(
            lines,
            vec!["top".to_string(), "".to_string(), "bot".to_string()]
        );
    }

    #[test]
    fn snapshot_text_lines_handles_zero_dims() {
        // Grid::new asserts cols > 0 && rows > 0, so a real Grid
        // can never produce a 0-dim snapshot. But the public
        // GridSnapshot struct has open fields, and `text_lines`
        // guards against it -- assert that explicitly so the
        // defensive branch doesn't bit-rot.
        let snap = GridSnapshot {
            cols: 0,
            rows: 0,
            cells: Vec::new(),
            cursor: (0, 0),
            scrollback_len: 0,
        };
        assert!(snap.text_lines().is_empty());
    }

    #[test]
    fn print_advances_cursor() {
        let mut g = Grid::new(4, 2);
        g.print('a');
        g.print('b');
        assert_eq!(g.cursor(), (0, 2));
        assert_eq!(g.cells()[0].ch, 'a');
        assert_eq!(g.cells()[1].ch, 'b');
    }

    #[test]
    fn print_wraps_at_right_edge() {
        let mut g = Grid::new(3, 2);
        g.print('a');
        g.print('b');
        g.print('c');
        // Next print should wrap then place at (1, 0).
        g.print('d');
        assert_eq!(g.cells()[3].ch, 'd');
        assert_eq!(g.cursor(), (1, 1));
    }

    #[test]
    fn carriage_return_resets_col() {
        let mut g = Grid::new(4, 2);
        g.print('a');
        g.print('b');
        g.carriage_return();
        assert_eq!(g.cursor(), (0, 0));
    }

    #[test]
    fn line_feed_advances_row() {
        let mut g = Grid::new(4, 3);
        g.line_feed();
        assert_eq!(g.cursor(), (1, 0));
    }

    #[test]
    fn line_feed_scrolls_when_at_bottom() {
        let mut g = Grid::new(2, 2);
        g.print('a');
        g.line_feed(); // cursor -> (1,1)
        g.print('b');
        g.line_feed(); // overflow -> scrolls 'a ' into scrollback
        assert_eq!(g.scrollback().len(), 1);
        assert_eq!(g.scrollback()[0][0].ch, 'a');
    }

    #[test]
    fn backspace_clamps_at_zero() {
        let mut g = Grid::new(4, 2);
        g.backspace();
        assert_eq!(g.cursor(), (0, 0));
        g.print('a');
        g.backspace();
        assert_eq!(g.cursor(), (0, 0));
    }

    #[test]
    fn tab_aligns_to_next_eight() {
        let mut g = Grid::new(40, 2);
        g.tab();
        assert_eq!(g.cursor().1, 8);
        g.print('a');
        g.tab();
        assert_eq!(g.cursor().1, 16);
    }

    #[test]
    fn erase_screen_clears_cells_keeps_scrollback() {
        let mut g = Grid::new(2, 2);
        g.print('x');
        g.scrollback.push(vec![Cell::blank(); 2]);
        g.erase_screen();
        assert!(g.cells().iter().all(|c| c.ch == ' '));
        assert_eq!(g.scrollback().len(), 1);
    }

    #[test]
    fn resize_preserves_cells_in_corner() {
        let mut g = Grid::new(4, 2);
        g.print('a');
        // line_feed only moves the row; col stays. CR first so 'b'
        // lands at the start of row 1.
        g.carriage_return();
        g.line_feed();
        g.print('b');
        g.resize(6, 4);
        assert_eq!(g.cols(), 6);
        assert_eq!(g.rows(), 4);
        // 'a' at (0,0) should still be there.
        assert_eq!(g.cells()[0].ch, 'a');
        // 'b' at (1,0) should still be there in new index (1 * 6).
        assert_eq!(g.cells()[6].ch, 'b');
    }

    #[test]
    fn snapshot_is_independent_clone() {
        let mut g = Grid::new(2, 1);
        g.print('a');
        let snap = g.snapshot();
        assert_eq!(snap.cols, 2);
        assert_eq!(snap.cells[0].ch, 'a');
        // Mutate the grid — snapshot stays put.
        g.print('b');
        assert_eq!(snap.cells[1].ch, ' ');
    }
}
