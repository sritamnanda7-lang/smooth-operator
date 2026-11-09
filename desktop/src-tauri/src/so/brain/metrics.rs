// SPDX-License-Identifier: FSL-1.1-MIT
//! The only file under `so/brain/**` allowed to log (test S1). Integers and bools only. Never the
//! transcript, prompt, reply, plan arguments, question, URL or key.

/// Outcome 15 = spend limit reached (Rev 2).
pub fn brain(_outcome: u8, _provider: u8, _ms: u64, _steps: u8, _http: u16, _reject: u8, _actions: &[u8]) {
    todo!()
}

// Stub phase: nothing in the app calls these yet (increments 1-6 wire them). This reference keeps them
// compiled without a dead-code warning and without an allow (strip-check s05).
const _: () = {
    let _ = brain;
};
