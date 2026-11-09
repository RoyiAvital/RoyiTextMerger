//! Dense line ids and Git's histogram diff (`xhistogram.c`) on them.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::ops::Range;

use rtm_text::Snapshot;

use crate::budget::Budget;
use crate::lines::{LineCursor, key};
use crate::{Cancelled, HashFn, Hunk, myers};

/// A line occurring more often than this in a region does not anchor it
/// (Git's `max_chain_length`); a region no line anchors goes to Myers.
const MAX_CHAIN: u32 = 64;

/// The lines of both sides as dense ids: equal hashes, equal ids.
pub(crate) struct Ids {
    pub left: Vec<u32>,
    pub right: Vec<u32>,
    /// The number of distinct ids.
    pub count: usize,
}

/// Map hasher for line hashes: they are mixed already.
#[derive(Default)]
struct LineHasher(u64);

impl Hasher for LineHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 << 8 | u64::from(b)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        }
    }

    fn write_u64(&mut self, n: u64) {
        // Spread weak hashes (tests) over the table too.
        self.0 = n.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
}

type IdMap = HashMap<u64, u32, BuildHasherDefault<LineHasher>>;

/// The ids of the lines of both ranges; `None` once past the deadline.
pub(crate) fn intern(
    left: &Snapshot,
    lr: &Range<u64>,
    right: &Snapshot,
    rr: &Range<u64>,
    trim: bool,
    hash: HashFn,
    budget: &mut Budget,
) -> Result<Option<Ids>, Cancelled> {
    let mut map = IdMap::default();
    let Some(left) = side_ids(left, lr, trim, hash, &mut map, budget)? else {
        return Ok(None);
    };
    let Some(right) = side_ids(right, rr, trim, hash, &mut map, budget)? else {
        return Ok(None);
    };
    Ok(Some(Ids {
        left,
        right,
        count: map.len(),
    }))
}

fn side_ids(
    s: &Snapshot,
    lines: &Range<u64>,
    trim: bool,
    hash: HashFn,
    map: &mut IdMap,
    budget: &mut Budget,
) -> Result<Option<Vec<u32>>, Cancelled> {
    let mut cursor = LineCursor::new(s, lines.start);
    let mut ids = Vec::with_capacity((lines.end - lines.start) as usize);
    for _ in lines.clone() {
        if !budget.tick(1)? {
            return Ok(None);
        }
        let h = hash(key(cursor.next_line(), trim));
        let next = map.len() as u32;
        ids.push(*map.entry(h).or_insert(next));
    }
    Ok(Some(ids))
}

/// Changed lines of both sides of a region, by index.
pub(crate) struct Changes {
    pub a: Vec<bool>,
    pub b: Vec<bool>,
}

impl Changes {
    pub fn new(a: usize, b: usize) -> Changes {
        Changes {
            a: vec![false; a],
            b: vec![false; b],
        }
    }

    pub fn mark(&mut self, a: Range<usize>, b: Range<usize>) {
        self.a[a].fill(true);
        self.b[b].fill(true);
    }

    /// The changed runs in order: each pairs the changed lines of both
    /// sides at the same place (one side may be empty).
    pub fn runs(&self) -> Vec<(Range<usize>, Range<usize>)> {
        let (a, b) = (&self.a, &self.b);
        let (mut i, mut j) = (0, 0);
        let mut out = Vec::new();
        while i < a.len() || j < b.len() {
            if a.get(i) == Some(&true) || b.get(j) == Some(&true) {
                let (si, sj) = (i, j);
                while a.get(i) == Some(&true) {
                    i += 1;
                }
                while b.get(j) == Some(&true) {
                    j += 1;
                }
                out.push((si..i, sj..j));
            } else {
                debug_assert!(i < a.len() && j < b.len(), "unchanged lines pair");
                i += 1;
                j += 1;
            }
        }
        out
    }

    /// The changed runs as hunks of lines starting at `a_off` and `b_off`.
    pub fn hunks(&self, a_off: u64, b_off: u64) -> Vec<Hunk> {
        let line = |off: u64, r: Range<usize>| off + r.start as u64..off + r.end as u64;
        self.runs()
            .into_iter()
            .map(|(a, b)| Hunk {
                left: line(a_off, a),
                right: line(b_off, b),
            })
            .collect()
    }
}

/// Marks the changed lines of `ids`. False when the deadline cut it short:
/// the regions left are marked whole.
///
/// A line whose id the other side lacks never pairs: it is marked changed
/// first and the histogram diff runs on the other lines (GNU diff and
/// imara-diff prune them too), which keeps unrelated texts fast.
pub(crate) fn diff(
    ids: &Ids,
    changes: &mut Changes,
    budget: &mut Budget,
) -> Result<bool, Cancelled> {
    let mut sides = vec![0u8; ids.count];
    for &id in &ids.left {
        sides[id as usize] |= 1;
    }
    for &id in &ids.right {
        sides[id as usize] |= 2;
    }
    let kept = |s: &[u32]| -> Vec<u32> {
        (0..s.len() as u32)
            .filter(|&i| sides[s[i as usize] as usize] == 3)
            .collect()
    };
    let (keep_a, keep_b) = (kept(&ids.left), kept(&ids.right));
    if keep_a.len() == ids.left.len() && keep_b.len() == ids.right.len() {
        return histogram(&ids.left, &ids.right, ids.count, changes, budget);
    }
    let a: Vec<u32> = keep_a.iter().map(|&i| ids.left[i as usize]).collect();
    let b: Vec<u32> = keep_b.iter().map(|&i| ids.right[i as usize]).collect();
    let mut kept_changes = Changes::new(a.len(), b.len());
    let complete = histogram(&a, &b, ids.count, &mut kept_changes, budget)?;
    changes.a.fill(true);
    changes.b.fill(true);
    for (k, &i) in keep_a.iter().enumerate() {
        changes.a[i as usize] = kept_changes.a[k];
    }
    for (k, &i) in keep_b.iter().enumerate() {
        changes.b[i as usize] = kept_changes.b[k];
    }
    Ok(complete)
}

/// Git's histogram diff of `a` and `b` (ids below `count`).
fn histogram(
    a: &[u32],
    b: &[u32],
    count: usize,
    changes: &mut Changes,
    budget: &mut Budget,
) -> Result<bool, Cancelled> {
    let mut index = Index {
        head: vec![0; count],
        count: vec![0; count],
        next: vec![0; a.len()],
    };
    let mut stack = vec![(0, a.len(), 0, b.len())];
    while let Some((mut a0, mut a1, mut b0, mut b1)) = stack.pop() {
        while a0 < a1 && b0 < b1 && a[a0] == b[b0] {
            a0 += 1;
            b0 += 1;
        }
        while a0 < a1 && b0 < b1 && a[a1 - 1] == b[b1 - 1] {
            a1 -= 1;
            b1 -= 1;
        }
        if a0 == a1 || b0 == b1 || !budget.tick((a1 - a0 + b1 - b0) as u64)? {
            changes.mark(a0..a1, b0..b1);
            continue;
        }
        match index.find_lcs(a, b, a0..a1, b0..b1, budget)? {
            Lcs::Found { a: la, b: lb } => {
                stack.push((la.end, a1, lb.end, b1));
                stack.push((a0, la.start, b0, lb.start));
            }
            Lcs::None | Lcs::Expired => changes.mark(a0..a1, b0..b1),
            Lcs::Frequent => myers::diff(&a[a0..a1], &b[b0..b1], a0, b0, changes, budget)?,
        }
    }
    Ok(!budget.expired())
}

enum Lcs {
    /// The longest common run anchored by the rarest line.
    Found { a: Range<usize>, b: Range<usize> },
    /// No common line.
    None,
    /// Common lines, all too frequent to anchor.
    Frequent,
    /// The deadline passed during the scan.
    Expired,
}

/// The occurrences of each id in the region of `a` being split. Positions
/// are stored plus one; 0 is none.
struct Index {
    /// First occurrence of each id.
    head: Vec<u32>,
    /// Occurrences of each id.
    count: Vec<u32>,
    /// Next occurrence of the id at each position.
    next: Vec<u32>,
}

impl Index {
    fn find_lcs(
        &mut self,
        a: &[u32],
        b: &[u32],
        ar: Range<usize>,
        br: Range<usize>,
        budget: &mut Budget,
    ) -> Result<Lcs, Cancelled> {
        for i in ar.clone().rev() {
            let id = a[i] as usize;
            self.next[i] = self.head[id];
            self.head[id] = i as u32 + 1;
            self.count[id] += 1;
        }
        let found = self.scan(a, b, ar.clone(), br, budget);
        for i in ar {
            let id = a[i] as usize;
            self.head[id] = 0;
            self.count[id] = 0;
        }
        found
    }

    /// Git's `find_lcs` / `try_lcs`: for each line of `b`, the runs around
    /// its occurrences in `a`; keeps the longest run, or one whose rarest
    /// line is rarer.
    fn scan(
        &self,
        a: &[u32],
        b: &[u32],
        ar: Range<usize>,
        br: Range<usize>,
        budget: &mut Budget,
    ) -> Result<Lcs, Cancelled> {
        let (a0, a1, b0, b1) = (ar.start, ar.end, br.start, br.end);
        // Inclusive: first and last line of the run in `a` and `b`.
        let mut best: Option<(usize, usize, usize, usize)> = None;
        let mut best_count = MAX_CHAIN + 1;
        let mut common = false;
        let mut bp = b0;
        while bp < b1 {
            if !budget.tick(1)? {
                return Ok(Lcs::Expired);
            }
            let mut b_next = bp + 1;
            let id = b[bp] as usize;
            let mut at = self.head[id];
            if at != 0 {
                common = true;
            }
            if self.count[id] > best_count {
                at = 0;
            }
            while at != 0 {
                let ap = at as usize - 1;
                let (mut s1, mut s2, mut e1, mut e2) = (ap, bp, ap, bp);
                let mut rc = self.count[id];
                while s1 > a0 && s2 > b0 && a[s1 - 1] == b[s2 - 1] {
                    s1 -= 1;
                    s2 -= 1;
                    if rc > 1 {
                        rc = rc.min(self.count[a[s1] as usize]);
                    }
                }
                while e1 + 1 < a1 && e2 + 1 < b1 && a[e1 + 1] == b[e2 + 1] {
                    e1 += 1;
                    e2 += 1;
                    if rc > 1 {
                        rc = rc.min(self.count[a[e1] as usize]);
                    }
                }
                if !budget.tick((e1 - s1) as u64)? {
                    return Ok(Lcs::Expired);
                }
                if b_next <= e2 {
                    b_next = e2 + 1;
                }
                let longer = best.is_none_or(|(bs1, _, be1, _)| be1 - bs1 < e1 - s1);
                if longer || rc < best_count {
                    best = Some((s1, s2, e1, e2));
                    best_count = rc;
                }
                // The next occurrence past this run.
                at = self.next[ap];
                while at != 0 && at as usize - 1 <= e1 {
                    at = self.next[at as usize - 1];
                }
            }
            bp = b_next;
        }
        Ok(match best {
            Some((s1, s2, e1, e2)) => Lcs::Found {
                a: s1..e1 + 1,
                b: s2..e2 + 1,
            },
            None if common => Lcs::Frequent,
            None => Lcs::None,
        })
    }
}
