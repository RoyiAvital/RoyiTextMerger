//! Work accounting: cancellation, the deadline and work limits.

use std::time::Instant;

use crate::{CancelToken, Cancelled};

/// Work units between checks of the cancel token and the clock.
const CHECK_EVERY: u64 = 1 << 14;

/// Counts the work of a computation and checks every so often whether it
/// was cancelled or ran past its deadline or work limit.
pub(crate) struct Budget<'a> {
    cancel: Option<&'a CancelToken>,
    deadline: Option<Instant>,
    limit: u64,
    work: u64,
    next_check: u64,
    expired: bool,
}

impl<'a> Budget<'a> {
    pub fn new(
        cancel: Option<&'a CancelToken>,
        deadline: Option<Instant>,
        limit: u64,
    ) -> Budget<'a> {
        Budget {
            cancel,
            deadline,
            limit,
            work: 0,
            next_check: 0,
            expired: false,
        }
    }

    /// Adds `work`; false once past the deadline or the limit (it stays so).
    pub fn tick(&mut self, work: u64) -> Result<bool, Cancelled> {
        if self.expired {
            return Ok(false);
        }
        self.work = self.work.saturating_add(work);
        if self.work > self.limit {
            self.expired = true;
            return Ok(false);
        }
        if self.work >= self.next_check {
            self.next_check = self.work.saturating_add(CHECK_EVERY);
            if self.cancel.is_some_and(CancelToken::is_cancelled) {
                return Err(Cancelled);
            }
            if self.deadline.is_some_and(|d| Instant::now() >= d) {
                self.expired = true;
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Adds `work` that must finish (verification): only cancelling stops it.
    pub fn check_cancel(&mut self, work: u64) -> Result<(), Cancelled> {
        self.work = self.work.saturating_add(work);
        if self.work >= self.next_check {
            self.next_check = self.work.saturating_add(CHECK_EVERY);
            if self.cancel.is_some_and(CancelToken::is_cancelled) {
                return Err(Cancelled);
            }
        }
        Ok(())
    }

    pub fn expired(&self) -> bool {
        self.expired
    }
}
