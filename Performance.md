# Performance

## 0.1.0

### Main Performance Oriented Changes

- Built on the Royi Text Editor engine: persistent piece tree, progressive
  loading of large files, a CPU renderer and syntax threads per pane.
- The diff runs on its own thread (Git's histogram diff with Myers, within a
  time budget); the panes are shown at once and aligned when it arrives. The
  first frame waits at most 10 ms for it, so small pairs open aligned.
- Edits map the shown diff on the UI thread (0.74 ms at 100,000 hunks) and
  only the edited regions are compared again, with 100 lines of context.
- Comparing two versions of one text skips their shared subtrees: an undo in
  a 1 GiB file takes 1.2 µs instead of 8.3 ms.
- A two-pane frame takes 0.84 ms at 1080p and 2.50 ms at 4K.

### Performance Results

Using the Merger Performance Benchmark v`1` (`cargo xtask bench-perf`).

Machine: Intel Core i9-14900, 128 GiB RAM, Windows 11 Enterprise, 1920 × 1080,
release build, warm file cache. Run from a PowerShell job with redirected
output; key-to-frame times vary by 1-2 ms between runs.

| ID  | Test                             | Metric             | 0.1.0 |
| --- | -------------------------------- | ------------------ | ----- |
| L01 | Two C files, 64 KiB              | First paint [ms]   | 26.6  |
| L01 | Two C files, 64 KiB              | Files read [ms]    | 12.0  |
| L01 | Two C files, 64 KiB              | Compared [ms]      | 17.4  |
| L01 | Two C files, 64 KiB              | Peak private [MiB] | 13.4  |
| L02 | Two C files, 8 MiB               | First paint [ms]   | 42.4  |
| L02 | Two C files, 8 MiB               | Files read [ms]    | 17.0  |
| L02 | Two C files, 8 MiB               | Compared [ms]      | 68.0  |
| L02 | Two C files, 8 MiB               | Peak private [MiB] | 48.2  |
| L03 | Two logs, 48 MiB                 | First paint [ms]   | 30.8  |
| L03 | Two logs, 48 MiB                 | Files read [ms]    | 40.7  |
| L03 | Two logs, 48 MiB                 | Compared [ms]      | 102.3 |
| L03 | Two logs, 48 MiB                 | Peak private [MiB] | 130.7 |
| L04 | No files                         | First paint [ms]   | 25.8  |
| L04 | No files                         | Peak private [MiB] | 5.1   |
| S01 | Find as you type, 8 MiB          | Key to frame [ms]  | 3.56  |
| S01 | Find as you type, 8 MiB          | p95 [ms]           | 4.47  |
| S02 | Find the next match, 8 MiB       | Key to frame [ms]  | 0.90  |
| S02 | Find the next match, 8 MiB       | p95 [ms]           | 3.00  |
| N01 | Next change, 500 changes         | Key to frame [ms]  | 1.40  |
| N01 | Next change, 500 changes         | p95 [ms]           | 3.22  |
| N02 | Previous change, 500 changes     | Key to frame [ms]  | 1.90  |
| N02 | Previous change, 500 changes     | p95 [ms]           | 3.41  |
| M01 | Copy and go to next, 500 changes | Key to frame [ms]  | 2.95  |
| M01 | Copy and go to next, 500 changes | p95 [ms]           | 4.10  |
| M02 | Copy and go to next, 8 MiB       | Key to frame [ms]  | 3.49  |
| M02 | Copy and go to next, 8 MiB       | p95 [ms]           | 4.04  |
| M03 | Undo a copy, 500 changes         | Key to frame [ms]  | 1.13  |
| M03 | Undo a copy, 500 changes         | p95 [ms]           | 2.99  |
| E01 | Type in a change, 500 changes    | Key to frame [ms]  | 3.37  |
| E01 | Type in a change, 500 changes    | p95 [ms]           | 3.78  |
| R01 | Page down, 500 changes           | Key to frame [ms]  | 1.48  |
| R01 | Page down, 500 changes           | p95 [ms]           | 3.04  |

Small pairs are compared before the first paint, so their first frame is
aligned. L02's first paint includes the 10 ms wait for the diff; the code
files are highlighted from the first frame. L04 starts with two empty panes.
The whole run took 3.4 s.

## Performance Benchmarks

### v1

Status: implemented in `cargo xtask bench-perf`
([xtask/src/perf.rs](xtask/src/perf.rs)). The suite should not exceed 20 s
on the reference machine, fixture generation included; it takes about 3 s.
`--only L01,M01` runs a subset (its total is not checked); unknown options or
test IDs and an empty selection are errors.

#### Procedure

- Builds the release binary, then runs every test in fresh processes.
- **Load tests** launch `RoyiTextMerger.exe --trace-startup --exit-after-load
  <left> <right>` (L04 without files). The process exits once its first frame
  is shown and both files are read and compared. Times are measured from
  process creation:
  - **First paint:** the first frame is drawn into the back buffer.
  - **Files read:** the loader threads have read both files (pairs only).
  - **Compared:** the first exact comparison is applied (trace stage
    `compared`; pairs only).
  - **Peak private:** `PeakPagefileUsage` reported at exit.
- **Input tests** run one process per test with a scripted input sequence
  (`--bench-setup <steps> --bench-keys <steps> --bench-runs <n>`), started
  once both files are compared, in the left pane. A step is a key or quoted
  text (`'abc'`, one character per step). Untimed setup steps come first.
  Each repetition is timed from the handling of its first step to the frame
  drawn after its work, the comparison it starts and the search results
  included. Peak private memory is reported at exit.
- Statistics: median of all repetitions; p95 for input tests. One warm-up
  per test, not counted.
- Each load test has a 5 s cap and each input test a 3 s cap; a test over its
  cap fails the run.
- Results are comparable only within one benchmark version. Once a version
  has published results, changing a fixture, a test or the procedure creates
  a new version.

#### Reproducibility

- **Command:** `cargo xtask bench-perf`. The harness, the fixture generators
  and the recorded fixture hashes are in the repository.
- **Fixtures:** generated by fixed code without randomness or time stamps and
  verified by XXH3-64 (table below). Existing files are reused when their hash
  matches and regenerated otherwise. A generator that produces a different
  hash fails the run. `cargo xtask bench-perf --print-hashes` regenerates all
  fixtures and prints their sizes and hashes.
- **Build:** the toolchain is pinned by `rust-toolchain.toml`; the release
  profile and target CPU are set in the repository.
- **Recorded environment:** the output starts with the application name and
  version from [Meta/AppMeta.json](Meta/AppMeta.json), the benchmark version,
  the `rustc` version and the CPU.
- **Machine state:** AC power, no other heavy load. The warm-up run of each
  test loads the fixtures into the file cache.

#### Fixtures

Generated into `target/perf/merger-v1/`, kept between runs. The right file of
each pair is the left one with one line changed in every group of lines (the
middle one), in turn replaced (prefixed by `// changed `), followed by an
inserted line, or removed.

| File               | Bytes      | XXH3-64              | Content                                            |
| ------------------ | ---------- | -------------------- | -------------------------------------------------- |
| `small.c`          | 65,082     | `0x9bc8db30d412b721` | C functions, LF, 1,416 lines                       |
| `small-edited.c`   | 64,648     | `0x36f96787c2ed645d` | `small.c`, a change every 40 lines (35 changes)    |
| `medium.c`         | 8,388,345  | `0x667a78bba9ac72ae` | C functions, LF, 176,916 lines                     |
| `medium-edited.c`  | 8,381,852  | `0x80a6eba70425e72d` | `medium.c`, a change every 400 lines (442 changes) |
| `large.log`        | 50,331,572 | `0x007167ae2cf01906` | Unique log lines, CRLF, 466,471 lines              |
| `large-edited.log` | 50,328,507 | `0x0aa1e5ba42080812` | `large.log`, a change every 4,000 lines (116)      |
| `changes.c`        | 464,767    | `0x057e008afce340c3` | C functions, LF, 9,996 lines                       |
| `changes-edited.c` | 465,481    | `0x655dfc81e88099cc` | `changes.c`, a change every 20 lines (500 changes) |

#### Tests

Load tests:

| ID  | Test                | Pair                            | Runs |
| --- | ------------------- | ------------------------------- | ---- |
| L01 | Two C files, 64 KiB | `small.c`, `small-edited.c`     | 10   |
| L02 | Two C files, 8 MiB  | `medium.c`, `medium-edited.c`   | 5    |
| L03 | Two logs, 48 MiB    | `large.log`, `large-edited.log` | 3    |
| L04 | No files            | None (two empty panes)          | 10   |

Input tests (the timed step is the first one):

| ID  | Test                             | Pair    | Setup                           | Steps              | Runs |
| --- | -------------------------------- | ------- | ------------------------------- | ------------------ | ---- |
| S01 | Find as you type, 8 MiB          | medium  | `Ctrl+F`, `'sum_'`              | `'1'`, `Backspace` | 10   |
| S02 | Find the next match, 8 MiB       | medium  | `Ctrl+F`, `'total'`             | `Enter`            | 20   |
| N01 | Next change, 500 changes         | changes | None                            | `Alt+F5`           | 30   |
| N02 | Previous change, 500 changes     | changes | None                            | `Shift+Alt+F5`     | 30   |
| M01 | Copy and go to next, 500 changes | changes | `Alt+F5`                        | `Ctrl+Alt+Right`   | 30   |
| M02 | Copy and go to next, 8 MiB       | medium  | `Alt+F5`                        | `Ctrl+Alt+Right`   | 10   |
| M03 | Undo a copy, 500 changes         | changes | `Alt+F5`, `Alt+Right`, `Ctrl+2` | `Ctrl+Z`, `Ctrl+Y` | 20   |
| E01 | Type in a change, 500 changes    | changes | `Alt+F5`                        | `'x'`              | 30   |
| R01 | Page down, 500 changes           | changes | None                            | `PgDn`             | 30   |

#### Short Benchmark

A subset that tells the story for users. Its table is the `### Performance`
section of the release notes in `CHANGELOG.md`.

| Question                                | Test | Measure                      |
| --------------------------------------- | ---- | ---------------------------- |
| How light is it?                        | L04  | Peak memory                  |
| How fast do two files open?             | L01  | Time to first frame          |
| How soon are they compared?             | L01  | Time to the exact comparison |
| How soon are larger files compared?     | L02  | Time to the exact comparison |
| Do two huge files still open instantly? | L03  | Time to first frame          |
| How soon are huge files compared?       | L03  | Time to the exact comparison |
| What memory do huge files cost?         | L03  | Peak memory                  |
| Is search instant?                      | S01  | Key to frame                 |
| Is the next change instant?             | N01  | Key to frame                 |
| Is merging instant?                     | M01  | Key to frame                 |
| Is typing instant while comparing?      | E01  | Key to frame                 |

Time to first frame is the First paint metric; Time to the exact comparison
is Compared.

- When the version's full results are recorded above, copy the matching rows
  from them; do not run the benchmark again.
- Otherwise, `cargo xtask bench-perf --short` runs only these tests (about
  2 s) and prints the release-notes table.
