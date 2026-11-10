//! The files of the session: empty panes, opening dropped files, closing
//! both files back to empty panes. One file per pane; nothing here starts,
//! closes or contacts another session.

use super::*;

impl Editor {
    /// True in the empty state (no file open).
    pub(super) fn no_file(&self) -> bool {
        self.pane.path.is_none()
    }

    /// Resets every per-file state to that of a new editor on `path` (the
    /// empty state for `None`). Session settings (zoom, highlighting on or
    /// off, the search query, favorites) stay.
    pub(super) fn reset_document(&mut self, path: Option<PathBuf>) {
        debug_assert!(self.pane.saving.is_none(), "a save is running");
        // An early syntax thread of the command-line file must never start late.
        if let Some(slot) = self.pane.early_syntax.take()
            && let Ok(mut slot) = slot.lock()
        {
            drop(slot.close());
        }
        // Dropping the load stops its loader.
        self.pane.loads = None;
        let mut doc = Document::new(EditOptions::default());
        doc.locks.no_file = path.is_none();
        doc.locks.user = self.settings.read_only && self.pane.side == Side::Left;
        doc.locks.diff = self.diff_mode;
        self.pane.doc = doc;
        self.pane.title = path
            .as_deref()
            .map_or_else(String::new, |p| p.display().to_string());
        self.pane.path = path;
        self.pane.info = None;
        self.pane.baseline = None;
        self.pane.status = LoadStatus::Ready;
        self.scroll = Scroll::default();
        self.reset_diff();
        self.notice = None;
        self.pane.prompt = None;
        self.pane.save_queued = None;
        self.pane.save_refused = false;
        self.drag = None;
        *self.pane.caret_column.get_mut() = None;
        // The seam cache holds a snapshot: it must not keep the old text alive.
        *self.pane.seams.get_mut() = (None, 0, Vec::new());
        // A Go to Line preview belongs to the old file; nothing is restored.
        self.pane.line_preview = None;
        self.pane.language = None;
        self.pane.language_chosen = false;
        // Dropping the syntax thread retires its trees on that thread.
        self.pane.syntax = None;
        self.pane.syntax_over_limit = false;
        self.pane.symbols_pending = false;
        *self.pane.bracket_cache.get_mut() = None;
        self.popup = None;
        self.chord = None;
        self.pane.cursor_history.clear();
        self.pane.column = None;
        self.pane.occurrence_word = None;
        self.pane.expansions.clear();
        self.pane.expanded = None;
        self.pane.symbol_offsets.clear();
        self.pane.marked = (false, false, false);
        self.reset_search();
        // Dropping the watcher stops its thread.
        self.pane.watching = Watching::Idle;
        self.pane.disk = None;
        self.pane.deletion_shown = false;
        self.pane.restore = None;
        self.pane.widest = 0;
    }

    /// Why the current file may not be replaced by `dropped` now.
    fn open_refusal(&self, dropped: &Path) -> Option<String> {
        let path = self.pane.path.as_deref()?;
        let (name, other) = (display_name(path), display_name(dropped));
        if self.pane.saving.is_some() {
            Some(format!("{name} is being saved; {other} was not opened"))
        } else if self.pane.doc.is_dirty() || self.pane.prompt.is_some() || self.closing.is_some() {
            Some(format!(
                "{name} has unsaved changes; save or close it to open {other}"
            ))
        } else {
            None
        }
    }

    /// Files dropped on the window: one opens in the pane under the drop
    /// point (elsewhere in the focused pane), two open as the left and the
    /// right file. A pane is replaced only without unsaved changes.
    pub(super) fn files_dropped(&mut self, dropped: Result<Dropped, String>) {
        if self.pane.prompt.is_some() || self.other.prompt.is_some() {
            return;
        }
        self.popup = None;
        let Dropped {
            path,
            second,
            others,
            point,
        } = match dropped {
            Ok(dropped) => dropped,
            Err(error) => {
                self.notice = Some(error);
                return;
            }
        };
        let ignored =
            (others > 0).then(|| format!("Two files open at a time; {others} more ignored"));
        let Some(second) = second else {
            let side = point
                .and_then(|(x, y)| self.side_at(x, y))
                .unwrap_or(self.pane.side);
            self.with_side(side, |e| e.drop_one(path, ignored));
            return;
        };
        let mut refusal = self.with_side(Side::Left, |e| e.open_refusal(&path));
        if refusal.is_none() {
            refusal = self.with_side(Side::Right, |e| e.open_refusal(&second));
        }
        if let Some(reason) = refusal {
            self.notice = Some(reason);
            return;
        }
        let opening = format!(
            "Opening {} and {}\u{2026}",
            display_name(&path),
            display_name(&second)
        );
        let notice = ignored.clone();
        self.with_side(Side::Left, |e| e.start_open(path, notice, None));
        self.with_side(Side::Right, |e| e.start_open(second, ignored, None));
        self.notice = Some(opening);
    }

    /// Opens `path` in the pane in place of a file without unsaved changes
    /// (or of the empty pane).
    fn drop_one(&mut self, path: PathBuf, notice: Option<String>) {
        if let Some(reason) = self.open_refusal(&path) {
            self.notice = Some(reason);
            return;
        }
        let name = display_name(&path);
        self.start_open(path, notice, None);
        self.notice = Some(format!("Opening {name}\u{2026}"));
    }

    /// Starts loading `path`; it replaces the current file at the loader's
    /// first answer. A newer open replaces (and stops) a pending one.
    pub(super) fn start_open(
        &mut self,
        path: PathBuf,
        notice: Option<String>,
        restore: Option<(Scroll, u64)>,
    ) {
        if self.highlighting
            && let Some(lang) = rtm_syntax::detect(&path, b"")
        {
            rtm_syntax::warm(lang.grammar);
        }
        self.cancel_pending();
        let wake = self.wake.clone();
        let load = loader::spawn(path.clone(), move || wake(), None, false);
        // A reload keeps the text read-only until it is replaced, so no edit
        // made meanwhile is dropped with it.
        if restore.is_some() {
            self.pane.doc.locks.loading = true;
        }
        self.pane.pending = Some(PendingOpen {
            notice,
            path,
            load,
            restore,
        });
    }

    /// Drops the pending open (stopping its loader).
    pub(super) fn cancel_pending(&mut self) {
        if let Some(pending) = self.pane.pending.take() {
            self.end_reload(&pending);
        }
    }

    /// A reload that did not replace the text gives the editing back.
    fn end_reload(&mut self, pending: &PendingOpen) {
        if pending.restore.is_some() {
            self.pane.doc.locks.loading = matches!(self.pane.status, LoadStatus::Loading { .. });
        }
    }

    /// The first answer of a pending open's loader: a folder or missing
    /// file leaves everything as it was; a file replaces the current one,
    /// unless it gained unsaved changes meanwhile.
    pub(super) fn drain_pending(&mut self) {
        let Some(pending) = &self.pane.pending else {
            return;
        };
        let first = match pending.load.events.try_recv() {
            Ok(event) => event,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                let name = display_name(&pending.path);
                self.cancel_pending();
                self.notice = Some(format!("{name} could not be opened"));
                return;
            }
        };
        let pending = self.pane.pending.take().expect("checked above");
        if let LoadEvent::Rejected(reason) = first {
            self.end_reload(&pending);
            self.notice = Some(reason);
            return;
        }
        // A reload opens despite unsaved changes: the user chose it.
        if pending.restore.is_none()
            && let Some(reason) = self.open_refusal(&pending.path)
        {
            self.notice = Some(reason);
            return;
        }
        self.reset_document(Some(pending.path));
        self.pane.restore = pending.restore;
        self.pane.doc.locks.loading = true;
        self.pane.status = LoadStatus::Loading {
            loaded: 0,
            total: 0,
        };
        self.pane.loads = Some(pending.load);
        self.apply_load(first);
        self.drain_loads();
        if pending.notice.is_some() {
            self.notice = pending.notice;
        }
    }

    /// `File: Close`: both panes back to the empty state, asking first about
    /// unsaved changes; waits for running saves.
    pub(super) fn close_file(&mut self) {
        self.cancel_pending();
        self.with_other(Editor::cancel_pending);
        if self.no_file() && self.other.path.is_none() {
            self.notice = Some("No file is open; Alt+F4 quits".to_owned());
            return;
        }
        if self.pane.saving.is_some() || self.other.saving.is_some() {
            self.closing = Some(Exit::CloseFile);
        } else if self.pane.dirty() || self.other.dirty() {
            self.pane.prompt = Some(Prompt::DirtyClose(Exit::CloseFile));
        } else {
            self.close_both();
        }
    }

    /// Both panes back to the empty state.
    fn close_both(&mut self) {
        self.reset_document(None);
        self.with_other(|e| e.reset_document(None));
    }

    /// Ends what a settled close asked for.
    pub(super) fn finish_exit(&mut self, exit: Exit, ctx: &mut Ctx) {
        match exit {
            Exit::Quit => ctx.close(),
            Exit::CloseFile => self.close_both(),
        }
    }
}
