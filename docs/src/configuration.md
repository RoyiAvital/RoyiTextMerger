# Configuration

Settings are read from an optional `RoyiTextMerger.yaml` beside the
executable.

- It is read once at startup; restart the application after changing it.
- The application never writes it.
- A missing file gives the defaults.
- Problems are shown on the status line:
  - An invalid setting (an unknown key, a value out of range) keeps its
    default; the other settings still apply.
  - Invalid YAML (a syntax error, an anchor, tag or alias) ignores the whole
    file: every setting keeps its default.
- Plain YAML only: maps, lists and values, one document, no anchors, tags
  or aliases.

A complete file with the defaults:

```yaml
# RoyiTextMerger.yaml: every setting is optional; delete what you don't change.

palette:
  # Commands run from the palette that Ctrl+P lists first, 0 to 20.
  recentCount: 3

editor:
  # Most selections Select All Matches, Select All Occurrences and the
  # cursor commands may create, 1 to 1,000,000. Larger requests are refused.
  multiCursorLimit: 10000

search:
  # Memory one Replace All may use, in MiB, 1 to 4096. A larger replacement
  # is refused before it starts.
  replaceAllMaxMiB: 512

syntax:
  # Files larger than this many MiB open as plain text, 1 to 1024.
  # Highlighting a large file costs memory: about 30 to 130 bytes per byte.
  maxFileMiB: 32
  # Highlight code embedded in other languages: fenced code blocks and front
  # matter in Markdown, <script> and <style> in HTML.
  injections: true

spelling:
  # Mark misspelled words of plain text and Markdown prose.
  enabled: false
  # builtin: a word list in the executable.
  # hunspell: the .aff / .dic pair named by spelling.hunspell.
  # windows: the Windows spell checker (US English must be installed).
  engine: builtin
  # Base path of the Hunspell files, relative to the executable
  # (Dictionaries/en_US means Dictionaries/en_US.aff and en_US.dic).
  hunspell: Dictionaries/en_US

diff:
  # Leading and trailing whitespace of a line is not a change.
  ignoreTrimWhitespace: true
  # Files larger than this many MiB are shown but not compared, 1 to 1024.
  maxFileMiB: 50
  # After this many milliseconds the diff is simplified: its changes are
  # coarser, 100 to 60000.
  maxComputationMs: 5000
```

## Palette

| Key           | Default | Values  | Remarks                                                                   |
| ------------- | ------- | ------- | ------------------------------------------------------------------------- |
| `recentCount` | `3`     | 0 to 20 | Recent commands at the top of `Ctrl+P`, above the favorites; 0 hides them |

## Editor

| Key                | Default | Values         | Remarks                                                                          |
| ------------------ | ------- | -------------- | -------------------------------------------------------------------------------- |
| `multiCursorLimit` | `10000` | 1 to 1,000,000 | Most selections at once; a command that would make more is refused with a notice |

## Search

| Key                | Default | Values    | Remarks                                                              |
| ------------------ | ------- | --------- | -------------------------------------------------------------------- |
| `replaceAllMaxMiB` | `512`   | 1 to 4096 | Memory of one Replace All; a larger one is refused before any change |

## Syntax

| Key          | Default | Values         | Remarks                                                                     |
| ------------ | ------- | -------------- | --------------------------------------------------------------------------- |
| `maxFileMiB` | `32`    | 1 to 1024      | Larger files open as plain text, also when an edit grows them past it       |
| `injections` | `true`  | `true`/`false` | Embedded languages (Markdown fences and front matter, HTML scripts, styles) |

`--syntax none` turns highlighting off for a session; see
[Syntax Highlighting](syntax-highlighting.md).

## Spelling

| Key        | Default              | Values                           | Remarks                                                                 |
| ---------- | -------------------- | -------------------------------- | ----------------------------------------------------------------------- |
| `enabled`  | `false`              | `true`/`false`                   | Spelling at startup; `View: Toggle Spelling` changes it for the session |
| `engine`   | `builtin`            | `builtin`, `hunspell`, `windows` | An engine that cannot start falls back to `builtin`                     |
| `hunspell` | `Dictionaries/en_US` | A relative path                  | Base of the `.aff` and `.dic` files, from the executable's folder       |

`--spelling off|builtin|hunspell|windows` overrides `enabled` and `engine`
for a session; see [Spelling](spelling.md).

## Diff

The defaults are VS Code's.

| Key                    | Default | Values         | Remarks                                                                 |
| ---------------------- | ------- | -------------- | ----------------------------------------------------------------------- |
| `ignoreTrimWhitespace` | `true`  | `true`/`false` | `Diff: Toggle Ignore Trim Whitespace` changes it for the session        |
| `maxFileMiB`           | `50`    | 1 to 1024      | A larger file is shown unaligned; the status line says why              |
| `maxComputationMs`     | `5000`  | 100 to 60000   | A longer diff is simplified: it stays exact but its changes are coarser |

See [Comparing and Merging](comparing-and-merging.md).

## Files Beside the Executable

| Path                              | Written by the application | Contents                                    |
| --------------------------------- | -------------------------- | ------------------------------------------- |
| `RoyiTextMerger.yaml`             | Never                      | These settings                              |
| `UserSettings/Palette.json`       | Yes                        | Favorite and recent commands                |
| `UserSettings/Spelling/en_US.txt` | Yes                        | Words added by Add to Dictionary            |
| `Dictionaries/`                   | Never                      | The shipped Hunspell dictionary and license |
