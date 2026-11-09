use std::ops::Range;
use std::time::Instant;

use proptest::prelude::*;
use rtm_text::{Edit, Snapshot, Transaction};

use super::*;
use crate::histogram::{Changes, Ids};

fn snap(text: &str) -> Snapshot {
    Snapshot::from_bytes(text.as_bytes())
}

/// Lines as the diff sees them: split at every `\n`.
fn lines(text: &str) -> Vec<&str> {
    text.split('\n').collect()
}

fn hunk(left: Range<u64>, right: Range<u64>) -> Hunk {
    Hunk { left, right }
}

fn whole(left: &str, right: &str) -> Hunk {
    hunk(0..lines(left).len() as u64, 0..lines(right).len() as u64)
}

fn options(trim: bool) -> Options {
    Options {
        ignore_trim_whitespace: trim,
        deadline: None,
    }
}

fn run(left: &str, right: &str, trim: bool) -> Diff {
    diff(
        &snap(left),
        &snap(right),
        &options(trim),
        &CancelToken::new(),
    )
    .unwrap()
}

/// Lines hash by their first byte: many collide.
fn first_byte(line: &[u8]) -> u64 {
    line.first().copied().unwrap_or(0).into()
}

fn run_weak(left: &str, right: &str, trim: bool) -> Diff {
    let region = whole(left, right);
    let token = CancelToken::new();
    compare(
        &snap(left),
        &snap(right),
        &region,
        &options(trim),
        &token,
        first_byte,
    )
    .unwrap()
}

/// Asserts that `hunks` turn `left` into `right`: in bounds, sorted,
/// separated by unchanged lines, which are equal by the comparison's rules.
fn check(left: &str, right: &str, hunks: &[Hunk], trim: bool) {
    let (a, b) = (lines(left), lines(right));
    let equal = |i: u64, j: u64| {
        let (x, y) = (a[i as usize].as_bytes(), b[j as usize].as_bytes());
        lines::key(x, trim) == lines::key(y, trim)
    };
    let (mut i, mut j) = (0, 0);
    for (k, h) in hunks.iter().enumerate() {
        assert!(!h.left.is_empty() || !h.right.is_empty(), "empty {hunks:?}");
        assert!(h.left.start >= i && h.right.start >= j, "sorted {hunks:?}");
        assert_eq!(h.left.start - i, h.right.start - j, "runs pair {hunks:?}");
        assert!(k == 0 || h.left.start > i, "separated {hunks:?}");
        while i < h.left.start {
            assert!(equal(i, j), "lines {i} and {j} differ: {hunks:?}");
            (i, j) = (i + 1, j + 1);
        }
        (i, j) = (h.left.end, h.right.end);
    }
    let (n, m) = (a.len() as u64, b.len() as u64);
    assert!(i <= n && j <= m, "in bounds {hunks:?}");
    assert_eq!(n - i, m - j, "tails pair {hunks:?}");
    while i < n {
        assert!(equal(i, j), "lines {i} and {j} differ: {hunks:?}");
        (i, j) = (i + 1, j + 1);
    }
}

/// Asserts that the unchanged elements of both sequences pair.
fn check_ids(a: &[u32], b: &[u32], changes: &Changes) {
    let kept = |s: &[u32], c: &[bool]| -> Vec<u32> {
        s.iter()
            .zip(c)
            .filter(|(_, c)| !**c)
            .map(|(x, _)| *x)
            .collect()
    };
    assert_eq!(kept(a, &changes.a), kept(b, &changes.b));
}

/// The length of the longest common subsequence.
fn lcs(a: &[u32], b: &[u32]) -> usize {
    let mut row = vec![0; b.len() + 1];
    for x in a {
        let mut diagonal = 0;
        for (j, y) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = if x == y {
                diagonal + 1
            } else {
                row[j + 1].max(row[j])
            };
            diagonal = above;
        }
    }
    row[b.len()]
}

#[test]
fn identical_texts_have_no_hunks() {
    for text in ["", "a", "a\n", "a\nb\nc", "a\r\nb\r\n"] {
        let d = run(text, text, false);
        assert_eq!(
            d,
            Diff {
                hunks: Vec::new(),
                complete: true
            }
        );
        assert_eq!(d.status(&snap(text), &snap(text), true), Status::Identical);
    }
}

#[test]
fn line_breaks_and_trimmed_whitespace_are_not_changes() {
    let (crlf, lf) = ("a\r\nb\r\n", "a\nb\n");
    let d = run(crlf, lf, false);
    assert!(d.hunks.is_empty());
    assert_eq!(d.status(&snap(crlf), &snap(lf), true), Status::Equivalent);
    let (spaced, plain) = (" a\n\tb  \n", "a\nb\n");
    assert_eq!(run(spaced, plain, false).hunks, [hunk(0..2, 0..2)]);
    assert!(run(spaced, plain, true).hunks.is_empty());
    // Equal bytes in different encodings are not identical files.
    let d = run("a", "a", false);
    assert_eq!(d.status(&snap("a"), &snap("a"), false), Status::Equivalent);
    let d = run("a\nb", "a\nc", false);
    assert_eq!(
        d.status(&snap("a\nb"), &snap("a\nc"), true),
        Status::Changes(1)
    );
}

#[test]
fn changes_at_the_start_the_end_and_against_an_empty_text() {
    assert_eq!(run("b\nc", "a\nb\nc", false).hunks, [hunk(0..0, 0..1)]);
    assert_eq!(run("a\nb", "a\nb\nc", false).hunks, [hunk(2..2, 2..3)]);
    assert_eq!(run("a\nb\nc", "a\nc", false).hunks, [hunk(1..2, 1..1)]);
    assert_eq!(run("a\nb\nc", "b\nc", false).hunks, [hunk(0..1, 0..0)]);
    // An empty text is one empty line.
    assert_eq!(run("", "a", false).hunks, [hunk(0..1, 0..1)]);
    assert_eq!(run("", "a\n", false).hunks, [hunk(0..0, 0..1)]);
    assert_eq!(run("a\n", "", false).hunks, [hunk(0..1, 0..0)]);
    // A final line break adds an empty last line.
    assert_eq!(run("a\nb", "a\nb\n", false).hunks, [hunk(2..2, 2..3)]);
}

#[test]
fn prefix_and_suffix_leave_the_changed_middle() {
    let (left, right) = ("1\n2\n3\n4\n5\n", "1\n2\nX\n4\n5\n");
    assert_eq!(run(left, right, false).hunks, [hunk(2..3, 2..3)]);
    let (left, right) = ("1\n2\n3\n4\n5", "1\n2\n4\n5");
    assert_eq!(run(left, right, false).hunks, [hunk(2..3, 2..2)]);
}

#[test]
fn an_inserted_function_ends_at_its_blank_line() {
    let left = "fn a() {\n    one();\n}\n\nfn c() {\n    three();\n}\n";
    let right = "fn a() {\n    one();\n}\n\nfn b() {\n    two();\n}\n\nfn c() {\n    three();\n}\n";
    assert_eq!(run(left, right, false).hunks, [hunk(4..4, 4..8)]);
    // An added block inside a function starts at its first line.
    let left = "{\n    a();\n}\n";
    let right = "{\n    a();\n\n    b();\n}\n";
    let d = run(left, right, false);
    check(left, right, &d.hunks, false);
    assert_eq!(d.hunks, [hunk(2..2, 2..4)]);
    // An appended function starts at its blank line, not after it (the
    // common prefix alone would end with the blank line).
    let left = "fn a() {\n}\n";
    let right = "fn a() {\n}\n\nfn b() {\n}\n";
    assert_eq!(run(left, right, false).hunks, [hunk(2..2, 2..5)]);
    // After an earlier change the common suffix alone would place the
    // function's lines as `}`, blank, `fn b() {` (Git: as below).
    let left = "CHANGED1\nfn a() {\n}\n\nfn c() {\n}\n";
    let right = "CHANGED2\nfn a() {\n}\n\nfn b() {\n}\n\nfn c() {\n}\n";
    assert_eq!(
        run(left, right, false).hunks,
        [hunk(0..1, 0..1), hunk(4..4, 4..7)]
    );
}

#[test]
fn a_forced_hash_collision_becomes_a_change() {
    let (left, right) = ("a\nb\nc\nd", "a\nx\nc\ny");
    let region = whole(left, right);
    let weak: HashFn = |_| 7;
    let token = CancelToken::new();
    let d = compare(
        &snap(left),
        &snap(right),
        &region,
        &options(false),
        &token,
        weak,
    )
    .unwrap();
    assert_eq!(d.hunks, [hunk(1..2, 1..2), hunk(3..4, 3..4)]);
}

#[test]
fn a_collision_next_to_a_slid_insertion_becomes_a_change() {
    // "b1" and "b2" collide; the inserted blank line slides past them.
    let (left, right) = ("k\nb1\n\nz\n", "k\nb2\n\n\nz\n");
    let d = run_weak(left, right, false);
    check(left, right, &d.hunks, false);
    assert!(d.hunks.iter().any(|h| h.left.contains(&1)), "{:?}", d.hunks);
}

#[test]
fn the_deadline_gives_a_valid_simplified_diff() {
    let left: String = (0..2000).map(|i| format!("line {i}\n")).collect();
    let right: String = (0..2000)
        .map(|i| format!("line {}\n", i * 7 % 2000))
        .collect();
    let options = Options {
        ignore_trim_whitespace: false,
        deadline: Some(Instant::now()),
    };
    let d = diff(&snap(&left), &snap(&right), &options, &CancelToken::new()).unwrap();
    assert!(!d.complete);
    check(&left, &right, &d.hunks, false);
}

#[test]
fn a_work_limit_marks_the_regions_left_whole() {
    let a: Vec<u32> = (0..400).map(|i| i % 50).collect();
    let b: Vec<u32> = (0..400).map(|i| (i * 3) % 50).collect();
    let ids = Ids {
        left: a.clone(),
        right: b.clone(),
        count: 50,
    };
    for limit in [0, 10, 100, 1000, 100_000] {
        let mut budget = Budget::new(None, None, limit);
        let mut changes = Changes::new(a.len(), b.len());
        histogram::diff(&ids, &mut changes, &mut budget).unwrap();
        check_ids(&a, &b, &changes);
    }
}

#[test]
fn alternating_lines_shift_by_one_line() {
    // Review M2-R1: "a b a b ..." against "b a b a ...". Bounded: a
    // regression fails at the deadline instead of hanging.
    let pairs = 50_000;
    let (left, right) = ("a\nb\n".repeat(pairs), "b\na\n".repeat(pairs));
    let options = Options {
        ignore_trim_whitespace: false,
        deadline: Some(Instant::now() + std::time::Duration::from_secs(30)),
    };
    let d = diff(&snap(&left), &snap(&right), &options, &CancelToken::new()).unwrap();
    assert!(d.complete);
    let n = 2 * pairs as u64;
    assert_eq!(d.hunks, [hunk(0..1, 0..0), hunk(n..n, n - 1..n)]);
}

#[test]
fn repetitive_texts_with_many_changes_stay_fast() {
    // Three kinds of lines in unrelated orders: no line anchors, and Myers
    // passes its cost limit. Without the limit this takes minutes.
    let mut state = 7u64;
    let mut kind = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        ["}", "", "    x();"][(state % 3) as usize]
    };
    let left: String = (0..100_000).map(|_| format!("{}\n", kind())).collect();
    let right: String = (0..100_000).map(|_| format!("{}\n", kind())).collect();
    let options = Options {
        ignore_trim_whitespace: false,
        deadline: Some(Instant::now() + std::time::Duration::from_secs(30)),
    };
    let d = diff(&snap(&left), &snap(&right), &options, &CancelToken::new()).unwrap();
    assert!(d.complete, "finished before the deadline");
    check(&left, &right, &d.hunks, false);
}

#[test]
fn myers_past_its_cost_limit_stays_valid_and_near_minimal() {
    let mut state = 11u64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state % 3) as u32
    };
    let a: Vec<u32> = (0..3000).map(|_| next()).collect();
    let b: Vec<u32> = (0..3000).map(|_| next()).collect();
    let mut budget = Budget::new(None, None, u64::MAX);
    let mut changes = Changes::new(a.len(), b.len());
    myers::diff(&a, &b, 0, 0, &mut changes, &mut budget).unwrap();
    check_ids(&a, &b, &changes);
    let edits = changes.a.iter().chain(&changes.b).filter(|&&c| c).count();
    let minimal = a.len() + b.len() - 2 * lcs(&a, &b);
    assert!(minimal > 2 * 256, "the cost limit is reached: {minimal}");
    assert!(edits * 4 <= minimal * 5, "{edits} edits, {minimal} minimal");
}

#[test]
fn cancelling_stops_the_diff() {
    let token = CancelToken::new();
    token.cancel();
    assert_eq!(
        diff(&snap("a"), &snap("b"), &Options::default(), &token),
        Err(Cancelled)
    );
}

#[test]
fn lines_only_one_side_has_are_changed_first() {
    let (a, b) = (vec![0, 1, 2, 3, 1], vec![4, 1, 5, 3, 6]);
    let ids = Ids {
        left: a.clone(),
        right: b.clone(),
        count: 7,
    };
    let mut budget = Budget::new(None, None, u64::MAX);
    let mut changes = Changes::new(a.len(), b.len());
    histogram::diff(&ids, &mut changes, &mut budget).unwrap();
    check_ids(&a, &b, &changes);
    assert_eq!(changes.a, [true, false, true, false, true]);
    assert_eq!(changes.b, [true, false, true, false, true]);
}

#[test]
fn frequent_lines_fall_back_to_a_minimal_myers_diff() {
    // No line is rarer than 64 occurrences.
    let a: Vec<u32> = (0..140).map(|i| i % 2).collect();
    let mut b = a.clone();
    b.insert(70, 1);
    b.remove(10);
    b[100] = 0;
    let ids = Ids {
        left: a.clone(),
        right: b.clone(),
        count: 2,
    };
    let mut budget = Budget::new(None, None, u64::MAX);
    let mut changes = Changes::new(a.len(), b.len());
    histogram::diff(&ids, &mut changes, &mut budget).unwrap();
    check_ids(&a, &b, &changes);
    let edits = changes.a.iter().chain(&changes.b).filter(|&&c| c).count();
    assert_eq!(edits, a.len() + b.len() - 2 * lcs(&a, &b));
}

#[test]
fn a_region_rediff_covers_only_the_region() {
    let (left, right) = ("1\n2\n3\n4\n5", "1\n2\nX\n4\n5");
    let token = CancelToken::new();
    let region = hunk(1..4, 1..4);
    let d = diff_region(&snap(left), &snap(right), &region, &options(false), &token).unwrap();
    assert_eq!(d.hunks, [hunk(2..3, 2..3)]);
    let region = hunk(2..3, 2..3);
    let d = diff_region(&snap(left), &snap(right), &region, &options(false), &token).unwrap();
    assert_eq!(
        d.hunks,
        [hunk(2..3, 2..3)],
        "hunks may touch the region's ends"
    );
    let region = hunk(2..2, 2..3);
    let d = diff_region(&snap(left), &snap(right), &region, &options(false), &token).unwrap();
    assert_eq!(d.hunks, [region]);
}

#[test]
fn alignment_maps_rows_lines_and_fillers() {
    // a b c d e  ->  a X Y c e: b becomes X and Y, d is deleted.
    let al = Alignment::new(vec![hunk(1..2, 1..3), hunk(3..4, 4..4)], 5, 5);
    assert_eq!(al.rows(), 6);
    let pair = |left, right, hunk| RowPair { left, right, hunk };
    use Cell::{Filler, Line};
    assert_eq!(al.row(0), pair(Line(0), Line(0), None));
    assert_eq!(al.row(1), pair(Line(1), Line(1), Some(0)));
    assert_eq!(al.row(2), pair(Filler { before: 2 }, Line(2), Some(0)));
    assert_eq!(al.row(3), pair(Line(2), Line(3), None));
    assert_eq!(al.row(4), pair(Line(3), Filler { before: 4 }, Some(1)));
    assert_eq!(al.row(5), pair(Line(4), Line(4), None));
    assert_eq!(al.hunk_rows(0), 1..3);
    assert_eq!(al.hunks_in(0..1), 0..0);
    assert_eq!(al.hunks_in(2..4), 0..1);
    assert_eq!(al.hunks_in(2..5), 0..2);
    assert_eq!(al.hunks_in(4..6), 1..2);
    assert_eq!(al.hunks_in(5..9), 2..2);
    assert_eq!(al.row_of(Side::Left, 4), 5);
    assert_eq!(al.row_of(Side::Right, 3), 3);
    // Gaps at the start and at the end of a text.
    let al = Alignment::new(vec![hunk(0..0, 0..2), hunk(1..1, 3..5)], 1, 5);
    assert_eq!(al.row(0).left, Filler { before: 0 });
    assert_eq!(al.row(4).left, Filler { before: 1 });
    // Before the first diff: fillers after the shorter text.
    let al = Alignment::identity(3, 5);
    assert_eq!(al.rows(), 5);
    assert_eq!(al.row(4), pair(Filler { before: 3 }, Line(4), None));
    assert_eq!(al.row_of(Side::Right, 4), 4);
}

#[test]
fn alignment_follows_edited_line_counts() {
    let pair = |left, right, hunk| RowPair { left, right, hunk };
    use Cell::{Filler, Line};
    // Hunks of longer texts than the current ones never panic.
    let stale = Alignment::new(vec![hunk(5..8, 5..6)], 3, 3);
    assert_eq!(stale.rows(), 8);
    assert_eq!(stale.row(7), pair(Line(7), Filler { before: 6 }, Some(0)));
    // Lines added after the last hunk extend its run; rows before it stay.
    let mut al = Alignment::new(vec![hunk(1..2, 1..3)], 4, 5);
    assert_eq!(al.rows(), 5);
    al.set_lines(6, 5);
    assert_eq!(al, Alignment::new(vec![hunk(1..2, 1..3)], 6, 5));
    assert_eq!(al.rows(), 7);
    assert_eq!(al.row(6), pair(Line(5), Filler { before: 5 }, None));
}

#[test]
fn edits_map_hunks_into_a_provisional_hunk() {
    let edit = |side, lines, added| LineEdit {
        side,
        lines,
        added,
        bytes: 0,
    };
    let base = || vec![hunk(1..2, 1..3), hunk(10..11, 12..12)];
    // An unchanged line becomes a hunk with its pair; later hunks shift.
    let mut hunks = base();
    assert_eq!(map_edit(&mut hunks, &edit(Side::Left, 4..5, 2)), (1, 0));
    assert_eq!(
        hunks,
        [hunk(1..2, 1..3), hunk(4..7, 5..6), hunk(12..13, 12..12)]
    );
    // Lines touching a hunk join it; the other side keeps its lines.
    let mut hunks = base();
    assert_eq!(map_edit(&mut hunks, &edit(Side::Left, 2..3, 1)), (0, 1));
    assert_eq!(hunks, [hunk(1..4, 1..4), hunk(11..12, 12..12)]);
    // The right side: its lines 11 and 12 pair left lines 10 and 11 around a
    // deleted left line.
    let mut hunks = base();
    assert_eq!(map_edit(&mut hunks, &edit(Side::Right, 11..13, -1)), (1, 1));
    assert_eq!(hunks, [hunk(1..2, 1..3), hunk(10..12, 11..12)]);
    // Two changes of one line are one edit; changes apart are two.
    let before = snap("a\nb\nc\nd\ne\n");
    let tx = Transaction::new(vec![
        Edit::insert(0, *b"x\n"),
        Edit::insert(1, *b"y"),
        Edit::new(6..8, *b""),
    ]);
    let (after, changes) = before.apply(&tx).unwrap();
    assert_eq!(
        line_edits(Side::Left, &before, &after, &changes),
        [
            LineEdit {
                bytes: 3,
                ..edit(Side::Left, 0..1, 1)
            },
            LineEdit {
                bytes: -2,
                ..edit(Side::Left, 3..5, -1)
            }
        ]
    );
}

#[test]
fn rediff_regions_keep_a_line_to_their_neighbors() {
    let hunks = [
        hunk(1..2, 1..3),
        hunk(10..11, 12..12),
        hunk(300..301, 301..301),
    ];
    let lines = [500, 500];
    // The first region reaches the text's start; a plain neighbor keeps one line.
    assert_eq!(
        rediff_region(&hunks, 0, |_| false, lines),
        hunk(0..9, 0..10)
    );
    // Half the way to a re-diffed neighbor, at most the context.
    assert_eq!(
        rediff_region(&hunks, 1, |n| n == 0, lines),
        hunk(7..111, 9..112)
    );
    assert_eq!(
        rediff_region(&hunks, 2, |_| false, lines),
        hunk(200..401, 201..401)
    );
}

fn inner_of(left: &[u8], right: &[u8], trim: bool) -> Option<Inner> {
    let (a, b) = (Snapshot::from_bytes(left), Snapshot::from_bytes(right));
    let h = hunk(0..a.line_count(), 0..b.line_count());
    inner(&a, &b, &h, trim)
}

fn changed(left: Vec<Range<u64>>, right: Vec<Range<u64>>) -> Option<Inner> {
    Some(Inner { left, right })
}

#[test]
fn inner_changes_cover_words_or_their_changed_characters() {
    // A different word changes whole.
    assert_eq!(
        inner_of(b"    w * w", b"    w * h", false),
        changed(vec![8..9], vec![8..9])
    );
    assert_eq!(
        inner_of(b"x = foo;", b"x = bar;", false),
        changed(vec![4..7], vec![4..7])
    );
    // A word two thirds equal: only its changed characters.
    assert_eq!(
        inner_of(b"let area = 1;", b"let areas = 1;", false),
        changed(vec![], vec![8..9])
    );
    // Characters, never parts of them; an invalid byte is one unit.
    assert_eq!(
        inner_of("café".as_bytes(), b"cafe", false),
        changed(vec![3..5], vec![3..4])
    );
    assert_eq!(
        inner_of(b"a \xFF b", b"a \xFE b", false),
        changed(vec![2..3], vec![2..3])
    );
    // Several lines; line breaks are compared, never marked.
    assert_eq!(
        inner_of(b"a\r\nb\r\nc", b"a\nB\nc", false),
        changed(vec![3..4], vec![2..3])
    );
    assert_eq!(
        inner_of(b"a\nb", b"a b", false),
        changed(vec![], vec![1..2])
    );
}

#[test]
fn inner_changes_follow_the_whitespace_option_and_the_bound() {
    assert_eq!(
        inner_of(b"  x = 1", b"x = 1  ", true),
        changed(vec![], vec![])
    );
    assert_eq!(
        inner_of(b"  x = 1", b"x = 1", false),
        changed(vec![0..2], vec![])
    );
    // One-sided hunks and large sides keep whole lines.
    let (a, b) = (snap("a\nb"), snap("a"));
    assert_eq!(inner(&a, &b, &hunk(1..2, 1..1), false), None);
    let big = "x".repeat(INNER_MAX_BYTES as usize + 1);
    assert_eq!(inner_of(big.as_bytes(), b"y", false), None);
}

/// Lines from a small vocabulary: repeated, blank, indented, with trailing
/// whitespace and line breaks of both kinds.
fn text() -> impl Strategy<Value = String> {
    const VOCABULARY: [&str; 12] = [
        "", "a", "b", "{", "}", "    x", "\tx", "x ", "fn f() {", "c\r", "    }", "  ",
    ];
    (
        prop::collection::vec(prop::sample::select(&VOCABULARY[..]), 0..60),
        any::<bool>(),
    )
        .prop_map(|(lines, final_break)| {
            let mut text = lines.join("\n");
            if final_break {
                text.push('\n');
            }
            text
        })
}

/// `text` with lines replaced, inserted and removed.
fn edited() -> impl Strategy<Value = (String, String)> {
    (
        text(),
        prop::collection::vec((any::<prop::sample::Index>(), 0..3u8, text()), 0..6),
    )
        .prop_map(|(left, edits)| {
            let mut lines: Vec<String> = left.split('\n').map(str::to_owned).collect();
            for (at, kind, new) in edits {
                let i = at.index(lines.len());
                match kind {
                    0 => lines[i] = new,
                    1 => lines.insert(i, new),
                    _ if lines.len() > 1 => {
                        lines.remove(i);
                    }
                    _ => {}
                }
            }
            (left, lines.join("\n"))
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    #[test]
    fn diffs_are_valid(left in text(), right in text(), trim in any::<bool>()) {
        let d = run(&left, &right, trim);
        check(&left, &right, &d.hunks, trim);
        prop_assert!(d.complete);
    }

    #[test]
    fn diffs_of_edited_texts_are_valid((left, right) in edited(), trim in any::<bool>()) {
        let d = run(&left, &right, trim);
        check(&left, &right, &d.hunks, trim);
    }

    #[test]
    fn collisions_never_pass_as_unchanged((left, right) in edited(), trim in any::<bool>()) {
        let d = run_weak(&left, &right, trim);
        check(&left, &right, &d.hunks, trim);
    }

    #[test]
    fn region_rediffs_splice_into_a_valid_diff(
        (left, right) in edited(),
        picks in any::<(prop::sample::Index, prop::sample::Index)>(),
    ) {
        let d = run(&left, &right, false);
        // Positions between unchanged pairs: the lines around them pair.
        let mut anchors = vec![(0, 0)];
        let (mut l, mut r) = (0, 0);
        for h in &d.hunks {
            while l < h.left.start {
                (l, r) = (l + 1, r + 1);
                anchors.push((l, r));
            }
            (l, r) = (h.left.end, h.right.end);
            anchors.push((l, r));
        }
        let end = (lines(&left).len() as u64, lines(&right).len() as u64);
        while l < end.0 {
            (l, r) = (l + 1, r + 1);
            anchors.push((l, r));
        }
        let (i, j) = (picks.0.index(anchors.len()), picks.1.index(anchors.len()));
        let (from, to) = (anchors[i.min(j)], anchors[i.max(j)]);
        let region = hunk(from.0..to.0, from.1..to.1);
        let token = CancelToken::new();
        let inside = diff_region(&snap(&left), &snap(&right), &region, &options(false), &token).unwrap();
        let mut hunks: Vec<Hunk> = d.hunks.iter().filter(|h| h.left.end <= from.0 && h.right.end <= from.1).cloned().collect();
        hunks.extend(inside.hunks);
        hunks.extend(d.hunks.iter().filter(|h| h.left.start >= to.0 && h.right.start >= to.1).cloned());
        let mut joined: Vec<Hunk> = Vec::new();
        for h in hunks {
            match joined.last_mut() {
                Some(last) if last.left.end == h.left.start && last.right.end == h.right.start => {
                    last.left.end = h.left.end;
                    last.right.end = h.right.end;
                }
                _ => joined.push(h),
            }
        }
        check(&left, &right, &joined, false);
    }

    #[test]
    fn mapped_and_rediffed_edits_stay_valid(
        (left, right) in edited(),
        edits in prop::collection::vec(
            (any::<prop::sample::Index>(), 0..12usize, prop::sample::select(&["", "x", "\n", "a\nb", "}\n{\n", "\r\n", "  "][..])),
            1..4,
        ),
        on_right in any::<bool>(),
        trim in any::<bool>(),
    ) {
        let side = if on_right { Side::Right } else { Side::Left };
        let before = snap(if on_right { &right } else { &left });
        let len = before.len() as usize;
        // Sorted, separated byte edits of one side.
        let mut ranges: Vec<(Range<u64>, &str)> = edits
            .iter()
            .map(|(at, n, text)| {
                let start = at.index(len + 1);
                (start as u64..(start + n).min(len) as u64, *text)
            })
            .collect();
        ranges.sort_by_key(|(r, _)| r.start);
        let mut kept: Vec<Edit> = Vec::new();
        let mut end = None;
        for (range, text) in ranges {
            if end.is_none_or(|e| range.start > e) {
                end = Some(range.end);
                kept.push(Edit::new(range, text.as_bytes()));
            }
        }
        let (after, changes) = before.apply(&Transaction::new(kept)).unwrap();
        let edited = String::from_utf8(after.to_vec(0..after.len())).unwrap();
        let (l2, r2) = if on_right { (left.clone(), edited) } else { (edited, right.clone()) };

        let mut hunks = run(&left, &right, trim).hunks;
        let mut provisional = vec![false; hunks.len()];
        for edit in line_edits(side, &before, &after, &changes).iter().rev() {
            let (at, replaced) = map_edit(&mut hunks, edit);
            provisional.splice(at..at + replaced, [true]);
        }
        // Unchanged lines stay unchanged: the mapped diff is valid.
        check(&l2, &r2, &hunks, trim);
        prop_assert_eq!(provisional.len(), hunks.len());

        // Re-diffs of the provisional regions splice into a valid diff.
        let (a, b) = (snap(&l2), snap(&r2));
        let lines = [a.line_count(), b.line_count()];
        let token = CancelToken::new();
        for i in (0..hunks.len()).rev().filter(|&i| provisional[i]) {
            let region = rediff_region(&hunks, i, |n| provisional[n], lines);
            let inside = diff_region(&a, &b, &region, &options(trim), &token).unwrap();
            hunks.splice(i..i + 1, inside.hunks);
        }
        check(&l2, &r2, &hunks, trim);
    }

    #[test]
    fn myers_is_minimal(
        a in prop::collection::vec(0..4u32, 0..40),
        b in prop::collection::vec(0..4u32, 0..40),
    ) {
        let mut budget = Budget::new(None, None, u64::MAX);
        let mut changes = Changes::new(a.len(), b.len());
        myers::diff(&a, &b, 0, 0, &mut changes, &mut budget).unwrap();
        check_ids(&a, &b, &changes);
        let edits = changes.a.iter().chain(&changes.b).filter(|&&c| c).count();
        prop_assert_eq!(edits, a.len() + b.len() - 2 * lcs(&a, &b));
    }

    #[test]
    fn alignment_round_trips((left, right) in edited()) {
        let d = run(&left, &right, false);
        let (n, m) = (lines(&left).len() as u64, lines(&right).len() as u64);
        let al = Alignment::new(d.hunks, n, m);
        let fillers = |side: Side| (0..al.rows()).filter(|&row| {
            let pair = al.row(row);
            matches!(if side == Side::Left { pair.left } else { pair.right }, Cell::Filler { .. })
        }).count() as u64;
        prop_assert_eq!(al.rows(), n + fillers(Side::Left));
        prop_assert_eq!(al.rows(), m + fillers(Side::Right));
        for (side, count) in [(Side::Left, n), (Side::Right, m)] {
            for line in 0..count {
                let pair = al.row(al.row_of(side, line));
                let cell = if side == Side::Left { pair.left } else { pair.right };
                prop_assert_eq!(cell, Cell::Line(line));
            }
        }
    }

    #[test]
    fn inner_ranges_are_sorted_character_ranges(left in text(), right in text(), trim in any::<bool>()) {
        let (a, b) = (snap(&left), snap(&right));
        let h = hunk(0..a.line_count(), 0..b.line_count());
        if let Some(inner) = inner(&a, &b, &h, trim) {
            for (ranges, text) in [(&inner.left, &left), (&inner.right, &right)] {
                for (k, r) in ranges.iter().enumerate() {
                    prop_assert!(r.start < r.end);
                    prop_assert!(k == 0 || ranges[k - 1].end < r.start);
                    let s = &text[r.start as usize..r.end as usize];
                    prop_assert!(!s.contains('\n'));
                }
            }
        }
    }
}
