// SPDX-License-Identifier: FSL-1.1-MIT
//! The only file under `so/link/**` allowed to log (test S1). Integers only. Never Origin, Host, the
//! pairing code, token, nonce, proof or any message text.

/// `outcome`: 0 listening, 1 port busy, 2 Origin, 3 Host, 4 busy, 5 path, 10 paired, 11 wrong code,
/// 12 locked, 13 expired or no window, 20 auth ok, 21 auth fail, 22 auth timeout, 23 bad message,
/// 24 too big, 25 closed or idle, 26 replaced, 27 Keychain error, 30 cmd sent, 31 result ignored (no such pending id), 32 result accepted. `code`: HTTP status or close code.
/// `ms`: TCP accept to auth or close.
pub fn link(outcome: u8, code: u16, ms: u64) {
    log::info!("link outcome={outcome} code={code} ms={ms}");
}
