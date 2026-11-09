//! Startup stage timing (`--trace-startup`), relative to process creation.

use std::io::Write;

pub struct Trace {
    enabled: bool,
    marks: Vec<(&'static str, f64)>,
}

impl Trace {
    pub fn new(enabled: bool) -> Trace {
        Trace {
            enabled,
            marks: Vec::new(),
        }
    }

    /// Records `stage` now.
    pub fn mark(&mut self, stage: &'static str) {
        if self.enabled {
            self.mark_at(stage, rtm_platform::ms_since_process_start());
        }
    }

    /// Records a time measured elsewhere (another thread). A stage reached
    /// again (by the other pane) keeps the later time: both reached it.
    pub fn mark_at(&mut self, stage: &'static str, ms: f64) {
        if !self.enabled {
            return;
        }
        match self.marks.iter_mut().find(|(s, _)| *s == stage) {
            Some((_, at)) => *at = at.max(ms),
            None => self.marks.push((stage, ms)),
        }
    }

    /// Prints `startup.<stage>_ms=<value>` lines and the peak private memory
    /// (`memory.peak_private_mib=<value>`) to stdout (a pipe when traced).
    pub fn report(&self) {
        if !self.enabled {
            return;
        }
        let mut out = std::io::stdout().lock();
        for (stage, ms) in &self.marks {
            let _ = writeln!(out, "startup.{stage}_ms={ms:.3}");
        }
        let peak = rtm_platform::peak_private_bytes() as f64 / (1024.0 * 1024.0);
        let _ = writeln!(out, "memory.peak_private_mib={peak:.2}");
        let _ = out.flush();
    }
}
