# Changelog

Notable implemented changes to the application are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0]

The first release: a Diff & Merge tool for a pair of text and code files, built
on Royi Text Editor.

### Added

 - The application, based on Royi Text Editor 0.6.2: each pane edits, searches,
   highlights syntax and checks spelling as the Editor does.
 - Two panes side by side. Each pane header shows the path, the unsaved dot `●`
   and `RO`; the window title lists both files.
 - Modes: `--read-only` makes the left file read only, `--diff-mode` makes both
   read only. The _Toggle Diff Mode_ command switches the latter.
 - Commands: _Focus Left Pane_ (`Ctrl+1`), _Focus Right Pane_ (`Ctrl+2`),
   _Switch Side_ (`Ctrl+Tab`), _Swap Sides_ (`Alt+\`), _Save All_ (`Ctrl+K S`).
 - Drag & drop: two files fill both panes; one file opens in the pane under the
   pointer.
 - Closing asks once for all unsaved files; saves run one at a time.
 - The aligned comparison: both panes scroll together; removed and inserted
   lines, their changed words and hatched filler rows are colored; the status
   line shows the changes (`3 changes`, `Identical`, `Comparing…`); the ruler at
   the right edge marks every change. Files above 50 MiB are not compared.
 - Merging: _Copy Change to Right_ / _Left_ (`Alt+Right` / `Alt+Left`) copies
   the changes at the caret or in the selection, in the target's line ending,
   as one undo step there; _and Go to Next_ (`Ctrl+Alt+Right` /
   `Ctrl+Alt+Left`) moves on. _Next_ / _Previous Change_ (`Alt+F5` /
   `Shift+Alt+F5`) wrap around and reach changes without lines on a side. The
   divider's arrows copy a change with a click. Edits keep both panes aligned
   while the changed region is compared again.
 - Undo and redo stay fast in very large files.
 - Settings: the `diff:` section of `RoyiTextMerger.yaml` (`ignoreTrimWhitespace`,
   `maxFileMiB`, `maxComputationMs`). The _Toggle Ignore Trim Whitespace_
   command compares again for the session; the status line shows
   `Whitespace compared` while whitespace counts.
 - The icon: two panels of text with the copy arrows (→ and ←) on a badge between
   them, after the icon of Royi File Manager.

### Documentation

 - The user documentation book: introduction, comparing and merging, files,
   keys, search, syntax highlighting, spelling, configuration and the command
   line, published at <https://RoyiAvital.github.io/RoyiTextMerger>.

### Performance

Merger Performance Benchmark v1, Intel Core i9-14900 (see `Performance.md`).

| Question                                | Scenario                          | Measure                      | Result    |
| --------------------------------------- | --------------------------------- | ---------------------------- | --------- |
| How light is it?                        | Start without files               | Peak memory                  | 5.1 MiB   |
| How fast do two files open?             | Open two 64 KiB C files           | Time to first frame          | 26.6 ms   |
| How soon are they compared?             | Open two 64 KiB C files           | Time to the exact comparison | 17.4 ms   |
| How soon are larger files compared?     | Open two 8 MiB C files            | Time to the exact comparison | 68.0 ms   |
| Do two huge files still open instantly? | Open two 48 MiB logs              | Time to first frame          | 30.8 ms   |
| How soon are huge files compared?       | Open two 48 MiB logs              | Time to the exact comparison | 102.3 ms  |
| What memory do huge files cost?         | Open two 48 MiB logs              | Peak memory                  | 130.7 MiB |
| Is search instant?                      | Find as you type in an 8 MiB file | Key to frame                 | 3.56 ms   |
| Is the next change instant?             | Next Change among 500 changes     | Key to frame                 | 1.40 ms   |
| Is merging instant?                     | Copy a change and go to the next  | Key to frame                 | 2.95 ms   |
| Is typing instant while comparing?      | Type in a change among 500        | Key to frame                 | 3.37 ms   |
