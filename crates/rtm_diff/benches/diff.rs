//! Diff benchmarks with imara-diff as the baseline. Run: `cargo bench -p rtm_diff`.
//!
//! Fixtures are generated deterministically: code-like lines, similar
//! (scattered one-line edits) or unrelated, many repeated lines, and a large
//! similar pair. imara-diff runs Git's histogram diff with its line
//! postprocessing; it has no cancellation or deadline.

use std::sync::{Arc, LazyLock};

use divan::{Bencher, black_box};
use imara_diff::{Algorithm, InternedInput};
use rtm_diff::{Alignment, CancelToken, Hunk, LineEdit, Options, Side};
use rtm_text::{Snapshot, TreeBuilder};

const MIB: usize = 1024 * 1024;
const BLOCK: usize = 16 * MIB;

/// Deterministic pseudo-random numbers (xorshift).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// About `bytes` of code-like lines: nested functions, statements, blank
/// lines.
fn code(bytes: usize, seed: u64) -> String {
    let mut rng = Rng(seed);
    let mut out = String::with_capacity(bytes + 100);
    let mut depth = 0usize;
    let mut i = 0;
    while out.len() < bytes {
        let r = rng.next();
        let indent = "    ".repeat(depth);
        match r % 10 {
            0 if depth < 4 => {
                out += &format!("{indent}fn item_{}() {{\n", r % 100_000);
                depth += 1;
            }
            1 if depth > 0 => {
                depth -= 1;
                out += &format!("{}}}\n", "    ".repeat(depth));
            }
            2 => out.push('\n'),
            _ => {
                out += &format!(
                    "{indent}let v{} = compute({}, &table[{}..]);\n",
                    r % 1000,
                    r % 97,
                    i % 31
                )
            }
        }
        i += 1;
    }
    out
}

/// `text` with `edits` lines changed, evenly spread.
fn edited(text: &str, edits: usize) -> String {
    let mut lines: Vec<String> = text.split('\n').map(str::to_owned).collect();
    let step = (lines.len() / (edits + 1)).max(1);
    for k in 1..=edits {
        if let Some(line) = lines.get_mut(k * step) {
            line.push_str(" // changed");
        }
    }
    lines.join("\n")
}

/// Lines of only a few kinds: no line is rare enough to anchor.
fn repeated(lines: usize, seed: u64) -> String {
    const KINDS: [&str; 4] = ["}", "", "    x();", "{"];
    let mut rng = Rng(seed);
    (0..lines)
        .map(|_| KINDS[(rng.next() % 4) as usize])
        .collect::<Vec<_>>()
        .join("\n")
}

fn snapshot(text: &str) -> Snapshot {
    let mut builder = TreeBuilder::new();
    for block in text.as_bytes().chunks(BLOCK) {
        builder.push_block(Arc::from(block));
    }
    builder.finish()
}

struct Pair {
    left: String,
    right: String,
    snapshots: (Snapshot, Snapshot),
}

impl Pair {
    fn new(left: String, right: String) -> Pair {
        let snapshots = (snapshot(&left), snapshot(&right));
        Pair {
            left,
            right,
            snapshots,
        }
    }

    fn rtm_diff(&self) -> usize {
        let (l, r) = &self.snapshots;
        let diff = rtm_diff::diff(
            black_box(l),
            black_box(r),
            &Options::default(),
            &CancelToken::new(),
        );
        diff.unwrap().hunks.len()
    }

    fn imara(&self) -> usize {
        let input = InternedInput::new(
            black_box(self.left.as_str()),
            black_box(self.right.as_str()),
        );
        let mut diff = imara_diff::Diff::compute(Algorithm::Histogram, &input);
        diff.postprocess_lines(&input);
        diff.hunks().count()
    }
}

static SIMILAR: LazyLock<Pair> = LazyLock::new(|| {
    let left = code(MIB, 0x2545_F491_4F6C_DD1D);
    let right = edited(&left, 10);
    Pair::new(left, right)
});

static UNRELATED: LazyLock<Pair> = LazyLock::new(|| {
    Pair::new(
        code(MIB, 0x2545_F491_4F6C_DD1D),
        code(MIB, 0x9E37_79B9_7F4A_7C15),
    )
});

static REPEATED: LazyLock<Pair> = LazyLock::new(|| {
    let left = repeated(100_000, 7);
    let right = edited(&left, 100);
    Pair::new(left, right)
});

static LARGE: LazyLock<Pair> = LazyLock::new(|| {
    let left = code(50 * MIB, 0x2545_F491_4F6C_DD1D);
    let right = edited(&left, 100);
    Pair::new(left, right)
});

fn main() {
    divan::main();
}

mod similar_1mib {
    use super::*;

    #[divan::bench]
    fn rtm_diff() -> usize {
        SIMILAR.rtm_diff()
    }

    #[divan::bench]
    fn imara_diff() -> usize {
        SIMILAR.imara()
    }
}

mod unrelated_1mib {
    use super::*;

    #[divan::bench(sample_count = 20)]
    fn rtm_diff() -> usize {
        UNRELATED.rtm_diff()
    }

    #[divan::bench(sample_count = 20)]
    fn imara_diff() -> usize {
        UNRELATED.imara()
    }
}

mod repeated_100k_lines {
    use super::*;

    #[divan::bench(sample_count = 20)]
    fn rtm_diff() -> usize {
        REPEATED.rtm_diff()
    }

    #[divan::bench(sample_count = 20)]
    fn imara_diff() -> usize {
        REPEATED.imara()
    }
}

mod similar_50mib {
    use super::*;

    #[divan::bench(sample_count = 5, sample_size = 1)]
    fn rtm_diff() -> usize {
        LARGE.rtm_diff()
    }

    #[divan::bench(sample_count = 5, sample_size = 1)]
    fn imara_diff() -> usize {
        LARGE.imara()
    }
}

mod rediff {
    use super::*;

    /// The region of one edited line in the similar 1 MiB pair with its
    /// unchanged neighbors: what a keystroke asks the worker for.
    #[divan::bench]
    fn one_line_edit(bencher: Bencher) {
        let (l, r) = &SIMILAR.snapshots;
        let (options, token) = (Options::default(), CancelToken::new());
        let full = rtm_diff::diff(l, r, &options, &token).unwrap();
        let h = &full.hunks[full.hunks.len() / 2];
        let region = Hunk {
            left: h.left.start - 1..h.left.end + 1,
            right: h.right.start - 1..h.right.end + 1,
        };
        bencher
            .bench(|| rtm_diff::diff_region(l, r, black_box(&region), &options, &token).unwrap());
    }

    /// The inner changes of one edited line.
    #[divan::bench]
    fn inner_changes(bencher: Bencher) {
        let (l, r) = &SIMILAR.snapshots;
        let full = rtm_diff::diff(l, r, &Options::default(), &CancelToken::new()).unwrap();
        let h = full.hunks[full.hunks.len() / 2].clone();
        bencher.bench(|| rtm_diff::inner(l, r, black_box(&h), false));
    }
}

mod mapping {
    use super::*;

    const HUNKS: u64 = 100_000;

    /// A change every 10 lines: 100,000 hunks over 1,000,000 lines.
    fn hunks() -> Vec<Hunk> {
        (0..HUNKS)
            .map(|i| Hunk {
                left: 10 * i + 5..10 * i + 6,
                right: 10 * i + 5..10 * i + 7,
            })
            .collect()
    }

    /// A keystroke before every hunk: the provisional mapping and the new
    /// alignment, the UI's work in the frame of the edit (the frame gate).
    #[divan::bench(args = [0, 1])]
    fn early_edit_100k_hunks(bencher: Bencher, added: i64) {
        let lines = 10 * HUNKS + 1;
        let edit = LineEdit {
            side: Side::Left,
            lines: 1..2,
            added,
            bytes: added,
        };
        bencher.with_inputs(hunks).bench_local_values(|mut hunks| {
            rtm_diff::map_edit(&mut hunks, black_box(&edit));
            Alignment::new(hunks, lines + added as u64, lines + HUNKS)
        });
    }
}
