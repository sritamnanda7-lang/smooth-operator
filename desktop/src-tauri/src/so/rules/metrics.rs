// SPDX-License-Identifier: FSL-1.1-MIT
//! The only file under `so/rules/**` allowed to log (test S1). Integers and bools only. Never text,
//! app names or paths.

/// `outcome`: 0 pass-through, 1 media, 2 open, 3 switch, 4 not found, 5 ambiguous, 6 action failed,
/// 7 dropped (stale). `ms_from_stop`: talk end to action.
pub fn fast_rule(outcome: u8, ms_from_stop: u64) {
    log::info!("fast_rule outcome={outcome} ms_from_stop={ms_from_stop}");
}

/// Debug builds only (the typed-phrase input does not exist in release).
/// `rejected`: 0 none, 1 too long, 2 bad utf-8, 3 empty, 4 wrong user, 5 timeout.
#[cfg(debug_assertions)]
pub fn debug_input(listening: bool, rejected: u8) {
    log::info!("debug_input listening={listening} rejected={rejected}");
}
