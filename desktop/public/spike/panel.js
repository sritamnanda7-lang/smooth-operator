// SPDX-License-Identifier: FSL-1.1-MIT
// Panel text setter. The only way text gets in is textContent: no HTML, no scripts.
window.soPanel = { set: function (text) { document.getElementById("t").textContent = String(text); } };
