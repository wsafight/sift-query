use std::fmt;

/// A failed addition exceeded the configured limit or overflowed `usize`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LimitExceeded;

impl fmt::Display for LimitExceeded {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("counter limit exceeded")
    }
}

impl std::error::Error for LimitExceeded {}

/// An allocation-free counter with checked additions and an inclusive limit.
/// Failed additions leave the counter unchanged. Domain-specific units and
/// error messages belong to the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundedCounter {
    limit: usize,
    used: usize,
}

impl BoundedCounter {
    #[must_use]
    pub const fn new(limit: usize) -> Self {
        Self { limit, used: 0 }
    }

    pub fn try_add(&mut self, amount: usize) -> Result<(), LimitExceeded> {
        let next = self.used.checked_add(amount).ok_or(LimitExceeded)?;
        if next > self.limit {
            return Err(LimitExceeded);
        }
        self.used = next;
        Ok(())
    }

    #[must_use]
    pub const fn used(&self) -> usize {
        self.used
    }

    #[must_use]
    pub const fn limit(&self) -> usize {
        self.limit
    }

    #[must_use]
    pub const fn remaining(&self) -> usize {
        self.limit - self.used
    }
}
