// SPDX-License-Identifier: FSL-1.1-MIT
//! The only file under `so/exec/**` allowed to log. Integers only: never a query, title, URL or page text.

pub fn exec(outcome: u8, op: u8, res: u8, ms: u64, retries: u8) {
    log::info!("exec outcome={outcome} op={op} res={res} ms={ms} retries={retries}");
}
