//! The shared row space of both panes: an unchanged run maps line to line,
//! a hunk takes `max(left, right)` rows with filler rows after the shorter
//! side. Row and line lookups take O(log hunks).

use std::ops::Range;

use crate::Hunk;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// What one side of a row shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    Line(u64),
    /// A filler row. The side's gap lies before line `before`: the start
    /// of the line after the gap, or the side's line count when the gap is
    /// at the end of the text.
    Filler {
        before: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowPair {
    pub left: Cell,
    pub right: Cell,
    /// The index of the hunk the row belongs to.
    pub hunk: Option<usize>,
}

impl RowPair {
    /// What `side` shows on the row.
    pub fn side(&self, side: Side) -> Cell {
        match side {
            Side::Left => self.left,
            Side::Right => self.right,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Alignment {
    hunks: Vec<Hunk>,
    /// The first row of each hunk.
    starts: Vec<u64>,
    lines: [u64; 2],
    rows: u64,
}

impl Alignment {
    /// The rows of a diff's `hunks` of texts with `left_lines` and
    /// `right_lines` lines. Hunks of texts edited since the diff are taken
    /// as they are: their rows may name lines past the texts' ends.
    pub fn new(hunks: Vec<Hunk>, left_lines: u64, right_lines: u64) -> Alignment {
        let mut starts = Vec::with_capacity(hunks.len());
        let (mut row, mut line) = (0, 0);
        for h in &hunks {
            row += h.left.start.saturating_sub(line);
            starts.push(row);
            row += size(h);
            line = h.left.end;
        }
        let mut alignment = Alignment {
            hunks,
            starts,
            lines: [0, 0],
            rows: 0,
        };
        alignment.set_lines(left_lines, right_lines);
        alignment
    }

    /// The same hunks over texts now `left_lines` and `right_lines` long
    /// (edited since the diff): only the rows after the last hunk change.
    pub fn set_lines(&mut self, left_lines: u64, right_lines: u64) {
        let (end, l, r) = match (self.hunks.last(), self.starts.last()) {
            (Some(h), Some(&start)) => (start + size(h), h.left.end, h.right.end),
            _ => (0, 0, 0),
        };
        self.lines = [left_lines, right_lines];
        self.rows = end
            + left_lines
                .saturating_sub(l)
                .max(right_lines.saturating_sub(r));
    }

    /// Before the first diff: row = line in both panes, filler rows after
    /// the shorter text.
    pub fn identity(left_lines: u64, right_lines: u64) -> Alignment {
        Alignment::new(Vec::new(), left_lines, right_lines)
    }

    pub fn rows(&self) -> u64 {
        self.rows
    }

    pub fn hunks(&self) -> &[Hunk] {
        &self.hunks
    }

    /// The left and right line counts.
    pub fn lines(&self) -> [u64; 2] {
        self.lines
    }

    /// The hunks, to map or splice them into a new alignment.
    pub fn into_hunks(self) -> Vec<Hunk> {
        self.hunks
    }

    /// The rows of hunk `i`.
    pub fn hunk_rows(&self, i: usize) -> Range<u64> {
        let start = self.starts[i];
        start..start + size(&self.hunks[i])
    }

    /// The hunks with a row in `rows`.
    pub fn hunks_in(&self, rows: Range<u64>) -> Range<usize> {
        let end = self.starts.partition_point(|&s| s < rows.end);
        let first = self.starts.partition_point(|&s| s < rows.start);
        let first = match first.checked_sub(1) {
            Some(k) if self.hunk_rows(k).end > rows.start => k,
            _ => first,
        };
        first..end.max(first)
    }

    /// What both sides show on `row` (below [`Alignment::rows`]).
    pub fn row(&self, row: u64) -> RowPair {
        let i = self.starts.partition_point(|&s| s <= row);
        let (base_row, base) = match i.checked_sub(1) {
            Some(k) => {
                let (h, start) = (&self.hunks[k], self.starts[k]);
                let n = row - start;
                if n < size(h) {
                    return RowPair {
                        left: in_hunk(&h.left, n),
                        right: in_hunk(&h.right, n),
                        hunk: Some(k),
                    };
                }
                (start + size(h), [h.left.end, h.right.end])
            }
            None => (0, [0, 0]),
        };
        let n = row - base_row;
        RowPair {
            left: in_run(base[0] + n, self.lines[0]),
            right: in_run(base[1] + n, self.lines[1]),
            hunk: None,
        }
    }

    /// The row of `line` on `side`.
    pub fn row_of(&self, side: Side, line: u64) -> u64 {
        let range = |h: &Hunk| match side {
            Side::Left => h.left.clone(),
            Side::Right => h.right.clone(),
        };
        let i = self.hunks.partition_point(|h| range(h).end <= line);
        if let Some(h) = self.hunks.get(i)
            && range(h).start <= line
        {
            return self.starts[i] + (line - range(h).start);
        }
        match i.checked_sub(1) {
            Some(k) => {
                let h = &self.hunks[k];
                self.starts[k] + size(h) + (line - range(h).end)
            }
            None => line,
        }
    }
}

/// Rows of a hunk.
fn size(h: &Hunk) -> u64 {
    (h.left.end - h.left.start).max(h.right.end - h.right.start)
}

/// Row `n` of a hunk on a side with `lines`.
fn in_hunk(lines: &Range<u64>, n: u64) -> Cell {
    if n < lines.end - lines.start {
        Cell::Line(lines.start + n)
    } else {
        Cell::Filler { before: lines.end }
    }
}

/// `line` of an unchanged run; filler past the side's `count` lines (only
/// the identity alignment of texts of different lengths has them).
fn in_run(line: u64, count: u64) -> Cell {
    if line < count {
        Cell::Line(line)
    } else {
        Cell::Filler { before: count }
    }
}
