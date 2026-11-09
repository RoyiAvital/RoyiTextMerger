//! Myers' O(ND) diff in linear space (bisection at the middle snake), for
//! the regions the histogram diff cannot anchor and for inner changes.
//!
//! As in Git (`xdl_split` with `mxcost`), a bisection that has spent √N edits
//! (at least 256) splits at the path that got furthest instead: the diff
//! stays valid and near O(N·√N) on repetitive texts with many changes, and is
//! no longer minimal there.

use crate::Cancelled;
use crate::budget::Budget;
use crate::histogram::Changes;

/// The least edit cost a bisection searches before splitting at the
/// furthest path (Git's `XDL_MAX_COST_MIN`).
const MIN_COST: isize = 256;

/// Marks the changes between `a` and `b`, which start at `a_off` and
/// `b_off` in `changes`. Past the budget, the regions left are marked whole.
pub(crate) fn diff(
    a: &[u32],
    b: &[u32],
    a_off: usize,
    b_off: usize,
    changes: &mut Changes,
    budget: &mut Budget,
) -> Result<(), Cancelled> {
    let cost = (((a.len() + b.len() + 3) as f64).sqrt() as isize).max(MIN_COST);
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
        if a0 < a1 && b0 < b1 {
            let (n, m) = (a1 - a0, b1 - b0);
            match bisect(&a[a0..a1], &b[b0..b1], cost, budget)? {
                Some((x, y)) if (x, y) != (0, 0) && (x, y) != (n, m) => {
                    stack.push((a0 + x, a1, b0 + y, b1));
                    stack.push((a0, a0 + x, b0, b0 + y));
                    continue;
                }
                _ => {}
            }
        }
        changes.mark(a_off + a0..a_off + a1, b_off + b0..b_off + b1);
    }
    Ok(())
}

/// The point where the forward and the backward shortest edit paths meet,
/// or past `cost` edits the furthest point either reached; `None` when the
/// texts share nothing or the budget ran out.
fn bisect(
    a: &[u32],
    b: &[u32],
    cost: isize,
    budget: &mut Budget,
) -> Result<Option<(usize, usize)>, Cancelled> {
    let (n, m) = (a.len() as isize, b.len() as isize);
    let max_d = (n + m + 1) / 2;
    let limit = max_d.min(cost);
    // Diagonals -limit - 1 ..= limit + 1.
    let offset = limit + 1;
    let len = 2 * offset + 1;
    let mut v1 = vec![-1isize; len as usize];
    let mut v2 = vec![-1isize; len as usize];
    v1[offset as usize + 1] = 0;
    v2[offset as usize + 1] = 0;
    let delta = n - m;
    // With an odd delta the forward path detects the overlap.
    let front = delta % 2 != 0;
    let (mut k1start, mut k1end, mut k2start, mut k2end) = (0, 0, 0, 0);
    for d in 0..max_d {
        if !budget.tick((2 * d + 2) as u64)? {
            return Ok(None);
        }
        let mut k1 = -d + k1start;
        while k1 <= d - k1end {
            let k1o = (offset + k1) as usize;
            let mut x1 = if k1 == -d || (k1 != d && v1[k1o - 1] < v1[k1o + 1]) {
                v1[k1o + 1]
            } else {
                v1[k1o - 1] + 1
            };
            let mut y1 = x1 - k1;
            while x1 < n && y1 < m && a[x1 as usize] == b[y1 as usize] {
                x1 += 1;
                y1 += 1;
            }
            v1[k1o] = x1;
            if x1 > n {
                k1end += 2;
            } else if y1 > m {
                k1start += 2;
            } else if front {
                let k2o = offset + delta - k1;
                if (0..len).contains(&k2o) && v2[k2o as usize] != -1 && x1 >= n - v2[k2o as usize] {
                    return Ok(Some((x1 as usize, y1 as usize)));
                }
            }
            k1 += 2;
        }
        let mut k2 = -d + k2start;
        while k2 <= d - k2end {
            let k2o = (offset + k2) as usize;
            let mut x2 = if k2 == -d || (k2 != d && v2[k2o - 1] < v2[k2o + 1]) {
                v2[k2o + 1]
            } else {
                v2[k2o - 1] + 1
            };
            let mut y2 = x2 - k2;
            while x2 < n && y2 < m && a[(n - x2 - 1) as usize] == b[(m - y2 - 1) as usize] {
                x2 += 1;
                y2 += 1;
            }
            v2[k2o] = x2;
            if x2 > n {
                k2end += 2;
            } else if y2 > m {
                k2start += 2;
            } else if !front {
                let k1o = offset + delta - k2;
                if (0..len).contains(&k1o) && v1[k1o as usize] != -1 {
                    let x1 = v1[k1o as usize];
                    let y1 = offset + x1 - k1o;
                    if x1 >= n - x2 {
                        return Ok(Some((x1 as usize, y1 as usize)));
                    }
                }
            }
            k2 += 2;
        }
        if d >= limit {
            let forward = furthest(&v1, offset, -d + k1start..=d - k1end, n, m);
            let backward = furthest(&v2, offset, -d + k2start..=d - k2end, n, m);
            let split = match (forward, backward) {
                (Some(f), Some(b)) if b.0 + b.1 <= f.0 + f.1 => f,
                (_, Some(b)) => (n - b.0, m - b.1),
                (Some(f), None) => f,
                (None, None) => return Ok(None),
            };
            return Ok(Some((split.0 as usize, split.1 as usize)));
        }
    }
    Ok(None)
}

/// The furthest point (largest x + y) the paths of `v` reached on the
/// diagonals `ks` (every second one), within the `n` by `m` grid.
fn furthest(
    v: &[isize],
    offset: isize,
    ks: std::ops::RangeInclusive<isize>,
    n: isize,
    m: isize,
) -> Option<(isize, isize)> {
    let mut best: Option<(isize, isize)> = None;
    for k in ks.step_by(2) {
        let x = v[(offset + k) as usize];
        if x < 0 {
            continue;
        }
        let mut x = x.min(n);
        let mut y = x - k;
        if y > m {
            (x, y) = (m + k, m);
        }
        if x >= 0 && y >= 0 && best.is_none_or(|(bx, by)| x + y > bx + by) {
            best = Some((x, y));
        }
    }
    best
}
