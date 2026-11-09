//! Inner changes: the changed characters of a hunk with lines on both
//! sides. Tokens (words, runs of spaces and tabs, single other characters,
//! line breaks) are diffed first. A word replaced by a word at least two
//! thirds equal is refined to its changed characters; otherwise it is
//! changed whole (VS Code extends a change to the whole word below that
//! ratio). An invalid UTF-8 byte is one unit, as the view draws it.

use std::collections::HashMap;
use std::ops::Range;

use rtm_text::Snapshot;

use crate::budget::Budget;
use crate::histogram::Changes;
use crate::lines::byte_at;
use crate::{Hunk, myers};

/// Larger sides keep whole-line highlighting only.
pub const INNER_MAX_BYTES: u64 = 64 * 1024;
/// Work of the diffs of one hunk; beyond it the hunk keeps whole lines.
const MAX_WORK: u64 = 1 << 22;
/// Units of invalid UTF-8 bytes: above every code point.
const INVALID: u32 = 0x11_0000;

/// The changed bytes of a hunk on each side: absolute offsets, sorted and
/// disjoint, never a line break.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Inner {
    pub left: Vec<Range<u64>>,
    pub right: Vec<Range<u64>>,
}

/// The inner changes of `hunk`; `None` for a hunk without lines on a side,
/// above [`INNER_MAX_BYTES`] per side or past the work limit: such hunks are
/// highlighted as whole lines. With `ignore_trim_whitespace` leading and
/// trailing whitespace is never a change.
pub fn inner(
    left: &Snapshot,
    right: &Snapshot,
    hunk: &Hunk,
    ignore_trim_whitespace: bool,
) -> Option<Inner> {
    if hunk.left.is_empty() || hunk.right.is_empty() {
        return None;
    }
    let a = Text::read(left, &hunk.left, ignore_trim_whitespace)?;
    let b = Text::read(right, &hunk.right, ignore_trim_whitespace)?;
    let mut ids: HashMap<&[u8], u32> = HashMap::new();
    let ia: Vec<u32> = (0..a.tokens.len())
        .map(|i| intern(&mut ids, a.token(i)))
        .collect();
    let ib: Vec<u32> = (0..b.tokens.len())
        .map(|i| intern(&mut ids, b.token(i)))
        .collect();
    let mut budget = Budget::new(None, None, MAX_WORK);
    let mut changes = Changes::new(ia.len(), ib.len());
    myers::diff(&ia, &ib, 0, 0, &mut changes, &mut budget).ok()?;
    if budget.expired() {
        return None;
    }
    let mut out = Inner::default();
    for (ra, rb) in changes.runs() {
        let words = ra.len() == 1
            && rb.len() == 1
            && a.tokens[ra.start].kind == Kind::Word
            && b.tokens[rb.start].kind == Kind::Word;
        if words && refine(&a, ra.start, &b, rb.start, &mut budget, &mut out) {
            continue;
        }
        for i in ra {
            a.push_token(i, &mut out.left);
        }
        for i in rb {
            b.push_token(i, &mut out.right);
        }
    }
    Some(out)
}

/// The id of `token`, a new one for new bytes.
fn intern<'t>(ids: &mut HashMap<&'t [u8], u32>, token: &'t [u8]) -> u32 {
    let next = ids.len() as u32;
    *ids.entry(token).or_insert(next)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Word,
    Space,
    Other,
    Break,
}

struct Token {
    /// Bytes in the text.
    range: Range<usize>,
    kind: Kind,
}

/// The lines of one side of a hunk, from the start of the first line to the
/// end of the last line without its line break.
struct Text {
    start: u64,
    bytes: Vec<u8>,
    tokens: Vec<Token>,
}

impl Text {
    fn read(s: &Snapshot, lines: &Range<u64>, trim: bool) -> Option<Text> {
        let start = byte_at(s, lines.start);
        let end = s.line_range(lines.end - 1)?.end;
        if end - start > INNER_MAX_BYTES {
            return None;
        }
        let bytes = s.to_vec(start..end);
        let tokens = tokenize(&bytes, trim);
        Some(Text {
            start,
            bytes,
            tokens,
        })
    }

    /// The compared bytes of token `i`: every line break is `\n`.
    fn token(&self, i: usize) -> &[u8] {
        let t = &self.tokens[i];
        match t.kind {
            Kind::Break => b"\n",
            _ => &self.bytes[t.range.clone()],
        }
    }

    fn push_token(&self, i: usize, out: &mut Vec<Range<u64>>) {
        let t = &self.tokens[i];
        if t.kind != Kind::Break {
            self.push(t.range.clone(), out);
        }
    }

    fn push(&self, range: Range<usize>, out: &mut Vec<Range<u64>>) {
        let range = self.start + range.start as u64..self.start + range.end as u64;
        match out.last_mut() {
            Some(last) if last.end >= range.start => last.end = last.end.max(range.end),
            _ => out.push(range),
        }
    }
}

/// The tokens of each line; a line break between lines is one token.
fn tokenize(bytes: &[u8], trim: bool) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut line = 0;
    loop {
        let br = memchr::memchr(b'\n', &bytes[line..]).map(|i| line + i);
        let (mut start, mut end) = (line, br.unwrap_or(bytes.len()));
        if end > start && bytes[end - 1] == b'\r' {
            end -= 1;
        }
        if trim {
            while start < end && bytes[start].is_ascii_whitespace() {
                start += 1;
            }
            while end > start && bytes[end - 1].is_ascii_whitespace() {
                end -= 1;
            }
        }
        words(&bytes[..end], start, &mut tokens);
        let Some(br) = br else {
            break;
        };
        tokens.push(Token {
            range: br..br + 1,
            kind: Kind::Break,
        });
        line = br + 1;
    }
    tokens
}

/// The tokens of `bytes[from..]`: words and runs of spaces and tabs whole,
/// other units one by one.
fn words(bytes: &[u8], from: usize, tokens: &mut Vec<Token>) {
    let mut i = from;
    while i < bytes.len() {
        let (u, len) = unit(bytes, i);
        let kind = kind_of(u);
        let mut j = i + len;
        if kind != Kind::Other {
            while j < bytes.len() {
                let (u, len) = unit(bytes, j);
                if kind_of(u) != kind {
                    break;
                }
                j += len;
            }
        }
        tokens.push(Token { range: i..j, kind });
        i = j;
    }
}

/// The unit at `at`: a code point, or an invalid byte above
/// [`char::MAX`]; and its length in bytes.
fn unit(bytes: &[u8], at: usize) -> (u32, usize) {
    let b = bytes[at];
    let len = match b {
        0x00..=0x7F => 1,
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => 0,
    };
    if len > 0
        && at + len <= bytes.len()
        && let Ok(s) = std::str::from_utf8(&bytes[at..at + len])
        && let Some(c) = s.chars().next()
    {
        return (c as u32, len);
    }
    (INVALID + u32::from(b), 1)
}

fn kind_of(unit: u32) -> Kind {
    match char::from_u32(unit) {
        Some(' ' | '\t') => Kind::Space,
        Some(c) if c.is_alphanumeric() || c == '_' => Kind::Word,
        _ => Kind::Other,
    }
}

/// The units of `bytes[range]` with their byte ranges.
fn units(bytes: &[u8], range: Range<usize>) -> Vec<(u32, Range<usize>)> {
    let bytes = &bytes[..range.end];
    let mut out = Vec::new();
    let mut i = range.start;
    while i < bytes.len() {
        let (u, len) = unit(bytes, i);
        out.push((u, i..i + len));
        i += len;
    }
    out
}

/// The changed characters of word `i` of `a` replaced by word `j` of `b`,
/// when at least two thirds of their units are equal; false otherwise.
fn refine(a: &Text, i: usize, b: &Text, j: usize, budget: &mut Budget, out: &mut Inner) -> bool {
    let ua = units(&a.bytes, a.tokens[i].range.clone());
    let ub = units(&b.bytes, b.tokens[j].range.clone());
    let ia: Vec<u32> = ua.iter().map(|(u, _)| *u).collect();
    let ib: Vec<u32> = ub.iter().map(|(u, _)| *u).collect();
    let mut changes = Changes::new(ia.len(), ib.len());
    if myers::diff(&ia, &ib, 0, 0, &mut changes, budget).is_err() || budget.expired() {
        return false;
    }
    let equal = changes.a.iter().filter(|&&c| !c).count();
    if 3 * equal < ia.len() + ib.len() {
        return false;
    }
    for (k, (_, r)) in ua.into_iter().enumerate() {
        if changes.a[k] {
            a.push(r, &mut out.left);
        }
    }
    for (k, (_, r)) in ub.into_iter().enumerate() {
        if changes.b[k] {
            b.push(r, &mut out.right);
        }
    }
    true
}
