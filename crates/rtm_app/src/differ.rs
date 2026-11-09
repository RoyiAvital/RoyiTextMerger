//! The diff thread: line diffs and inner changes of the two panes' texts.
//! One thread per session, started with it (comparing is the main feature).
//! A newer line request replaces a queued one; the UI cancels a running one
//! through its token. Inner work runs only while no line work waits and
//! stops when some arrives. Replies carry their request's generation,
//! revision and snapshots: the UI accepts only current ones.

use std::ops::Range;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use rtm_diff::{Alignment, CancelToken, Diff, Hunk, Inner, Options};
use rtm_text::Snapshot;

use crate::editor::Wake;

/// Inner work between replies: results reach the view in slices.
const INNER_SLICE: Duration = Duration::from_millis(8);

/// What a line request compares.
pub enum Work {
    /// Both whole texts.
    Full,
    /// These regions only (the provisional hunks with context); the hunks
    /// outside them stay. `rest`: their number.
    Regions { regions: Vec<Hunk>, rest: usize },
}

/// A line diff of both texts.
pub struct LineRequest {
    pub generation: u64,
    /// The revision of the hunks the regions belong to.
    pub revision: u64,
    pub left: Snapshot,
    pub right: Snapshot,
    pub ignore_trim_whitespace: bool,
    /// The diff becomes coarse after this long (from its start).
    pub budget: Duration,
    pub cancel: CancelToken,
    /// Rows around the view: their hunks get inner changes with the reply.
    pub view: Range<u64>,
    pub work: Work,
}

/// Inner changes of hunks of the current diff, nearest to the view first.
pub struct InnerRequest {
    pub generation: u64,
    pub revision: u64,
    pub left: Snapshot,
    pub right: Snapshot,
    pub ignore_trim_whitespace: bool,
    pub hunks: Vec<(usize, Hunk)>,
}

/// Inner changes of hunks by their index (in the diff, or in a region).
pub type InnerChanges = Vec<(usize, Option<Inner>)>;

/// The result of a line request.
pub enum Lines {
    Full(Diff, InnerChanges),
    /// Per region, in the request's order.
    Regions(Vec<(Diff, InnerChanges)>),
}

pub enum Reply {
    Lines {
        generation: u64,
        revision: u64,
        left: Snapshot,
        right: Snapshot,
        lines: Lines,
        /// No hunk is left and the texts are byte-equal.
        identical: bool,
    },
    Inner {
        generation: u64,
        revision: u64,
        left: Snapshot,
        right: Snapshot,
        inner: InnerChanges,
    },
}

enum Msg {
    Lines(LineRequest),
    Inner(InnerRequest),
    Stop,
}

/// The UI's handle; dropping it stops the thread.
pub struct DiffThread {
    tx: Sender<Msg>,
    rx: Receiver<Reply>,
}

impl DiffThread {
    pub fn spawn(wake: Wake) -> DiffThread {
        let (tx, jobs) = mpsc::channel();
        let (replies, rx) = mpsc::channel();
        // Without a thread the panes stay unaligned; nothing else depends on it.
        let _ = thread::Builder::new()
            .name("diff".to_owned())
            .spawn(move || run(&jobs, &replies, &wake));
        DiffThread { tx, rx }
    }

    pub fn lines(&self, request: LineRequest) {
        let _ = self.tx.send(Msg::Lines(request));
    }

    pub fn inner(&self, request: InnerRequest) {
        let _ = self.tx.send(Msg::Inner(request));
    }

    pub fn try_recv(&self) -> Option<Reply> {
        self.rx.try_recv().ok()
    }

    /// The next reply within `timeout` (startup waits for the first diff).
    pub fn recv_timeout(&self, timeout: Duration) -> Option<Reply> {
        self.rx.recv_timeout(timeout).ok()
    }
}

impl Drop for DiffThread {
    fn drop(&mut self) {
        let _ = self.tx.send(Msg::Stop);
    }
}

/// Work waiting on the thread: the newest line request, and the inner work
/// for the newest hunks (the newest view's hunks first).
#[derive(Default)]
struct Queue {
    lines: Option<LineRequest>,
    inner: Option<InnerRequest>,
    stop: bool,
}

impl Queue {
    fn take(&mut self, msg: Msg) {
        match msg {
            Msg::Lines(request) => self.lines = Some(request),
            Msg::Inner(request) => self.add_inner(request, true),
            Msg::Stop => self.stop = true,
        }
    }

    /// Queues inner work. Work for the same hunks (generation, revision and
    /// texts) joins, the newer work's hunks first; otherwise the newer work
    /// replaces the older. `newer`: `request` came after the queued work.
    fn add_inner(&mut self, request: InnerRequest, newer: bool) {
        let (mut first, rest) = match self.inner.take() {
            None => {
                self.inner = Some(request);
                return;
            }
            Some(queued) if newer => (request, queued),
            Some(queued) => (queued, request),
        };
        if first.generation == rest.generation
            && first.revision == rest.revision
            && first.left.same_tree(&rest.left)
            && first.right.same_tree(&rest.right)
        {
            first.hunks.extend(rest.hunks);
        }
        self.inner = Some(first);
    }

    /// Takes every waiting message; false when the UI is gone.
    fn drain(&mut self, rx: &Receiver<Msg>) -> bool {
        loop {
            match rx.try_recv() {
                Ok(msg) => self.take(msg),
                Err(TryRecvError::Empty) => return true,
                Err(TryRecvError::Disconnected) => return false,
            }
        }
    }
}

fn run(rx: &Receiver<Msg>, replies: &Sender<Reply>, wake: &Wake) {
    let mut queue = Queue::default();
    loop {
        if queue.lines.is_none() && queue.inner.is_none() {
            match rx.recv() {
                Ok(msg) => queue.take(msg),
                Err(_) => return,
            }
        }
        if !queue.drain(rx) || queue.stop {
            return;
        }
        let reply = if let Some(request) = queue.lines.take() {
            lines(request)
        } else if let Some(request) = queue.inner.take() {
            inner(request, rx, &mut queue)
        } else {
            None
        };
        if let Some(reply) = reply {
            if replies.send(reply).is_err() {
                return;
            }
            wake();
        }
    }
}

/// Inner changes of `hunks` (with their indices) for one slice of time.
fn inner_slice<'h>(
    request: &LineRequest,
    hunks: impl Iterator<Item = (usize, &'h Hunk)>,
) -> InnerChanges {
    let start = Instant::now();
    let mut done = Vec::new();
    for (i, h) in hunks {
        if start.elapsed() > INNER_SLICE || request.cancel.is_cancelled() {
            break;
        }
        let trim = request.ignore_trim_whitespace;
        done.push((i, rtm_diff::inner(&request.left, &request.right, h, trim)));
    }
    done
}

/// The diff of a line request, with the inner changes of the hunks around
/// its view (of a full diff) or of the re-diffed regions; `None` when it
/// was cancelled.
fn lines(request: LineRequest) -> Option<Reply> {
    let options = Options {
        ignore_trim_whitespace: request.ignore_trim_whitespace,
        deadline: Some(Instant::now() + request.budget),
    };
    let (left, right) = (&request.left, &request.right);
    let (lines, empty) = match &request.work {
        Work::Full => {
            let diff = rtm_diff::diff(left, right, &options, &request.cancel).ok()?;
            let alignment =
                Alignment::new(diff.hunks.clone(), left.line_count(), right.line_count());
            let near = alignment.hunks_in(request.view.clone());
            let inner = inner_slice(&request, near.map(|i| (i, &diff.hunks[i])));
            let empty = diff.hunks.is_empty();
            (Lines::Full(diff, inner), empty)
        }
        Work::Regions { regions, rest } => {
            let mut diffs = Vec::with_capacity(regions.len());
            for region in regions {
                let diff =
                    rtm_diff::diff_region(left, right, region, &options, &request.cancel).ok()?;
                diffs.push(diff);
            }
            let empty = *rest == 0 && diffs.iter().all(|d| d.hunks.is_empty());
            let mut results = Vec::with_capacity(diffs.len());
            for diff in diffs {
                let inner = inner_slice(&request, diff.hunks.iter().enumerate());
                results.push((diff, inner));
            }
            (Lines::Regions(results), empty)
        }
    };
    let identical = empty && rtm_diff::identical(left, right);
    Some(Reply::Lines {
        generation: request.generation,
        revision: request.revision,
        left: request.left,
        right: request.right,
        lines,
        identical,
    })
}

/// Inner changes of the requested hunks for one slice of time; the rest
/// waits behind newer inner work, unless line work arrived meanwhile (its
/// diff replaces the hunks).
fn inner(request: InnerRequest, rx: &Receiver<Msg>, queue: &mut Queue) -> Option<Reply> {
    let start = Instant::now();
    let mut done = Vec::new();
    let mut rest = request.hunks.into_iter();
    for (i, h) in rest.by_ref() {
        let trim = request.ignore_trim_whitespace;
        done.push((i, rtm_diff::inner(&request.left, &request.right, &h, trim)));
        if start.elapsed() > INNER_SLICE {
            break;
        }
    }
    let _ = queue.drain(rx);
    let hunks: Vec<(usize, Hunk)> = rest.collect();
    if queue.lines.is_none() && !hunks.is_empty() {
        let rest = InnerRequest {
            generation: request.generation,
            revision: request.revision,
            left: request.left.clone(),
            right: request.right.clone(),
            ignore_trim_whitespace: request.ignore_trim_whitespace,
            hunks,
        };
        queue.add_inner(rest, false);
    }
    (!done.is_empty()).then_some(Reply::Inner {
        generation: request.generation,
        revision: request.revision,
        left: request.left,
        right: request.right,
        inner: done,
    })
}
