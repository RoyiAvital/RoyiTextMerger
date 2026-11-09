# Files and Saving

Each pane opens and saves its file as Royi Text Editor does. This page
describes the two-file behavior and the unusual cases.

## Opening Files

- **Command line:** the first path opens in the left pane and the second in
  the right pane; more paths are ignored with a notice. With one path or
  none, the missing panes show `Drop a file here`.
- **Drag and drop:** a file dropped on a pane opens in it; dropped on the
  title strip, the divider or the status line it opens in the focused pane.
  Two files dropped together open as the left and the right file. A pane
  with unsaved changes is not replaced (the status line says why), and
  folders are refused.
- **One file in both panes** is allowed: two documents of one file.
- **Close and quit:** `Ctrl+W` closes both files and leaves two empty panes;
  `Alt+F4` quits. Both ask once about all unsaved changes:
  `Save changes to a.rs and b.rs?   Enter: Save   D: Discard   Esc: Cancel`.

> [!NOTE]
> Windows does not deliver drops from Explorer to an application running as
> administrator.

## Saving

`Ctrl+S` saves the focused pane; `Ctrl+K S` (`File: Save All`) saves every
pane with unsaved changes, the focused one first. A save writes the text to a
temporary file next to the original, then replaces the original in one step
and checks the result. A failed save never leaves the file half written.

- **Unsaved changes** show `●` in the pane's header and in the window title.
- **Changed by another program** (an agent, a formatter, `git checkout`):
  the change is noticed at once, also while you edit, and the status line
  asks `<name> changed on disk.   R: Reload   S: Save Current`. Only these
  two keys answer; a question about the other pane waits for this one.
  - `R` shows the disk version. Unsaved changes and the undo history are
    dropped; the caret and the scroll position stay.
  - `S` saves your text over the disk version.

  Saving checks again: if the file changed since it was last loaded or
  saved, the same question appears and nothing is written. On some network
  shares a change is noticed only when saving.
- **One file in both panes:** saving both makes the second save find the
  first one's version on disk, so it asks `R: Reload   S: Save Current`;
  choose the version that stays.
- **Deleted by another program:** the status line says so; the next save
  creates the file again.
- **Changed during the save:** the other program's version is kept next to
  the file, and the status line names it.
- **New file:** a path from the command line that does not exist opens an
  empty document (`New file` on the status line); the first save creates
  it.
- **Access denied or in use:** nothing is changed, no temporary file is
  left, and the status line names the cause.

## Read-Only Files

A file with the Windows **Read-only** attribute opens for editing, but it is
not saved. `Ctrl+S` writes nothing; the unsaved dot turns orange and the
status line asks to clear the attribute:

![The right file is read-only: after Ctrl+S its header shows an orange unsaved dot and the status line asks to clear the Read-only attribute](assets/read-only-save.png)

Clear the attribute (Explorer: Properties, uncheck **Read-only**), then
press `Ctrl+S` again. The application never changes file attributes itself.

To compare without allowing edits, start with `--read-only` (the left file)
or `--diff-mode` (both); see [Modes](comparing-and-merging.md#modes).

## Text Encoding and Line Breaks

| File                                     | What happens                                                    |
| ---------------------------------------- | --------------------------------------------------------------- |
| UTF-8, with or without BOM               | Edited and saved as it was                                      |
| UTF-16 with a BOM                        | Edited as text, saved as UTF-16                                 |
| UTF-16 without a BOM                     | Read as UTF-8: the zero bytes show as `␀`                       |
| Malformed UTF-16                         | Read-only, never saved (`Malformed UTF-16, read-only`)          |
| Invalid UTF-8 bytes                      | Shown as `�`, saved unchanged                                   |
| LF or CRLF line breaks                   | New lines use the style most lines have (CRLF on a tie or none) |
| Line breaks with a lone CR (classic Mac) | Shown as one line, each CR as `␍`; the status line says so      |

Files of different encodings or line breaks are compared by their text: a
UTF-8 file and its UTF-16 copy show
`No changes (whitespace, line breaks or encoding differ)`.

## Large Files and Long Lines

- **Loading:** a large file shows its beginning at once and the status line
  shows `Loading N%`. It can be read, but not edited, until it is loaded;
  the comparison starts once both files are loaded.
- **Comparing:** files above `diff.maxFileMiB` (50 MiB) are shown but not
  compared.
- **Long lines:** lines longer than 64 KiB are drawn to their end. Past
  64 KiB every byte takes one column (exact for ASCII), and the status line
  shows `Col ~N`.
- **Highlighting:** files above `syntax.maxFileMiB` (32 MiB) are plain
  text. See [Configuration](configuration.md).
