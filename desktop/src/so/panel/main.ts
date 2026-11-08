// SPDX-License-Identifier: FSL-1.1-MIT
// Panel page entry. It runs first and alone: it defines the frozen `window.soPanel`, which Rust
// calls with `eval` (see panel.rs). The page has no IPC access at all.
import "./panel.css";
import { installSoPanel } from "./render";

const root = document.getElementById("panel");
if (root) installSoPanel(window, root);
