//! Verification: unchanged lines the diff paired by id are compared
//! bytewise (by the comparison's rules); a hash collision becomes a change.

use rtm_text::Snapshot;

use crate::budget::Budget;
use crate::lines::{LineCursor, key};
use crate::slider::MAX_SLIDING;
use crate::{Cancelled, Hunk};

/// Compares the unchanged lines of `hunks` that may have been paired by id:
/// those of `middle` and those the slider may have moved next to it. The
/// common prefix and suffix further out were compared bytewise.
pub(crate) fn verify(
    left: &Snapshot,
    right: &Snapshot,
    region: &Hunk,
    middle: &Hunk,
    hunks: Vec<Hunk>,
    trim: bool,
    budget: &mut Budget,
) -> Result<Vec<Hunk>, Cancelled> {
    let lo = middle
        .left
        .start
        .saturating_sub(MAX_SLIDING)
        .max(region.left.start);
    let hi = (middle.left.end + MAX_SLIDING).min(region.left.end);
    // The unchanged runs: left start, right start and length.
    let mut runs = Vec::with_capacity(hunks.len() + 1);
    let (mut l, mut r) = (region.left.start, region.right.start);
    for h in &hunks {
        runs.push((l, r, h.left.start - l));
        (l, r) = (h.left.end, h.right.end);
    }
    runs.push((l, r, region.left.end - l));
    let mut differ = Vec::new();
    for (l, r, n) in runs {
        let (from, to) = (l.max(lo), (l + n).min(hi));
        if from >= to {
            continue;
        }
        let r_from = r + (from - l);
        let mut a = LineCursor::new(left, from);
        let mut b = LineCursor::new(right, r_from);
        for k in 0..to - from {
            budget.check_cancel(1)?;
            if key(a.next_line(), trim) != key(b.next_line(), trim) {
                differ.push(Hunk {
                    left: from + k..from + k + 1,
                    right: r_from + k..r_from + k + 1,
                });
            }
        }
    }
    Ok(merge(hunks, differ))
}

/// The hunks plus the line pairs that differ, touching ones joined.
fn merge(mut hunks: Vec<Hunk>, differ: Vec<Hunk>) -> Vec<Hunk> {
    if differ.is_empty() {
        return hunks;
    }
    hunks.extend(differ);
    hunks.sort_by_key(|h| (h.left.start, h.right.start));
    let mut out: Vec<Hunk> = Vec::with_capacity(hunks.len());
    for h in hunks {
        match out.last_mut() {
            Some(last) if last.left.end == h.left.start && last.right.end == h.right.start => {
                last.left.end = h.left.end;
                last.right.end = h.right.end;
            }
            _ => out.push(h),
        }
    }
    out
}
