// intentcode: ../../intent/utils.icode
// spec-hash: 172cda0bfacc5fae  (sha256 prefix of: utils.icode)
// generated: claude-sonnet-5-5, by hand-driven compile, not reproducible
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use chrono::{DateTime, Utc};

// intent: utils::retry
/// `@retry($backoff, $attempts tries) on $errors`
///
/// Runs `op` up to `attempts` times. Only errors accepted by `retryable` are retried,
/// sleeping `backoff(n)` before attempt n+1 (n starts at 0). Any other error is returned at once.
pub async fn retry<T, E, Fut>(
    backoff: impl Fn(u32) -> Duration,
    attempts: u32,
    retryable: impl Fn(&E) -> bool,
    mut op: impl FnMut() -> Fut,
) -> Result<T, E>
where
    Fut: Future<Output = Result<T, E>>,
{
    let mut n = 0;
    loop {
        match op().await {
            Ok(value) => return Ok(value),
            Err(e) if retryable(&e) && n + 1 < attempts => {
                tokio::time::sleep(backoff(n)).await;
                n += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

/// `2^n * base` seconds, the exponential backoff used by every `@retry` in the spec.
pub fn exponential(base_secs: f64) -> impl Fn(u32) -> Duration {
    move |n| Duration::from_secs_f64(base_secs * 2f64.powi(n as i32))
}

// The spec's `now`. Tests use `Clock::manual` to control time.
#[derive(Clone)]
pub struct Clock(Option<Arc<AtomicI64>>);

impl Clock {
    pub fn system() -> Self {
        Clock(None)
    }

    pub fn manual(start: DateTime<Utc>) -> Self {
        Clock(Some(Arc::new(AtomicI64::new(start.timestamp_millis()))))
    }

    pub fn advance(&self, by: chrono::Duration) {
        if let Some(t) = &self.0 {
            t.fetch_add(by.num_milliseconds(), Ordering::SeqCst);
        }
    }

    pub fn now(&self) -> DateTime<Utc> {
        match &self.0 {
            None => Utc::now(),
            Some(t) => DateTime::from_timestamp_millis(t.load(Ordering::SeqCst)).unwrap(),
        }
    }
}
