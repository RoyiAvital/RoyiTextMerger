//! Line diff, inner changes and alignment of two snapshots: pure functions
//! without threads or I/O (the app runs them on its diff thread).
//!
//! Lines are compared without their line breaks (`\n` and a `\r` before
//! it); with [`Options::ignore_trim_whitespace`] also without leading and
//! trailing whitespace. A diff:
//!
//! 1. takes the common prefix and suffix lines bytewise;
//! 2. hashes the other lines (XXH3) into dense ids, one map for both sides;
//! 3. runs Git's histogram diff on the ids, with Myers where no rare line
//!    anchors a region;
//! 4. slides ambiguous insertions and deletions to readable boundaries
//!    (Git's indent heuristic), comparing lines bytewise;
//! 5. compares every unchanged line not compared bytewise yet, so a hash
//!    collision becomes a change: "unchanged" is exact.
//!
//! Edits map the hunks of a shown diff ([`map_edit`]) until a re-diff of the
//! edited regions ([`diff_region`]) makes it exact again.

mod align;
mod budget;
mod ends;
mod histogram;
mod inner;
mod lines;
mod mapping;
mod myers;
mod slider;
mod verify;

#[cfg(test)]
#[allow(clippy::single_range_in_vec_init)]
mod tests;

use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use rtm_text::Snapshot;

pub use align::{Alignment, Cell, RowPair, Side};
pub use inner::{INNER_MAX_BYTES, Inner, inner};
pub use mapping::{LineEdit, REDIFF_CONTEXT, line_edits, map_edit, rediff_region};

use budget::Budget;
use histogram::Changes;

/// Left lines replaced by right lines. An empty range is a pure insertion
/// (left) or deletion (right) before that line.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Hunk {
    pub left: Range<u64>,
    pub right: Range<u64>,
}

/// The changes between two texts: sorted hunks, each separated from the
/// next by at least one unchanged line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diff {
    pub hunks: Vec<Hunk>,
    /// False when the deadline passed: some hunks are coarse, still exact
    /// about where the texts differ, only larger.
    pub complete: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    /// Leading and trailing whitespace is not a change
    /// (`diff.ignoreTrimWhitespace`).
    pub ignore_trim_whitespace: bool,
    /// Past it, the regions not resolved yet become whole hunks and
    /// [`Diff::complete`] is false (`diff.maxComputationMs`). Without one the
    /// diff runs to the end, near O(N·√N) at worst (repetitive texts with
    /// many changes).
    pub deadline: Option<Instant>,
}

/// Stops a running diff: it returns [`Cancelled`] soon after
/// [`CancelToken::cancel`]. Clones share the flag.
#[derive(Clone, Debug, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> CancelToken {
        CancelToken::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cancelled;

/// What the status line says about a comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Byte-equal texts of the same encoding.
    Identical,
    /// No changes, but differences the comparison ignores: whitespace, line
    /// breaks or the encoding.
    Equivalent,
    /// The number of hunks.
    Changes(usize),
}

impl Diff {
    /// The status of this diff of `left` and `right`; `same_encoding`: both
    /// files have the same encoding.
    pub fn status(&self, left: &Snapshot, right: &Snapshot, same_encoding: bool) -> Status {
        if !self.hunks.is_empty() {
            Status::Changes(self.hunks.len())
        } else if same_encoding && identical(left, right) {
            Status::Identical
        } else {
            Status::Equivalent
        }
    }
}

/// The texts are equal byte for byte.
pub fn identical(left: &Snapshot, right: &Snapshot) -> bool {
    left.same_tree(right)
        || (left.len() == right.len()
            && ends::common_prefix(left, 0..left.len(), right, 0..right.len()) == left.len())
}

/// The diff of two texts.
pub fn diff(
    left: &Snapshot,
    right: &Snapshot,
    options: &Options,
    cancel: &CancelToken,
) -> Result<Diff, Cancelled> {
    let region = Hunk {
        left: 0..left.line_count(),
        right: 0..right.line_count(),
    };
    diff_region(left, right, &region, options, cancel)
}

/// The diff of the lines of `region` only; the lines around it count as
/// unchanged. Hunks may touch the region's ends. Re-diffing the region of
/// edited hunks keeps a diff exact without a full diff.
pub fn diff_region(
    left: &Snapshot,
    right: &Snapshot,
    region: &Hunk,
    options: &Options,
    cancel: &CancelToken,
) -> Result<Diff, Cancelled> {
    compare(
        left,
        right,
        region,
        options,
        cancel,
        xxhash_rust::xxh3::xxh3_64,
    )
}

/// Line hash; tests force collisions with a weak one.
type HashFn = fn(&[u8]) -> u64;

fn compare(
    left: &Snapshot,
    right: &Snapshot,
    region: &Hunk,
    options: &Options,
    cancel: &CancelToken,
    hash: HashFn,
) -> Result<Diff, Cancelled> {
    let mut budget = Budget::new(Some(cancel), options.deadline, u64::MAX);
    budget.check_cancel(0)?;
    let trim = options.ignore_trim_whitespace;
    let (lr, rr) = (region.left.clone(), region.right.clone());
    if lr.is_empty() || rr.is_empty() {
        return Ok(Diff {
            hunks: one_hunk(region),
            complete: true,
        });
    }
    // 1. Common prefix and suffix lines, bytewise.
    let (a0, a1) = (lines::byte_at(left, lr.start), lines::byte_at(left, lr.end));
    let (b0, b1) = (
        lines::byte_at(right, rr.start),
        lines::byte_at(right, rr.end),
    );
    let p = ends::common_prefix(left, a0..a1, right, b0..b1);
    let at_end = (lr.end == left.line_count(), rr.end == right.line_count());
    if p == a1 - a0 && p == b1 - b0 && at_end.0 == at_end.1 {
        return Ok(Diff {
            hunks: Vec::new(),
            complete: true,
        });
    }
    // Every `\n` of the common prefix ends an equal line.
    let prefix = left.line_of(a0 + p) - lr.start;
    let limit = (a1 - a0).min(b1 - b0) - p;
    let s = ends::common_suffix(left, a0..a1, right, b0..b1, limit);
    let suffix = lines::lines_from(left, a1 - s, lr.end)
        .min(lines::lines_from(right, b1 - s, rr.end))
        .min(lr.end - lr.start - prefix)
        .min(rr.end - rr.start - prefix);
    let middle = Hunk {
        left: lr.start + prefix..lr.end - suffix,
        right: rr.start + prefix..rr.end - suffix,
    };
    // 2, 3. Ids and the histogram diff of the middle.
    let mut complete = true;
    let mut hunks = if middle.left.is_empty() || middle.right.is_empty() {
        one_hunk(&middle)
    } else {
        let ids = histogram::intern(
            left,
            &middle.left,
            right,
            &middle.right,
            trim,
            hash,
            &mut budget,
        )?;
        match ids {
            None => {
                complete = false;
                one_hunk(&middle)
            }
            Some(ids) => {
                let mut changes = Changes::new(ids.left.len(), ids.right.len());
                complete = histogram::diff(&ids, &mut changes, &mut budget)?;
                changes.hunks(middle.left.start, middle.right.start)
            }
        }
    };
    // 4. Readable boundaries.
    slider::slide(left, right, region, &mut hunks, trim, &mut budget)?;
    // 5. Exact unchanged lines.
    let hunks = verify::verify(left, right, region, &middle, hunks, trim, &mut budget)?;
    Ok(Diff { hunks, complete })
}

/// The region as one hunk, or none when it is empty on both sides.
fn one_hunk(region: &Hunk) -> Vec<Hunk> {
    if region.left.is_empty() && region.right.is_empty() {
        Vec::new()
    } else {
        vec![region.clone()]
    }
}
