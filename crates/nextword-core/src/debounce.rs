//! 50ms debouncer with cancellation. The latest call wins; in-flight
//! work is cancelled via the returned `CancellationToken`.

use std::sync::Arc;
use std::time::Duration;
use parking_lot::Mutex;
use tokio_util::sync::CancellationToken;

const DEBOUNCE_MS: u64 = 50;

#[derive(Default)]
pub struct Debouncer {
    state: Mutex<Option<CancellationToken>>,
}

impl Debouncer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Schedule a debounced call. Returns the token bound to the new attempt.
    /// Caller should await `tokio::time::sleep(DEBOUNCE_MS)` and then check
    /// `token.is_cancelled()` before doing the real work.
    pub fn schedule(&self) -> DebounceGuard {
        let token = CancellationToken::new();
        let mut state = self.state.lock();
        if let Some(prev) = state.take() {
            prev.cancel();
        }
        *state = Some(token.clone());
        DebounceGuard { token, debounce: Duration::from_millis(DEBOUNCE_MS) }
    }

    pub fn cancel_all(&self) {
        if let Some(t) = self.state.lock().take() {
            t.cancel();
        }
    }
}

pub struct DebounceGuard {
    pub token: CancellationToken,
    pub debounce: Duration,
}

impl DebounceGuard {
    pub async fn wait(&self) -> bool {
        tokio::select! {
            _ = tokio::time::sleep(self.debounce) => !self.token.is_cancelled(),
            _ = self.token.cancelled() => false,
        }
    }
}

pub fn shared() -> Arc<Debouncer> {
    Arc::new(Debouncer::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn second_call_cancels_first() {
        let d = Debouncer::new();
        let g1 = d.schedule();
        let g2 = d.schedule();
        assert!(g1.token.is_cancelled());
        assert!(!g2.token.is_cancelled());
        assert!(g2.wait().await);
    }
}
