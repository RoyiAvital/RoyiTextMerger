//! The comparison of the two panes: requests to the diff thread, the diff
//! shown, the shared row space, and what the panes draw of it (rows,
//! decorations, overview marks, the status). Until a diff arrives, and above
//! the size limit, the rows are the identity: row = line in both panes.
//!
//! An edit maps the shown diff in the same frame ([`rtm_diff::map_edit`]):
//! the edited lines and the hunks touching them become a provisional hunk,
//! later hunks shift. The next line request re-diffs the regions of all
//! provisional hunks (a full diff when they cover much of the texts); its
//! reply splices into the hunks of the revision it was asked for. A fresh
//! diff (current texts, no provisional hunk) is the merge authority.

use std::ops::Range;
use std::time::Duration;

use rtm_diff::{Alignment, CancelToken, Cell as RowCell, Diff, Hunk, Inner};
use rtm_text::ChangeSet;
use rtm_view::{Mark, Row, RowKind};

use super::*;
use crate::differ::{DiffThread, InnerChanges, InnerRequest, LineRequest, Lines, Reply, Work};

/// The `diff:` settings (VS Code's defaults).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiffSettings {
    /// Leading and trailing whitespace is not a change.
    pub ignore_trim_whitespace: bool,
    /// Larger files are shown but not compared.
    pub max_file_mib: u64,
    /// After this the diff is simplified (coarse hunks).
    pub max_computation_ms: u64,
}

impl Default for DiffSettings {
    fn default() -> DiffSettings {
        DiffSettings {
            ignore_trim_whitespace: true,
            max_file_mib: 50,
            max_computation_ms: 5000,
        }
    }
}

/// An accepted diff, mapped through the edits since; its hunks are the
/// alignment's. The vectors hold one entry per hunk.
struct Shown {
    generation: u64,
    /// The texts the hunks describe: the panes' texts, once mapped.
    left: Snapshot,
    right: Snapshot,
    /// No region was simplified at the deadline.
    complete: bool,
    /// No hunk is left and the texts are byte-equal.
    identical: bool,
    /// Inner changes: `None` until computed; `Some(None)` keeps whole lines.
    inner: Vec<Option<Option<Inner>>>,
    /// Inner changes asked for.
    asked: Vec<bool>,
    /// Mapped through an edit: may hold unchanged lines until re-diffed.
    provisional: Vec<bool>,
    /// How many hunks are provisional.
    provisionals: usize,
}

impl Shown {
    fn new(generation: u64, left: Snapshot, right: Snapshot, diff: &Diff) -> Shown {
        let n = diff.hunks.len();
        Shown {
            generation,
            left,
            right,
            complete: diff.complete,
            identical: false,
            inner: vec![None; n],
            asked: vec![false; n],
            provisional: vec![false; n],
            provisionals: 0,
        }
    }

    /// Replaces the data of hunks `at..at + replaced` with that of `count`
    /// new hunks, their inner changes given by index from `at`.
    fn splice(
        &mut self,
        at: usize,
        replaced: usize,
        count: usize,
        provisional: bool,
        inner: InnerChanges,
    ) {
        let range = at..at + replaced;
        let mut slots = vec![None; count];
        let mut asked = vec![false; count];
        for (i, value) in inner {
            slots[i] = Some(value);
            asked[i] = true;
        }
        self.inner.splice(range.clone(), slots);
        self.asked.splice(range.clone(), asked);
        let removed = self.provisional[range.clone()]
            .iter()
            .filter(|&&p| p)
            .count();
        self.provisionals = self.provisionals - removed + if provisional { count } else { 0 };
        self.provisional
            .splice(range, std::iter::repeat_n(provisional, count));
    }

    /// Inner work in flight is of an older revision and is dropped: the
    /// hunks without a result are asked again.
    fn ask_again(&mut self) {
        for (asked, inner) in self.asked.iter_mut().zip(&self.inner) {
            *asked = inner.is_some();
        }
    }

    fn texts_mut(&mut self, side: Side) -> &mut Snapshot {
        match side {
            Side::Left => &mut self.left,
            Side::Right => &mut self.right,
        }
    }
}

/// The line request in flight.
struct Pending {
    generation: u64,
    revision: u64,
    left: Snapshot,
    right: Snapshot,
    cancel: CancelToken,
}

/// A copy pressed while the diff was not fresh: it runs on the reply with
/// the state it was pressed in. An edit (other texts), a new generation,
/// another focus or selection, a move to another change cancels it.
pub(super) struct WaitingCopy {
    pub to: Side,
    pub next: bool,
    /// The divider's arrow of the current change; otherwise the changes of
    /// the focused pane's selections.
    pub divider: bool,
    pub generation: u64,
    pub left: Snapshot,
    pub right: Snapshot,
    /// The focused pane and its selections.
    pub side: Side,
    pub selections: Selections,
}

/// The session's comparison.
pub(super) struct Compare {
    thread: DiffThread,
    /// Grows with every change that is not an edit (a file opened or closed,
    /// a swap): older replies are dropped.
    pub(super) generation: u64,
    /// Grows whenever the hunks change (an edit mapped, a reply applied):
    /// region results and inner changes splice only into their revision.
    revision: u64,
    pending: Option<Pending>,
    shown: Option<Shown>,
    /// Rows of both panes; holds the shown diff's hunks.
    alignment: Alignment,
    /// Grows when what the panes draw of the comparison changes.
    version: u64,
    /// Overview marks for (version, rows, track height).
    marks: (Option<(u64, u64, u32)>, Vec<Mark>),
    /// The current change (WinMerge's current difference) when reached by
    /// Next / Previous Change or a click: a hunk index, kept through edits
    /// and re-diffs.
    pub(super) current: Option<usize>,
    pub(super) waiting_copy: Option<WaitingCopy>,
    /// The first exact diff was traced (`compared`).
    traced: bool,
}

impl Compare {
    pub(super) fn new(wake: Wake) -> Compare {
        Compare {
            thread: DiffThread::spawn(wake),
            generation: 0,
            revision: 0,
            pending: None,
            shown: None,
            alignment: Alignment::identity(1, 1),
            version: 0,
            marks: (None, Vec::new()),
            current: None,
            waiting_copy: None,
            traced: false,
        }
    }

    pub(super) fn alignment(&self) -> &Alignment {
        &self.alignment
    }

    pub(super) fn version(&self) -> u64 {
        self.version
    }

    /// The overview ruler's marks of the last [`Editor::refresh_marks`].
    pub(super) fn marks(&self) -> &[Mark] {
        &self.marks.1
    }

    fn cancel_pending(&mut self) {
        if let Some(pending) = self.pending.take() {
            pending.cancel.cancel();
        }
    }

    /// The changed bytes of hunk `i` on `side`, once computed.
    fn inner_of(&self, i: usize, side: rtm_diff::Side) -> &[Range<u64>] {
        let inner = self
            .shown
            .as_ref()
            .and_then(|s| s.inner.get(i))
            .and_then(|slot| slot.as_ref())
            .and_then(|inner| inner.as_ref());
        match (inner, side) {
            (Some(inner), rtm_diff::Side::Left) => &inner.left,
            (Some(inner), rtm_diff::Side::Right) => &inner.right,
            (None, _) => &[],
        }
    }

    /// The hunks changed: new rows, a new revision and a repaint.
    fn changed(&mut self) {
        self.revision += 1;
        self.version += 1;
        if let Some(s) = &mut self.shown {
            s.ask_again();
        }
    }
}

pub(super) fn diff_side(side: Side) -> rtm_diff::Side {
    match side {
        Side::Left => rtm_diff::Side::Left,
        Side::Right => rtm_diff::Side::Right,
    }
}

/// Line ranges `a` and `b` share a line, or an empty one lies at or inside
/// the other.
fn meets(a: &Range<u64>, b: &Range<u64>) -> bool {
    if a.is_empty() || b.is_empty() {
        a.start <= b.end && b.start <= a.end
    } else {
        a.start < b.end && b.start < a.end
    }
}

/// Keeps the current change's index when hunks `at..at + replaced` become
/// `count` hunks: one inside them becomes the first new one, or none.
fn follow(current: &mut Option<usize>, at: usize, replaced: usize, count: usize) {
    if let Some(c) = *current {
        *current = if c < at {
            Some(c)
        } else if c < at + replaced {
            (count > 0).then_some(at)
        } else {
            Some(c + count - replaced)
        };
    }
}

/// The rows `side` draws from `top` on: its lines and fillers, with their
/// change and its inner changes.
pub(super) fn rows_of(compare: &Compare, side: Side, top: u64, count: u32) -> Vec<Row<'_>> {
    let alignment = &compare.alignment;
    let end = (top + u64::from(count)).min(alignment.rows());
    let ds = diff_side(side);
    let changed = match side {
        Side::Left => RowKind::Removed,
        Side::Right => RowKind::Inserted,
    };
    (top..end)
        .map(|r| {
            let pair = alignment.row(r);
            match (pair.side(ds), pair.hunk) {
                (RowCell::Filler { .. }, _) => Row {
                    line: None,
                    kind: RowKind::Filler,
                    inner: &[],
                },
                (RowCell::Line(line), None) => Row {
                    line: Some(line),
                    kind: RowKind::Same,
                    inner: &[],
                },
                (RowCell::Line(line), Some(i)) => Row {
                    line: Some(line),
                    kind: changed,
                    inner: compare.inner_of(i, ds),
                },
            }
        })
        .collect()
}

impl Editor {
    /// The texts to compare: both panes hold a loaded file within the size
    /// limit.
    pub(super) fn compared_texts(&self) -> Option<(Snapshot, Snapshot)> {
        let (l, r) = (self.pane_of(Side::Left), self.pane_of(Side::Right));
        let loaded = |p: &Pane| {
            p.path.is_some() && p.pending.is_none() && matches!(p.status, LoadStatus::Ready)
        };
        if !loaded(l) || !loaded(r) || self.too_large() {
            return None;
        }
        Some((l.doc.snapshot.clone(), r.doc.snapshot.clone()))
    }

    /// A pane's text is above `diff.maxFileMiB`: shown, not compared.
    pub(super) fn too_large(&self) -> bool {
        let max = self.settings.diff.max_file_mib * 1024 * 1024;
        [Side::Left, Side::Right]
            .iter()
            .any(|&side| self.pane_of(side).doc.snapshot.len() > max)
    }

    /// The shown diff when it is the merge authority: of the current
    /// generation and texts, without provisional hunks.
    fn shown_current(&self) -> Option<&Shown> {
        let (l, r) = (self.pane_of(Side::Left), self.pane_of(Side::Right));
        self.compare.shown.as_ref().filter(|s| {
            s.generation == self.compare.generation
                && s.left.same_tree(&l.doc.snapshot)
                && s.right.same_tree(&r.doc.snapshot)
                && s.provisionals == 0
        })
    }

    /// The diff shown is exact for the current texts (copies may run).
    pub(super) fn diff_fresh(&self) -> bool {
        self.shown_current().is_some()
    }

    /// Two files are compared but the exact diff (or a copy waiting for it)
    /// is still to come: benchmarks and `--exit-after-load` wait for it.
    pub(super) fn comparing(&self) -> bool {
        self.compared_texts().is_some()
            && (!self.diff_fresh() || self.compare.waiting_copy.is_some())
    }

    /// A pane's document was replaced (opened, reloaded, closed): the shown
    /// diff is of other texts.
    pub(super) fn reset_diff(&mut self) {
        self.compare.generation += 1;
        self.compare.cancel_pending();
        self.compare.shown = None;
        self.compare.current = None;
        self.compare.alignment = Alignment::identity(1, 1);
        self.compare.changed();
        self.refresh_alignment();
    }

    /// `Diff: Toggle Ignore Trim Whitespace`, for the session (the
    /// configuration file is never written). The shown diff stays drawn,
    /// of an older generation, until the diff under the new option arrives.
    pub(super) fn toggle_trim_whitespace(&mut self) {
        let diff = &mut self.settings.diff;
        diff.ignore_trim_whitespace = !diff.ignore_trim_whitespace;
        self.compare.generation += 1;
        self.compare.cancel_pending();
        self.compare.changed();
        let state = if diff.ignore_trim_whitespace {
            "on"
        } else {
            "off"
        };
        self.notice = Some(format!("Ignore trim whitespace {state}"));
    }

    /// The sides were swapped: the mirrored diff is a diff of the swapped
    /// texts, so it stays; work in flight is of the old sides. A diff of
    /// an older generation (another option) is mirrored for display only
    /// and stays stale, so the diff under the current option replaces it.
    pub(super) fn mirror_diff(&mut self) {
        let current = self.compare.generation;
        self.compare.generation += 1;
        self.compare.cancel_pending();
        let generation = self.compare.generation;
        let lines = self.compare.alignment.lines();
        let mut hunks = std::mem::take(&mut self.compare.alignment).into_hunks();
        for h in &mut hunks {
            std::mem::swap(&mut h.left, &mut h.right);
        }
        self.compare.alignment = Alignment::new(hunks, lines[1], lines[0]);
        if let Some(s) = &mut self.compare.shown {
            if s.generation == current {
                s.generation = generation;
            }
            std::mem::swap(&mut s.left, &mut s.right);
            for inner in s.inner.iter_mut().flatten().flatten() {
                std::mem::swap(&mut inner.left, &mut inner.right);
            }
        }
        self.compare.changed();
        self.refresh_alignment();
    }

    /// Maps the shown diff through the panes' edits since it was mapped
    /// last, and keeps the rows to the texts' line counts. Called before
    /// anything reads the rows.
    pub(super) fn refresh_alignment(&mut self) {
        let texts = [Side::Left, Side::Right].map(|side| {
            let pane = self.pane_of_mut(side);
            let journal = pane.doc.diff_journal.as_mut().map(std::mem::take);
            (pane.doc.snapshot.clone(), journal.unwrap_or_default())
        });
        if let Some(s) = &self.compare.shown {
            let [(left, l_journal), (right, r_journal)] = texts;
            if s.left.same_tree(&left) && s.right.same_tree(&right) {
                return;
            }
            let mut hunks = std::mem::take(&mut self.compare.alignment).into_hunks();
            let edited = self.map_edits(Side::Left, &left, l_journal, &mut hunks)
                | self.map_edits(Side::Right, &right, r_journal, &mut hunks);
            self.compare.alignment = Alignment::new(hunks, left.line_count(), right.line_count());
            // New trees of the same bytes (undoing an edit that changed
            // nothing) keep the texts' equality.
            if edited && let Some(s) = &mut self.compare.shown {
                s.identical = false;
            }
            self.compare.changed();
            return;
        }
        let [(left, _), (right, _)] = texts;
        let lines = [left.line_count(), right.line_count()];
        if self.compare.alignment.lines() != lines {
            self.compare.alignment.set_lines(lines[0], lines[1]);
        }
    }

    /// Maps the shown `hunks` through the edits of `side` that turned its
    /// shown text into `current`: the journaled ones, then one change from
    /// `Snapshot::diff` for the rest (undo, redo, a full journal). Returns
    /// whether any change was mapped (the bytes may have changed).
    fn map_edits(
        &mut self,
        side: Side,
        current: &Snapshot,
        journal: Vec<Journaled>,
        hunks: &mut Vec<Hunk>,
    ) -> bool {
        let Some(s) = &mut self.compare.shown else {
            return false;
        };
        let shown = s.texts_mut(side);
        if shown.same_tree(current) {
            return false;
        }
        let mut steps: Vec<(Snapshot, Snapshot, ChangeSet)> = Vec::new();
        let mut at = shown.clone();
        for entry in journal {
            if entry.before.same_tree(&at) {
                at = entry.after.clone();
                steps.push((entry.before, entry.after, entry.changes));
            }
        }
        if !at.same_tree(current) {
            let changes = at.diff(current);
            steps.push((at, current.clone(), changes));
        }
        *shown = current.clone();
        let ds = diff_side(side);
        let mut edited = false;
        for (before, after, changes) in &steps {
            edited |= !changes.is_empty();
            for edit in rtm_diff::line_edits(ds, before, after, changes)
                .iter()
                .rev()
            {
                let (at, replaced) = rtm_diff::map_edit(hunks, edit);
                s.splice(at, replaced, 1, true, Vec::new());
                follow(&mut self.compare.current, at, replaced, 1);
                // Inner changes after the edit move with its text.
                for inner in s.inner[at + 1..].iter_mut().flatten().flatten() {
                    let ranges = match side {
                        Side::Left => &mut inner.left,
                        Side::Right => &mut inner.right,
                    };
                    for r in ranges {
                        *r = (r.start as i64 + edit.bytes) as u64
                            ..(r.end as i64 + edit.bytes) as u64;
                    }
                }
            }
        }
        edited
    }

    /// Asks for the diff of the current texts when it is not fresh and not
    /// on the way, and for the inner changes around the view. Called after
    /// every event.
    pub(super) fn sync_diff(&mut self) {
        self.refresh_alignment();
        self.drop_stale_copy();
        let Some((left, right)) = self.compared_texts() else {
            self.compare.cancel_pending();
            // Above the size limit the texts are shown unaligned.
            if self.too_large() && self.compare.shown.is_some() {
                self.reset_diff();
            }
            return;
        };
        if self.diff_fresh() {
            self.request_inner();
            return;
        }
        let (generation, revision) = (self.compare.generation, self.compare.revision);
        if self.compare.pending.as_ref().is_some_and(|p| {
            p.generation == generation
                && p.revision == revision
                && p.left.same_tree(&left)
                && p.right.same_tree(&right)
        }) {
            return;
        }
        self.compare.cancel_pending();
        let cancel = CancelToken::new();
        let diff = self.settings.diff;
        self.compare.thread.lines(LineRequest {
            generation,
            revision,
            left: left.clone(),
            right: right.clone(),
            ignore_trim_whitespace: diff.ignore_trim_whitespace,
            budget: Duration::from_millis(diff.max_computation_ms),
            cancel: cancel.clone(),
            view: self.rows_near_view(),
            work: self.line_work(),
        });
        self.compare.pending = Some(Pending {
            generation,
            revision,
            left,
            right,
            cancel,
        });
    }

    /// The regions of the provisional hunks, or a full diff without a shown
    /// diff of this generation (another option) or when the regions cover
    /// half of the longer text.
    fn line_work(&self) -> Work {
        let Some(s) = self
            .compare
            .shown
            .as_ref()
            .filter(|s| s.generation == self.compare.generation)
        else {
            return Work::Full;
        };
        let hunks = self.compare.alignment.hunks();
        let lines = self.compare.alignment.lines();
        let picked: Vec<usize> = (0..hunks.len()).filter(|&i| s.provisional[i]).collect();
        if picked.is_empty() {
            return Work::Full;
        }
        let regions: Vec<Hunk> = picked
            .iter()
            .map(|&i| rtm_diff::rediff_region(hunks, i, |n| s.provisional[n], lines))
            .collect();
        let span: u64 = regions
            .iter()
            .map(|r| (r.left.end - r.left.start).max(r.right.end - r.right.start))
            .sum();
        if span.saturating_mul(2) > lines[0].max(lines[1]) {
            return Work::Full;
        }
        Work::Regions {
            regions,
            rest: hunks.len() - picked.len(),
        }
    }

    /// The rows of the view and a page above and below it.
    fn rows_near_view(&self) -> Range<u64> {
        let page = self.page();
        let top = self.scroll.top_row;
        top.saturating_sub(page)..top + 2 * page
    }

    /// Asks for the inner changes of the hunks near the view not asked yet,
    /// nearest first.
    fn request_inner(&mut self) {
        let near = self.rows_near_view();
        let center = self.scroll.top_row + self.page() / 2;
        let alignment = &self.compare.alignment;
        let Some(s) = &mut self.compare.shown else {
            return;
        };
        let mut hunks: Vec<(u64, usize, Hunk)> = Vec::new();
        for i in alignment.hunks_in(near) {
            if s.asked[i] {
                continue;
            }
            s.asked[i] = true;
            let rows = alignment.hunk_rows(i);
            hunks.push((rows.start.abs_diff(center), i, alignment.hunks()[i].clone()));
        }
        if hunks.is_empty() {
            return;
        }
        hunks.sort_by_key(|(distance, ..)| *distance);
        let request = InnerRequest {
            generation: s.generation,
            revision: self.compare.revision,
            left: s.left.clone(),
            right: s.right.clone(),
            ignore_trim_whitespace: self.settings.diff.ignore_trim_whitespace,
            hunks: hunks.into_iter().map(|(_, i, h)| (i, h)).collect(),
        };
        self.compare.thread.inner(request);
    }

    /// Applies the diff thread's replies that are still current, then a
    /// copy waiting for them.
    pub(super) fn drain_diff(&mut self) {
        while let Some(reply) = self.compare.thread.try_recv() {
            self.apply_diff(reply);
        }
        self.run_waiting_copy();
    }

    /// Waits up to `timeout` for the diff of the command-line files, so the
    /// first frame shows them aligned (startup).
    pub fn await_diff(&mut self, timeout: Duration) {
        self.sync_diff();
        if self.compare.pending.is_none() {
            return;
        }
        if let Some(reply) = self.compare.thread.recv_timeout(timeout) {
            self.apply_diff(reply);
        }
    }

    /// Applies a reply of the diff thread when it is of the current files,
    /// texts and (for regions and inner changes) hunks; drops it otherwise.
    pub(super) fn apply_diff(&mut self, reply: Reply) {
        self.refresh_alignment();
        let current = |editor: &Editor, l: &Snapshot, r: &Snapshot| {
            editor.pane_of(Side::Left).doc.snapshot.same_tree(l)
                && editor.pane_of(Side::Right).doc.snapshot.same_tree(r)
        };
        match reply {
            Reply::Lines {
                generation,
                revision,
                left,
                right,
                lines,
                identical,
            } => {
                if generation != self.compare.generation || !current(self, &left, &right) {
                    return;
                }
                if self.compare.pending.as_ref().is_some_and(|p| {
                    p.generation == generation
                        && p.revision == revision
                        && p.left.same_tree(&left)
                        && p.right.same_tree(&right)
                }) {
                    self.compare.pending = None;
                }
                match lines {
                    Lines::Full(diff, inner) => {
                        self.show_full(generation, left, right, diff, inner);
                    }
                    Lines::Regions(results) => {
                        if revision != self.compare.revision || !self.splice_regions(results) {
                            return;
                        }
                    }
                }
                if let Some(s) = &mut self.compare.shown {
                    s.identical = identical;
                }
                self.compare.changed();
                if !self.compare.traced && self.diff_fresh() {
                    self.compare.traced = true;
                    self.trace.mark("compared");
                }
            }
            Reply::Inner {
                generation,
                revision,
                left,
                right,
                inner,
            } => {
                let Some(s) = &mut self.compare.shown else {
                    return;
                };
                if s.generation != generation
                    || revision != self.compare.revision
                    || !s.left.same_tree(&left)
                    || !s.right.same_tree(&right)
                {
                    return;
                }
                for (i, value) in inner {
                    if let Some(slot) = s.inner.get_mut(i) {
                        *slot = Some(value);
                    }
                }
                self.compare.version += 1;
            }
        }
    }

    /// Shows a full diff of the same texts; the current change stays by
    /// its lines.
    fn show_full(
        &mut self,
        generation: u64,
        left: Snapshot,
        right: Snapshot,
        diff: Diff,
        inner: InnerChanges,
    ) {
        let mut shown = Shown::new(generation, left.clone(), right.clone(), &diff);
        for (i, value) in inner {
            shown.inner[i] = Some(value);
            shown.asked[i] = true;
        }
        let old = self
            .compare
            .current
            .and_then(|c| self.compare.alignment.hunks().get(c).cloned());
        self.compare.current = old.and_then(|old| {
            diff.hunks
                .iter()
                .position(|h| meets(&h.left, &old.left) || meets(&h.right, &old.right))
        });
        self.compare.alignment = Alignment::new(diff.hunks, left.line_count(), right.line_count());
        self.compare.shown = Some(shown);
    }

    /// Replaces each provisional hunk with its region's hunks. False when
    /// the regions do not match the provisional hunks (dropped).
    fn splice_regions(&mut self, results: Vec<(Diff, InnerChanges)>) -> bool {
        let Some(s) = &mut self.compare.shown else {
            return false;
        };
        let picked: Vec<usize> = (0..s.provisional.len())
            .filter(|&i| s.provisional[i])
            .collect();
        if picked.len() != results.len() || picked.len() != s.provisionals {
            return false;
        }
        let lines = self.compare.alignment.lines();
        let mut hunks = std::mem::take(&mut self.compare.alignment).into_hunks();
        for (&i, (diff, inner)) in picked.iter().zip(results).rev() {
            let count = diff.hunks.len();
            s.complete &= diff.complete;
            hunks.splice(i..i + 1, diff.hunks);
            s.splice(i, 1, count, false, inner);
            follow(&mut self.compare.current, i, 1, count);
        }
        self.compare.alignment = Alignment::new(hunks, lines[0], lines[1]);
        true
    }

    /// The comparison on the status line: `None` with fewer than two files
    /// or while one loads.
    pub(super) fn comparison_text(&self) -> Option<String> {
        let (l, r) = (self.pane_of(Side::Left), self.pane_of(Side::Right));
        let ready = |p: &Pane| p.path.is_some() && matches!(p.status, LoadStatus::Ready);
        if !ready(l) || !ready(r) {
            return None;
        }
        if self.too_large() {
            let max = self.settings.diff.max_file_mib;
            return Some(format!("Not compared: a file is larger than {max} MiB"));
        }
        let Some(s) = self.shown_current() else {
            return Some("Comparing\u{2026}".to_owned());
        };
        let n = self.compare.alignment.hunks().len();
        if n > 0 {
            let mut text = format!("{n} change{}", plural(n as u64));
            if !s.complete {
                let secs = self.settings.diff.max_computation_ms as f64 / 1000.0;
                text.push_str(&format!(
                    "; diff simplified after {secs} s: changes are coarse"
                ));
            }
            return Some(text);
        }
        let encoding = |p: &Pane| p.info.map(|i| i.encoding);
        Some(if s.identical && encoding(l) == encoding(r) {
            "Identical".to_owned()
        } else {
            "No changes (whitespace, line breaks or encoding differ)".to_owned()
        })
    }

    /// Computes the overview ruler's marks for a track `height` pixels high:
    /// the rows of each change, left changes in the left half and right
    /// changes in the right half, at least two pixels high. Kept while the
    /// diff, the rows and the height stay.
    pub(super) fn refresh_marks(&mut self, height: u32) {
        let key = (self.compare.version, self.compare.alignment.rows(), height);
        if self.compare.marks.0 != Some(key) {
            let alignment = &self.compare.alignment;
            let rows = alignment.rows().max(1);
            let h = u64::from(height);
            let mut sides = vec![0u8; height as usize];
            for (i, hunk) in alignment.hunks().iter().enumerate() {
                let r = alignment.hunk_rows(i);
                let top = (r.start * h / rows).min(h.saturating_sub(1));
                let bottom = (r.end * h / rows).max(top + 2).min(h);
                let bits = u8::from(!hunk.left.is_empty()) | u8::from(!hunk.right.is_empty()) << 1;
                for side in &mut sides[top as usize..bottom as usize] {
                    *side |= bits;
                }
            }
            let mut marks = Vec::new();
            for (bit, side) in [(1u8, Side::Left), (2, Side::Right)] {
                let mut y = 0;
                while y < sides.len() {
                    if sides[y] & bit == 0 {
                        y += 1;
                        continue;
                    }
                    let start = y;
                    while y < sides.len() && sides[y] & bit != 0 {
                        y += 1;
                    }
                    marks.push(Mark {
                        top: start as u32,
                        height: (y - start) as u32,
                        side,
                    });
                }
            }
            self.compare.marks = (Some(key), marks);
        }
    }

    /// All rows of the view.
    pub(super) fn row_count(&self) -> u64 {
        self.compare.alignment.rows()
    }

    /// The row of `line` of the focused pane.
    pub(super) fn row_of(&self, line: u64) -> u64 {
        self.compare
            .alignment
            .row_of(diff_side(self.pane.side), line)
    }

    /// What the focused pane shows on `row` (below [`Editor::row_count`]).
    pub(super) fn row_cell(&self, row: u64) -> RowCell {
        self.compare
            .alignment
            .row(row)
            .side(diff_side(self.pane.side))
    }

    /// The line `row` shows of the focused pane, or for a filler the line
    /// after its gap; clamped to the text.
    pub(super) fn line_at_row(&self, row: u64) -> u64 {
        let last = self.pane.doc.snapshot.line_count() - 1;
        let row = row.min(self.row_count().saturating_sub(1));
        match self.row_cell(row) {
            RowCell::Line(line) | RowCell::Filler { before: line } => line.min(last),
        }
    }

    /// The lines of the focused pane the view shows.
    pub(super) fn view_lines(&self) -> Range<u64> {
        let rows = self.row_count();
        let count = self.pane.doc.snapshot.line_count();
        let top = self.scroll.top_row.min(rows.saturating_sub(1));
        let bottom = (top + u64::from(self.layout().rows.max(1))).min(rows);
        if top >= bottom {
            return count..count;
        }
        let first = match self.row_cell(top) {
            RowCell::Line(line) | RowCell::Filler { before: line } => line,
        };
        let end = match self.row_cell(bottom - 1) {
            RowCell::Line(line) => line + 1,
            RowCell::Filler { before } => before,
        };
        first.min(count)..end.min(count).max(first.min(count))
    }
}
