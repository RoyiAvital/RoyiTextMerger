//! Hunks through edits of one side (the provisional mapping): the panes stay
//! aligned in the frame of the edit, and the worker re-diffs only the edited
//! regions.
//!
//! The lines an edit touched and the hunks touching them become one
//! provisional hunk: it may hold unchanged lines, but every line outside the
//! hunks is still unchanged, so a re-diff of its region makes the diff exact
//! again. Later hunks shift by the edit's line delta.

use std::ops::Range;

use rtm_text::{ChangeSet, Snapshot};

use crate::{Hunk, Side};

/// Unchanged lines a re-diff region takes around a hunk, so the slider can
/// move the hunk's edges as in a full diff (its reach).
pub const REDIFF_CONTEXT: u64 = 100;

/// An edit of one side in lines: the old lines `lines` (at least one) became
/// `lines.end - lines.start + added` lines, and the text after them moved by
/// `bytes`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineEdit {
    pub side: Side,
    pub lines: Range<u64>,
    pub added: i64,
    pub bytes: i64,
}

/// The line edits of `changes`, which turned `before` into `after`, on
/// `side`: sorted, one per group of changes sharing or touching lines.
pub fn line_edits(
    side: Side,
    before: &Snapshot,
    after: &Snapshot,
    changes: &ChangeSet,
) -> Vec<LineEdit> {
    let mut edits: Vec<LineEdit> = Vec::new();
    let mut delta = 0i64;
    for c in changes.changes() {
        let start = (c.old.start as i64 + delta) as u64;
        let bytes = c.new_len as i64 - (c.old.end - c.old.start) as i64;
        delta += bytes;
        let old = before.line_of(c.old.start)..before.line_of(c.old.end) + 1;
        let new_lines = after.line_of(start + c.new_len) + 1 - after.line_of(start);
        let added = new_lines as i64 - (old.end - old.start) as i64;
        match edits.last_mut() {
            Some(last) if old.start <= last.lines.end => {
                last.lines.end = last.lines.end.max(old.end);
                last.added += added;
                last.bytes += bytes;
            }
            _ => edits.push(LineEdit {
                side,
                lines: old,
                added,
                bytes,
            }),
        }
    }
    edits
}

fn on(side: Side, h: &Hunk) -> &Range<u64> {
    match side {
        Side::Left => &h.left,
        Side::Right => &h.right,
    }
}

fn on_mut(side: Side, h: &mut Hunk) -> &mut Range<u64> {
    match side {
        Side::Left => &mut h.left,
        Side::Right => &mut h.right,
    }
}

fn other(side: Side) -> Side {
    match side {
        Side::Left => Side::Right,
        Side::Right => Side::Left,
    }
}

/// Maps `hunks` through `edit`: the edited lines and the hunks touching them
/// become one provisional hunk, later hunks shift. Returns where that hunk
/// is and how many hunks it replaced (to splice data kept per hunk).
///
/// Several edits of one change set apply last first: each one's lines are
/// then still positions of the text before the change set.
pub fn map_edit(hunks: &mut Vec<Hunk>, edit: &LineEdit) -> (usize, usize) {
    let (a, b) = (edit.side, other(edit.side));
    let (e0, e1) = (edit.lines.start, edit.lines.end);
    let j = hunks.partition_point(|h| on(a, h).end < e0);
    let k = j + hunks[j..].partition_point(|h| on(a, h).start <= e1);
    // The other side's line of `line` in the unchanged run after `before`.
    let paired = |line: u64, before: Option<&Hunk>| {
        let offset = before.map_or(0, |h| on(b, h).end as i64 - on(a, h).end as i64);
        (line as i64 + offset) as u64
    };
    let (start, start_b) = match hunks[j..k].first() {
        Some(h) if on(a, h).start <= e0 => (on(a, h).start, on(b, h).start),
        _ => (e0, paired(e0, j.checked_sub(1).map(|i| &hunks[i]))),
    };
    let (end, end_b) = match hunks[j..k].last() {
        Some(h) if on(a, h).end >= e1 => (on(a, h).end, on(b, h).end),
        _ => (e1, paired(e1, k.checked_sub(1).map(|i| &hunks[i]))),
    };
    let mut hunk = Hunk {
        left: 0..0,
        right: 0..0,
    };
    *on_mut(a, &mut hunk) = start..(end as i64 + edit.added) as u64;
    *on_mut(b, &mut hunk) = start_b..end_b;
    hunks.splice(j..k, [hunk]);
    if edit.added != 0 {
        for h in &mut hunks[j + 1..] {
            let r = on_mut(a, h);
            *r = (r.start as i64 + edit.added) as u64..(r.end as i64 + edit.added) as u64;
        }
    }
    (j, k - j)
}

/// The region a re-diff of hunk `i` covers: the hunk and up to
/// [`REDIFF_CONTEXT`] unchanged lines around it, keeping an unchanged line
/// to its neighbors, and at most half the way to a neighbor `rediffed` too
/// (regions never overlap). `lines`: the left and right line counts.
pub fn rediff_region(
    hunks: &[Hunk],
    i: usize,
    rediffed: impl Fn(usize) -> bool,
    lines: [u64; 2],
) -> Hunk {
    let h = &hunks[i];
    let reach = |gap: u64, neighbor: Option<usize>| -> u64 {
        let room = match neighbor {
            None => gap,
            Some(n) if rediffed(n) => gap.saturating_sub(1) / 2,
            Some(_) => gap.saturating_sub(1),
        };
        room.min(REDIFF_CONTEXT)
    };
    let before = match i.checked_sub(1) {
        Some(p) => reach(h.left.start - hunks[p].left.end, Some(p)),
        None => reach(h.left.start.min(h.right.start), None),
    };
    let after = match hunks.get(i + 1) {
        Some(n) => reach(n.left.start - h.left.end, Some(i + 1)),
        None => reach((lines[0] - h.left.end).min(lines[1] - h.right.end), None),
    };
    Hunk {
        left: h.left.start - before..h.left.end + after,
        right: h.right.start - before..h.right.end + after,
    }
}
