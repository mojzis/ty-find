// Displays the current package version next to the title in the top menu bar.
// The VERSION constant below is the single line updated by docs/gen-version.sh
// (run automatically in the docs deploy workflow) and on each `cargo release`.
(function () {
  "use strict";
  var VERSION = "0.4.1";

  function injectVersion() {
    var title = document.querySelector(".menu-title");
    if (!title || title.querySelector(".version-badge")) {
      return;
    }
    var badge = document.createElement("span");
    badge.className = "version-badge";
    badge.textContent = "v" + VERSION;
    title.appendChild(badge);
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", injectVersion);
  } else {
    injectVersion();
  }
})();
