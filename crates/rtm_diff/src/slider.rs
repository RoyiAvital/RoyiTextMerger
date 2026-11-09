//! Git's indent heuristic (`xdl_change_compact` with
//! `XDF_INDENT_HEURISTIC`): an insertion or deletion that could equally sit
//! a few lines higher or lower moves to where a reader expects it, next to
//! blank lines and at indentation boundaries. Lines are compared
//! bytewise, so a hash collision cannot move a hunk.

use rtm_text::Snapshot;

use crate::budget::Budget;
use crate::lines::{LineSource, byte, indent};
use crate::{Cancelled, Hunk};

/// Lines a hunk may move each way (Git's `INDENT_HEURISTIC_MAX_SLIDING`).
pub(crate) const MAX_SLIDING: u64 = 100;
const MAX_BLANKS: i32 = 20;
const START_OF_FILE_PENALTY: i32 = 1;
const END_OF_FILE_PENALTY: i32 = 21;
const TOTAL_BLANK_WEIGHT: i32 = -30;
const POST_BLANK_WEIGHT: i32 = 6;
const RELATIVE_INDENT_PENALTY: i32 = -4;
const RELATIVE_INDENT_WITH_BLANK_PENALTY: i32 = 10;
const RELATIVE_OUTDENT_PENALTY: i32 = 24;
const RELATIVE_OUTDENT_WITH_BLANK_PENALTY: i32 = 17;
const RELATIVE_DEDENT_PENALTY: i32 = 23;
const RELATIVE_DEDENT_WITH_BLANK_PENALTY: i32 = 17;
const INDENT_WEIGHT: i32 = 60;

/// Moves each insertion and deletion of `hunks` to its best position
/// inside `region`, keeping an unchanged line between hunks. Stops at the
/// deadline (the positions are cosmetic).
pub(crate) fn slide(
    left: &Snapshot,
    right: &Snapshot,
    region: &Hunk,
    hunks: &mut [Hunk],
    trim: bool,
    budget: &mut Budget,
) -> Result<(), Cancelled> {
    let mut sources = [LineSource::new(left), LineSource::new(right)];
    let records = [records(left), records(right)];
    for i in 0..hunks.len() {
        if !budget.tick(1)? {
            break;
        }
        // The side holding the lines.
        let side = match (hunks[i].left.is_empty(), hunks[i].right.is_empty()) {
            (true, false) => 1,
            (false, true) => 0,
            _ => continue,
        };
        let range = |h: &Hunk| {
            if side == 0 {
                h.left.clone()
            } else {
                h.right.clone()
            }
        };
        let group = range(&hunks[i]);
        let bounds = range(region);
        let lo = if i > 0 {
            range(&hunks[i - 1]).end + 1
        } else {
            bounds.start
        };
        let records = records[side];
        // The empty line after a final line break is not one of Git's
        // lines: a hunk does not slide into it.
        let hi = hunks
            .get(i + 1)
            .map_or(bounds.end, |h| range(h).start - 1)
            .min(records.max(group.end));
        let src = &mut sources[side];
        let mut up = 0;
        while up < MAX_SLIDING
            && group.start - up > lo
            && src.equal(group.start - up - 1, group.end - up - 1, trim)
        {
            up += 1;
        }
        let mut down = 0;
        while down < MAX_SLIDING
            && group.end + down < hi
            && src.equal(group.start + down, group.end + down, trim)
        {
            down += 1;
        }
        if up == 0 && down == 0 {
            continue;
        }
        let size = group.end - group.start;
        let latest = group.end + down;
        let first = (group.end - up)
            .max(latest.saturating_sub(size + 1))
            .max(latest.saturating_sub(MAX_SLIDING));
        let mut best: Option<(u64, Score)> = None;
        for end in first..=latest {
            let mut score = Score::default();
            score.add(&Measure::new(src, end, records));
            score.add(&Measure::new(src, end - size, records));
            if best.as_ref().is_none_or(|(_, b)| score.cmp(b) <= 0) {
                best = Some((end, score));
            }
        }
        let end = best.map_or(group.end, |(end, _)| end);
        let h = &mut hunks[i];
        if end >= group.end {
            let d = end - group.end;
            h.left = h.left.start + d..h.left.end + d;
            h.right = h.right.start + d..h.right.end + d;
        } else {
            let d = group.end - end;
            h.left = h.left.start - d..h.left.end - d;
            h.right = h.right.start - d..h.right.end - d;
        }
    }
    Ok(())
}

/// Git's line count of a text: a final line break ends a line, it does not
/// start an empty one.
fn records(s: &Snapshot) -> u64 {
    if s.is_empty() {
        0
    } else if byte(s, s.len() - 1) == Some(b'\n') {
        s.line_count() - 1
    } else {
        s.line_count()
    }
}

/// The surroundings of a split before line `split`.
struct Measure {
    /// The split is at the end of the text.
    end_of_file: bool,
    /// Indent of the line after the split; -1 when blank.
    indent: i32,
    /// Blank lines right above the split.
    pre_blank: i32,
    /// Indent of the nearest non-blank line above; -1 when none.
    pre_indent: i32,
    /// Blank lines after the line after the split.
    post_blank: i32,
    /// Indent of the nearest non-blank line below that; -1 when none.
    post_indent: i32,
}

impl Measure {
    fn new(src: &mut LineSource, split: u64, records: u64) -> Measure {
        let (end_of_file, line_indent) = if split >= records {
            (true, -1)
        } else {
            (false, indent(src.line(split)))
        };
        let (mut pre_blank, mut pre_indent) = (0, -1);
        let mut i = split.min(records);
        while i > 0 {
            i -= 1;
            pre_indent = indent(src.line(i));
            if pre_indent != -1 {
                break;
            }
            pre_blank += 1;
            if pre_blank == MAX_BLANKS {
                pre_indent = 0;
                break;
            }
        }
        let (mut post_blank, mut post_indent) = (0, -1);
        let mut i = split + 1;
        while i < records {
            post_indent = indent(src.line(i));
            if post_indent != -1 {
                break;
            }
            post_blank += 1;
            if post_blank == MAX_BLANKS {
                post_indent = 0;
                break;
            }
            i += 1;
        }
        Measure {
            end_of_file,
            indent: line_indent,
            pre_blank,
            pre_indent,
            post_blank,
            post_indent,
        }
    }
}

/// Git's split score: lower is better.
#[derive(Default)]
struct Score {
    effective_indent: i32,
    penalty: i32,
}

impl Score {
    fn add(&mut self, m: &Measure) {
        if m.pre_indent == -1 && m.pre_blank == 0 {
            self.penalty += START_OF_FILE_PENALTY;
        }
        if m.end_of_file {
            self.penalty += END_OF_FILE_PENALTY;
        }
        let post_blank = if m.indent == -1 { 1 + m.post_blank } else { 0 };
        let total_blank = m.pre_blank + post_blank;
        self.penalty += TOTAL_BLANK_WEIGHT * total_blank;
        self.penalty += POST_BLANK_WEIGHT * post_blank;
        let indent = if m.indent != -1 {
            m.indent
        } else {
            m.post_indent
        };
        let any_blanks = total_blank != 0;
        self.effective_indent += indent;
        if indent == -1 || m.pre_indent == -1 || indent == m.pre_indent {
            // No adjustment.
        } else if indent > m.pre_indent {
            self.penalty += if any_blanks {
                RELATIVE_INDENT_WITH_BLANK_PENALTY
            } else {
                RELATIVE_INDENT_PENALTY
            };
        } else if m.post_indent != -1 && m.post_indent > indent {
            self.penalty += if any_blanks {
                RELATIVE_OUTDENT_WITH_BLANK_PENALTY
            } else {
                RELATIVE_OUTDENT_PENALTY
            };
        } else {
            self.penalty += if any_blanks {
                RELATIVE_DEDENT_WITH_BLANK_PENALTY
            } else {
                RELATIVE_DEDENT_PENALTY
            };
        }
    }

    /// Negative when `self` is better than `other`.
    fn cmp(&self, other: &Score) -> i32 {
        let indents = (self.effective_indent > other.effective_indent) as i32
            - (self.effective_indent < other.effective_indent) as i32;
        INDENT_WEIGHT * indents + (self.penalty - other.penalty)
    }
}
