// SPDX-License-Identifier: FSL-1.1-MIT
//! Our HTTP rules: timeouts, body cap, retry, status mapping. (The client itself comes in increment 2.)

use super::provider::{ProviderError, ProviderId};
use std::time::Duration;

pub struct RetryPolicy {
    pub max_attempts: u32,
    pub backoff: Duration,
    pub max_retry_after: Duration,
}

pub const RETRY: RetryPolicy = RetryPolicy {
    max_attempts: 2,
    backoff: Duration::from_secs(1),
    max_retry_after: Duration::from_secs(5),
};
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
pub const TOTAL_TIMEOUT: Duration = Duration::from_secs(20);
pub const MAX_BODY: usize = 65_536;

/// Runs `f`, retrying once for timeout, 429 (if Retry-After is short) and 5xx. `sleep` is injected.
pub fn with_retry<T, F: FnMut() -> Result<T, ProviderError>>(
    _p: &RetryPolicy,
    _sleep: &dyn Fn(Duration),
    _f: F,
) -> Result<T, ProviderError> {
    todo!()
}

pub fn map_status(_provider: ProviderId, _status: u16, _retry_after: Option<&str>, _body: &[u8]) -> ProviderError {
    todo!()
}

/// Reads the body up to `cap` bytes (content-length first, then chunk by chunk); more = `TooLarge`.
pub async fn read_capped(_resp: reqwest::Response, _cap: usize) -> Result<Vec<u8>, ProviderError> {
    todo!()
}

// Stub phase: nothing in the app calls these yet (increments 1-6 wire them). This reference keeps them
// compiled without a dead-code warning and without an allow (strip-check s05).
const _: () = {
    let _ = RETRY;
    let _ = CONNECT_TIMEOUT;
    let _ = TOTAL_TIMEOUT;
    let _ = MAX_BODY;
    let _ = with_retry::<(), fn() -> Result<(), ProviderError>>;
    let _ = map_status;
    let _ = read_capped;
    let _ = (RETRY.max_attempts, RETRY.backoff, RETRY.max_retry_after);
};
