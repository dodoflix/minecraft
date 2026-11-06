//! `RetryOptions`.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetryOptions {
    pub number_of_tries: i32,
    pub halt_on_failure: bool,
}

impl RetryOptions {
    pub const fn new(number_of_tries: i32, halt_on_failure: bool) -> Self {
        Self {
            number_of_tries,
            halt_on_failure,
        }
    }

    /// `noRetries()`.
    pub const fn no_retries() -> Self {
        Self::new(1, true)
    }

    pub fn unlimited_tries(&self) -> bool {
        self.number_of_tries < 1
    }

    pub fn has_tries_left(&self, attempts: i32, successes: i32) -> bool {
        let has_failures = attempts != successes;
        let has_more_attempts = self.unlimited_tries() || attempts < self.number_of_tries;
        has_more_attempts && (!has_failures || !self.halt_on_failure)
    }

    pub fn has_retries(&self) -> bool {
        self.number_of_tries != 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_rules_match_vanilla() {
        let none = RetryOptions::no_retries();
        assert!(!none.has_retries() && !none.has_tries_left(1, 1));
        let three = RetryOptions::new(3, false);
        assert!(
            three.has_retries()
                && three.has_tries_left(1, 0)
                && three.has_tries_left(2, 2)
                && !three.has_tries_left(3, 3)
        );
        let until_failed = RetryOptions::new(0, true);
        assert!(
            until_failed.unlimited_tries()
                && until_failed.has_tries_left(50, 50)
                && !until_failed.has_tries_left(50, 49)
        );
    }
}
