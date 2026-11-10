//! Full-frame CPU cost (layout + draw, without presentation) for a screen of
//! code at 1080p and 4K: one pane, and two panes with a diff (the Merger's
//! frame). Run: `cargo bench -p rtm_render`.

use std::borrow::Cow;

use divan::Bencher;
use rtm_edit::Selection;
use rtm_render::{Renderer, Target};
use rtm_text::Snapshot;
use rtm_view::{
    Arrows, CaretStyle, Chrome, DisplayList, Frame, Header, Layout, Mark, Options, Row, RowKind,
    Scroll, Side, SplitFrame, Theme, ViewBuilder,
};

static FONT: &[u8] = rtm_assets::DEFAULT_FONT;
static BOLD_FONT: &[u8] = rtm_assets::BOLD_FONT;

fn main() {
    divan::main();
}

fn code() -> Snapshot {
    let line = "    let value = compute(index, &table[offset..]); // comment with more text here\n";
    Snapshot::from_bytes(line.repeat(5000).as_bytes())
}

fn renderer(px: f32) -> Renderer {
    let mut renderer =
        Renderer::new_with_bold(Cow::Borrowed(FONT), Cow::Borrowed(BOLD_FONT), px).unwrap();
    renderer.warm_ascii();
    renderer
}

/// Screen sizes with the font size scaled like 100% and 200% DPI.
#[divan::bench(args = [(1920, 1080, 14.0), (3840, 2160, 28.0)])]
fn full_frame(bencher: Bencher, (width, height, px): (u32, u32, f32)) {
    frames(bencher, &code(), width, height, px);
}

/// Adversarial rows: one letter followed by 32,760 combining accents (the
/// 64 KiB row scan limit), at 1080p.
#[divan::bench]
fn combining_marks(bencher: Bencher) {
    let mut line = "a".to_string();
    line.push_str(&"\u{301}".repeat(32_760));
    line.push('\n');
    frames(
        bencher,
        &Snapshot::from_bytes(line.repeat(4100).as_bytes()),
        1920,
        1080,
        14.0,
    );
}

fn frames(bencher: Bencher, snapshot: &Snapshot, width: u32, height: u32, px: f32) {
    let mut renderer = renderer(px);
    let cell = renderer.cell();
    let options = Options::default();
    let layout = Layout::new(width, height, cell, snapshot.line_count(), &options);
    let mut view = ViewBuilder::default();
    let mut list = DisplayList::default();
    let mut pixels = vec![0u32; (width * height) as usize];
    let mut top = 0;
    bencher.bench_local(|| {
        top = (top + 1) % 4000;
        let rows = Row::lines(top, layout.rows, snapshot.line_count());
        let frame = Frame {
            snapshot,
            layout: &layout,
            scroll: Scroll {
                top_row: top,
                first_col: 0,
            },
            rows: &rows,
            row_count: snapshot.line_count(),
            overview: &[],
            cell,
            theme: &Theme::DARK,
            chrome: Chrome {
                title: "C:\\Projects\\sample.rs",
                title_mark: Some(Theme::DARK.unsaved),
                app_name: "RoyiTextMerger",
                status_left: "5000 lines",
                status_right: "UTF-8 · CRLF",
                prompt: false,
            },
            options: &options,
            // Every visible row is partly selected; one caret.
            selections: &[Selection::new(100, 300_000)],
            caret: CaretStyle::Bar,
            highlights: Default::default(),
            empty: None,
            slider_active: false,
            measure: true,
        };
        view.build(&frame, &mut list);
        renderer.draw(
            &list,
            &mut Target {
                pixels: &mut pixels,
                width,
                height,
            },
        );
    });
}

/// Rows of one side of a busy diff: every 8th row changed (with an inner
/// change of 12 bytes), every 16th row a filler.
fn diff_rows(top: u64, count: u32, kind: RowKind, inner: &[std::ops::Range<u64>]) -> Vec<Row<'_>> {
    let mut line = top;
    (0..u64::from(count))
        .map(|row| match row % 16 {
            15 => Row {
                line: None,
                kind: RowKind::Filler,
                inner: &[],
            },
            7 => {
                line += 1;
                Row {
                    line: Some(line - 1),
                    kind,
                    inner,
                }
            }
            _ => {
                line += 1;
                Row {
                    line: Some(line - 1),
                    kind: RowKind::Same,
                    inner: &[],
                }
            }
        })
        .collect()
}

/// The Merger's frame: title headers, divider, two panes of a busy diff, the
/// overview ruler and the status line.
#[divan::bench(args = [(1920, 1080, 14.0), (3840, 2160, 28.0)])]
fn two_panes(bencher: Bencher, (width, height, px): (u32, u32, f32)) {
    let mut renderer = renderer(px);
    let cell = renderer.cell();
    let snapshot = code();
    let ruler = (14.0 * px / 14.0) as u32;
    let layouts = [Side::Left, Side::Right].map(|side| {
        let options = Options {
            pane: Some(side),
            scroll_strip: if side == Side::Right { ruler } else { 0 },
            ..Options::default()
        };
        Layout::new(width, height, cell, snapshot.line_count(), &options)
    });
    let options = Options::default();
    let marks: Vec<Mark> = (0..200)
        .map(|i| Mark {
            top: i * 4,
            height: 2,
            side: if i % 2 == 0 { Side::Left } else { Side::Right },
        })
        .collect();
    let mut view = ViewBuilder::default();
    let mut list = DisplayList::default();
    let mut pixels = vec![0u32; (width * height) as usize];
    let mut top = 0;
    let header = |name, focused| Header {
        dir: "C:\\Projects\\",
        name,
        mark: None,
        read_only: false,
        focused,
    };
    // Copy arrows on the first row of each change (every 16th row).
    let arrows: Vec<Arrows> = (0..layouts[0].rows)
        .filter(|row| row % 16 == 7)
        .map(|row| Arrows {
            row,
            right: true,
            left: true,
        })
        .collect();
    bencher.bench_local(|| {
        top = (top + 1) % 4000;
        view.build_split_chrome(
            &SplitFrame {
                layouts: [&layouts[0], &layouts[1]],
                cell,
                theme: &Theme::DARK,
                headers: [header("left.rs", true), header("right.rs", false)],
                app_name: "Royi Text Merger",
                status_left: "312 changes",
                status_right: "Left  ·  Ln 1, Col 1  ·  UTF-8  ·  LF  ·  Rust",
                prompt: false,
                arrows: &arrows,
            },
            &mut list,
        );
        for (i, (kind, start)) in [(RowKind::Removed, 4u64), (RowKind::Inserted, 20)]
            .into_iter()
            .enumerate()
        {
            let layout = &layouts[i];
            let first = snapshot.line_start(top).unwrap();
            let inner: Vec<std::ops::Range<u64>> = (0..u64::from(layout.rows))
                .map(|r| {
                    let at = first + r * 80 + start;
                    at..at + 12
                })
                .collect();
            let rows = diff_rows(top, layout.rows, kind, &inner);
            let frame = Frame {
                snapshot: &snapshot,
                layout,
                scroll: Scroll {
                    top_row: top,
                    first_col: 0,
                },
                rows: &rows,
                row_count: snapshot.line_count(),
                overview: if i == 1 { &marks } else { &[] },
                cell,
                theme: &Theme::DARK,
                chrome: Chrome::default(),
                options: &options,
                selections: &[Selection::caret(first)],
                caret: if i == 0 {
                    CaretStyle::Bar
                } else {
                    CaretStyle::Hidden
                },
                highlights: Default::default(),
                empty: None,
                slider_active: false,
                measure: true,
            };
            view.build_pane(&frame, &mut list);
        }
        renderer.draw(
            &list,
            &mut Target {
                pixels: &mut pixels,
                width,
                height,
            },
        );
    });
}
