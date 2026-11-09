# Comparing and Merging

The left file is the original and the right file the modified one (as VS
Code). The comparison starts once both panes hold a file and runs on its own
thread: the window never waits for it.

![Two Rust files compared: the status line counts the changes, the changed line shows its changed characters, the inserted lines face hatched filler rows and the divider shows the copy arrows](assets/merger-screenshot.png)

## Reading the Comparison

- **Rows.** Unchanged lines face each other. A change takes the rows of its
  longer side; the shorter side shows hatched filler rows.
- **Colors.** Lines of a change are red on the left and green on the right;
  the changed characters inside them are brighter. The gutter marks them with
  `-` and `+`.
- **Overview ruler.** The right edge shows every change of the comparison,
  left changes in its left half and right changes in its right half. Press
  or drag it to scroll.
- **Status line.** The left side shows the state of the comparison:

| Status                                                    | Meaning                                                                   |
| --------------------------------------------------------- | ------------------------------------------------------------------------- |
| `Comparing…`                                              | The comparison of the current texts is computed                           |
| `3 changes`                                               | The texts differ in 3 changes                                             |
| `Identical`                                               | The files are equal byte for byte (encoding included)                     |
| `No changes (whitespace, line breaks or encoding differ)` | Only ignored differences remain                                           |
| `diff simplified after 5 s: changes are coarse`           | The diff took longer than `diff.maxComputationMs`; its changes are larger |
| `Not compared: a file is larger than 50 MiB`              | A file is above `diff.maxFileMiB`; the panes are shown unaligned          |

Lines are compared without their line breaks, so LF and CRLF files compare
equal. Leading and trailing whitespace is ignored by default; while it is
compared, the right side of the status line shows `Whitespace compared`.

## Moving Between Changes

`Alt+F5` goes to the next change and `Shift+Alt+F5` to the previous one,
from the caret of the focused pane; both wrap at the ends. The change reached
becomes the current change: the caret moves to its first line and the view
shows it.

## Copying Changes

| Keys                              | Action                                     |
| --------------------------------- | ------------------------------------------ |
| `Alt+Right`; `Alt+Left`           | Copy the changes to the right; to the left |
| `Ctrl+Alt+Right`; `Ctrl+Alt+Left` | The same, then go to the next change       |
| Click `→` or `←` in the divider   | Copy that change to the right; to the left |

A copy replaces the target file's lines of a change with the source file's
lines. It acts on the changes the focused pane's carets and selections touch
(a selection also takes the changes between its lines), otherwise on the
current change. All of them are copied in one step: `Ctrl+Z` in the target
pane undoes it.

- The copied lines take the line breaks of the target file.
- At the end of a file, a copy also copies whether the last line ends with
  a line break.
- A copy into a read-only pane is refused, and so are bytes that are not
  UTF-8 into a UTF-16 file; the status line says why.
- Pressed while the comparison is computed, a copy waits for it
  (`The copy waits for the comparison…`). Moving the caret, moving the
  focus or editing cancels it.

## Editing

Both panes are editors. An edit moves the changes around it at once; the
edited lines are compared again on the diff thread and the result follows
within milliseconds. Undo and redo work per pane.

## Modes

| Mode      | Command line  | Left pane | Right pane | Copying changes                 |
| --------- | ------------- | --------- | ---------- | ------------------------------- |
| Merge     | (default)     | Editable  | Editable   | Both ways                       |
| Read-only | `--read-only` | Read-only | Editable   | Left to right (as VS Code)      |
| Diff      | `--diff-mode` | Read-only | Read-only  | None: reviewing the differences |

`Diff: Toggle Diff Mode` switches the diff mode during a session, and
`File: Toggle Read-Only` the focused pane alone. A read-only pane shows `RO`
in its header; a copy arrow shows only toward an editable pane.

## Panes

| Keys               | Action                                                  |
| ------------------ | ------------------------------------------------------- |
| `Ctrl+1`; `Ctrl+2` | Focus the left pane; the right pane                     |
| `Ctrl+Tab`         | Focus the other pane                                    |
| `Alt+\`            | Swap the files between the panes (unsaved changes stay) |

A click focuses its pane. Keys, search and the Command Palette act on the
focused pane; the other pane shows its selections dimmed and no caret.

## Options

| Setting                     | Default | Command                               |
| --------------------------- | ------- | ------------------------------------- |
| `diff.ignoreTrimWhitespace` | `true`  | `Diff: Toggle Ignore Trim Whitespace` |
| `diff.maxFileMiB`           | `50`    | None                                  |
| `diff.maxComputationMs`     | `5000`  | None                                  |

The command changes the option for the session; the configuration file is
never written. See [Configuration](configuration.md#diff).
