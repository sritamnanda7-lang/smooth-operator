// SPDX-License-Identifier: FSL-1.1-MIT
//! The only file under `so/brain/**` allowed to log (test S1). Integers and bools only. Never the
//! transcript, prompt, reply, plan arguments, question, URL or key.

/// `outcome`: 0 not wanted, 1 plan, 2 question, 3 no key, 4 offline, 5 timeout, 6 key rejected, 7 rate
/// limited, 8 server problem, 9 bad reply (see `reject`), 10 too long, 11 limit (hour or in flight),
/// 12 dropped (newer talk), 13 Keychain, 14 model or settings problem (also a caught panic),
/// 15 spend limit. `provider`: 0 none, 1 Claude, 2 Gemini. `http`: status number or 0. `reject`: the
/// `Reject::code()` or 0. `actions`: fixed action codes 0 to 7.
pub fn brain(outcome: u8, provider: u8, ms: u64, steps: u8, http: u16, reject: u8, actions: &[u8]) {
    log::info!("brain outcome={outcome} provider={provider} ms={ms} steps={steps} http={http} reject={reject} actions={actions:?}");
}
