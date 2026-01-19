use std::time::Duration;

const BASE_DELAY: Duration = Duration::from_secs(60);
const MAX_DELAY: Duration = Duration::from_secs(600);
const BACKOFF_FACTOR: u32 = 2;

#[derive(Debug, Clone)]
pub struct RetryState {
    consecutive_failures: u32,
}

impl RetryState {
    pub fn new() -> Self {
        Self {
            consecutive_failures: 0,
        }
    }

    pub fn record_success(&mut self) {
        self.consecutive_failures = 0;
    }

    pub fn record_failure(&mut self) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
    }

    pub fn current_delay(&self) -> Duration {
        if self.consecutive_failures == 0 {
            return BASE_DELAY;
        }

        let factor = BACKOFF_FACTOR.saturating_pow(self.consecutive_failures - 1);
        let delay_secs = BASE_DELAY.as_secs().saturating_mul(factor as u64);

        Duration::from_secs(delay_secs).min(MAX_DELAY)
    }

    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }

    pub fn is_in_backoff(&self) -> bool {
        self.consecutive_failures > 0
    }
}

impl Default for RetryState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_delay() {
        let state = RetryState::new();
        assert_eq!(state.current_delay(), Duration::from_secs(60));
        assert_eq!(state.consecutive_failures(), 0);
        assert!(!state.is_in_backoff());
    }

    #[test]
    fn test_exponential_backoff() {
        let mut state = RetryState::new();

        state.record_failure();
        assert_eq!(state.current_delay(), Duration::from_secs(60));
        assert!(state.is_in_backoff());

        state.record_failure();
        assert_eq!(state.current_delay(), Duration::from_secs(120));

        state.record_failure();
        assert_eq!(state.current_delay(), Duration::from_secs(240));

        state.record_failure();
        assert_eq!(state.current_delay(), Duration::from_secs(480));
    }

    #[test]
    fn test_max_delay_cap() {
        let mut state = RetryState::new();

        for _ in 0..10 {
            state.record_failure();
        }

        assert_eq!(state.current_delay(), Duration::from_secs(600));
    }

    #[test]
    fn test_success_resets_backoff() {
        let mut state = RetryState::new();

        state.record_failure();
        state.record_failure();
        state.record_failure();
        assert_eq!(state.consecutive_failures(), 3);

        state.record_success();
        assert_eq!(state.consecutive_failures(), 0);
        assert_eq!(state.current_delay(), Duration::from_secs(60));
        assert!(!state.is_in_backoff());
    }

    #[test]
    fn test_failure_count_saturates() {
        let mut state = RetryState::new();

        for _ in 0..100 {
            state.record_failure();
        }

        assert_eq!(state.consecutive_failures(), 100);
        assert_eq!(state.current_delay(), Duration::from_secs(600));
    }
}
