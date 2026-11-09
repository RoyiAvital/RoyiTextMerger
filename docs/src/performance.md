# Performance

Two files are shown at once while large files finish loading in the
background; the comparison follows on its own thread. The figures below are
medians from the Merger Performance Benchmark v1 for release 0.1.0.

## Recorded Results

| Scenario                      | First paint | Compared | Peak private memory | Key to frame |
| ----------------------------- | ----------- | -------- | ------------------- | ------------ |
| No files                      | 25.8 ms     | —        | 5.1 MiB             | —            |
| Two C files, 64 KiB           | 26.6 ms     | 17.4 ms  | 13.4 MiB            | —            |
| Two C files, 8 MiB            | 42.4 ms     | 68.0 ms  | 48.2 MiB            | —            |
| Two logs, 48 MiB              | 30.8 ms     | 102.3 ms | 130.7 MiB           | —            |
| Find as you type, 8 MiB       | —           | —        | —                   | 3.56 ms      |
| Next Change among 500 changes | —           | —        | —                   | 1.40 ms      |
| Copy a change and go to next  | —           | —        | —                   | 2.95 ms      |
| Type in a change among 500    | —           | —        | —                   | 3.37 ms      |

The machine was an Intel Core i9-14900 with 128 GiB RAM, Windows 11
Enterprise and a 1920 × 1080 display. The release build used a warm file
cache. These values are specific to that setup and benchmark version.

**First paint** is the time from process start until the first frame is drawn
to the back buffer. **Compared** is the time until the first exact comparison
is shown; small files are compared before the first paint, so their first
frame is aligned. **Key to frame** measures the time from a handled key to the
frame that shows its result, including the comparison it starts.

## Reproduce

Run the short report from the repository root:

```powershell
cargo xtask bench-perf --short
```

The complete procedure, fixtures, and recorded results are in
[Performance.md](https://github.com/RoyiAvital/RoyiTextMerger/blob/main/Performance.md).
The harness also provides `cargo xtask bench-startup --pair` for the startup
gate. Crate-specific benchmarks are available with `cargo bench -p rtm_diff`,
`cargo bench -p rtm_text`, `cargo bench -p rtm_edit`, and
`cargo bench -p rtm_render`.
