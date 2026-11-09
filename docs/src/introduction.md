# Royi Text Merger

A portable, keyboard-focused, lightweight and fast Diff & Merge tool for a
pair of text and code files on Windows.

- **Side by side:** two files, aligned, with their changed lines and
  characters marked.
- **Merging:** copy changes between the files from the keyboard or with a
  click.
- **Performance:** two files open in about 30 ms; the comparison runs on its
  own thread and huge files stay responsive.
- **Keyboard** oriented workflow, compatible with VS Code keys.
- **Each pane is an editor:** the editing, search, syntax highlighting,
  spelling and safe saving of
  [Royi Text Editor](https://github.com/RoyiAvital/RoyiTextEditor).

<figure class="product-shot">
  <img src="assets/merger-screenshot.png" alt="RoyiTextMerger comparing two Rust files: a changed line with its changed characters, an inserted block with hatched filler rows on the left, copy arrows in the divider and change marks on the overview ruler">
  <figcaption>Royi Text Merger</figcaption>
</figure>

## Installation

1. Open the [latest release](https://github.com/RoyiAvital/RoyiTextMerger/releases/latest).
2. Download `RoyiTextMerger-<version>-windows-x86_64.zip`.
3. Extract the ZIP file to a folder of your choice.
4. Run `RoyiTextMerger.exe` from the extracted folder.

> [!TIP]
> RoyiTextMerger is portable. Keep the extracted files together and move the
> folder whenever needed.

Requirements: Windows 11, x64 CPU with AVX2.

## Usage

```text
RoyiTextMerger.exe [--read-only | --diff-mode] [--syntax <id>] [--spelling <engine>] <left> <right>
```

| Option                | Effect                                                                                     |
| --------------------- | ------------------------------------------------------------------------------------------ |
| `--read-only`         | The left file is read-only: changes copy from left to right only (as VS Code)              |
| `--diff-mode`         | Both files are read-only: review the differences                                           |
| `--syntax <id>`       | Highlighting language, e.g. `json` ([ids](cli.md#syntax-ids)); `none` or `off` disables it |
| `--spelling <engine>` | Spelling `off`, or on with `builtin`, `hunspell` or `windows`                              |

The left file is the original and the right file the modified one. With one
path or none, the missing panes are empty: drop a file on a pane to open it.
A path that does not exist opens an empty document; the first save creates
it.

## The Window

- **Title strip:** a header per pane with its file name, `●` for unsaved
  changes and `RO` when it is read-only. The focused pane's header is bright.
- **Panes:** the left and the right file, scrolling together. Unchanged
  lines face each other; a change takes the rows of its longer side, and the
  other side shows hatched filler rows.
- **Divider:** `→` and `←` on the first row of each change copy it to the
  right or to the left with a click.
- **Overview ruler:** the changes of the whole comparison at the right edge,
  over the scroll slider.
- **Status line:** the comparison (`3 changes`, `Comparing…`, `Identical`)
  or a notice on the left; the focused pane's side, position, encoding, line
  breaks and language on the right.

[Comparing and Merging](comparing-and-merging.md) describes the comparison
and the copy commands.

## Keys

| Keys                                                                                        | Action                                       |
| ------------------------------------------------------------------------------------------- | -------------------------------------------- |
| <kbd>Alt</kbd>+<kbd>F5</kbd> / <kbd>Shift</kbd>+<kbd>Alt</kbd>+<kbd>F5</kbd>                | Next / previous change                       |
| <kbd>Alt</kbd>+<kbd>Right</kbd> / <kbd>Alt</kbd>+<kbd>Left</kbd>                            | Copy the change to the right / left          |
| <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>Right</kbd> / <kbd>Left</kbd>                           | Copy the change, then go to the next one     |
| <kbd>Ctrl</kbd>+<kbd>1</kbd> / <kbd>Ctrl</kbd>+<kbd>2</kbd>; <kbd>Ctrl</kbd>+<kbd>Tab</kbd> | Focus the left / right pane; switch side     |
| <kbd>Alt</kbd>+<kbd>\\</kbd>                                                                | Swap the files between the panes             |
| <kbd>Ctrl</kbd>+<kbd>S</kbd>; <kbd>Ctrl</kbd>+<kbd>K</kbd> <kbd>S</kbd>                     | Save the focused file; save both             |
| <kbd>Ctrl</kbd>+<kbd>Z</kbd>, <kbd>Ctrl</kbd>+<kbd>Y</kbd>                                  | Undo, redo in the focused pane               |
| <kbd>Ctrl</kbd>+<kbd>F</kbd> / <kbd>Ctrl</kbd>+<kbd>H</kbd>                                 | Find / replace in the focused pane           |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>P</kbd>, <kbd>F1</kbd>                                | Command Palette                              |
| <kbd>Ctrl</kbd>+<kbd>P</kbd>                                                                | Recent and favorite commands                 |
| <kbd>Ctrl</kbd>+<kbd>=</kbd> / <kbd>Ctrl</kbd>+<kbd>-</kbd> / <kbd>Ctrl</kbd>+<kbd>0</kbd>  | Zoom in / out / reset                        |
| <kbd>Ctrl</kbd>+<kbd>W</kbd>                                                                | Close both files (unsaved changes ask first) |
| <kbd>Alt</kbd>+<kbd>F4</kbd>, <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>W</kbd>                 | Quit; unsaved changes ask first              |

Editing, selection and search keys are the editor's; all of them are listed
in [Keyboard Shortcuts](keyboard-shortcuts.md).

## Command Palette

Every command is in the palette, grouped in families: `Application:`,
`Diff:`, `Edit:`, `File:`, `Go:`, `Search:`, `Selection:`, `Spelling:` and
`View:`. Type any part of a name (for example `dnext`), choose with
<kbd>Up</kbd> / <kbd>Down</kbd>, run with <kbd>Enter</kbd> or a click, close
with <kbd>Esc</kbd>.

- Click the star of a command, or press <kbd>Ctrl</kbd>+<kbd>Space</kbd>, to
  add or remove it from the favorites.
- <kbd>Ctrl</kbd>+<kbd>P</kbd> lists the last commands run from the palette,
  then the favorites.
- `Diff: Toggle Diff Mode` makes both files read-only or editable again.
- `Diff: Toggle Ignore Trim Whitespace` compares leading and trailing
  whitespace, or ignores it again.
- `Application: Keyboard Shortcuts` lists all keys; `Application: About`
  shows the version and links.

Favorites and recent commands are kept in `UserSettings/Palette.json` beside
the executable.

## Configuration

An optional `RoyiTextMerger.yaml` beside the executable is read at startup.
The application never writes it. The comparison settings and their defaults
(VS Code's):

```yaml
diff:
  ignoreTrimWhitespace: true  # leading and trailing whitespace is not a change
  maxFileMiB: 50              # larger files are not compared, 1 to 1024
  maxComputationMs: 5000      # then the diff is simplified, 100 to 60000
```

Every setting, its values and remarks: [Configuration](configuration.md).
