//! Common prefix and suffix of two byte ranges, compared chunk by chunk.

use std::ops::Range;

use rtm_text::Snapshot;

/// Bytes compared at once before looking for the first difference.
const BLOCK: usize = 64;

/// The number of bytes both ranges start with.
pub(crate) fn common_prefix(a: &Snapshot, ar: Range<u64>, b: &Snapshot, br: Range<u64>) -> u64 {
    let (mut ca, mut cb) = (a.chunks(ar), b.chunks(br));
    let (mut sa, mut sb): (&[u8], &[u8]) = (&[], &[]);
    let mut total = 0;
    loop {
        if sa.is_empty() {
            match ca.next() {
                Some(chunk) => sa = chunk,
                None => return total,
            }
        }
        if sb.is_empty() {
            match cb.next() {
                Some(chunk) => sb = chunk,
                None => return total,
            }
        }
        let n = sa.len().min(sb.len());
        let same = equal_head(&sa[..n], &sb[..n]);
        total += same as u64;
        if same < n {
            return total;
        }
        sa = &sa[n..];
        sb = &sb[n..];
    }
}

/// The number of bytes both ranges end with, at most `limit`.
pub(crate) fn common_suffix(
    a: &Snapshot,
    ar: Range<u64>,
    b: &Snapshot,
    br: Range<u64>,
    limit: u64,
) -> u64 {
    // The ends of the bytes not compared yet.
    let (mut ea, mut eb) = (ar.end, br.end);
    let (mut sa, mut sb): (&[u8], &[u8]) = (&[], &[]);
    let mut total = 0;
    while total < limit {
        if sa.is_empty() {
            sa = slice_before(a, ea, ar.start);
        }
        if sb.is_empty() {
            sb = slice_before(b, eb, br.start);
        }
        let n = sa.len().min(sb.len()).min((limit - total) as usize);
        if n == 0 {
            break;
        }
        let same = equal_tail(&sa[sa.len() - n..], &sb[sb.len() - n..]);
        total += same as u64;
        if same < n {
            break;
        }
        sa = &sa[..sa.len() - n];
        sb = &sb[..sb.len() - n];
        ea -= n as u64;
        eb -= n as u64;
    }
    total
}

/// The bytes of the leaf before `end`, not before `start`.
fn slice_before(s: &Snapshot, end: u64, start: u64) -> &[u8] {
    if end <= start {
        return &[];
    }
    let (leaf, bytes) = s.chunk_at(end - 1).expect("end is inside the text");
    let from = start.max(leaf);
    &bytes[(from - leaf) as usize..(end - leaf) as usize]
}

/// The number of equal leading bytes of two equally long slices.
fn equal_head(a: &[u8], b: &[u8]) -> usize {
    let mut i = 0;
    while i + BLOCK <= a.len() && a[i..i + BLOCK] == b[i..i + BLOCK] {
        i += BLOCK;
    }
    while i < a.len() && a[i] == b[i] {
        i += 1;
    }
    i
}

/// The number of equal trailing bytes of two equally long slices.
fn equal_tail(a: &[u8], b: &[u8]) -> usize {
    let n = a.len();
    let mut i = 0;
    while i + BLOCK <= n && a[n - i - BLOCK..n - i] == b[n - i - BLOCK..n - i] {
        i += BLOCK;
    }
    while i < n && a[n - 1 - i] == b[n - 1 - i] {
        i += 1;
    }
    i
}
