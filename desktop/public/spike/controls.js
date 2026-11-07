// SPDX-License-Identifier: FSL-1.1-MIT
// Spike control buttons. Results are written with textContent only.
(function () {
  var out = document.getElementById("out");
  window.soControls = { set: function (t) { out.textContent = String(t); } };
  document.querySelectorAll("button[data-cmd]").forEach(function (b) {
    b.addEventListener("click", function () {
      var cmd = b.getAttribute("data-cmd");
      out.textContent = "running " + cmd + " ...";
      window.__TAURI_INTERNALS__.invoke(cmd).then(
        function () { if (cmd === "spike_stall") out.textContent = "stalled 2 s, done"; },
        function (e) { out.textContent = "error: " + String(e); }
      );
    });
  });
})();
