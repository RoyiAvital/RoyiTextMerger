//! Viewport layout: turns the visible part of a snapshot plus window chrome
//! into a [`DisplayList`] of filled rectangles and glyph cells.
//!
//! All geometry is integer: every character occupies one or two cells of a
//! fixed size (monospaced font, LTR, no shaping).

mod find;
mod popup;

pub use find::{
    FIND_SYMBOLS, FindField, FindFrame, FindGeometry, FindHit, LINE_BREAK, ReplaceGeometry,
    Severity, TOGGLES, draw_find, draw_find_tooltip,
};
pub use popup::{
    Image, Layer, PALETTE_ROWS, PaletteFrame, PaletteGeometry, PaletteRow, PanelFrame,
    PanelGeometry, PanelLine, Star, Tone, draw_palette, draw_panel,
};

use rtm_edit::{Selection, column_after, column_of, content_range, offset_in_line, scan_end};
use rtm_text::Snapshot;
use std::fmt::Write;

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Color as `0x00RRGGBB`.
pub type Color = u32;

/// Bytes scanned per visible row and frame; longer lines are cut off.
pub const ROW_SCAN_LIMIT: usize = 64 * 1024;

/// Zero-width marks drawn on one cell; further marks are skipped (not removed).
pub const MAX_MARKS_PER_CELL: u32 = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Theme {
    pub background: Color,
    pub text: Color,
    pub control: Color,
    pub line_number: Color,
    pub title_background: Color,
    pub title_text: Color,
    pub status_background: Color,
    pub status_text: Color,
    pub selection: Color,
    pub caret: Color,
    /// Unsaved marker of the title strip.
    pub unsaved: Color,
    /// Warnings of the search box (text and border); the unsaved marker
    /// after a refused save.
    pub warning: Color,
    pub prompt_background: Color,
    pub prompt_text: Color,
    pub popup_background: Color,
    pub popup_border: Color,
    pub popup_input: Color,
    pub popup_dim: Color,
    /// Matched characters of a filtered row.
    pub popup_match: Color,
    pub popup_selected: Color,
    pub favorite: Color,
    pub link: Color,
    /// Visible search matches other than the current one.
    pub search_match: Color,
    /// The current match (also the selection) and its border.
    pub search_current: Color,
    pub search_current_border: Color,
    /// Find-in-selection range.
    pub search_range: Color,
    pub error_text: Color,
    pub error_background: Color,
    pub error_border: Color,
    /// Text colors of the syntax styles, indexed by [`SyntaxSpan::style`]:
    /// keyword, string, comment, number, type, function, constant, operator,
    /// punctuation, property, tag, attribute.
    pub syntax: [Color; 12],
    /// Background of a matching bracket pair.
    pub bracket_match: Color,
    /// The scroll slider, and while it is dragged.
    pub scroll_slider: Color,
    pub scroll_slider_active: Color,
    /// Wavy underline of misspelled words.
    pub spelling: Color,
    /// The strip between the panes of a two-pane window, and its edges.
    pub divider: Color,
    pub divider_line: Color,
    /// Selections of the pane without the keyboard focus.
    pub selection_inactive: Color,
    /// Diff decorations (VS Code's dark diff colors blended over the
    /// background): removed (left) and inserted (right) lines, their changed
    /// characters, the hatch of filler rows and the gutter signs.
    pub diff_removed_line: Color,
    pub diff_removed_text: Color,
    pub diff_inserted_line: Color,
    pub diff_inserted_text: Color,
    pub diff_filler: Color,
    pub diff_removed_sign: Color,
    pub diff_inserted_sign: Color,
    /// The divider's copy arrows.
    pub diff_arrow: Color,
}

impl Theme {
    pub const DARK: Theme = Theme {
        background: 0x1E1F22,
        text: 0xD4D4D4,
        control: 0x6A6F78,
        line_number: 0x5C6370,
        title_background: 0x18191B,
        title_text: 0xA0A4AB,
        status_background: 0x18191B,
        status_text: 0x8B9099,
        selection: 0x264F78,
        caret: 0xAEAFAD,
        unsaved: 0x4FA3E0,
        warning: 0xE0A040,
        prompt_background: 0x5A3E14,
        prompt_text: 0xF0E0C0,
        popup_background: 0x26282C,
        popup_border: 0x3E4148,
        popup_input: 0x1B1C1F,
        popup_dim: 0x80858F,
        popup_match: 0x5FB3F0,
        popup_selected: 0x264F78,
        favorite: 0xE8A33A,
        link: 0x5FB3F0,
        search_match: 0x603317,
        search_current: 0x515C6A,
        search_current_border: 0x8FA3BA,
        search_range: 0x2A2D33,
        error_text: 0xF48771,
        error_background: 0x5A1D1D,
        error_border: 0xE05252,
        syntax: [
            0x569CD6, // keyword
            0xCE9178, // string
            0x6A9955, // comment
            0xB5CEA8, // number
            0x4EC9B0, // type
            0xDCDCAA, // function
            0x4FC1FF, // constant
            0xD4D4D4, // operator
            0xA9AEB6, // punctuation
            0x9CDCFE, // property
            0x569CD6, // tag
            0x9CDCFE, // attribute
        ],
        bracket_match: 0x3E4A45,
        // VS Code's dark slider colors over the background.
        scroll_slider: 0x434446,
        scroll_slider_active: 0x5E5F61,
        spelling: 0x3794FF,
        divider: 0x1B1C1F,
        divider_line: 0x2C2E33,
        // VS Code's inactive selection.
        selection_inactive: 0x3A3D41,
        // #ff000033 and #9bb95533 lines, #ff000033 and #9ccc2c33 text over
        // them, #cccccc33 diagonal fill; git decoration signs.
        diff_removed_line: 0x4B191B,
        diff_removed_text: 0x6F1416,
        diff_inserted_line: 0x373E2C,
        diff_inserted_text: 0x4B5A2C,
        diff_filler: 0x414244,
        diff_removed_sign: 0xE06C6C,
        diff_inserted_sign: 0x81B88B,
        diff_arrow: 0x5FB3F0,
    };
}

/// A pane of a two-pane window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    pub fn other(self) -> Side {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Side::Left => "Left",
            Side::Right => "Right",
        }
    }
}

/// Width of the divider between two panes, in cells: `→`, a space, `←`.
pub const DIVIDER_CELLS: u32 = 3;

/// The divider's copy arrows on the first row of a change: `row` counts the
/// visible rows; `right` copies it to the right pane, `left` to the left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Arrows {
    pub row: u32,
    pub right: bool,
    pub left: bool,
}

/// Size of one character cell in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w as i32 && y < self.y + self.h as i32
    }

    /// Intersection, or `None` when empty.
    pub fn intersect(&self, other: &Rect) -> Option<Rect> {
        let x0 = self.x.max(other.x);
        let y0 = self.y.max(other.y);
        let x1 = (self.x + self.w as i32).min(other.x + other.w as i32);
        let y1 = (self.y + self.h as i32).min(other.y + other.h as i32);
        (x0 < x1 && y0 < y1).then(|| Rect {
            x: x0,
            y: y0,
            w: (x1 - x0) as u32,
            h: (y1 - y0) as u32,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    pub tab_width: u32,
    pub line_numbers: bool,
    /// The title strip is shown (not in full screen).
    pub title_strip: bool,
    /// Width in pixels of the scroll strip right of the text (0: none).
    pub scroll_strip: u32,
    /// The pane of a two-pane window this layout covers; `None`: the whole
    /// width.
    pub pane: Option<Side>,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            tab_width: 4,
            line_numbers: true,
            title_strip: true,
            scroll_strip: 0,
            pane: None,
        }
    }
}

/// Screen areas for a window size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub title: Rect,
    pub gutter: Rect,
    pub text: Rect,
    /// Strip of the scroll slider, right of the text.
    pub scrollbar: Rect,
    pub status: Rect,
    /// Strip between the panes of a two-pane window (empty otherwise).
    pub divider: Rect,
    /// Fully or partially visible text rows.
    pub rows: u32,
    /// Fully visible text columns.
    pub cols: u32,
    /// Gutter columns after the line numbers for the change signs: a pane
    /// of a two-pane window has [`SIGN_COLS`], a whole window none.
    pub signs: u32,
}

/// The `-` / `+` column of a pane's gutter and the space after it.
pub const SIGN_COLS: u32 = 2;

impl Layout {
    pub fn new(width: u32, height: u32, cell: Cell, line_count: u64, options: &Options) -> Layout {
        let bar = cell.height + cell.height / 2;
        let title = Rect {
            x: 0,
            y: 0,
            w: width,
            h: if options.title_strip {
                bar.min(height)
            } else {
                0
            },
        };
        let status_h = bar.min(height.saturating_sub(title.h));
        let status = Rect {
            x: 0,
            y: (height - status_h) as i32,
            w: width,
            h: status_h,
        };
        let body_y = title.h as i32;
        let body_h = height - title.h - status_h;
        // The pane's columns: the whole width, or a half beside the divider.
        let (pane_x, pane_w, divider) = match options.pane {
            None => (0, width, Rect::default()),
            Some(side) => {
                let div = (DIVIDER_CELLS * cell.width).min(width);
                let left = (width - div) / 2;
                let divider = Rect {
                    x: left as i32,
                    y: body_y,
                    w: div,
                    h: body_h,
                };
                match side {
                    Side::Left => (0, left, divider),
                    Side::Right => (left + div, width - left - div, divider),
                }
            }
        };
        let signs = if options.pane.is_some() { SIGN_COLS } else { 0 };
        let gutter_cols = signs
            + if options.line_numbers {
                digits(line_count) + 2
            } else {
                1
            };
        let gutter = Rect {
            x: pane_x as i32,
            y: body_y,
            w: (gutter_cols * cell.width).min(pane_w),
            h: body_h,
        };
        let strip = options.scroll_strip.min(pane_w - gutter.w);
        let text = Rect {
            x: gutter.x + gutter.w as i32,
            y: body_y,
            w: pane_w - gutter.w - strip,
            h: body_h,
        };
        let scrollbar = Rect {
            x: text.x + text.w as i32,
            y: body_y,
            w: strip,
            h: body_h,
        };
        Layout {
            title,
            gutter,
            text,
            scrollbar,
            status,
            divider,
            rows: body_h.div_ceil(cell.height.max(1)),
            cols: text.w / cell.width.max(1),
            signs,
        }
    }

    /// The pane's area: gutter, text and scroll strip.
    pub fn pane(&self) -> Rect {
        Rect {
            x: self.gutter.x,
            y: self.gutter.y,
            w: self.gutter.w + self.text.w + self.scrollbar.w,
            h: self.gutter.h,
        }
    }
}

fn digits(n: u64) -> u32 {
    n.max(1).ilog10() + 1
}

/// The scroll slider in `layout.scrollbar` for `rows` rows, a view from
/// `top_row` showing `page` full rows, at least `min` pixels high. The
/// bottom of the track is the last row at the top of the view (as far as
/// scrolling goes). `None` when there is no strip or nothing scrolls (one
/// row); a text shorter than the view still scrolls to its last row.
pub fn slider(layout: &Layout, rows: u64, top_row: u64, page: u64, min: u32) -> Option<Rect> {
    let track = layout.scrollbar;
    let rows = rows.max(1);
    if track.w == 0 || track.h == 0 || rows == 1 {
        return None;
    }
    let last = rows - 1;
    let h = u64::from(track.h);
    let size = (h * page / (last + page)).max(u64::from(min)).min(h);
    let y = ((h - size) * top_row.min(last))
        .checked_div(last)
        .unwrap_or(0);
    Some(Rect {
        x: track.x,
        y: track.y + y as i32,
        w: track.w,
        h: size as u32,
    })
}

/// Full text rows of a layout (at least one).
pub fn page_rows(layout: &Layout, cell: Cell) -> u64 {
    u64::from((layout.text.h / cell.height.max(1)).max(1))
}

/// Track of the horizontal slider: the bottom of the text area, `thickness`
/// pixels high (the vertical strip's width; `None` for 0).
pub fn htrack(layout: &Layout, thickness: u32) -> Option<Rect> {
    let text = layout.text;
    let h = thickness.min(text.h);
    (h > 0 && text.w > 0).then(|| Rect {
        x: text.x,
        y: text.y + (text.h - h) as i32,
        w: text.w,
        h,
    })
}

/// The horizontal slider in [`htrack`] for rows `total` columns wide, a view
/// from `first_col` showing `layout.cols` columns, at least `min` pixels
/// wide. `None` when the rows fit.
pub fn hslider(
    layout: &Layout,
    thickness: u32,
    total: u64,
    first_col: u64,
    min: u32,
) -> Option<Rect> {
    let track = htrack(layout, thickness)?;
    let cols = u64::from(layout.cols);
    if total <= cols {
        return None;
    }
    let w = u64::from(track.w);
    let size = (w * cols / total).max(u64::from(min)).min(w);
    let last = total - cols;
    let x = (w - size) * first_col.min(last) / last;
    Some(Rect {
        x: track.x + x as i32,
        w: size as u32,
        ..track
    })
}

/// First visible row and column of the text area. Both panes share it: a
/// row shows a line or a filler of each pane.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Scroll {
    pub top_row: u64,
    pub first_col: u64,
}

/// What a row shows of its line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RowKind {
    /// Unchanged.
    #[default]
    Same,
    /// A left line of a change: removed or replaced.
    Removed,
    /// A right line of a change: inserted or replacing.
    Inserted,
    /// No line: the other pane's side of the change is longer.
    Filler,
}

/// One text row of a pane.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Row<'a> {
    /// The line drawn; `None` for a filler row.
    pub line: Option<u64>,
    pub kind: RowKind,
    /// Sorted, disjoint changed bytes of the line's change (inner changes).
    pub inner: &'a [std::ops::Range<u64>],
}

impl Row<'_> {
    /// Unchanged rows of the lines from `top` on: a view without a diff.
    pub fn lines(top: u64, count: u32, line_count: u64) -> Vec<Row<'static>> {
        let end = (top + u64::from(count)).min(line_count);
        (top..end)
            .map(|line| Row {
                line: Some(line),
                kind: RowKind::Same,
                inner: &[],
            })
            .collect()
    }
}

/// A change in the overview ruler: pixel rows of its track and the side
/// whose half it fills.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mark {
    pub top: u32,
    pub height: u32,
    pub side: Side,
}

/// Text of the title strip and status line.
#[derive(Clone, Copy, Debug, Default)]
pub struct Chrome<'a> {
    pub title: &'a str,
    /// Color of the unsaved marker after the title, when shown.
    pub title_mark: Option<Color>,
    /// Right-aligned in the title strip; hidden when the title would reach it.
    pub app_name: &'a str,
    pub status_left: &'a str,
    pub status_right: &'a str,
    /// The status line shows a question awaiting an answer.
    pub prompt: bool,
}

/// The title of one pane in a two-pane window.
#[derive(Clone, Copy, Debug, Default)]
pub struct Header<'a> {
    /// Folder of the file with its trailing separator, drawn dim; dropped
    /// when the header is too narrow.
    pub dir: &'a str,
    pub name: &'a str,
    /// Color of the unsaved marker after the name, when shown.
    pub mark: Option<Color>,
    /// `RO` after the name and marker.
    pub read_only: bool,
    /// The pane has the keyboard focus: drawn bright.
    pub focused: bool,
}

/// Window chrome of a two-pane window: one header per pane, the divider,
/// the application name and the status line.
pub struct SplitFrame<'a> {
    /// Layouts of the left and the right pane.
    pub layouts: [&'a Layout; 2],
    pub cell: Cell,
    pub theme: &'a Theme,
    pub headers: [Header<'a>; 2],
    /// Right-aligned in the title strip when the right header leaves room.
    pub app_name: &'a str,
    pub status_left: &'a str,
    pub status_right: &'a str,
    pub prompt: bool,
    /// Copy arrows in the divider, by visible row.
    pub arrows: &'a [Arrows],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CaretStyle {
    #[default]
    Bar,
    /// Overtype mode.
    Underline,
    Hidden,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fill {
    pub rect: Rect,
    pub color: Color,
}

/// One character drawn with its cell's top-left corner at `(x, y)`; pixels
/// outside `clip` (its screen area) are not drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Glyph {
    pub x: i32,
    pub y: i32,
    pub ch: char,
    pub bold: bool,
    pub color: Color,
    pub clip: Rect,
}

/// Diagonal lines (`/`) across `rect`, `period` pixels apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hatch {
    pub rect: Rect,
    pub period: u32,
    pub color: Color,
}

/// Spacing of the filler hatch: a third of a row (at least 4 px), whatever
/// part of the row is drawn.
pub fn hatch_period(cell: Cell) -> u32 {
    (cell.height / 3).max(4)
}

#[derive(Clone, Debug, Default)]
pub struct DisplayList {
    pub fills: Vec<Fill>,
    /// Diagonal lines across their rectangles (filler rows), drawn after the
    /// fills.
    pub hatches: Vec<Hatch>,
    pub glyphs: Vec<Glyph>,
    /// Wavy lines filling their rectangles, drawn after the glyphs.
    pub waves: Vec<Fill>,
    /// Filled after the glyphs (carets).
    pub overlays: Vec<Fill>,
    /// Drawn above everything else (palette, panels).
    pub popup: Layer,
    /// Drawn last: tooltips.
    pub tooltip: Layer,
    /// Columns of the widest drawn row when [`Frame::measure`]; rows going
    /// past the view count at most (see [`max_cols`]).
    pub widest: u64,
}

impl DisplayList {
    pub fn clear(&mut self) {
        self.fills.clear();
        self.hatches.clear();
        self.glyphs.clear();
        self.waves.clear();
        self.overlays.clear();
        self.popup.clear();
        self.tooltip.clear();
        self.widest = 0;
    }
}

/// Builds display lists, reusing its buffers between frames.
#[derive(Default)]
pub struct ViewBuilder {
    line: Vec<u8>,
    /// Visible bytes of a long row past its scan end.
    tail: Vec<u8>,
    /// Line-number text, reused between rows and frames.
    number: String,
}

/// Inputs of one frame.
pub struct Frame<'a> {
    pub snapshot: &'a Snapshot,
    pub layout: &'a Layout,
    pub scroll: Scroll,
    /// The rows from `scroll.top_row` on (at most `layout.rows`).
    pub rows: &'a [Row<'a>],
    /// All rows of the view (the slider's extent).
    pub row_count: u64,
    /// Changes drawn in the scroll strip (the overview ruler).
    pub overview: &'a [Mark],
    pub cell: Cell,
    pub theme: &'a Theme,
    pub chrome: Chrome<'a>,
    pub options: &'a Options,
    /// Sorted, non-overlapping selections.
    pub selections: &'a [Selection],
    pub caret: CaretStyle,
    pub highlights: Highlights<'a>,
    /// No document: the body shows only this hint, centered (no line
    /// numbers, rows, selections or caret).
    pub empty: Option<&'a str>,
    /// The scroll slider is being dragged.
    pub slider_active: bool,
    /// Report the widest drawn row in [`DisplayList::widest`].
    pub measure: bool,
}

/// Search decorations and syntax colors of a frame.
#[derive(Clone, Debug, Default)]
pub struct Highlights<'a> {
    /// Sorted, non-overlapping matches (at least the visible ones).
    pub matches: &'a [std::ops::Range<u64>],
    /// Find-in-selection range.
    pub scope: Option<std::ops::Range<u64>>,
    /// Sorted, non-overlapping syntax styles; other text has the text color.
    pub syntax: &'a [SyntaxSpan],
    /// Offsets of matching brackets (the pair at the caret).
    pub brackets: &'a [u64],
    /// Sorted, non-overlapping misspelled words (wavy underlines).
    pub spelling: &'a [std::ops::Range<u64>],
}

/// Bytes drawn in a syntax color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyntaxSpan {
    pub start: u64,
    pub end: u64,
    /// Index into [`Theme::syntax`].
    pub style: u8,
}

impl ViewBuilder {
    /// Clears `out` and draws a one-pane window: title strip, status line
    /// and the pane of `frame`.
    pub fn build(&mut self, frame: &Frame<'_>, out: &mut DisplayList) {
        out.clear();
        let Frame {
            layout,
            cell,
            theme,
            chrome,
            ..
        } = *frame;
        draw_title(out, layout, cell, theme, &chrome);
        draw_status(
            out,
            layout.status,
            cell,
            theme,
            (chrome.status_left, chrome.status_right),
            chrome.prompt,
        );
        self.build_pane(frame, out);
    }

    /// Clears `out` and draws the chrome of a two-pane window: the headers,
    /// the divider, the application name and the status line. The panes
    /// follow with [`ViewBuilder::build_pane`].
    pub fn build_split_chrome(&mut self, frame: &SplitFrame<'_>, out: &mut DisplayList) {
        out.clear();
        let [left, right] = frame.layouts;
        let (cell, theme) = (frame.cell, frame.theme);
        let title = left.title;
        out.fills.push(Fill {
            rect: title,
            color: theme.title_background,
        });
        let div = left.divider;
        if div.w > 0 && div.h > 0 {
            out.fills.push(Fill {
                rect: div,
                color: theme.divider,
            });
            for x in [div.x, div.x + div.w as i32 - 1] {
                out.fills.push(Fill {
                    rect: Rect { x, w: 1, ..div },
                    color: theme.divider_line,
                });
            }
            let body = left.text;
            for arrows in frame.arrows {
                let y = body.y + (arrows.row * cell.height) as i32;
                let cells = [
                    (arrows.right, "\u{2192}", div.x),
                    (arrows.left, "\u{2190}", div.x + (2 * cell.width) as i32),
                ];
                for (shown, arrow, x) in cells {
                    if shown {
                        text_run(&mut out.glyphs, arrow, x, y, cell, theme.diff_arrow, div);
                    }
                }
            }
        }
        let y = title.y + (title.h.saturating_sub(cell.height) / 2) as i32;
        let left_area = Rect {
            x: left.gutter.x,
            y: title.y,
            w: left.pane().w,
            h: title.h,
        };
        let right_area = Rect {
            x: right.gutter.x,
            y: title.y,
            w: (title.x + title.w as i32 - right.gutter.x).max(0) as u32,
            h: title.h,
        };
        draw_header(out, &frame.headers[0], left_area, y, cell, theme);
        let end = draw_header(out, &frame.headers[1], right_area, y, cell, theme);
        // "│ Name " at the right end when the right header leaves room.
        let name_w = frame.app_name.width() as u32 * cell.width;
        let name_x = i64::from(title.x) + i64::from(title.w) - i64::from(name_w + cell.width);
        let bar_x = name_x - i64::from(2 * cell.width);
        if bar_x >= i64::from(end) + i64::from(cell.width) {
            text_run(
                &mut out.glyphs,
                "\u{2502}",
                bar_x as i32,
                y,
                cell,
                theme.line_number,
                title,
            );
            text_run_bold(
                &mut out.glyphs,
                frame.app_name,
                name_x as i32,
                y,
                cell,
                theme.unsaved,
                title,
            );
        }
        draw_status(
            out,
            left.status,
            cell,
            theme,
            (frame.status_left, frame.status_right),
            frame.prompt,
        );
    }

    /// Adds the pane of `frame` (gutter, text, scroll strip; no title strip
    /// or status line) to `out`.
    pub fn build_pane(&mut self, frame: &Frame<'_>, out: &mut DisplayList) {
        let Frame {
            snapshot,
            layout,
            scroll,
            rows,
            row_count,
            overview,
            cell,
            theme,
            chrome: _,
            options,
            selections: _,
            caret,
            highlights: _,
            empty,
            slider_active,
            measure,
        } = *frame;
        let fill = |out: &mut DisplayList, rect: Rect, color| out.fills.push(Fill { rect, color });
        fill(out, layout.gutter, theme.background);
        fill(out, layout.text, theme.background);
        let track = layout.scrollbar;
        if track.w > 0 {
            fill(out, track, theme.background);
        }
        if empty.is_none() {
            let page = page_rows(layout, cell);
            if let Some(rect) = slider(layout, row_count, scroll.top_row, page, cell.height) {
                let color = if slider_active {
                    theme.scroll_slider_active
                } else {
                    theme.scroll_slider
                };
                fill(out, rect, color);
            }
            // Over the slider, so the changes it covers stay visible.
            let half = track.w / 2;
            for mark in overview {
                let (x, w, color) = match mark.side {
                    Side::Left => (track.x, half, theme.diff_removed_sign),
                    Side::Right => (
                        track.x + half as i32,
                        track.w - half,
                        theme.diff_inserted_sign,
                    ),
                };
                let rect = Rect {
                    x,
                    y: track.y + mark.top as i32,
                    w,
                    h: mark.height,
                };
                if let Some(rect) = rect.intersect(&track) {
                    fill(out, rect, color);
                }
            }
        }

        if let Some(hint) = empty {
            let text = layout.text;
            let cols = hint.width() as u32;
            let x = text.x + (text.w.saturating_sub(cols * cell.width) / 2) as i32;
            let y = text.y + (text.h.saturating_sub(cell.height) / 2) as i32;
            text_run(&mut out.glyphs, hint, x, y, cell, theme.popup_dim, text);
            return;
        }

        for (i, view_row) in rows.iter().take(layout.rows as usize).enumerate() {
            let y = layout.text.y + (i as u32 * cell.height) as i32;
            let row_rect = Rect {
                x: layout.text.x,
                y,
                w: layout.text.w,
                h: cell.height,
            };
            let (line_color, inner_color, sign) = match view_row.kind {
                RowKind::Same => (None, theme.background, None),
                RowKind::Removed => (
                    Some(theme.diff_removed_line),
                    theme.diff_removed_text,
                    Some(('-', theme.diff_removed_sign)),
                ),
                RowKind::Inserted => (
                    Some(theme.diff_inserted_line),
                    theme.diff_inserted_text,
                    Some(('+', theme.diff_inserted_sign)),
                ),
                // Decorations stay in the pane's body: a partial last row
                // must not cover the status line.
                RowKind::Filler => {
                    if let Some(rect) = row_rect.intersect(&layout.text) {
                        out.hatches.push(Hatch {
                            rect,
                            period: hatch_period(cell),
                            color: theme.diff_filler,
                        });
                    }
                    continue;
                }
            };
            if let Some(color) = line_color {
                let gutter_row = Rect {
                    y,
                    h: cell.height,
                    ..layout.gutter
                };
                for rect in [
                    gutter_row.intersect(&layout.gutter),
                    row_rect.intersect(&layout.text),
                ]
                .into_iter()
                .flatten()
                {
                    fill(out, rect, color);
                }
            }
            if let Some((ch, color)) = sign {
                // The first sign column; a whole window uses the last column.
                let cols = layout.signs.max(1);
                let x = layout.gutter.x + layout.gutter.w as i32 - (cols * cell.width) as i32;
                let mut buf = [0u8; 4];
                text_run(
                    &mut out.glyphs,
                    ch.encode_utf8(&mut buf),
                    x,
                    y,
                    cell,
                    color,
                    layout.gutter,
                );
            }
            // A line past the end: the text changed since the diff.
            let Some(line) = view_row.line else {
                continue;
            };
            let Some(range) = snapshot.line_range(line) else {
                continue;
            };
            if options.line_numbers {
                let number = &mut self.number;
                number.clear();
                let _ = write!(number, "{}", line + 1);
                let pad = (1 + layout.signs) * cell.width;
                let right = layout.gutter.x + layout.gutter.w as i32 - pad as i32;
                let x = right - number.len() as i32 * cell.width as i32;
                text_run(
                    &mut out.glyphs,
                    number,
                    x,
                    y,
                    cell,
                    theme.line_number,
                    layout.gutter,
                );
            }

            let end = range.end.min(range.start + ROW_SCAN_LIMIT as u64);
            // Rows inside one leaf are borrowed; others are joined into the reused buffer.
            let mut chunks = snapshot.chunks(range.start..end);
            let first = chunks.next().unwrap_or(&[]);
            let mut bytes: &[u8] = match chunks.next() {
                None => first,
                Some(second) => {
                    self.line.clear();
                    self.line.extend_from_slice(first);
                    self.line.extend_from_slice(second);
                    for chunk in chunks {
                        self.line.extend_from_slice(chunk);
                    }
                    &self.line
                }
            };
            // CRLF displays as one line break; a CR not followed by LF stays visible.
            let crlf = range.end < snapshot.len();
            let long = end < range.end;
            let mut content_end = range.end;
            if long {
                content_end = content_range(snapshot, line).end;
            } else if crlf && bytes.last() == Some(&b'\r') {
                bytes = &bytes[..bytes.len() - 1];
                content_end -= 1;
            }
            let row_rect = Rect {
                x: layout.text.x,
                y,
                w: layout.text.w,
                h: cell.height,
            };
            let content = range.start..content_end;
            let row_ranges = RowRanges {
                row: row_rect,
                bytes,
                content: content.clone(),
                has_break: crlf,
            };
            if line_color.is_some() {
                row_ranges.fill(out, frame, view_row.inner, inner_color);
            }
            if let Some(scope) = &frame.highlights.scope {
                row_ranges.fill(out, frame, std::slice::from_ref(scope), theme.search_range);
            }
            selection_row(out, frame, row_rect, bytes, content.clone(), crlf);
            row_ranges.matches(out, frame);
            row_ranges.waves(out, frame);
            for &at in frame.highlights.brackets {
                if (content.start..content.end).contains(&at)
                    && let Some(rect) = row_ranges.rect(frame, &(at..at + 1))
                {
                    fill(out, rect, theme.bracket_match);
                }
            }
            let text_x = layout.text.x;
            let clip = layout.text;
            let syntax = frame.highlights.syntax;
            // Spans ending inside or after the row, advanced as offsets grow.
            let mut span = syntax.partition_point(|s| s.end <= range.start);
            let mut emit = |col: u32, ch, kind, offset: usize| {
                let color = match kind {
                    Kind::Text => {
                        let at = range.start + offset as u64;
                        while span < syntax.len() && syntax[span].end <= at {
                            span += 1;
                        }
                        match syntax.get(span) {
                            Some(s) if s.start <= at => theme.syntax[s.style as usize],
                            _ => theme.text,
                        }
                    }
                    Kind::Control => theme.control,
                };
                out.glyphs.push(Glyph {
                    x: text_x + (col * cell.width) as i32,
                    y,
                    ch,
                    bold: false,
                    color,
                    clip,
                });
            };
            // A long line is drawn exactly up to its scan end, then one
            // column per byte (the rule of carets, clicks and selections).
            let seam = if long { scan_end(bytes) } else { bytes.len() };
            let cols = layout.cols + 1;
            let reached = layout_line(
                &bytes[..seam],
                scroll.first_col,
                cols,
                options.tab_width,
                &mut emit,
            );
            if long
                && let Some(LineEnd {
                    col: seam_col,
                    last,
                }) = reached
            {
                let end_col = scroll.first_col + u64::from(cols);
                if end_col > seam_col {
                    let base = range.start + seam as u64;
                    let from = base + scroll.first_col.saturating_sub(seam_col).saturating_sub(3);
                    let to = (base + (end_col - seam_col) + 3).min(content_end);
                    if from < to {
                        self.tail.clear();
                        for chunk in snapshot.chunks(from..to) {
                            self.tail.extend_from_slice(chunk);
                        }
                        layout_beyond(
                            &self.tail,
                            (from - range.start) as usize,
                            (seam, seam_col),
                            scroll.first_col,
                            cols,
                            last,
                            &mut emit,
                        );
                    }
                }
            }
            // Past the scan end each byte is a column (the long-line rule).
            if measure {
                let beyond = if long {
                    content_end - range.start - seam as u64
                } else {
                    0
                };
                let width = match reached {
                    Some(end) => end.col + beyond,
                    None => (max_cols(&bytes[..seam], options.tab_width) + beyond)
                        .max(scroll.first_col + u64::from(cols)),
                };
                out.widest = out.widest.max(width);
            }
            if caret != CaretStyle::Hidden {
                caret_row(out, frame, row_rect, bytes, content);
            }
        }
    }
}

/// The title strip of a one-pane window: the title, its unsaved marker and
/// the application name at the right end when the title leaves room.
fn draw_title(
    out: &mut DisplayList,
    layout: &Layout,
    cell: Cell,
    theme: &Theme,
    chrome: &Chrome<'_>,
) {
    out.fills.push(Fill {
        rect: layout.title,
        color: theme.title_background,
    });
    let title_y = layout.title.y + (layout.title.h.saturating_sub(cell.height) / 2) as i32;
    let title_cols = text_run(
        &mut out.glyphs,
        chrome.title,
        cell.width as i32,
        title_y,
        cell,
        theme.title_text,
        layout.title,
    );
    if let Some(color) = chrome.title_mark {
        let x = (cell.width * (title_cols + 2)) as i32;
        text_run(
            &mut out.glyphs,
            "\u{25CF}",
            x,
            title_y,
            cell,
            color,
            layout.title,
        );
    }
    // Title columns: margin, title, then a gap and the marker when shown.
    let title_end = cell.width * (1 + title_cols + if chrome.title_mark.is_some() { 2 } else { 0 });
    let name_w = chrome.app_name.width() as u32 * cell.width;
    let name_x = layout.title.w as i64 - i64::from(name_w + cell.width);
    // "│ Name ": the divider ends the area of the title.
    let divider_x = name_x - i64::from(2 * cell.width);
    if divider_x >= i64::from(title_end + cell.width) {
        text_run(
            &mut out.glyphs,
            "\u{2502}",
            divider_x as i32,
            title_y,
            cell,
            theme.line_number,
            layout.title,
        );
        text_run_bold(
            &mut out.glyphs,
            chrome.app_name,
            name_x as i32,
            title_y,
            cell,
            theme.unsaved,
            layout.title,
        );
    }
}

/// The status line: `texts` left- and right-aligned, in the prompt colors
/// while a question waits for an answer.
fn draw_status(
    out: &mut DisplayList,
    status: Rect,
    cell: Cell,
    theme: &Theme,
    texts: (&str, &str),
    prompt: bool,
) {
    let (background, color) = if prompt {
        (theme.prompt_background, theme.prompt_text)
    } else {
        (theme.status_background, theme.status_text)
    };
    out.fills.push(Fill {
        rect: status,
        color: background,
    });
    let y = status.y + (status.h.saturating_sub(cell.height) / 2) as i32;
    text_run(
        &mut out.glyphs,
        texts.0,
        status.x + cell.width as i32,
        y,
        cell,
        color,
        status,
    );
    let right_w = texts.1.chars().count() as i32 * cell.width as i32;
    let right_x = status.x + status.w as i32 - right_w - cell.width as i32;
    text_run(&mut out.glyphs, texts.1, right_x, y, cell, color, status);
}

/// One pane's header in `area` (clipped to it): the folder dim, the name, the
/// unsaved marker and `RO`; the folder is dropped when they do not fit.
/// Returns the x after its last cell.
fn draw_header(
    out: &mut DisplayList,
    header: &Header<'_>,
    area: Rect,
    y: i32,
    cell: Cell,
    theme: &Theme,
) -> i32 {
    let (dir_color, name_color) = if header.focused {
        (theme.status_text, theme.text)
    } else {
        (theme.control, theme.popup_dim)
    };
    let tail = if header.mark.is_some() { 2 } else { 0 } + if header.read_only { 3 } else { 0 };
    // Columns between the left and the right margin.
    let room = (area.w / cell.width.max(1)).saturating_sub(2);
    let full = (header.dir.width() + header.name.width()) as u32 + tail;
    let dir = if full <= room { header.dir } else { "" };
    let cw = cell.width as i32;
    let mut x = area.x + cw;
    x += text_run(&mut out.glyphs, dir, x, y, cell, dir_color, area) as i32 * cw;
    x += text_run(&mut out.glyphs, header.name, x, y, cell, name_color, area) as i32 * cw;
    if let Some(color) = header.mark {
        text_run(&mut out.glyphs, "\u{25CF}", x + cw, y, cell, color, area);
        x += 2 * cw;
    }
    if header.read_only {
        text_run(&mut out.glyphs, "RO", x + cw, y, cell, theme.warning, area);
        x += 3 * cw;
    }
    x
}

/// Columns of increasing offsets of one row: each walks on from the last
/// (an earlier offset restarts at the row start), so sorted ranges cost one
/// pass over the row.
struct ColumnWalk<'b> {
    bytes: &'b [u8],
    tab: u32,
    at: (usize, u64),
}

impl<'b> ColumnWalk<'b> {
    fn new(bytes: &'b [u8], tab: u32) -> ColumnWalk<'b> {
        ColumnWalk {
            bytes,
            tab,
            at: (0, 0),
        }
    }

    fn column(&mut self, offset: usize) -> u64 {
        if offset < self.at.0 {
            self.at = (0, 0);
        }
        self.at = (offset, column_after(self.bytes, self.at, offset, self.tab));
        self.at.1
    }
}

/// One visible row for drawing range fills.
struct RowRanges<'b> {
    row: Rect,
    bytes: &'b [u8],
    content: std::ops::Range<u64>,
    has_break: bool,
}

impl RowRanges<'_> {
    /// Screen rectangle of `range` on this row, if visible.
    fn rect(&self, frame: &Frame<'_>, range: &std::ops::Range<u64>) -> Option<Rect> {
        let content = &self.content;
        if range.start > content.end || range.end < content.start || range.is_empty() {
            return None;
        }
        let tab = frame.options.tab_width;
        let column = |offset: u64| column_of(self.bytes, (offset - content.start) as usize, tab);
        let start = column(range.start.max(content.start));
        let end = column(range.end.min(content.end));
        self.rect_cols(frame, range, start, end)
    }

    /// Rectangle of `range` given the columns of its visible part.
    fn rect_cols(
        &self,
        frame: &Frame<'_>,
        range: &std::ops::Range<u64>,
        start: u64,
        mut end: u64,
    ) -> Option<Rect> {
        if range.end > self.content.end && self.has_break {
            end += 1;
        }
        let first_col = frame.scroll.first_col;
        if end <= first_col || end <= start {
            return None;
        }
        let max = u64::from(frame.layout.cols) + 2;
        let s = start.saturating_sub(first_col).min(max) as u32;
        let e = (end - first_col).min(max) as u32;
        let cell_w = frame.cell.width;
        let rect = Rect {
            x: self.row.x + (s * cell_w) as i32,
            y: self.row.y,
            w: (e - s) * cell_w,
            h: self.row.h,
        };
        rect.intersect(&frame.layout.text)
    }

    /// Fills the parts of sorted `ranges` on this row.
    fn fill(
        &self,
        out: &mut DisplayList,
        frame: &Frame<'_>,
        ranges: &[std::ops::Range<u64>],
        color: Color,
    ) {
        let first = ranges.partition_point(|r| r.end < self.content.start);
        for range in &ranges[first..] {
            if range.start > self.content.end {
                break;
            }
            if let Some(rect) = self.rect(frame, range) {
                out.fills.push(Fill { rect, color });
            }
        }
    }

    /// Wavy underlines of the misspelled words on this row; past the scanned
    /// part of a long row every byte takes one column, as for selections.
    fn waves(&self, out: &mut DisplayList, frame: &Frame<'_>) {
        let words = frame.highlights.spelling;
        if words.is_empty() {
            return;
        }
        let content = &self.content;
        let h = (frame.cell.height / 6).max(3).min(self.row.h);
        let last_col = frame.scroll.first_col + u64::from(frame.layout.cols) + 2;
        let mut walk = ColumnWalk::new(self.bytes, frame.options.tab_width);
        let mut column = |offset: u64| walk.column((offset - content.start) as usize);
        let first = words.partition_point(|r| r.end <= content.start);
        for word in &words[first..] {
            if word.start >= content.end {
                break;
            }
            let start = column(word.start.max(content.start));
            if start > last_col {
                break;
            }
            let end = column(word.end.min(content.end));
            if let Some(rect) = self.rect_cols(frame, word, start, end) {
                out.waves.push(Fill {
                    rect: Rect {
                        y: self.row.y + (self.row.h - h) as i32,
                        h,
                        ..rect
                    },
                    color: frame.theme.spelling,
                });
            }
        }
    }

    /// Match fills; a match equal to a selection is the current one and
    /// gets a border. Columns are computed in one pass over the row.
    fn matches(&self, out: &mut DisplayList, frame: &Frame<'_>) {
        let theme = frame.theme;
        let matches = frame.highlights.matches;
        let content = &self.content;
        let last_col = frame.scroll.first_col + u64::from(frame.layout.cols) + 2;
        let mut walk = ColumnWalk::new(self.bytes, frame.options.tab_width);
        let mut column = |offset: u64| walk.column((offset - content.start) as usize);
        let first = matches.partition_point(|r| r.end < content.start);
        for range in &matches[first..] {
            if range.start > content.end {
                break;
            }
            if range.end < content.start || range.is_empty() {
                continue;
            }
            let start = column(range.start.max(content.start));
            if start > last_col {
                break;
            }
            let end = column(range.end.min(content.end));
            let Some(rect) = self.rect_cols(frame, range, start, end) else {
                continue;
            };
            let current = frame
                .selections
                .binary_search_by_key(&(range.start, range.end), |s| (s.start(), s.end()))
                .is_ok();
            if !current {
                out.fills.push(Fill {
                    rect,
                    color: theme.search_match,
                });
                continue;
            }
            let border = theme.search_current_border;
            out.fills.push(Fill {
                rect,
                color: border,
            });
            let inner = Rect {
                x: rect.x + 1,
                y: rect.y + 1,
                w: rect.w.saturating_sub(2),
                h: rect.h.saturating_sub(2),
            };
            out.fills.push(Fill {
                rect: inner,
                color: theme.search_current,
            });
        }
    }
}

/// Selection fills of one row (`bytes`: its scanned content); a selected
/// line break adds one cell. Sorted selections take one column walk and stop
/// past the right edge.
fn selection_row(
    out: &mut DisplayList,
    frame: &Frame<'_>,
    row: Rect,
    bytes: &[u8],
    content: std::ops::Range<u64>,
    has_break: bool,
) {
    let sels = frame.selections;
    let first = sels.partition_point(|s| s.end() < content.start);
    let cell_w = frame.cell.width;
    let first_col = frame.scroll.first_col;
    let max = u64::from(frame.layout.cols) + 2;
    let mut walk = ColumnWalk::new(bytes, frame.options.tab_width);
    let mut column = |offset: u64| walk.column((offset - content.start) as usize);
    for sel in &sels[first..] {
        if sel.start() > content.end {
            break;
        }
        if sel.is_empty() {
            continue;
        }
        let start = column(sel.start().max(content.start));
        if start > first_col + max {
            break;
        }
        let mut end = column(sel.end().min(content.end));
        if sel.end() > content.end && has_break {
            end += 1;
        }
        if end <= first_col || end <= start {
            continue;
        }
        let s = start.saturating_sub(first_col).min(max) as u32;
        let e = (end - first_col).min(max) as u32;
        let rect = Rect {
            x: row.x + (s * cell_w) as i32,
            y: row.y,
            w: (e - s) * cell_w,
            h: row.h,
        };
        if let Some(rect) = rect.intersect(&frame.layout.text) {
            out.fills.push(Fill {
                rect,
                color: frame.theme.selection,
            });
        }
    }
}

fn caret_row(
    out: &mut DisplayList,
    frame: &Frame<'_>,
    row: Rect,
    bytes: &[u8],
    content: std::ops::Range<u64>,
) {
    let sels = frame.selections;
    let cell = frame.cell;
    let thickness = (cell.height / 10).max(1);
    let first = sels.partition_point(|s| s.end() < content.start);
    // Heads of sorted, disjoint selections increase: one column walk.
    let mut walk = ColumnWalk::new(bytes, frame.options.tab_width);
    for sel in &sels[first..] {
        if sel.start() > content.end {
            break;
        }
        if sel.head < content.start || sel.head > content.end {
            continue;
        }
        let col = walk.column((sel.head - content.start) as usize);
        let Some(visible) = col.checked_sub(frame.scroll.first_col) else {
            continue;
        };
        if visible > u64::from(frame.layout.cols) + 1 {
            break;
        }
        let x = row.x + (visible as u32 * cell.width) as i32;
        let rect = match frame.caret {
            CaretStyle::Underline => Rect {
                x,
                y: row.y + (cell.height - thickness) as i32,
                w: cell.width,
                h: thickness,
            },
            _ => Rect {
                x,
                y: row.y,
                w: thickness,
                h: cell.height,
            },
        };
        if let Some(rect) = rect.intersect(&frame.layout.text) {
            out.overlays.push(Fill {
                rect,
                color: frame.theme.caret,
            });
        }
    }
}

/// The offset a click at window column `x` on `line` lands on: the nearest
/// character boundary, at most the line's end.
pub fn hit_line(
    snapshot: &Snapshot,
    layout: &Layout,
    first_col: u64,
    cell: Cell,
    options: &Options,
    line: u64,
    x: i32,
) -> u64 {
    let dx = (x - layout.text.x).max(0) as u64;
    let col = first_col + (dx + u64::from(cell.width) / 2) / u64::from(cell.width.max(1));
    offset_in_line(
        snapshot,
        content_range(snapshot, line),
        col,
        options.tab_width,
    )
}

/// Draws `text` from `(x, y)` and returns the cells it occupies.
fn text_run(
    out: &mut Vec<Glyph>,
    text: &str,
    x: i32,
    y: i32,
    cell: Cell,
    color: Color,
    clip: Rect,
) -> u32 {
    let mut col = 0;
    for ch in text.chars() {
        let width = ch.width().unwrap_or(1) as u32;
        let gx = x + (col * cell.width) as i32;
        if gx >= clip.x
            && gx + (width * cell.width) as i32 <= clip.x + clip.w as i32
            && !ch.is_whitespace()
        {
            out.push(Glyph {
                x: gx,
                y,
                ch,
                bold: false,
                color,
                clip,
            });
        }
        col += width;
    }
    col
}

/// [`text_run`] in bold type.
fn text_run_bold(
    out: &mut Vec<Glyph>,
    text: &str,
    x: i32,
    y: i32,
    cell: Cell,
    color: Color,
    clip: Rect,
) -> u32 {
    let first = out.len();
    let cols = text_run(out, text, x, y, cell, color, clip);
    for glyph in &mut out[first..] {
        glyph.bold = true;
    }
    cols
}

/// Columns `bytes` take at most, without laying them out: two for a
/// character of three or four bytes, one for other characters and invalid
/// bytes, a full tab stop for a tab. Exact for ASCII without tabs inside the
/// line, for most scripts and for wide CJK.
fn max_cols(bytes: &[u8], tab_width: u32) -> u64 {
    let tab = tab_width.max(1) as u64 - 1;
    let mut cols = 0;
    for chunk in bytes.utf8_chunks() {
        cols += chunk
            .valid()
            .bytes()
            .map(|b| {
                u64::from((b as i8) >= -0x40) + u64::from(b >= 0xE0) + u64::from(b == b'\t') * tab
            })
            .sum::<u64>();
        cols += chunk.invalid().len() as u64;
    }
    cols
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Text,
    /// Control character or invalid byte, drawn as a symbol.
    Control,
}

/// Where [`layout_line`] ended after all of its bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineEnd {
    /// Column after the last byte.
    pub col: u64,
    /// Visible column of the last drawn character and the marks drawn on
    /// it; zero-width marks that follow join that cell.
    pub last: Option<(u32, u32)>,
}

/// Lays out one line, calling `emit(visible_col, char, kind, offset)` for
/// every character visible in `[first_col, first_col + cols)`; `offset` is
/// the character's byte offset in `bytes`. Returns where it ended when
/// every byte was laid out (`None` when the visible columns ended first).
pub fn layout_line(
    bytes: &[u8],
    first_col: u64,
    cols: u32,
    tab_width: u32,
    mut emit: impl FnMut(u32, char, Kind, usize),
) -> Option<LineEnd> {
    let end_col = first_col + u64::from(cols);
    let tab = u64::from(tab_width.max(1));
    let mut col = 0u64;
    // Visible column of the previous character and the marks drawn on it.
    let mut last_visible: Option<(u32, u32)> = None;
    let mut place = |col: &mut u64,
                     last: &mut Option<(u32, u32)>,
                     ch: char,
                     width: u64,
                     kind: Kind,
                     offset: usize| {
        if width == 0 {
            if let Some((v, marks)) = last
                && *marks < MAX_MARKS_PER_CELL
            {
                *marks += 1;
                emit(*v, ch, kind, offset);
            }
            return;
        }
        *last = None;
        if *col >= first_col && *col + width <= end_col {
            let v = (*col - first_col) as u32;
            emit(v, ch, kind, offset);
            *last = Some((v, 0));
        }
        *col += width;
    };
    let mut offset = 0;
    for chunk in bytes.utf8_chunks() {
        for ch in chunk.valid().chars() {
            if col >= end_col {
                return None;
            }
            let at = offset;
            offset += ch.len_utf8();
            let last = &mut last_visible;
            match ch {
                '\t' => {
                    col = (col / tab + 1) * tab;
                    *last = None;
                }
                '\0'..='\u{1F}' => {
                    let symbol = char::from_u32(0x2400 + ch as u32).unwrap();
                    place(&mut col, last, symbol, 1, Kind::Control, at);
                }
                '\u{7F}' => place(&mut col, last, '\u{2421}', 1, Kind::Control, at),
                _ => place(
                    &mut col,
                    last,
                    ch,
                    ch.width().unwrap_or(1) as u64,
                    Kind::Text,
                    at,
                ),
            }
        }
        for _ in chunk.invalid() {
            if col >= end_col {
                return None;
            }
            let at = offset;
            offset += 1;
            place(
                &mut col,
                &mut last_visible,
                '\u{FFFD}',
                1,
                Kind::Control,
                at,
            );
        }
    }
    Some(LineEnd {
        col,
        last: last_visible,
    })
}

/// Lays out bytes of a long line past its scan end (`seam`, exact column
/// `seam_col`), where each byte takes one column: a character is drawn once
/// at the column of its first byte. `window` holds the line's bytes from
/// offset `start` (at or after `seam`); `emit` gets offsets from the line
/// start. Characters starting before `first_col` are skipped, not shifted;
/// zero-width marks join the previous drawn cell, which is `last` (from
/// [`LineEnd`]) for marks right after the seam.
pub fn layout_beyond(
    window: &[u8],
    start: usize,
    seam: (usize, u64),
    first_col: u64,
    cols: u32,
    last: Option<(u32, u32)>,
    mut emit: impl FnMut(u32, char, Kind, usize),
) {
    let end_col = first_col + u64::from(cols);
    // The cell before the seam only takes marks that start right at it.
    let mut last = if start == seam.0 { last } else { None };
    let mut offset = start;
    for chunk in window.utf8_chunks() {
        let valid = chunk.valid().chars().map(|ch| (ch, ch.len_utf8()));
        let invalid = chunk.invalid().iter().map(|_| ('\u{FFFD}', 1));
        for (ch, len) in valid.chain(invalid) {
            let at = offset;
            offset += len;
            let col = seam.1 + (at - seam.0) as u64;
            if col >= end_col {
                return;
            }
            let (ch, kind) = match ch {
                '\t' => {
                    last = None;
                    continue;
                }
                '\0'..='\u{1F}' => (char::from_u32(0x2400 + ch as u32).unwrap(), Kind::Control),
                '\u{7F}' => ('\u{2421}', Kind::Control),
                '\u{FFFD}' if len == 1 => (ch, Kind::Control),
                _ => (ch, Kind::Text),
            };
            if kind == Kind::Text && ch.width() == Some(0) {
                if let Some((v, marks)) = &mut last
                    && *marks < MAX_MARKS_PER_CELL
                {
                    *marks += 1;
                    emit(*v, ch, kind, at);
                }
                continue;
            }
            last = None;
            if col >= first_col {
                let v = (col - first_col) as u32;
                emit(v, ch, kind, at);
                last = Some((v, 0));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cells(bytes: &[u8], first_col: u64, cols: u32) -> Vec<(u32, char, Kind)> {
        let mut out = Vec::new();
        layout_line(bytes, first_col, cols, 4, |c, ch, k, _| {
            out.push((c, ch, k))
        });
        out
    }

    #[test]
    fn ascii_and_tabs() {
        let got = cells(b"a\tb", 0, 10);
        assert_eq!(got, vec![(0, 'a', Kind::Text), (4, 'b', Kind::Text)]);
        let got = cells(b"ab\tc", 0, 10);
        assert_eq!(
            got,
            vec![
                (0, 'a', Kind::Text),
                (1, 'b', Kind::Text),
                (4, 'c', Kind::Text)
            ]
        );
    }

    #[test]
    fn horizontal_scroll_and_clip() {
        let got = cells(b"abcdef", 2, 3);
        assert_eq!(
            got,
            vec![
                (0, 'c', Kind::Text),
                (1, 'd', Kind::Text),
                (2, 'e', Kind::Text)
            ]
        );
    }

    #[test]
    fn wide_combining_control_and_invalid() {
        let got = cells("中e\u{301}\u{1}".as_bytes(), 0, 10);
        assert_eq!(
            got,
            vec![
                (0, '中', Kind::Text),
                (2, 'e', Kind::Text),
                (2, '\u{301}', Kind::Text),
                (3, '\u{2401}', Kind::Control),
            ]
        );
        let got = cells(b"a\xFFb", 0, 10);
        assert_eq!(
            got,
            vec![
                (0, 'a', Kind::Text),
                (1, '\u{FFFD}', Kind::Control),
                (2, 'b', Kind::Text)
            ]
        );
    }

    #[test]
    fn wide_char_cut_by_left_edge_is_hidden() {
        let got = cells("中a".as_bytes(), 1, 5);
        assert_eq!(got, vec![(1, 'a', Kind::Text)]);
    }

    #[test]
    fn layout_areas() {
        let cell = Cell {
            width: 10,
            height: 20,
        };
        let l = Layout::new(800, 600, cell, 12345, &Options::default());
        assert_eq!(l.title.h, 30);
        assert_eq!(l.status.y, 570);
        assert_eq!(l.gutter.w, 70);
        assert_eq!(l.text.x, 70);
        assert_eq!(l.text.h, 540);
        assert_eq!(l.rows, 27);
        assert_eq!(l.cols, 73);
    }

    #[test]
    fn split_layouts_share_the_window_beside_the_divider() {
        let cell = Cell {
            width: 10,
            height: 20,
        };
        let side = |pane| {
            let options = Options {
                scroll_strip: 6,
                pane: Some(pane),
                ..Options::default()
            };
            Layout::new(400, 300, cell, 10, &options)
        };
        let (left, right) = (side(Side::Left), side(Side::Right));
        // 400 = 185 + 30 (divider) + 185.
        assert_eq!((left.divider.x, left.divider.w), (185, 30));
        assert_eq!(left.divider, right.divider);
        assert_eq!((left.gutter.x, left.pane().w), (0, 185));
        assert_eq!((right.gutter.x, right.pane().w), (215, 185));
        assert_eq!(left.scrollbar.x + left.scrollbar.w as i32, 185);
        assert_eq!(right.text.x, 215 + right.gutter.w as i32);
        // Numbers, a space, the change sign and a space before the text.
        assert_eq!(left.gutter.w, (2 + 2 + SIGN_COLS) * 10);
        assert_eq!((left.title, left.status), (right.title, right.status));
        let whole = Layout::new(400, 300, cell, 10, &Options::default());
        assert_eq!((whole.divider.w, whole.pane().w), (0, 400));
    }

    /// Characters drawn in the title strip between `from` and `to`, by x.
    fn title_text(list: &DisplayList, title: Rect, from: i32, to: i32) -> String {
        let mut glyphs: Vec<&Glyph> = list
            .glyphs
            .iter()
            .filter(|g| title.contains(g.x, g.y) && (from..to).contains(&g.x))
            .collect();
        glyphs.sort_by_key(|g| g.x);
        glyphs.iter().map(|g| g.ch).collect()
    }

    fn split_chrome(width: u32, headers: [Header<'_>; 2]) -> ([Layout; 2], DisplayList) {
        let cell = Cell {
            width: 10,
            height: 20,
        };
        let side = |pane| {
            let options = Options {
                pane: Some(pane),
                ..Options::default()
            };
            Layout::new(width, 200, cell, 10, &options)
        };
        let layouts = [side(Side::Left), side(Side::Right)];
        let mut list = DisplayList::default();
        ViewBuilder::default().build_split_chrome(
            &SplitFrame {
                layouts: [&layouts[0], &layouts[1]],
                cell,
                theme: &Theme::DARK,
                headers,
                app_name: "App",
                status_left: "left",
                status_right: "right",
                prompt: false,
                arrows: &[],
            },
            &mut list,
        );
        (layouts, list)
    }

    #[test]
    fn split_chrome_draws_a_header_per_pane() {
        let theme = Theme::DARK;
        let headers = [
            Header {
                dir: r"C:\a\",
                name: "l.rs",
                mark: Some(theme.warning),
                read_only: true,
                focused: false,
            },
            Header {
                name: "y.rs",
                mark: Some(theme.unsaved),
                focused: true,
                ..Header::default()
            },
        ];
        let ([left, right], list) = split_chrome(600, headers);
        let title = left.title;
        let divider = left.divider;
        assert_eq!(
            title_text(&list, title, 0, divider.x),
            r"C:\a\l.rs●RO",
            "the folder, the name, the marker and RO"
        );
        assert_eq!(title_text(&list, title, right.gutter.x, 600), "y.rs●│App");
        let color = |ch| list.glyphs.iter().find(|g| g.ch == ch).map(|g| g.color);
        assert_eq!(color('l'), Some(theme.popup_dim), "unfocused names are dim");
        assert_eq!(color('y'), Some(theme.text), "the focused name is bright");
        assert_eq!(color('R'), Some(theme.warning));
        let marks: Vec<u32> = list
            .glyphs
            .iter()
            .filter(|g| g.ch == '\u{25CF}')
            .map(|g| g.color)
            .collect();
        assert_eq!(marks, vec![theme.warning, theme.unsaved]);
        assert!(list.fills.contains(&Fill {
            rect: divider,
            color: theme.divider,
        }));
        assert_eq!(title_text(&list, left.status, 0, 600), "leftright");

        // A narrow header drops the folder, then the application name goes.
        let ([left, right], list) = split_chrome(200, headers);
        let narrow = title_text(&list, left.title, 0, left.divider.x);
        assert!(narrow.starts_with("l.rs●"), "{narrow}");
        assert_eq!(title_text(&list, left.title, right.gutter.x, 200), "y.rs●");
    }

    fn build(bytes: &[u8], width: u32, height: u32) -> (Layout, DisplayList) {
        build_with(bytes, width, height, &[], CaretStyle::Hidden)
    }

    fn build_with(
        bytes: &[u8],
        width: u32,
        height: u32,
        selections: &[Selection],
        caret: CaretStyle,
    ) -> (Layout, DisplayList) {
        build_full(
            bytes,
            width,
            height,
            selections,
            caret,
            Highlights::default(),
        )
    }

    fn build_full(
        bytes: &[u8],
        width: u32,
        height: u32,
        selections: &[Selection],
        caret: CaretStyle,
        highlights: Highlights<'_>,
    ) -> (Layout, DisplayList) {
        build_at(
            bytes,
            width,
            height,
            Scroll::default(),
            selections,
            caret,
            highlights,
        )
    }

    fn build_at(
        bytes: &[u8],
        width: u32,
        height: u32,
        scroll: Scroll,
        selections: &[Selection],
        caret: CaretStyle,
        highlights: Highlights<'_>,
    ) -> (Layout, DisplayList) {
        let snapshot = Snapshot::from_bytes(bytes);
        let cell = Cell {
            width: 10,
            height: 20,
        };
        let options = Options::default();
        let layout = Layout::new(width, height, cell, snapshot.line_count(), &options);
        let rows = Row::lines(scroll.top_row, layout.rows, snapshot.line_count());
        let mut list = DisplayList::default();
        ViewBuilder::default().build(
            &Frame {
                snapshot: &snapshot,
                layout: &layout,
                scroll,
                rows: &rows,
                row_count: snapshot.line_count(),
                overview: &[],
                cell,
                theme: &Theme::DARK,
                chrome: Chrome {
                    title: "title",
                    title_mark: Some(Theme::DARK.unsaved),
                    app_name: "App",
                    status_left: "left",
                    status_right: "right",
                    prompt: false,
                },
                options: &options,
                selections,
                caret,
                highlights,
                empty: None,
                slider_active: false,
                measure: true,
            },
            &mut list,
        );
        (layout, list)
    }

    fn body_text(list: &DisplayList, layout: &Layout) -> String {
        let clip = layout.text;
        list.glyphs
            .iter()
            .filter(|g| g.clip == clip)
            .map(|g| g.ch)
            .collect()
    }

    /// A misspelled word gets a wave at the bottom of its cells; the text
    /// keeps its color.
    #[test]
    fn misspelled_words_get_a_wave_under_their_cells() {
        let spelling = [4..10, 15..18];
        let highlights = Highlights {
            spelling: &spelling,
            ..Highlights::default()
        };
        let (layout, list) = build_full(
            b"the recive\nnext tex",
            400,
            200,
            &[],
            CaretStyle::Hidden,
            highlights,
        );
        let rects: Vec<Rect> = list.waves.iter().map(|w| w.rect).collect();
        let row = |r: u32, col: u32, cols: u32| Rect {
            x: layout.text.x + (col * 10) as i32,
            y: layout.text.y + (r * 20 + 17) as i32,
            w: cols * 10,
            h: 3,
        };
        assert_eq!(rects, [row(0, 4, 6), row(1, 4, 3)]);
        assert!(list.waves.iter().all(|w| w.color == Theme::DARK.spelling));
        assert!(list.glyphs.iter().all(|g| g.color != Theme::DARK.spelling));
    }

    #[test]
    fn misspelled_words_past_the_long_line_seam_get_a_wave() {
        let at = ROW_SCAN_LIMIT as u64 + 101;
        let text = format!("{} teh more", "x".repeat(ROW_SCAN_LIMIT + 100));
        let word = at..at + 3;
        let highlights = Highlights {
            spelling: std::slice::from_ref(&word),
            ..Highlights::default()
        };
        let scroll = Scroll {
            first_col: at - 5,
            ..Scroll::default()
        };
        let (layout, list) = build_at(
            text.as_bytes(),
            400,
            200,
            scroll,
            &[],
            CaretStyle::Hidden,
            highlights,
        );
        let x = layout.text.x + 50;
        let t = list
            .glyphs
            .iter()
            .find(|g| g.ch == 't' && g.clip == layout.text)
            .unwrap();
        assert_eq!(t.x, x, "the word is drawn at its column");
        let rects: Vec<Rect> = list.waves.iter().map(|w| w.rect).collect();
        assert_eq!(
            rects,
            [Rect {
                x,
                y: layout.text.y + 17,
                w: 30,
                h: 3
            }]
        );
    }

    #[test]
    fn empty_frames_show_only_the_hint_inside_the_text_area() {
        let cell = Cell {
            width: 10,
            height: 20,
        };
        let options = Options::default();
        for (width, height) in [(400, 200), (120, 70), (30, 50), (0, 0)] {
            let snapshot = Snapshot::empty();
            let layout = Layout::new(width, height, cell, 1, &options);
            let mut list = DisplayList::default();
            let sels = [Selection::caret(0)];
            ViewBuilder::default().build(
                &Frame {
                    snapshot: &snapshot,
                    layout: &layout,
                    scroll: Scroll::default(),
                    rows: &[],
                    row_count: 1,
                    overview: &[],
                    cell,
                    theme: &Theme::DARK,
                    chrome: Chrome::default(),
                    options: &options,
                    selections: &sels,
                    caret: CaretStyle::Bar,
                    highlights: Highlights::default(),
                    empty: Some("Drop a file"),
                    slider_active: false,
                    measure: true,
                },
                &mut list,
            );
            assert!(list.overlays.is_empty(), "no caret at {width}x{height}");
            let body: Vec<&Glyph> = list
                .glyphs
                .iter()
                .filter(|g| g.clip != layout.title && g.clip != layout.status)
                .collect();
            assert!(body.iter().all(|g| g.clip == layout.text));
            if width == 400 {
                let text: String = body.iter().map(|g| g.ch).collect();
                assert_eq!(text, "Dropafile", "no line number, only the hint");
                assert!(body.iter().all(|g| layout.text.contains(g.x, g.y)));
            }
        }
    }

    #[test]
    fn syntax_spans_color_their_characters() {
        let spans = [
            SyntaxSpan {
                start: 0,
                end: 2,
                style: 0,
            },
            SyntaxSpan {
                start: 7,
                end: 10,
                style: 1,
            },
        ];
        let highlights = Highlights {
            syntax: &spans,
            ..Highlights::default()
        };
        let (layout, list) = build_full(
            "fn é\n\t\"x\" y".as_bytes(),
            400,
            200,
            &[],
            CaretStyle::Hidden,
            highlights,
        );
        let theme = Theme::DARK;
        let colors: Vec<(char, Color)> = list
            .glyphs
            .iter()
            .filter(|g| g.clip == layout.text)
            .map(|g| (g.ch, g.color))
            .collect();
        assert_eq!(
            colors,
            [
                ('f', theme.syntax[0]),
                ('n', theme.syntax[0]),
                (' ', theme.text),
                ('é', theme.text),
                ('"', theme.syntax[1]),
                ('x', theme.syntax[1]),
                ('"', theme.syntax[1]),
                (' ', theme.text),
                ('y', theme.text),
            ]
        );
    }

    #[test]
    fn the_slider_follows_the_view() {
        let cell = Cell {
            width: 10,
            height: 20,
        };
        let options = Options {
            scroll_strip: 6,
            ..Options::default()
        };
        let layout = Layout::new(400, 300, cell, 1000, &options);
        let plain = Layout::new(400, 300, cell, 1000, &Options::default());
        assert_eq!(layout.text.w + 6, plain.text.w);
        assert_eq!(layout.scrollbar.x, layout.text.x + layout.text.w as i32);
        let track = layout.scrollbar;
        let page = page_rows(&layout, cell);
        assert_eq!(slider(&layout, 1, 0, page, 20), None, "nothing scrolls");
        // A file shorter than the view keeps its slider at the top too.
        let short = slider(&layout, page, 0, page, 20).unwrap();
        assert_eq!(short.y, track.y);
        assert!(slider(&layout, page, 1, page, 20).is_some(), "scrolled");
        let top = slider(&layout, 1000, 0, page, 20).unwrap();
        assert_eq!((top.y, top.w), (track.y, 6));
        let bottom = slider(&layout, 1000, 999, page, 20).unwrap();
        assert_eq!(bottom.y + bottom.h as i32, track.y + track.h as i32);
        let middle = slider(&layout, 1000, 500, page, 20).unwrap();
        assert!(top.y < middle.y && middle.y < bottom.y);
        let huge = slider(&layout, 10_000_000, 5_000_000, page, 20).unwrap();
        assert_eq!(huge.h, 20, "minimum height");
        assert_eq!(slider(&plain, 1000, 0, page, 20), None, "no strip");

        let snapshot = Snapshot::from_bytes("x\n".repeat(999).as_bytes());
        let rows = Row::lines(0, layout.rows, snapshot.line_count());
        for (active, color) in [
            (false, Theme::DARK.scroll_slider),
            (true, Theme::DARK.scroll_slider_active),
        ] {
            let mut list = DisplayList::default();
            ViewBuilder::default().build(
                &Frame {
                    snapshot: &snapshot,
                    layout: &layout,
                    scroll: Scroll::default(),
                    rows: &rows,
                    row_count: snapshot.line_count(),
                    overview: &[],
                    cell,
                    theme: &Theme::DARK,
                    chrome: Chrome::default(),
                    options: &options,
                    selections: &[],
                    caret: CaretStyle::Hidden,
                    highlights: Highlights::default(),
                    empty: None,
                    slider_active: active,
                    measure: false,
                },
                &mut list,
            );
            assert!(list.fills.contains(&Fill { rect: top, color }));
        }
    }

    #[test]
    fn the_horizontal_slider_follows_the_view() {
        let cell = Cell {
            width: 10,
            height: 20,
        };
        let options = Options {
            scroll_strip: 6,
            ..Options::default()
        };
        let layout = Layout::new(400, 300, cell, 10, &options);
        let track = htrack(&layout, 6).unwrap();
        let text = layout.text;
        assert_eq!((track.x, track.w, track.h), (text.x, text.w, 6));
        assert_eq!(
            track.y + 6,
            text.y + text.h as i32,
            "over the text's bottom"
        );
        let cols = u64::from(layout.cols);
        assert_eq!(hslider(&layout, 6, cols, 0, 20), None, "the rows fit");
        let left = hslider(&layout, 6, 2 * cols, 0, 20).unwrap();
        assert_eq!(
            left,
            Rect {
                w: track.w / 2,
                ..track
            }
        );
        let right = hslider(&layout, 6, 2 * cols, cols, 20).unwrap();
        assert_eq!(right.x + right.w as i32, track.x + track.w as i32);
        let middle = hslider(&layout, 6, 2 * cols, cols / 2, 20).unwrap();
        assert!(left.x < middle.x && middle.x < right.x);
        let huge = hslider(&layout, 6, 10_000_000, 0, 20).unwrap();
        assert_eq!(huge.w, 20, "minimum width");
        // A pane without the vertical strip has a track of the same height.
        let plain = Layout::new(400, 300, cell, 10, &Options::default());
        assert_eq!(htrack(&plain, 6).map(|t| t.h), Some(6));
        assert_eq!(htrack(&plain, 0), None, "no thickness");
    }

    /// Rows ending in the view report their exact width; wider ones a bound
    /// at least their width, so scrolling reaches their end; a long line
    /// counts a column per byte past its scan end.
    #[test]
    fn the_widest_drawn_row_is_reported() {
        let widest = |bytes: &[u8], first_col: u64| {
            let scroll = Scroll {
                first_col,
                ..Scroll::default()
            };
            let highlights = Highlights::default();
            let (_, list) = build_at(bytes, 400, 200, scroll, &[], CaretStyle::Hidden, highlights);
            list.widest
        };
        assert_eq!(widest(b"ab\n\tc\nxyz", 0), 5, "a tab stop");
        assert_eq!(
            widest("\u{e9}\u{6f22}".as_bytes(), 0),
            3,
            "a wide character"
        );
        let wide = "x".repeat(100) + "\n" + &"\u{e9}".repeat(90);
        assert_eq!(widest(wide.as_bytes(), 0), 100, "estimated past the view");
        assert_eq!(
            widest(wide.as_bytes(), 80),
            100,
            "exact once its end is drawn"
        );
        let tabs = "\tx".repeat(50);
        assert_eq!(widest(tabs.as_bytes(), 200), 201, "exact");
        assert!(widest(tabs.as_bytes(), 0) >= 201, "a bound past the view");
        let cjk = "\u{6f22}".repeat(500);
        assert_eq!(widest(cjk.as_bytes(), 0), 1000, "two columns each");
        assert_eq!(widest(&[0x80; 100], 0), 100, "invalid bytes");
        let long = "x".repeat(ROW_SCAN_LIMIT + 100);
        let end = (ROW_SCAN_LIMIT + 100) as u64;
        assert_eq!(widest(long.as_bytes(), 0), end);
        assert_eq!(widest(long.as_bytes(), end - 20), end);
        assert_eq!(widest(b"", 0), 0);
    }

    #[test]
    fn full_screen_layouts_have_no_title_strip() {
        let cell = Cell {
            width: 10,
            height: 20,
        };
        let options = Options {
            title_strip: false,
            ..Options::default()
        };
        let layout = Layout::new(400, 300, cell, 10, &options);
        assert_eq!((layout.title.h, layout.text.y), (0, 0));
        assert_eq!(layout.status.y + layout.status.h as i32, 300);
    }

    #[test]
    fn builds_rows_with_line_numbers_and_crlf() {
        let (layout, list) = build(b"ab\r\ncd\n", 400, 200);
        let numbers: String = list
            .glyphs
            .iter()
            .filter(|g| g.clip == layout.gutter)
            .map(|g| g.ch)
            .collect();
        assert_eq!(numbers, "123");
        // The '\r' of the CRLF is not shown.
        assert_eq!(body_text(&list, &layout), "abcd");
        assert_eq!(list.fills.len(), 4);
    }

    #[test]
    fn carriage_return_without_line_feed_stays_visible() {
        let (layout, list) = build(b"a\r", 400, 200);
        assert_eq!(body_text(&list, &layout), "a\u{240D}");
        let (layout, list) = build(b"a\r\r\nb", 400, 200);
        assert_eq!(body_text(&list, &layout), "a\u{240D}b");
    }

    #[test]
    fn every_glyph_is_clipped_to_its_region() {
        for height in [0, 1, 5, 19, 20, 30, 50, 60, 61, 200] {
            for width in [0, 1, 9, 10, 400] {
                let (layout, list) = build(b"first line\nsecond\n", width, height);
                for g in &list.glyphs {
                    assert!(
                        [layout.title, layout.status, layout.gutter, layout.text].contains(&g.clip),
                        "glyph {g:?} has an unknown clip at {width}x{height}"
                    );
                }
            }
        }
    }

    #[test]
    fn zero_width_marks_per_cell_are_capped() {
        let mut line = b"a".to_vec();
        for _ in 0..10_000 {
            line.extend_from_slice("\u{301}".as_bytes());
        }
        line.push(b'b');
        let got = cells(&line, 0, 10);
        assert_eq!(got.len(), 2 + MAX_MARKS_PER_CELL as usize);
        assert_eq!(got.last(), Some(&(1, 'b', Kind::Text)));
    }

    #[test]
    fn many_same_row_selections_and_carets_draw_at_their_columns() {
        // "a\tb\u{301} " repeated: tabs, a zero-width mark and spaces.
        let unit = "a\tb\u{301} ";
        let text = unit.repeat(400);
        let starts: Vec<u64> = (0..400).map(|i| (i * unit.len()) as u64).collect();
        let sels: Vec<Selection> = starts
            .iter()
            .enumerate()
            .map(|(i, &s)| {
                if i % 2 == 0 {
                    Selection::new(s, s + 1)
                } else {
                    Selection::new(s + 2, s)
                }
            })
            .collect();
        let (layout, list) = build_with(text.as_bytes(), 800, 200, &sels, CaretStyle::Bar);
        let x0 = layout.text.x;
        let col = |offset: u64| column_of(text.as_bytes(), offset as usize, 4);
        let cols = u64::from(layout.cols);
        let fills: Vec<i32> = list
            .fills
            .iter()
            .filter(|f| f.color == Theme::DARK.selection && f.rect.y == layout.text.y)
            .map(|f| f.rect.x)
            .collect();
        let right = layout.text.x + layout.text.w as i32;
        let expected: Vec<i32> = sels
            .iter()
            .map(|s| col(s.start()))
            .take_while(|&c| c <= cols + 2)
            .map(|c| x0 + (c * 10) as i32)
            .filter(|&x| x < right)
            .collect();
        assert_eq!(fills, expected);
        let carets: Vec<i32> = list.overlays.iter().map(|f| f.rect.x).collect();
        let expected: Vec<i32> = sels
            .iter()
            .map(|s| col(s.head))
            .take_while(|&c| c <= cols + 1)
            .map(|c| x0 + (c * 10) as i32)
            .filter(|&x| x < right)
            .collect();
        assert_eq!(carets, expected);
    }

    #[test]
    fn selection_fills_include_selected_line_breaks() {
        let sels = [Selection::new(1, 5)];
        let (layout, list) = build_with(b"abc\ndef", 400, 200, &sels, CaretStyle::Hidden);
        let selected: Vec<Rect> = list
            .fills
            .iter()
            .filter(|f| f.color == Theme::DARK.selection)
            .map(|f| f.rect)
            .collect();
        let (x, y) = (layout.text.x, layout.text.y);
        assert_eq!(
            selected,
            vec![
                Rect {
                    x: x + 10,
                    y,
                    w: 30,
                    h: 20
                },
                Rect {
                    x,
                    y: y + 20,
                    w: 10,
                    h: 20
                },
            ]
        );
        assert!(list.overlays.is_empty());
    }

    #[test]
    fn matches_scope_and_the_current_match_are_filled() {
        let matches = [0..2, 3..5];
        let highlights = Highlights {
            matches: &matches,
            scope: Some(0..7),
            ..Highlights::default()
        };
        let sels = [Selection::new(3, 5)];
        let (layout, list) = build_full(
            b"ab\nab\nab",
            400,
            200,
            &sels,
            CaretStyle::Hidden,
            highlights,
        );
        let t = Theme::DARK;
        let rects = |color| -> Vec<Rect> {
            list.fills
                .iter()
                .filter(|f| f.color == color)
                .map(|f| f.rect)
                .collect()
        };
        let (x, y) = (layout.text.x, layout.text.y);
        assert_eq!(rects(t.search_match), vec![Rect { x, y, w: 20, h: 20 }]);
        assert_eq!(rects(t.search_current_border).len(), 1);
        assert_eq!(
            rects(t.search_current),
            vec![Rect {
                x: x + 1,
                y: y + 21,
                w: 18,
                h: 18
            }]
        );
        assert_eq!(
            rects(t.search_range).len(),
            3,
            "rows 0, 1 and the start of 2"
        );
    }

    #[test]
    fn dense_matches_on_a_long_row_draw_only_visible_columns() {
        let text = "a\tb".repeat(ROW_SCAN_LIMIT / 3);
        let matches: Vec<_> = (0..text.len() as u64).map(|i| i..i + 1).collect();
        let highlights = Highlights {
            matches: &matches,
            scope: None,
            ..Highlights::default()
        };
        let started = std::time::Instant::now();
        let (layout, list) = build_full(
            text.as_bytes(),
            400,
            200,
            &[],
            CaretStyle::Hidden,
            highlights,
        );
        assert!(started.elapsed() < std::time::Duration::from_millis(200));
        let fills: Vec<Rect> = list
            .fills
            .iter()
            .filter(|f| f.color == Theme::DARK.search_match)
            .map(|f| f.rect)
            .collect();
        let cols = layout.text.w / 10;
        assert!(fills.len() as u32 <= cols + 2, "{}", fills.len());
        // Tab stops follow the incremental columns: `a` 0, tab 1-3, `b` 4.
        let x = layout.text.x;
        assert_eq!(
            fills[1],
            Rect {
                x: x + 10,
                y: layout.text.y,
                w: 30,
                h: 20
            }
        );
        assert_eq!(fills[2].x, x + 40);
    }

    #[test]
    fn carets_are_overlays_at_their_column() {
        let sels = [Selection::caret(2), Selection::caret(4)];
        let (layout, list) = build_with(b"a\tb\ncd", 400, 200, &sels, CaretStyle::Bar);
        let (x, y) = (layout.text.x, layout.text.y);
        assert_eq!(
            list.overlays,
            vec![
                Fill {
                    rect: Rect {
                        x: x + 40,
                        y,
                        w: 2,
                        h: 20
                    },
                    color: Theme::DARK.caret
                },
                Fill {
                    rect: Rect {
                        x,
                        y: y + 20,
                        w: 2,
                        h: 20
                    },
                    color: Theme::DARK.caret
                },
            ]
        );
        let (_, list) = build_with(b"ab", 400, 200, &sels[..1], CaretStyle::Underline);
        assert_eq!(list.overlays[0].rect.w, 10);
        assert_eq!(list.overlays[0].rect.h, 2);
    }

    #[test]
    fn title_mark_follows_the_title() {
        let (layout, list) = build(b"", 400, 200);
        let mark = list.glyphs.iter().find(|g| g.ch == '\u{25CF}').unwrap();
        assert_eq!(mark.color, Theme::DARK.unsaved);
        assert_eq!(mark.x, 70);
        assert_eq!(mark.clip, layout.title);
    }

    #[test]
    fn app_name_is_right_aligned_and_yields_to_the_title() {
        let name = |width| {
            let (layout, list) = build(b"", width, 200);
            let glyphs: Vec<&Glyph> = list
                .glyphs
                .iter()
                .filter(|g| g.clip == layout.title && "\u{2502}App".contains(g.ch))
                .collect();
            (
                glyphs.iter().map(|g| g.ch).collect::<String>(),
                glyphs.first().map(|g| g.x),
                glyphs
                    .iter()
                    .find(|g| g.ch == 'A')
                    .map(|g| (g.color, g.bold)),
                glyphs.iter().find(|g| g.ch == '\u{2502}').map(|g| g.bold),
            )
        };
        // 400 px: "│ App" ends one cell before the right edge.
        assert_eq!(
            name(400),
            (
                "\u{2502}App".to_owned(),
                Some(400 - 60),
                Some((Theme::DARK.unsaved, true)),
                Some(false),
            )
        );
        // "title" and its marker end at 80 px; the divider needs a 1-cell gap.
        assert_eq!(
            name(150),
            (
                "\u{2502}App".to_owned(),
                Some(90),
                Some((Theme::DARK.unsaved, true)),
                Some(false),
            )
        );
        assert_eq!(name(149), (String::new(), None, None, None));
    }

    #[test]
    fn diff_rows_draw_backgrounds_signs_inner_changes_and_fillers() {
        let snapshot = Snapshot::from_bytes(b"same\nold line\nend");
        let cell = Cell {
            width: 10,
            height: 20,
        };
        let options = Options {
            scroll_strip: 14,
            ..Options::default()
        };
        let layout = Layout::new(400, 200, cell, snapshot.line_count(), &options);
        let changed_bytes = 5..8;
        let inner = std::slice::from_ref(&changed_bytes);
        let row = |line, kind, inner| Row { line, kind, inner };
        let rows = [
            row(Some(0), RowKind::Same, &[][..]),
            row(Some(1), RowKind::Removed, inner),
            row(None, RowKind::Filler, &[][..]),
            row(Some(2), RowKind::Same, &[][..]),
            // A line past the end (edited since the diff) draws nothing.
            row(Some(7), RowKind::Same, &[][..]),
        ];
        let overview = [Mark {
            top: 10,
            height: 3,
            side: Side::Left,
        }];
        let mut list = DisplayList::default();
        ViewBuilder::default().build(
            &Frame {
                snapshot: &snapshot,
                layout: &layout,
                scroll: Scroll::default(),
                rows: &rows,
                row_count: 5,
                overview: &overview,
                cell,
                theme: &Theme::DARK,
                chrome: Chrome::default(),
                options: &options,
                selections: &[],
                caret: CaretStyle::Hidden,
                highlights: Highlights::default(),
                empty: None,
                slider_active: false,
                measure: false,
            },
            &mut list,
        );
        let theme = Theme::DARK;
        let text = layout.text;
        let row_rect = |i: i32| Rect {
            y: text.y + i * 20,
            h: 20,
            ..text
        };
        let has = |rect, color| list.fills.contains(&Fill { rect, color });
        assert!(has(row_rect(1), theme.diff_removed_line));
        let gutter_row = Rect {
            y: text.y + 20,
            h: 20,
            ..layout.gutter
        };
        assert!(has(gutter_row, theme.diff_removed_line));
        let changed = Rect {
            w: 30,
            ..row_rect(1)
        };
        assert!(has(changed, theme.diff_removed_text), "the inner change");
        assert_eq!(
            list.hatches,
            [Hatch {
                rect: row_rect(2),
                period: 6,
                color: theme.diff_filler
            }]
        );
        let sign = list
            .glyphs
            .iter()
            .find(|g| g.ch == '-' && g.clip == layout.gutter)
            .unwrap();
        assert_eq!((sign.y, sign.color), (text.y + 20, theme.diff_removed_sign));
        assert_eq!(sign.x, layout.gutter.x + layout.gutter.w as i32 - 10);
        assert_eq!(body_text(&list, &layout), "sameold lineend");
        let numbers: Vec<i32> = list
            .glyphs
            .iter()
            .filter(|g| g.clip == layout.gutter && g.ch.is_ascii_digit())
            .map(|g| (g.y - text.y) / 20)
            .collect();
        assert_eq!(
            numbers,
            [0, 1, 3],
            "no number on the filler or past the end"
        );
        let track = layout.scrollbar;
        let mark = Rect {
            x: track.x,
            y: track.y + 10,
            w: track.w / 2,
            h: 3,
        };
        assert!(has(mark, theme.diff_removed_sign), "the overview mark");
        // A pane's gutter keeps a space between the number, the sign and the text.
        let options = Options {
            pane: Some(Side::Right),
            ..options
        };
        let layout = Layout::new(400, 200, cell, snapshot.line_count(), &options);
        let mut list = DisplayList::default();
        ViewBuilder::default().build_pane(
            &Frame {
                snapshot: &snapshot,
                layout: &layout,
                scroll: Scroll::default(),
                rows: &rows,
                row_count: 5,
                overview: &[],
                cell,
                theme: &Theme::DARK,
                chrome: Chrome::default(),
                options: &options,
                selections: &[],
                caret: CaretStyle::Hidden,
                highlights: Highlights::default(),
                empty: None,
                slider_active: false,
                measure: false,
            },
            &mut list,
        );
        let end = layout.gutter.x + layout.gutter.w as i32;
        let at = |ch: char| {
            list.glyphs
                .iter()
                .find(|g| g.ch == ch && g.clip == layout.gutter)
                .map(|g| g.x)
        };
        assert_eq!((at('2'), at('-')), (Some(end - 40), Some(end - 20)));
    }

    /// The last row of a body that is not a whole number of rows tall is
    /// partly visible: its decorations end at the status line, and a clipped
    /// hatch keeps the spacing of a whole row.
    #[test]
    fn partial_rows_keep_diff_decorations_above_the_status_line() {
        let snapshot = Snapshot::from_bytes(b"a\nb\nc\n");
        let cell = Cell {
            width: 10,
            height: 20,
        };
        let options = Options {
            pane: Some(Side::Left),
            ..Options::default()
        };
        // A body of 50 px: rows at 30, 50 and 70, the last one 10 px high.
        let layout = Layout::new(400, 110, cell, snapshot.line_count(), &options);
        assert_eq!((layout.text.h, layout.rows), (50, 3));
        let row = |line, kind| Row {
            line,
            kind,
            inner: &[],
        };
        for last in [row(Some(2), RowKind::Removed), row(None, RowKind::Filler)] {
            let rows = [
                row(Some(0), RowKind::Same),
                row(Some(1), RowKind::Inserted),
                last,
            ];
            let mut list = DisplayList::default();
            ViewBuilder::default().build_pane(
                &Frame {
                    snapshot: &snapshot,
                    layout: &layout,
                    scroll: Scroll::default(),
                    rows: &rows,
                    row_count: 3,
                    overview: &[],
                    cell,
                    theme: &Theme::DARK,
                    chrome: Chrome::default(),
                    options: &options,
                    selections: &[],
                    caret: CaretStyle::Hidden,
                    highlights: Highlights::default(),
                    empty: None,
                    slider_active: false,
                    measure: false,
                },
                &mut list,
            );
            let rects = list.fills.iter().map(|f| f.rect);
            for rect in rects.chain(list.hatches.iter().map(|h| h.rect)) {
                assert!(
                    rect.y + rect.h as i32 <= layout.status.y,
                    "{rect:?} reaches the status line at {}",
                    layout.status.y
                );
            }
            if last.kind == RowKind::Filler {
                let hatch = list.hatches[0];
                assert_eq!((hatch.rect.y, hatch.rect.h, hatch.period), (70, 10, 6));
            }
        }
    }

    #[test]
    fn hits_match_drawn_columns() {
        let text = "x\na\t\u{4E2D}e\u{301}z\u{1}\r\nlast";
        let snapshot = Snapshot::from_bytes(text.as_bytes());
        let cell = Cell {
            width: 10,
            height: 20,
        };
        let options = Options::default();
        let layout = Layout::new(400, 200, cell, snapshot.line_count(), &options);
        let hit = |col: i32, line: u64| {
            hit_line(
                &snapshot,
                &layout,
                0,
                cell,
                &options,
                line,
                layout.text.x + col * 10,
            )
        };
        // Each drawn base glyph starts at the offset hit at its column.
        let line = &text.as_bytes()[2..12];
        let mut drawn = Vec::new();
        layout_line(line, 0, 40, 4, |c, ch, _, _| drawn.push((c, ch)));
        let starts = [0u64, 2, 5, 8, 9];
        let bases: Vec<u32> = drawn
            .iter()
            .filter(|(_, ch)| *ch != '\u{301}')
            .map(|(c, _)| *c)
            .collect();
        assert_eq!(bases, vec![0, 4, 6, 7, 8]);
        for (col, start) in bases.iter().zip(starts) {
            assert_eq!(hit(*col as i32, 1), 2 + start, "column {col}");
        }
        // Inside a tab or wide glyph: nearest boundary; past the end: line end before CRLF.
        assert_eq!(hit(2, 1), 3);
        assert_eq!(hit(3, 1), 4);
        assert_eq!(hit(30, 1), 12);
        assert_eq!(hit(-3, 0), 0);
        assert_eq!(hit(9, 2), snapshot.len());
    }

    #[test]
    fn hits_beyond_the_scan_limit_land_on_character_boundaries() {
        let mut text = vec![b'a'; ROW_SCAN_LIMIT];
        for _ in 0..100 {
            text.extend_from_slice("\u{4E2D}".as_bytes());
        }
        let snapshot = Snapshot::from_bytes(&text);
        let cell = Cell {
            width: 10,
            height: 20,
        };
        let options = Options::default();
        let layout = Layout::new(400, 200, cell, 1, &options);
        let first_col = ROW_SCAN_LIMIT as u64 - 6;
        for col in 0..30 {
            let x = layout.text.x + col * 10;
            let pos = hit_line(&snapshot, &layout, first_col, cell, &options, 0, x);
            assert!(
                std::str::from_utf8(&snapshot.to_vec(0..pos)).is_ok(),
                "column {col} hit {pos}"
            );
        }
    }

    /// Body glyphs `(visible column, char)` and caret columns of one row
    /// scrolled to `first_col`.
    fn long_row(bytes: &[u8], first_col: u64, carets: &[u64]) -> (Vec<(i32, char)>, Vec<i32>) {
        let snapshot = Snapshot::from_bytes(bytes);
        let cell = Cell {
            width: 10,
            height: 20,
        };
        let options = Options::default();
        let layout = Layout::new(400, 200, cell, snapshot.line_count(), &options);
        let sels: Vec<Selection> = carets.iter().map(|&c| Selection::caret(c)).collect();
        let rows = Row::lines(0, layout.rows, snapshot.line_count());
        let mut list = DisplayList::default();
        ViewBuilder::default().build(
            &Frame {
                snapshot: &snapshot,
                layout: &layout,
                scroll: Scroll {
                    top_row: 0,
                    first_col,
                },
                rows: &rows,
                row_count: snapshot.line_count(),
                overview: &[],
                cell,
                theme: &Theme::DARK,
                chrome: Chrome::default(),
                options: &options,
                selections: &sels,
                caret: CaretStyle::Bar,
                highlights: Highlights::default(),
                empty: None,
                slider_active: false,
                measure: false,
            },
            &mut list,
        );
        let x = layout.text.x;
        let glyphs = list
            .glyphs
            .iter()
            .filter(|g| g.clip == layout.text && g.y == layout.text.y)
            .map(|g| ((g.x - x) / 10, g.ch))
            .collect();
        let carets = list.overlays.iter().map(|f| (f.rect.x - x) / 10).collect();
        (glyphs, carets)
    }

    #[test]
    fn long_lines_draw_their_end() {
        let mut text = vec![b'a'; 200 * 1024 - 3];
        text.extend_from_slice(b"END");
        let len = text.len() as u64;
        let (glyphs, carets) = long_row(&text, len - 10, &[len]);
        let tail: String = glyphs.iter().map(|g| g.1).collect();
        assert!(tail.ends_with("aEND"), "{tail}");
        assert_eq!(glyphs.last(), Some(&(9, 'D')));
        assert_eq!(carets, vec![10]);
    }

    #[test]
    fn a_character_straddling_the_scan_end_is_drawn_whole() {
        let mut text = vec![b'a'; ROW_SCAN_LIMIT - 1];
        text.extend_from_slice("\u{4E2D}bc".as_bytes());
        let first = ROW_SCAN_LIMIT as u64 - 5;
        let after = ROW_SCAN_LIMIT as u64 + 2;
        let (glyphs, carets) = long_row(&text, first, &[after]);
        assert!(glyphs.iter().all(|g| g.1 != '\u{FFFD}'), "{glyphs:?}");
        assert!(glyphs.contains(&(4, '\u{4E2D}')));
        // One column per byte past the scan end: 'b' and the caret after the character.
        assert!(glyphs.contains(&(7, 'b')));
        assert_eq!(carets, vec![7]);
    }

    #[test]
    fn characters_cut_by_the_left_edge_past_the_scan_end_are_skipped() {
        let mut text = vec![b'a'; ROW_SCAN_LIMIT];
        for _ in 0..100 {
            text.extend_from_slice("\u{4E2D}".as_bytes());
        }
        // Column 4 past the scan end is inside the second character (bytes 3..6).
        let (glyphs, _) = long_row(&text, ROW_SCAN_LIMIT as u64 + 4, &[]);
        assert!(glyphs.iter().all(|g| g.1 == '\u{4E2D}'), "{glyphs:?}");
        assert_eq!(glyphs.first(), Some(&(2, '\u{4E2D}')));
        assert_eq!(glyphs.last().map(|g| g.0 % 3), Some(2));
    }

    #[test]
    fn marks_invalid_bytes_and_crlf_past_the_scan_end() {
        let mut text = vec![b'a'; ROW_SCAN_LIMIT];
        text.extend_from_slice(b"e\xCC\x81\xFFz\r\nq");
        let (glyphs, _) = long_row(&text, ROW_SCAN_LIMIT as u64 - 2, &[]);
        assert_eq!(
            glyphs,
            vec![
                (0, 'a'),
                (1, 'a'),
                (2, 'e'),
                (2, '\u{301}'),
                (5, '\u{FFFD}'),
                (6, 'z'),
            ]
        );
    }

    #[test]
    fn marks_at_the_scan_end_join_the_cell_before_it() {
        let mark = "\u{301}".as_bytes();
        // A mark right at the seam, and one straddling it.
        for before in [ROW_SCAN_LIMIT, ROW_SCAN_LIMIT - 1] {
            let mut text = vec![b'a'; before];
            text.extend_from_slice(mark);
            text.push(b'z');
            let (glyphs, _) = long_row(&text, before as u64 - 2, &[]);
            assert_eq!(
                glyphs,
                vec![(0, 'a'), (1, 'a'), (1, '\u{301}'), (4, 'z')],
                "{before}"
            );
        }
        // Four marks before the seam: a fifth after it is not drawn.
        let mut text = vec![b'a'; ROW_SCAN_LIMIT - 4 * mark.len()];
        for _ in 0..5 {
            text.extend_from_slice(mark);
        }
        text.push(b'z');
        let first = (ROW_SCAN_LIMIT - 4 * mark.len()) as u64 - 1;
        let (glyphs, _) = long_row(&text, first, &[]);
        let marks = glyphs.iter().filter(|g| g.1 == '\u{301}').count();
        assert_eq!((glyphs[0], marks), ((0, 'a'), 4));
    }
}
