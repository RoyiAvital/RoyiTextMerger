# Syntax Highlighting

Code is detected from its file name or `#!` line, for each pane on its own.
It opens immediately as plain text; syntax colors follow as parsing
completes, also over the diff colors.

Press `Ctrl+K M` to choose the language of the focused pane, or **Plain
Text**. The picker filters as you type. To toggle highlighting, open the
Command Palette with `Ctrl+Shift+P` and run **View: Toggle Syntax
Highlighting**.

![Language picker with supported modes](assets/syntax-language-picker.png)

Supported modes: Bash, Batch, C, C#, C++, CSS, Go, HTML, INI, Java,
JavaScript, JSON, Julia, Lua, Markdown, MATLAB, Octave, PowerShell, Python,
Rust, SQL, TOML, TSX, TypeScript, XML, and YAML.

Files larger than 32 MiB are plain text by default. Set `syntax.maxFileMiB`
from 1 to 1024 in `RoyiTextMerger.yaml` to change the limit. Markdown and HTML
also highlight embedded code by default; set `syntax.injections: false` to
disable it. Launch with `--syntax <id>` to highlight both command-line files
as one language, or `--syntax none` to start with highlighting off.
