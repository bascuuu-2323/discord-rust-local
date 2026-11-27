use std::time::Duration;

/// Retry delay used by Discord desktop overlay2, in milliseconds.
pub struct Backoff(f64);

impl Default for Backoff {
    fn default() -> Self {
        Self(500.)
    }
}

impl Backoff {
    fn advance(&mut self, jitter: f64) -> Duration {
        self.0 = (self.0 + self.0 * 2. * jitter).min(5000.);
        Duration::from_millis(self.0 as u64)
    }
    pub fn failure(&mut self) -> Duration {
        self.advance(rand::random())
    }
    pub fn success(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn jitter_cap_and_reset() {
        let mut retry = Backoff::default();
        let delays = [0., 0.5, 0.75, 0.9, 0.25].map(|value| retry.advance(value).as_millis());
        assert_eq!(delays, [500, 1000, 2500, 5000, 5000]);
        retry.success();
        assert_eq!(retry.advance(0.25).as_millis(), 750);
    }
}
