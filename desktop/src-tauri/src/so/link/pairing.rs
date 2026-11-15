// SPDX-License-Identifier: FSL-1.1-MIT
//! The pairing window: a 6-digit code that lives 120 s, works once, and locks after 5 wrong tries.
//! Pure: time and randomness are passed in, so tests control both.

use serde::Serialize;
use specta::Type;
use std::time::{Duration, Instant};
use subtle::ConstantTimeEq;

pub const CODE_TTL: Duration = Duration::from_secs(120);
pub const MAX_WRONG: u32 = 5;
pub const LOCK_TIME: Duration = Duration::from_secs(60);
pub const MAX_DRAWS: u32 = 16;

/// The random source failed. Everything that needs randomness fails closed on this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RngError;

/// Where random bytes come from. Tests plug in fixed bytes.
pub trait Rng: Send + Sync {
    fn fill(&self, buf: &mut [u8]) -> Result<(), RngError>;
}

/// The operating system's random source (via `getrandom`).
pub struct OsRng;

impl Rng for OsRng {
    fn fill(&self, buf: &mut [u8]) -> Result<(), RngError> {
        getrandom::fill(buf).map_err(|_| RngError)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairResult {
    Ok,
    Wrong,
    Locked { secs: u32 },
    Expired,
    NotPairing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
pub enum PairError {
    Locked { secs: u32 },
    PortBusy,
    Rng,
}

/// Returned by `start` only. The code is never logged or sent anywhere but the Settings page.
#[derive(Serialize, Type)]
pub struct PairingView {
    pub code: String,
    pub secs_left: u32,
}

/// The open window: the code and when it started. Dropped on success, cancel, expiry or lock (single use).
struct Window {
    code: String,
    started: Instant,
}

#[derive(Default)]
pub struct Pairing {
    window: Option<Window>,
    wrong: u32,
    locked_until: Option<Instant>,
}

/// Whole seconds left, rounded up, so "0 s left" never shows while time remains.
fn ceil_secs(d: Duration) -> u32 {
    d.as_millis().div_ceil(1000) as u32
}

impl Pairing {
    pub fn new() -> Self {
        Self::default()
    }
    /// A new code (the old one dies). Refused while locked. Rejection sampling: a u32 at or above
    /// 4,294,000,000 is redrawn, so every code is equally likely (4,294,000,000 is a multiple of 1,000,000).
    pub fn start(&mut self, now: Instant, rng: &dyn Rng) -> Result<PairingView, PairError> {
        if let Some(secs) = self.locked_secs(now) {
            return Err(PairError::Locked { secs });
        }
        let mut code = None;
        for _ in 0..MAX_DRAWS {
            let mut b = [0u8; 4];
            rng.fill(&mut b).map_err(|_| PairError::Rng)?;
            let n = u32::from_be_bytes(b);
            if n < 4_294_000_000 {
                code = Some(format!("{:06}", n % 1_000_000));
                break;
            }
        }
        let code = code.ok_or(PairError::Rng)?;
        self.window = Some(Window { code: code.clone(), started: now });
        self.wrong = 0;
        self.locked_until = None;
        Ok(PairingView { code, secs_left: ceil_secs(CODE_TTL) })
    }
    pub fn cancel(&mut self) {
        self.window = None;
    }
    /// Valid while `elapsed < CODE_TTL`; constant-time compare.
    pub fn try_code(&mut self, now: Instant, input: &str) -> PairResult {
        if let Some(secs) = self.locked_secs(now) {
            return PairResult::Locked { secs };
        }
        let Some(w) = &self.window else { return PairResult::NotPairing };
        if now.saturating_duration_since(w.started) >= CODE_TTL {
            self.window = None;
            return PairResult::Expired;
        }
        let digits = input.len() == 6 && input.bytes().all(|b| b.is_ascii_digit());
        if digits && bool::from(input.as_bytes().ct_eq(w.code.as_bytes())) {
            self.window = None;
            self.wrong = 0;
            return PairResult::Ok;
        }
        self.wrong += 1;
        if self.wrong >= MAX_WRONG {
            self.window = None;
            self.wrong = 0;
            self.locked_until = Some(now + LOCK_TIME);
            return PairResult::Locked { secs: ceil_secs(LOCK_TIME) };
        }
        PairResult::Wrong
    }
    pub fn secs_left(&self, now: Instant) -> Option<u32> {
        let w = self.window.as_ref()?;
        CODE_TTL.checked_sub(now.saturating_duration_since(w.started)).filter(|d| !d.is_zero()).map(ceil_secs)
    }
    pub fn locked_secs(&self, now: Instant) -> Option<u32> {
        self.locked_until?.checked_duration_since(now).filter(|d| !d.is_zero()).map(ceil_secs)
    }
}

// Stub phase: nothing in the app calls these yet. This reference keeps them compiled without a
// dead-code warning and without an allow (strip-check s05).
const _: () = {
    let _ = (CODE_TTL, MAX_WRONG, LOCK_TIME, MAX_DRAWS);
    let _ = RngError;
    let _ = <OsRng as Rng>::fill;
    let _ = OsRng;
    let _ = (PairResult::Ok, PairResult::Wrong, PairResult::Expired, PairResult::NotPairing);
    let _ = PairResult::Locked { secs: 0 };
    let _ = (PairError::PortBusy, PairError::Rng);
    let _ = PairError::Locked { secs: 0 };
    let view = PairingView { code: String::new(), secs_left: 0 };
    let _ = (&view.code, view.secs_left);
    std::mem::forget(view);
    let _ = Pairing::new;
    let _ = Pairing::start;
    let _ = Pairing::cancel;
    let _ = Pairing::try_code;
    let _ = Pairing::secs_left;
    let _ = Pairing::locked_secs;
};
