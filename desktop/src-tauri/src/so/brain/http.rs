// SPDX-License-Identifier: FSL-1.1-MIT
//! Our HTTP rules: client settings, timeouts, body cap, retry, status mapping. Providers build on this.
//! The key is only ever a header marked sensitive; never part of a URL. Nothing here logs.

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

/// A new client for one call (dropped after it, so no idle pool threads). Redirects are off: a key
/// never follows a redirect. `strict` = https only; shipped code always passes `true`, and only the
/// provider tests (`with_base`, a local plain-http server) pass `false`.
pub fn client(strict: bool) -> Result<reqwest::Client, ProviderError> {
    let builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(TOTAL_TIMEOUT);
    let builder = if strict { builder.https_only(true) } else { builder };
    builder.build().map_err(|_| ProviderError::Offline)
}

/// Maps a transport failure to a fixed error (never keeps the error text: it can hold a URL).
pub fn map_transport(e: &reqwest::Error) -> ProviderError {
    if e.is_timeout() {
        ProviderError::Timeout
    } else if e.is_connect() {
        ProviderError::Offline
    } else {
        ProviderError::BadReply
    }
}

/// Runs `f`, retrying once for a 429 (if Retry-After is at most `max_retry_after`) and a
/// 5xx. Never retries anything else (a timeout already cost the full wait). `sleep` is injected so tests do not wait.
pub fn with_retry<T, F: FnMut() -> Result<T, ProviderError>>(
    p: &RetryPolicy,
    sleep: &dyn Fn(Duration),
    mut f: F,
) -> Result<T, ProviderError> {
    let mut attempt = 1;
    loop {
        let err = match f() {
            Ok(v) => return Ok(v),
            Err(e) => e,
        };
        if attempt >= p.max_attempts {
            return Err(err);
        }
        let wait = match &err {
            ProviderError::Server(_) => p.backoff,
            ProviderError::RateLimited { retry_after_s: None } => p.backoff,
            ProviderError::RateLimited { retry_after_s: Some(s) } => {
                let wait = Duration::from_secs(*s);
                if wait > p.max_retry_after {
                    return Err(err);
                }
                wait
            }
            _ => return Err(err),
        };
        sleep(wait);
        attempt += 1;
    }
}

/// `Retry-After` as whole seconds, or nothing (dates, signs, decimals and junk are ignored).
fn retry_after_seconds(h: Option<&str>) -> Option<u64> {
    let h = h?.trim();
    if h.is_empty() || !h.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    h.parse().ok()
}

/// Turns an error status plus (capped) body into a fixed error. The body is only searched for known
/// markers; its text is never kept or shown.
pub fn map_status(provider: ProviderId, status: u16, retry_after: Option<&str>, body: &[u8]) -> ProviderError {
    let text = String::from_utf8_lossy(body).to_lowercase();
    let spend = text.contains("spend limit") || text.contains("enforced_spend_limit_reached");
    match (provider, status) {
        (_, 401) => ProviderError::KeyInvalid,
        (ProviderId::Gemini, 400 | 403) if text.contains("api_key_invalid") || text.contains("api key not valid") => {
            ProviderError::KeyInvalid
        }
        (_, 403) => ProviderError::Forbidden,
        (_, 404) => ProviderError::NotFound,
        (ProviderId::Gemini, 402) => ProviderError::SpendLimit,
        (ProviderId::Claude, 400 | 429) if spend => ProviderError::SpendLimit,
        (_, 429) => ProviderError::RateLimited { retry_after_s: retry_after_seconds(retry_after) },
        (_, 500..=599) => ProviderError::Server(status),
        _ => ProviderError::BadRequest(status),
    }
}

/// Reads the body, at most `cap` bytes: the declared length first, then chunk by chunk, counting
/// decoded bytes (a small gzip body can expand a lot). More than `cap` = `TooLarge`, and the rest
/// is never read.
pub async fn read_capped(mut resp: reqwest::Response, cap: usize) -> Result<Vec<u8>, ProviderError> {
    if resp.content_length().is_some_and(|n| n > cap as u64) {
        return Err(ProviderError::TooLarge);
    }
    let mut out: Vec<u8> = Vec::new();
    loop {
        match resp.chunk().await {
            Ok(Some(part)) => {
                if out.len() + part.len() > cap {
                    return Err(ProviderError::TooLarge);
                }
                out.extend_from_slice(&part);
            }
            Ok(None) => return Ok(out),
            Err(e) => return Err(map_transport(&e)),
        }
    }
}

/// One finished HTTP exchange: status, `Retry-After` and the capped body.
pub struct Answer {
    pub status: u16,
    pub retry_after: Option<String>,
    pub body: Vec<u8>,
}

/// Sends the request and reads the capped body. Blocking: runs on Tauri's async runtime; call it only
/// from our own threads or inside `spawn_blocking`, never on the main thread.
pub fn send(req: reqwest::RequestBuilder) -> Result<Answer, ProviderError> {
    tauri::async_runtime::block_on(async {
        let resp = req.send().await.map_err(|e| map_transport(&e))?;
        let status = resp.status().as_u16();
        let retry_after = resp.headers().get("retry-after").and_then(|v| v.to_str().ok()).map(str::to_string);
        let body = read_capped(resp, MAX_BODY).await?;
        Ok(Answer { status, retry_after, body })
    })
}

/// A 2xx body, or the mapped error.
pub fn into_body(provider: ProviderId, a: Answer) -> Result<Vec<u8>, ProviderError> {
    if (200..300).contains(&a.status) {
        Ok(a.body)
    } else {
        Err(map_status(provider, a.status, a.retry_after.as_deref(), &a.body))
    }
}
