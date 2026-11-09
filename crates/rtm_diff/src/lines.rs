//! Lines of a snapshot and the parts of a line the diff compares.

use rtm_text::{Chunks, Snapshot};

/// Lines a [`LineSource`] window holds.
const WINDOW: u64 = 32;
/// Windows a [`LineSource`] keeps.
const WINDOWS: usize = 8;

/// The byte offset where `line` starts; the text's length past the last line.
pub(crate) fn byte_at(s: &Snapshot, line: u64) -> u64 {
    s.line_start(line).unwrap_or(s.len())
}

/// The byte at `at`, if any.
pub(crate) fn byte(s: &Snapshot, at: u64) -> Option<u8> {
    s.chunk_at(at)
        .map(|(start, bytes)| bytes[(at - start) as usize])
}

/// The number of lines before line `end` that start at or after byte `at`.
pub(crate) fn lines_from(s: &Snapshot, at: u64, end: u64) -> u64 {
    let line = s.line_of(at);
    let first = if at == 0 || byte(s, at - 1) == Some(b'\n') {
        line
    } else {
        line + 1
    };
    end.saturating_sub(first)
}

/// The compared part of a line (given without its `\n`): without a final
/// `\r`, and without leading and trailing whitespace when `trim`.
pub(crate) fn key(line: &[u8], trim: bool) -> &[u8] {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    if trim { line.trim_ascii() } else { line }
}

/// Git's indent of a line: a space counts one column, a tab up to the next
/// multiple of 8, capped at 200; -1 for a blank line.
pub(crate) fn indent(line: &[u8]) -> i32 {
    const MAX_INDENT: i32 = 200;
    let mut columns = 0;
    for &c in line {
        if !c.is_ascii_whitespace() && c != 0x0B {
            return columns;
        }
        if c == b' ' {
            columns += 1;
        } else if c == b'\t' {
            columns += 8 - columns % 8;
        }
        if columns >= MAX_INDENT {
            return MAX_INDENT;
        }
    }
    -1
}

/// Lines from a given one on, read in order without their `\n` (a `\r`
/// before it stays). Past the end it returns empty lines.
pub(crate) struct LineCursor<'a> {
    chunks: Chunks<'a>,
    rest: &'a [u8],
    /// A line split across chunks.
    carry: Vec<u8>,
}

impl<'a> LineCursor<'a> {
    pub fn new(s: &'a Snapshot, line: u64) -> LineCursor<'a> {
        LineCursor {
            chunks: s.chunks(byte_at(s, line)..s.len()),
            rest: &[],
            carry: Vec::new(),
        }
    }

    pub fn next_line(&mut self) -> &[u8] {
        self.carry.clear();
        loop {
            let rest = self.rest;
            if let Some(i) = memchr::memchr(b'\n', rest) {
                self.rest = &rest[i + 1..];
                if self.carry.is_empty() {
                    return &rest[..i];
                }
                self.carry.extend_from_slice(&rest[..i]);
                return &self.carry;
            }
            self.carry.extend_from_slice(rest);
            match self.chunks.next() {
                Some(chunk) => self.rest = chunk,
                None => {
                    self.rest = &[];
                    return &self.carry;
                }
            }
        }
    }
}

/// Lines by number, read a window of lines at a time (the slider looks at
/// lines around each insertion and deletion).
pub(crate) struct LineSource<'a> {
    s: &'a Snapshot,
    windows: Vec<Window>,
    /// The slot replaced next.
    next: usize,
}

struct Window {
    first: u64,
    /// End of each line in `bytes`.
    ends: Vec<usize>,
    bytes: Vec<u8>,
}

impl Window {
    fn read(s: &Snapshot, first: u64) -> Window {
        let count = WINDOW.min(s.line_count() - first);
        let mut cursor = LineCursor::new(s, first);
        let mut bytes = Vec::new();
        let mut ends = Vec::with_capacity(count as usize);
        for _ in 0..count {
            bytes.extend_from_slice(cursor.next_line());
            ends.push(bytes.len());
        }
        Window { first, ends, bytes }
    }

    fn holds(&self, line: u64) -> bool {
        line >= self.first && line < self.first + self.ends.len() as u64
    }

    fn line(&self, line: u64) -> &[u8] {
        let i = (line - self.first) as usize;
        let start = if i == 0 { 0 } else { self.ends[i - 1] };
        &self.bytes[start..self.ends[i]]
    }
}

impl<'a> LineSource<'a> {
    pub fn new(s: &'a Snapshot) -> LineSource<'a> {
        LineSource {
            s,
            windows: Vec::new(),
            next: 0,
        }
    }

    /// The slot of the window holding `line` (which exists), read when
    /// needed without replacing slot `keep`.
    fn load(&mut self, line: u64, keep: Option<usize>) -> usize {
        if let Some(i) = self.windows.iter().position(|w| w.holds(line)) {
            return i;
        }
        let window = Window::read(self.s, line - line % WINDOW);
        if self.windows.len() < WINDOWS {
            self.windows.push(window);
            return self.windows.len() - 1;
        }
        if Some(self.next) == keep {
            self.next = (self.next + 1) % WINDOWS;
        }
        let slot = self.next;
        self.windows[slot] = window;
        self.next = (slot + 1) % WINDOWS;
        slot
    }

    /// Line `line` (which exists) without its `\n`.
    pub fn line(&mut self, line: u64) -> &[u8] {
        let i = self.load(line, None);
        self.windows[i].line(line)
    }

    /// Lines `x` and `y` are equal by the diff's rules.
    pub fn equal(&mut self, x: u64, y: u64, trim: bool) -> bool {
        let i = self.load(x, None);
        let j = self.load(y, Some(i));
        key(self.windows[i].line(x), trim) == key(self.windows[j].line(y), trim)
    }
}
