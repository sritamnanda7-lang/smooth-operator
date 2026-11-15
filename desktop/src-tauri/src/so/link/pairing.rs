// SPDX-License-Identifier: FSL-1.1-MIT
//! The pairing window: a 6-digit code that lives 120 s, works once, and locks after 5 wrong tries.
//! Pure: time and randomness are passed in, so tests control both.

use serde::Serialize;
use specta::Type;
use std::time::{Duration, Instant};

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
    fn fill(&self, _buf: &mut [u8]) -> Result<(), RngError> {
        todo!()
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

#[derive(Default)]
pub struct Pairing;

impl Pairing {
    pub fn new() -> Self {
        Pairing
    }
    pub fn start(&mut self, _now: Instant, _rng: &dyn Rng) -> Result<PairingView, PairError> {
        todo!()
    }
    pub fn cancel(&mut self) {
        todo!()
    }
    /// Valid while `elapsed < CODE_TTL`; constant-time compare.
    pub fn try_code(&mut self, _now: Instant, _input: &str) -> PairResult {
        todo!()
    }
    pub fn secs_left(&self, _now: Instant) -> Option<u32> {
        todo!()
    }
    pub fn locked_secs(&self, _now: Instant) -> Option<u32> {
        todo!()
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
