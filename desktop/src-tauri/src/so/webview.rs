// SPDX-License-Identifier: FSL-1.1-MIT
//! Main-window navigation rule. STUB. Tests: `so/tests/nav_tests.rs` (T26).

/// True only for the app's own origin: `tauri://localhost/..`, `http(s)://tauri.localhost/..`,
/// and (debug builds only) the dev server `http://localhost:1420/..`. Everything else is refused.
pub fn main_nav_allowed(_url: &tauri::Url) -> bool {
    todo!("step 1: main window navigation rule")
}
