// aesop docs chrome for mdBook — GENERATED COPY.
// Canonical source: mojzis/aesop, docs-theme/aesop-docs.js. Edit there and run
// docs-theme/sync.py. Per-tool values arrive as window.AESOP from theme/head.hbs.
(function () {
  "use strict";
  var A = window.AESOP;
  if (!A) return;

  function el(tag, cls, text) {
    var e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text) e.textContent = text;
    return e;
  }
  function mascot(size) {
    var img = el("img");
    img.src = A.mascot;
    img.alt = A.animal;
    img.width = img.height = size;
    img.onerror = function () { img.remove(); };
    return img;
  }

  // Top bar: "● aesop / tool". version.js appends its badge to .menu-title.
  function crumb() {
    var title = document.querySelector(".menu-title");
    if (!title || title.querySelector(".a-home")) return;
    var badge = title.querySelector(".version-badge");
    title.textContent = "";
    var home = el("a", "a-home");
    home.href = A.home;
    home.title = "Part of aesop — tools for agentic Python";
    home.appendChild(el("span", "a-dot"));
    home.appendChild(el("span", "", "aesop"));
    var tool = el("a", "a-tool", A.name);
    tool.href = (typeof path_to_root === "string" ? path_to_root : "") + "index.html";
    title.appendChild(home);
    title.appendChild(el("span", "a-sep", "/"));
    title.appendChild(tool);
    if (badge) title.appendChild(badge);
  }

  // Sidebar: mascot card on top, "part of aesop" underneath.
  function sidebar() {
    var box = document.querySelector("#mdbook-sidebar .sidebar-scrollbox");
    if (!box || box.querySelector(".a-card")) return !!box;
    var card = el("a", "a-card");
    card.href = A.page;
    card.title = A.name + " on aesop";
    card.appendChild(mascot(52));
    var txt = el("span");
    txt.appendChild(el("b", "", A.name));
    txt.appendChild(el("i", "", A.animal === A.name ? "part of aesop" : A.animal));
    card.appendChild(txt);
    box.insertBefore(card, box.firstChild);

    var part = el("a", "a-part");
    part.href = A.home;
    part.appendChild(el("span", "a-dot"));
    var s = el("span", "", "Part of ");
    s.appendChild(el("b", "", "aesop"));
    s.appendChild(document.createTextNode(" — the agentic-Python menagerie →"));
    part.appendChild(s);
    box.appendChild(part);
    navTitles(box);
    return true;
  }

  // Prev/next cards: name the chapter they lead to, from the sidebar's links.
  function navTitles(box) {
    var byHref = {};
    box.querySelectorAll("ol.chapter a[href]").forEach(function (a) {
      if (!a.closest(".on-this-page")) byHref[a.href.split("#")[0]] = a.textContent.replace(/^\s*[\d.]+\s+/, "").trim();
    });
    document.querySelectorAll(".nav-wrapper .mobile-nav-chapters").forEach(function (a) {
      var name = byHref[a.href.split("#")[0]];
      if (name && !a.querySelector(".a-nav-title")) a.appendChild(el("span", "a-nav-title", name));
    });
  }

  // Intro page: mascot, animal and tagline around the title.
  function hero() {
    var root = typeof path_to_root === "string" ? path_to_root : "";
    var file = location.pathname.split("/").pop();
    if (root !== "" || (file !== "" && file !== "index.html")) return;
    var h1 = document.querySelector(".content main > h1:first-child");
    if (!h1 || h1.closest(".a-hero")) return;
    var wrap = el("div", "a-hero");
    var txt = el("div");
    h1.parentNode.insertBefore(wrap, h1);
    wrap.appendChild(mascot(148));
    if (A.animal !== A.name) txt.appendChild(el("p", "a-animal", A.animal));
    txt.appendChild(h1);
    txt.appendChild(el("p", "a-tagline", A.tagline));
    wrap.appendChild(txt);
  }

  function run() {
    crumb();
    hero();
    if (!sidebar()) {
      // The sidebar is a custom element filled in by toc.js; wait for it.
      var tries = 0;
      var t = setInterval(function () {
        if (sidebar() || ++tries > 40) clearInterval(t);
      }, 50);
    }
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", run);
  } else {
    run();
  }
})();
