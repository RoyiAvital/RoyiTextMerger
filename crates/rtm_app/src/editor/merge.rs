//! Merging: the current change, Copy to Right / Left (also going to the next
//! change), Next / Previous Change and the divider's copy arrows.
//!
//! A copy needs a fresh diff ([`Editor::diff_fresh`]): pressed while the
//! diff catches up, it waits for the reply; an edit or a new generation
//! cancels it. It replaces the target pane's lines of the picked changes
//! with the source pane's lines in one transaction: one undo step there.

use std::ops::Range;

use rtm_diff::Hunk;
use rtm_edit::EditKind;
use rtm_text::{Edit, Transaction};

use super::compare::WaitingCopy;
use super::*;

/// The lines of `h` on `side`.
fn lines_on(h: &Hunk, side: Side) -> Range<u64> {
    match side {
        Side::Left => h.left.clone(),
        Side::Right => h.right.clone(),
    }
}

/// `bytes` with every line break (`\n`, `\r\n`) as `ending`.
pub(super) fn with_line_ending(bytes: &[u8], ending: LineEnding) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\r' if bytes.get(i + 1) == Some(&b'\n') => {
                out.extend_from_slice(ending.as_bytes());
                i += 2;
            }
            b'\n' => {
                out.extend_from_slice(ending.as_bytes());
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    out
}

/// The bytes of `lines` of `s` a copy reads or replaces: each line with its
/// break; at the end of the text the break before each line instead (the
/// text before keeps its own last break), or the whole text from line 0.
fn copied_bytes(s: &Snapshot, lines: &Range<u64>) -> Range<u64> {
    let start = |line| s.line_start(line).unwrap_or(s.len());
    if lines.end < s.line_count() {
        start(lines.start)..start(lines.end)
    } else if lines.start == 0 {
        0..s.len()
    } else {
        edit::content_range(s, lines.start - 1).end..s.len()
    }
}

/// The first of `0..n` where `pred` holds, `pred` being false then true.
fn first_where(n: usize, pred: impl Fn(usize) -> bool) -> usize {
    let (mut lo, mut hi) = (0, n);
    while lo < hi {
        let mid = (lo + hi) / 2;
        if pred(mid) {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    lo
}

impl Editor {
    fn hunks(&self) -> &[Hunk] {
        self.compare.alignment().hunks()
    }

    /// Where the focused caret goes for change `i`: its first line, or its
    /// gap position when the focused side has no lines there (the start of
    /// the line after the gap, the end of the text past the last line).
    pub(super) fn change_position(&self, i: usize) -> u64 {
        let s = &self.pane.doc.snapshot;
        let line = lines_on(&self.hunks()[i], self.pane.side).start;
        s.line_start(line).unwrap_or(s.len())
    }

    /// The change at the focused caret: the one holding its line, or the
    /// current change when the caret is at its position.
    pub(super) fn caret_change(&self) -> Option<usize> {
        let (s, side) = (&self.pane.doc.snapshot, self.pane.side);
        let head = self.pane.doc.selections.primary().head;
        let line = s.line_of(head);
        let hunks = self.hunks();
        let i = hunks.partition_point(|h| lines_on(h, side).end <= line);
        if hunks
            .get(i)
            .is_some_and(|h| lines_on(h, side).contains(&line))
        {
            return Some(i);
        }
        self.compare
            .current
            .filter(|&c| c < hunks.len() && self.change_position(c) == head)
    }

    /// The changes a copy acts on: those the focused pane's selections
    /// touch (a caret by its line; a selection by its lines and the gaps
    /// at and between them), otherwise the current change.
    fn picked_changes(&self) -> Vec<usize> {
        let (s, side) = (&self.pane.doc.snapshot, self.pane.side);
        let hunks = self.hunks();
        let mut picked: Vec<usize> = Vec::new();
        for sel in self.pane.doc.selections.as_slice() {
            let first = s.line_of(sel.start());
            let mut last = s.line_of(sel.end());
            if last > first && s.line_start(last) == Some(sel.end()) {
                last -= 1;
            }
            let from = hunks.partition_point(|h| lines_on(h, side).end < first);
            for (i, h) in hunks.iter().enumerate().skip(from) {
                let lines = lines_on(h, side);
                if lines.start > last + 1 {
                    break;
                }
                let touched = if lines.is_empty() {
                    !sel.is_empty() && (first..=last + 1).contains(&lines.start)
                } else {
                    lines.start <= last && lines.end > first
                };
                if touched {
                    picked.push(i);
                }
            }
        }
        picked.sort_unstable();
        picked.dedup();
        if picked.is_empty()
            && let Some(c) = self.compare.current.filter(|&c| c < hunks.len())
        {
            picked.push(c);
        }
        picked
    }

    /// Why nothing can be copied while the files are not compared.
    fn no_comparison(&self) -> String {
        let has_file = |side| self.pane_of(side).path.is_some();
        if !has_file(Side::Left) || !has_file(Side::Right) {
            "Nothing to copy: open a file in each pane".to_owned()
        } else if self.too_large() {
            let max = self.settings.diff.max_file_mib;
            format!("Nothing to copy: a file is larger than {max} MiB")
        } else {
            "Nothing to copy until both files are loaded".to_owned()
        }
    }

    /// `Diff: Copy Change to Right / Left`; `next`: then Next Change.
    pub(super) fn copy_change(&mut self, to: Side, next: bool) {
        self.copy_or_wait(to, next, false);
    }

    /// Copies the picked changes (`divider`: the current change) toward
    /// `to` when the diff is fresh; otherwise the copy waits for it.
    fn copy_or_wait(&mut self, to: Side, next: bool, divider: bool) {
        self.refresh_alignment();
        let Some((left, right)) = self.compared_texts() else {
            self.notice = Some(self.no_comparison());
            return;
        };
        if let Some(reason) = self.pane_of(to).doc.locks.edit_refusal() {
            self.notice = Some(format!(
                "Not copied into the {} pane: {reason}",
                side_word(to)
            ));
            return;
        }
        if self.diff_fresh() {
            self.copy_now(to, next, divider);
            return;
        }
        self.compare.waiting_copy = Some(WaitingCopy {
            to,
            next,
            divider,
            generation: self.compare.generation,
            left,
            right,
            side: self.pane.side,
            selections: self.pane.doc.selections.clone(),
        });
        self.notice = Some("The copy waits for the comparison\u{2026}".to_owned());
    }

    /// Runs a waiting copy once the diff is fresh.
    pub(super) fn run_waiting_copy(&mut self) {
        self.drop_stale_copy();
        if !self.diff_fresh() {
            return;
        }
        if let Some(copy) = self.compare.waiting_copy.take() {
            self.copy_now(copy.to, copy.next, copy.divider);
        }
    }

    /// A waiting copy runs only in the state it was pressed in: an edit, a
    /// new generation, another focus or selection cancels it.
    pub(super) fn drop_stale_copy(&mut self) {
        let Some(copy) = &self.compare.waiting_copy else {
            return;
        };
        let (l, r) = (self.pane_of(Side::Left), self.pane_of(Side::Right));
        let reason = if copy.generation != self.compare.generation
            || !copy.left.same_tree(&l.doc.snapshot)
            || !copy.right.same_tree(&r.doc.snapshot)
        {
            "the text changed"
        } else if copy.side != self.pane.side {
            "the focus moved"
        } else if copy.selections != self.pane.doc.selections {
            "the caret moved"
        } else {
            return;
        };
        self.cancel_copy(reason);
    }

    /// Cancels a waiting copy, saying why.
    fn cancel_copy(&mut self, reason: &str) {
        if self.compare.waiting_copy.take().is_some() {
            self.notice = Some(format!("Copy cancelled: {reason}"));
        }
    }

    /// Copies toward `to` on a fresh diff.
    fn copy_now(&mut self, to: Side, next: bool, divider: bool) {
        let count = self.hunks().len();
        let picked = match divider {
            true => self
                .compare
                .current
                .filter(|&c| c < count)
                .into_iter()
                .collect(),
            false => self.picked_changes(),
        };
        let Some(&last) = picked.last() else {
            self.notice = Some("No change at the caret: Alt+F5 goes to the next change".to_owned());
            return;
        };
        let from = to.other();
        let (source, target) = (
            self.pane_of(from).doc.snapshot.clone(),
            self.pane_of(to).doc.snapshot.clone(),
        );
        let ending = self.pane_of(to).doc.options.line_ending;
        // Ranges of separate changes can touch (an empty line between them
        // at the end of the text): they join, in order, into one edit.
        let mut edits: Vec<(Range<u64>, Vec<u8>)> = Vec::with_capacity(picked.len());
        for &i in &picked {
            let h = &self.hunks()[i];
            let read = copied_bytes(&source, &lines_on(h, from));
            let text = with_line_ending(&source.to_vec(read), ending);
            let range = copied_bytes(&target, &lines_on(h, to));
            match edits.last_mut() {
                Some((last, joined)) if last.end == range.start => {
                    last.end = range.end;
                    joined.extend_from_slice(&text);
                }
                _ => edits.push((range, text)),
            }
        }
        let target_info = self.pane_of(to).info;
        if target_info.is_some_and(|i| matches!(i.encoding, Encoding::Utf16Le | Encoding::Utf16Be))
            && edits
                .iter()
                .any(|(_, text)| std::str::from_utf8(text).is_err())
        {
            self.notice = Some(format!(
                "Not copied: the lines hold bytes that are not UTF-8, which the {} file (UTF-16) cannot store",
                side_word(to)
            ));
            return;
        }
        // The next change, placed before the edit so the edit maps the caret.
        let next_change = (last + 1) % count;
        let go = next && !picked.contains(&next_change);
        if go {
            let at = self.change_position(next_change);
            self.select_single(Selection::caret(at));
            self.compare.current = Some(next_change);
        }
        let edits = edits
            .into_iter()
            .map(|(range, text)| Edit::new(range, text))
            .collect();
        let cmd = Command {
            tx: Transaction::new(edits),
            kind: EditKind::Other,
            collapse: false,
        };
        let applied = self.with_side(to, |e| {
            e.pane.doc.history.seal();
            e.pane.doc.apply(&cmd, None)
        });
        if let Err(reason) = applied {
            self.notice = Some(format!(
                "Not copied into the {} pane: {reason}",
                side_word(to)
            ));
            return;
        }
        let n = picked.len();
        let rest = if next && !go { "; no other change" } else { "" };
        self.notice = Some(format!(
            "Copied {n} change{} to the {} pane{rest}",
            plural(n as u64),
            side_word(to)
        ));
        self.refresh_alignment();
        if go && let Some(c) = self.compare.current {
            self.reveal_change(c);
        }
    }

    /// `Diff: Next / Previous Change`: the change after (before) the
    /// focused caret's row, wrapping at the ends. It becomes current; the
    /// caret goes to its position and the view shows it.
    pub(super) fn go_to_change(&mut self, forward: bool) {
        self.cancel_copy("another change was reached");
        self.refresh_alignment();
        let count = self.hunks().len();
        if count == 0 {
            self.notice = Some(match self.comparison_text() {
                Some(text) if text.starts_with("Comparing") => text,
                _ => "No changes".to_owned(),
            });
            return;
        }
        let head = self.pane.doc.selections.primary().head;
        let row = self.row_of(self.pane.doc.snapshot.line_of(head));
        let alignment = self.compare.alignment();
        let target = match (self.caret_change(), forward) {
            (Some(c), true) => (c + 1) % count,
            (Some(c), false) => (c + count - 1) % count,
            (None, true) => first_where(count, |i| alignment.hunk_rows(i).start > row) % count,
            (None, false) => first_where(count, |i| alignment.hunk_rows(i).end > row)
                .checked_sub(1)
                .unwrap_or(count - 1),
        };
        let at = self.change_position(target);
        self.select_single(Selection::caret(at));
        self.compare.current = Some(target);
        self.reveal_change(target);
        self.reveal();
    }

    /// Scrolls change `i` into the upper third of the view unless it is
    /// visible.
    fn reveal_change(&mut self, i: usize) {
        let rows = self.compare.alignment().hunk_rows(i);
        let page = self.page();
        let top = self.scroll.top_row;
        if rows.start < top || rows.end > top + page {
            self.scroll.top_row = rows.start.saturating_sub(page / 3);
            self.clamp_scroll();
        }
    }

    /// The visible row at window `y`, if any.
    fn row_at(&self, y: i32) -> Option<u64> {
        let text = self.layout().text;
        let dy = y - text.y;
        if dy < 0 {
            return None;
        }
        let row = self.scroll.top_row + (dy / self.renderer.cell().height.max(1) as i32) as u64;
        (row < self.row_count()).then_some(row)
    }

    /// A click in the text: the change on its row becomes current; a row
    /// without a change clears it.
    pub(super) fn click_change(&mut self, y: i32) {
        self.cancel_copy("the caret moved");
        self.compare.current = self
            .row_at(y)
            .and_then(|row| self.compare.alignment().row(row).hunk);
    }

    /// A press in the divider: an arrow copies its change that way.
    /// Returns whether the press was in the divider.
    pub(super) fn divider_click(&mut self, x: i32, y: i32) -> bool {
        let layout = self.layout();
        let div = layout.divider;
        if !div.contains(x, y) {
            return false;
        }
        let Some(row) = self.row_at(y) else {
            return true;
        };
        let Some(arrows) = self
            .arrows()
            .into_iter()
            .find(|a| self.scroll.top_row + u64::from(a.row) == row)
        else {
            return true;
        };
        let cell = (x - div.x) / self.renderer.cell().width.max(1) as i32;
        let to = match cell {
            0 if arrows.right => Side::Right,
            2 if arrows.left => Side::Left,
            _ => return true,
        };
        self.compare.current = self.compare.alignment().row(row).hunk;
        self.copy_or_wait(to, false, true);
        true
    }

    /// Whether a copy may edit the left and the right pane.
    pub(super) fn editable_sides(&self) -> [bool; 2] {
        [Side::Left, Side::Right].map(|side| {
            let pane = self.pane_of(side);
            pane.path.is_some() && pane.doc.locks.edit_refusal().is_none()
        })
    }

    /// The divider's arrows: on the first visible row of each change, one
    /// per editable target pane.
    pub(super) fn arrows(&self) -> Vec<Arrows> {
        let [left, right] = self.editable_sides();
        if self.compare.alignment().hunks().is_empty() || (!right && !left) {
            return Vec::new();
        }
        let alignment = self.compare.alignment();
        let top = self.scroll.top_row;
        let end = top + u64::from(self.layout().rows);
        alignment
            .hunks_in(top..end)
            .map(|i| alignment.hunk_rows(i).start)
            .filter(|&start| start >= top)
            .map(|start| Arrows {
                row: (start - top) as u32,
                right,
                left,
            })
            .collect()
    }
}

/// `"left"` or `"right"`.
fn side_word(side: Side) -> &'static str {
    match side {
        Side::Left => "left",
        Side::Right => "right",
    }
}
