[![Visitors](https://api.visitorbadge.io/api/combined?path=https%3A%2F%2Fgithub.com%2FRoyiAvital%2FRoyiTextMerger&labelColor=%23f47373&countColor=%23555555&style=plastic)](https://github.com/RoyiAvital/RoyiTextMerger) <!-- https://www.visitorbadge.io -->

# Royi Text Merger - Development

## Design 

### Guidelines

 - Main Use Case  
   Dual pane text merger optimized for code.
   Launched for editing or viewing from a _File Manager_.  
 - Speed
   Most important feature. Time to load empty file, time to load short file, time to show command palette, keystroke rendering delay.  
   Aim for less than 50 [Mili Sec] load time for a short file.  
   Aim for small memory footprint when it serves the performance need.
 - Single Session  
   Editing a pair of files.
   Launched by `RoyiTextMerger <PathToLeftFile> <PathToRightFile>`; files can also be dropped onto a pane.
   Single session. No awareness of other sessions.
   No ability to interact, launch or close other session.
   Each application window is self contained.
 - No Menus / Keyboard Focused
   All window is dedicated to text.  
   Commands through the _Command Palette_ / _Command Center_.
 - Advanced Text Editing Capabilities  
   Parity with _RoyiTextEditor_.
 - Advanced Search Capabilities  
   Parity with _RoyiTextEditor_ per Pane.


### Main Features

 - Syntax Highlighting  
   Efficient Tree Sitter based Syntax Highlighting engine.
 - _Command Center_ / _Command Palette_ Driven  
   Access all commands by <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>P</kbd>.  
   Access last 3 commands and favorites commands by <kbd>Ctrl</kbd>+<kbd>P</kbd>.  
 - Merge Mode / Read Only Mode / Diff Mode  
   By default both sides are editable and changes copy both ways (_merge mode_).  
   _Read only mode_ (`--read-only`) makes the left side read only, as VS Code.  
   _Diff mode_ (`--diff-mode`) makes both sides read only, focusing on the differences.  
   Both through `cli` and command (_Command Palette_); read only also per file.
 - CLI Parameters  
   Use CLI parameters to configure the session: Read Only / Diff Mode.
 - Safety
   Awareness to external editing.
   Parity with _RoyiTextEditor_


### Design Choices

 - Fonts  
   Using limited number of Monospaced fonts built in.  
 - Customized UI / Text Engine Framework  
   Customized CPU renderer, with persistent piece tree text engine.  
   Frameless window.
 - No IDE
   NO LSP or dynamic code analysis. 
 - Optimized for Use from a File Manager
   Optimized for the use case of being launched from a _File Manager_ with a pair of files.  
   No need for _Save As_ or _New File_ operations within the application.  
   Files come from the command line or by _Drag & Drop_ onto a pane.  
   A pane without a file shows an _Empty State_: nothing to edit, so nothing to lose.  
   Closing (<kbd>Ctrl</kbd>+<kbd>W</kbd>) returns both panes to the empty state; <kbd>Alt</kbd>+<kbd>F4</kbd> quits.  
   An editable document that can not be saved (an _Untitled_ scratch) does not exist.
 - Configuration File
   Based on a single `yaml` file next to the executable: `RoyiTextMerger.yaml`.  
   The application only reads it.
 - Left to Right Editing  
   Being focused on code, the merger is LTR only.
 - Modern Architecture  
   Design aims for modern CPU's and Operating System.   
   The CPU requirement is by `x86-64-v3`.
   The OS requirement is Windows 11 22H2 and above (Not strictly forced).
   The Toolchain in use: `x86_64-pc-windows-gnu`.
 - Large Memory Computer  
   While the application is optimized towards low memory footprint it
   is also designed to use available memory. Hence it loads large files
   into memory to keep them smooth. 
 - Follow VS Code  
   In general, we follow VS Code behavior in its Diff / Merge mode.  
 - User Knows What He Does  
   Don't protect misuse / "stupid user".
 - Single User, Single Computer  
   The use case of the application is within a single computer by a single user.  
   No protection against user initiated operations that happen closer together than a person can act.
 - Startup Time / Periodic Checks  
   Don't do things at startup which can be done later.  
   First strive to show the file to the user.  
   Hence refrain from checks on startup or periodically.  
   For instance, check for read only attribute of a file only before saving.

### Alternatives

 - [Beyond Compare](https://www.scootersoftware.com/).
 - [Meld Merge](https://gnome.pages.gitlab.gnome.org/meld).

## Environment

The workspace is created by M0 of [Architecture 001](Plan/Architecture001.md)
(a fork of _RoyiTextEditor_); the commands below apply once it exists.

Requirements:

 - Windows 11 x64 with an AVX2-class CPU (`x86-64-v3`).
 - [rustup](https://rustup.rs). `rust-toolchain.toml` selects
   `stable-x86_64-pc-windows-gnu` with `rustfmt` and `clippy`, so MSVC-default
   machines also build with GNU. Linking uses the MinGW runtime bundled with
   Rust (`.cargo/config.toml`).
 - MinGW-w64 with `gcc` and `windres` on `PATH` (for example, the `bin` folder
   of a WinLibs or MSYS2 UCRT64 toolchain). `gcc` compiles the Tree-sitter
   grammars (C and C++ sources); the MinGW runtime bundled with Rust is not a
   compiler. `windres` (or `WINDRES` set to its path) compiles the application
   manifest and icon into the executable.
 - Internet access on the first build (crates and fonts).

```powershell
cargo build --release -p rtm_app
cargo run --release -p rtm_app -- <left> <right>
```

The first build downloads the five bundled font families into `assets/fonts`
(not in Git): the `rtm_assets` build script runs `tools/Get-Fonts.ps1`, which
fetches pinned releases and verifies their SHA-256 hashes. Archives are cached
in `target/font-cache`; verified files are reused offline.

`Meta/AppMeta.json` holds the display `name` (window title, About, release and
documentation titles), the `file name` (executable, configuration file and
release archive), the version and the repository and documentation links. The
`rtm_app` build script fails when its version differs from `[workspace.package]`
in `Cargo.toml` or its `file name` differs from the `[[bin]]` name. It also
renders `Meta/Icon.svg` (64 px and up) and
`Meta/IconSmall.svg` (48 px and below) into the executable's icon.

Auxiliary tools that are not part of the build live in `tools/`; usage is at the
top of each file.

### Tests and Packaging

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo xtask check-imports
cargo xtask package
```

During development run the tests of the changed crates (`cargo test -p rtm_view`)
rather than the whole workspace. Format, Clippy and the workspace tests run on
GitHub only in the `Release` workflow (tags `vX.Y.Z` and manual dry runs);
ordinary pushes and pull requests run no checks.

`check-imports` fails when the release executable imports a MinGW runtime DLL
(`libgcc_s_seh-1.dll`, `libwinpthread-1.dll`, `libstdc++-6.dll`) or lacks the
embedded manifest. `package` builds the release, runs that check and writes
`target/package/<file name>-<version>-windows-x86_64.zip` (from
`Meta/AppMeta.json`). The ZIP contains a top-level `<file name>/` folder with
the executable, `Fonts`, `README.md` and `CHANGELOG.md`; only the ZIP filename
includes the version and platform. It prints the archive path last.

### Releasing

Releases are built by the `Release` GitHub Actions workflow
(`.github/workflows/release.yml`). It never runs on ordinary pushes; it runs
only when a version tag is pushed:

1. Set the same `version` in `Meta/AppMeta.json` and in `[workspace.package]`
   of `Cargo.toml`. CI sets the workspace crates' version in `Cargo.lock`
   (`.github/scripts/Sync-LockVersions.ps1`); dependencies stay locked.
2. In `CHANGELOG.md`, add `## [X.Y.Z]` below the first section,
   `## [Unreleased]`, and move this release's entries into it. A
   ` - YYYY-MM-DD` date suffix is optional. When performance results are
   wanted, `### Performance` holds the Short Benchmark table (see below).
3. Check the versions locally:

   ```powershell
   .\.github\scripts\Resolve-Version.ps1
   .\.github\scripts\Get-ReleaseNotes.ps1 -Version X.Y.Z
   ```

4. Commit, tag and push:

   ```powershell
   git add -A
   git commit -m "Release vX.Y.Z"
   git push origin main
   git tag -a vX.Y.Z -m "RoyiTextMerger vX.Y.Z"
   git push origin vX.Y.Z
   ```

The workflow verifies that `Meta/AppMeta.json` and `Cargo.toml` agree, that the
tag is annotated and equals `v<version>`, that `## [Unreleased]` is the first
changelog section and that a matching `## [X.Y.Z]` section exists. It runs the
format check, Clippy, the workspace tests and `cargo xtask package` (no
benchmarks), then publishes a
GitHub release titled `<name> X.Y.Z`. The notes are that version's section; the
assets are the ZIP and its SHA-256. Tags with a suffix (`v1.0.0-rc.1`) are
published as pre-releases. An existing release is never replaced.

Run the workflow manually from the Actions tab (`workflow_dispatch`) on a branch
to build without publishing; the ZIP and the notes (taken from
`## [Unreleased]`) are then available as a workflow artifact.

#### Retrying a Failed Release

In case of a failure of the GitHub Action before the release is published,
apply the fix and move the tag to the new commit:

```powershell
git add -A
git commit -m "Release vX.Y.Z"
git push origin main
git tag -f -a vX.Y.Z -m "RoyiTextMerger vX.Y.Z"
git push --force origin refs/tags/vX.Y.Z
```

To rerun the workflow on an unchanged tag, dispatch it on the tag (this
publishes):

```powershell
gh workflow run release.yml --ref vX.Y.Z
```

An alternative without the GitHub CLI:

```powershell
$repo = (git remote get-url origin) -replace '^.*github\.com[:/](.+?)(\.git)?$', '$1'
$token = Read-Host "GitHub token" -AsSecureString
$credential = New-Object System.Net.NetworkCredential('', $token)

Invoke-RestMethod -Method Post `
  -Uri "https://api.github.com/repos/$repo/actions/workflows/release.yml/dispatches" `
  -Headers @{
    Authorization = "Bearer $($credential.Password)"
    Accept = 'application/vnd.github+json'
  } `
  -ContentType 'application/json' `
  -Body '{"ref":"vX.Y.Z"}'
```

When a broken release was already published, delete it (and its tag) on GitHub
first; the workflow refuses to replace an existing release.

### Documentation

The user documentation is an [mdBook](https://rust-lang.github.io/mdBook/)
under `docs/`: pages in `docs/src` (navigation in `docs/src/SUMMARY.md`),
configuration in `docs/book.toml` and the Material-like dark theme in
`docs/theme`. It is published to GitHub Pages by the `Pages` workflow on
pushes to `main` that change the documentation, its tools or the application
(its screenshots), and on demand. Publishing needs the repository's
**Settings > Pages > Source** set to **GitHub Actions** (once); until then the
workflow only builds the book and shows a warning.

`docs/src/SUMMARY.md` defines sections, page order and hierarchy. Content lives
in separate files; the introduction is `docs/src/introduction.md`. The build
reads the summary directly and regenerates all screenshots for the book,
restoring existing source assets afterwards.

Serve it locally with live reload:

```powershell
.\tools\Build-Docs.ps1 -Serve
```

The browser opens at <http://localhost:3000>; saving a page under `docs/`
rebuilds and refreshes it. Stop the server with <kbd>Ctrl</kbd>+<kbd>C</kbd>.

Build it once, as the `Pages` workflow does (output in `target/book`):

```powershell
.\tools\Build-Docs.ps1
```

The screenshot step needs the GNU Rust toolchain and MinGW-w64 `gcc` and
`windres` (see Environment above). The Pages workflow installs Rust before this build.
Python and Node.js are not required. The script downloads pinned releases of
`mdbook`, `mdbook-pagetoc` and `mdbook-mermaid` into `target/docs-tools`, verifies
their SHA-256 hashes and reuses them afterwards. The book title comes from
`Meta/AppMeta.json`.

Callouts use mdBook's built-in syntax:

```markdown
> [!TIP]
> Text of the tip.
```

Kinds: `NOTE`, `TIP`, `IMPORTANT`, `WARNING` and `CAUTION`. Keys are written as
`<kbd>Ctrl</kbd>+<kbd>S</kbd>`.

### Performance Benchmark

The Merger Performance Benchmark, its specification and the results of each
version are in [Performance.md](Performance.md):

```powershell
cargo xtask bench-perf                 # Performance Benchmark (about 3 s)
cargo xtask bench-perf --short         # Short Benchmark: release-notes table
cargo xtask bench-perf --only L01,M01  # A subset
cargo xtask bench-startup              # Startup stages; gate: p95 < 100 ms
cargo xtask bench-startup --pair       # The same with a pair of files
cargo bench -p rtm_text                # Text engine (Ropey baseline)
cargo bench -p rtm_diff                # Diff engine (imara-diff baseline)
cargo bench -p rtm_render              # Full-frame render cost (1080p, 4K)
cargo bench -p rtm_edit                # Worst-case caret movement latency
```

`bench-perf` builds the release executable and launches a fresh process per
load repetition; its fixtures are generated deterministically under
`target/perf` and verified by hashes. With two files, the startup trace ends
with the `compared` stage (the first exact comparison). For the release
notes, `### Performance` holds the Short Benchmark table
(`cargo xtask bench-perf --short`, or the matching rows of `Performance.md`).

Measure on the reference machine on AC power, without other heavy load, and do
not run tests or builds concurrently. Benchmarks run only locally; no workflow
runs them.

`tools/SyntaxProbe` measures tree-sitter parse time and syntax-tree memory.