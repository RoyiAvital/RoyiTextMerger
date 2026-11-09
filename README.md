[![Visitors](https://api.visitorbadge.io/api/combined?path=https%3A%2F%2Fgithub.com%2FRoyiAvital%2FRoyiTextMerger&labelColor=%23f47373&countColor=%23555555&style=plastic)](https://github.com/RoyiAvital/RoyiTextMerger) <!-- https://www.visitorbadge.io -->

# Royi Text Merger

`RoyiTextMerger` is a portable, keyboard workflow focused, lightweight and
performance oriented Diff & Merge tool for a pair of text and code files.

It focuses on:
 - Two files side by side, aligned, with their changes marked.
 - Copying changes between the files.
 - **Commands** through the _Command Palette_ and a **keyboard** oriented workflow.
 - High performance to generate a **reactive** experience.
 - Developed and verified on **Windows OS**.

Each pane edits and searches as [Royi Text Editor](https://github.com/RoyiAvital/RoyiTextEditor) does.

The [documentation](https://RoyiAvital.github.io/RoyiTextMerger) describes
comparing and merging, the keys and the settings.

## Installation

1. Download `RoyiTextMerger-<version>-windows-x86_64.zip` from the
   [latest release](https://github.com/RoyiAvital/RoyiTextMerger/releases/latest).
2. Extract it to a folder of your choice and run `RoyiTextMerger.exe`.

It is portable: keep the extracted files together. Requirements: Windows 11,
x64 CPU with AVX2.

## Usage

```text
RoyiTextMerger.exe [--read-only | --diff-mode] <left> <right>
```

By default both files are editable. `--read-only` makes the left file read
only (as VS Code); `--diff-mode` makes both read only. Files can also be
dropped onto a pane.

The panes scroll together. A change takes the rows of its longer side; the
other side shows hatched filler rows. The status line counts the changes and
the ruler at the right edge marks them. A copy replaces the other file's
lines of the changes at the caret (or in the selection); the arrows in the
divider copy a change with a click.

| Key                                | Command                                    |
| ---------------------------------- | ------------------------------------------ |
| `Alt+F5` / `Shift+Alt+F5`          | Next / Previous Change                     |
| `Alt+Right` / `Alt+Left`           | Copy Change to Right / Left                |
| `Ctrl+Alt+Right` / `Ctrl+Alt+Left` | Copy Change to Right / Left and Go to Next |
| `Ctrl+1`                           | Focus Left Pane                            |
| `Ctrl+2`                           | Focus Right Pane                           |
| `Ctrl+Tab`                         | Switch Side                                |
| `Alt+\`                            | Swap Sides                                 |
| `Ctrl+K S`                         | Save All                                   |
| Palette                            | Toggle Diff Mode                           |
| Palette                            | Toggle Ignore Trim Whitespace              |

`RoyiTextMerger.yaml` next to the executable sets the comparison (VS Code's
defaults):

```yaml
diff:
  ignoreTrimWhitespace: true  # leading and trailing whitespace is not a change
  maxFileMiB: 50              # larger files are not compared, 1 to 1024
  maxComputationMs: 5000      # then the diff is simplified, 100 to 60000
```

## Development

See [DEVELOPMENT.md](DEVELOPMENT.md).
