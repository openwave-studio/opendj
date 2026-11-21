// Point the download buttons at the latest release's files and highlight
// the visitor's platform. Without JS (or if GitHub's API is unreachable)
// every button still goes to the latest release page.
(function () {
  var REPO = "openwave-studio/opendj";
  var ua = navigator.userAgent;
  var os = /Mac/.test(ua) && !/iPhone|iPad/.test(ua) ? "mac" : /Linux/.test(ua) && !/Android/.test(ua) ? "linux" : /Windows/.test(ua) ? "windows" : null;

  if (os) {
    var card = document.querySelector('.dl-card[data-os="' + os + '"]');
    if (card) card.classList.add("is-mine");
  }
  // On Windows the main button keeps its generic label until a release has
  // the Windows download (see showWindows).
  if (os && os !== "windows") {
    var primary = document.querySelector("[data-download-primary]");
    if (primary) primary.textContent = os === "mac" ? "Download for Mac" : "Download for Linux";
  }

  var patterns = {
    macos: /macos.*\.zip$/i,
    appimage: /\.AppImage$/,
    tarball: /linux.*\.tar\.gz$/i,
    deb: /\.deb$/i,
    rpm: /\.rpm$/i,
    winsetup: /windows.*setup\.exe$/i,
    winzip: /windows.*\.zip$/i
  };

  function showWindows() {
    var button = document.querySelector("[data-windows-button]");
    if (button) {
      button.textContent = "Download for Windows";
      button.classList.replace("btn-ghost", "btn-primary");
    }
    document.querySelectorAll("[data-windows-zip]").forEach(function (el) { el.hidden = false; });
    document.querySelectorAll("[data-windows-note]").forEach(function (el) {
      el.textContent = "Builds aren't signed yet, so Windows asks before the first start: More info → Run anyway.";
    });
    if (os === "windows") {
      var primary = document.querySelector("[data-download-primary]");
      if (primary) primary.textContent = "Download for Windows";
    }
  }

  fetch("https://api.github.com/repos/" + REPO + "/releases/latest")
    .then(function (r) { return r.ok ? r.json() : Promise.reject(r.status); })
    .then(function (rel) {
      var version = (rel.tag_name || "").replace(/^v/, "");
      if (version) {
        document.querySelectorAll("[data-version]").forEach(function (el) { el.textContent = "v" + version + " available"; });
        document.querySelectorAll("[data-version-text]").forEach(function (el) { el.textContent = "Version " + version + "."; });
      }
      (rel.assets || []).forEach(function (asset) {
        Object.keys(patterns).forEach(function (key) {
          if (patterns[key].test(asset.name)) {
            if (key === "winsetup") showWindows();
            document.querySelectorAll('[data-asset="' + key + '"]').forEach(function (a) { a.href = asset.browser_download_url; });
          }
        });
      });
    })
    .catch(function () {});
})();

// Latest releases list and the download section's previous versions, from
// GitHub, leaving out pre-releases (the rolling nightly, stem models,
// betas). Without JS (or if GitHub's API is unreachable) the lists written
// in the page stay.
(function () {
  var latest = document.querySelector("[data-releases]");
  var previous = document.querySelector("[data-previous]");
  if (!latest && !previous) return;
  function fill(list, rels) {
    if (!list || !rels.length) return;
    list.textContent = "";
    rels.forEach(function (rel) {
      var li = document.createElement("li");
      var a = document.createElement("a");
      a.href = rel.html_url;
      a.textContent = "openDJ " + (rel.tag_name || "").replace(/^v/, "");
      var date = document.createElement("span");
      date.textContent = new Date(rel.published_at).toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" });
      li.append(a, " ", date);
      list.appendChild(li);
    });
  }
  fetch("https://api.github.com/repos/openwave-studio/opendj/releases?per_page=20")
    .then(function (r) { return r.ok ? r.json() : Promise.reject(r.status); })
    .then(function (rels) {
      rels = rels.filter(function (rel) { return !rel.prerelease && !rel.draft; });
      fill(latest, rels.slice(0, 4));
      fill(previous, rels.slice(1, 4));
    })
    .catch(function () {});
})();

// Nightly: named after the release it leads up to, plus "Alpha", as its
// files are (openDJ-0.9.0-nightly.20261120.abc1234-macos.zip). Shows when
// and from which commit it was built, and links its files directly.
(function () {
  var lane = document.getElementById("nightly");
  if (!lane) return;
  var patterns = {
    macos: /macos.*\.zip$/i,
    appimage: /\.AppImage$/,
    deb: /\.deb$/i,
    winsetup: /windows.*setup\.exe$/i
  };
  fetch("https://api.github.com/repos/openwave-studio/opendj/releases/tags/nightly")
    .then(function (r) { return r.ok ? r.json() : Promise.reject(r.status); })
    .then(function (rel) {
      var found = null;
      (rel.assets || []).forEach(function (asset) {
        var m = asset.name.match(/(\d+\.\d+\.\d+)-nightly\.(\d{8})\.([0-9a-f]+)/);
        if (m && !found) found = m;
        Object.keys(patterns).forEach(function (key) {
          if (patterns[key].test(asset.name)) {
            lane.querySelectorAll('[data-nightly-asset="' + key + '"]').forEach(function (a) { a.href = asset.browser_download_url; });
          }
        });
      });
      if (!found) return;
      lane.querySelector("[data-nightly-version]").textContent = found[1] + " Alpha";
      var d = found[2];
      var built = new Date(Date.UTC(+d.slice(0, 4), +d.slice(4, 6) - 1, +d.slice(6, 8)));
      var meta = lane.querySelector("[data-nightly-meta]");
      meta.textContent = "Built " + built.toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric", timeZone: "UTC" }) + " from commit ";
      var commit = document.createElement("a");
      commit.href = "https://github.com/openwave-studio/opendj/commit/" + found[3];
      var code = document.createElement("code");
      code.textContent = found[3];
      commit.appendChild(code);
      meta.appendChild(commit);
    })
    .catch(function () {});
})();

// Screenshot viewer: a click on a screenshot opens it large in a dialog,
// with arrows, keys (← → Esc) and swipes to move through the gallery.
// Without JS, or without <dialog>, the links open the image itself.
(function () {
  var links = Array.prototype.slice.call(document.querySelectorAll(".gal a"));
  if (!links.length || typeof HTMLDialogElement !== "function") return;
  var shots = links.map(function (a) {
    var cap = a.parentNode.querySelector("figcaption");
    return { src: a.getAttribute("href"), alt: a.querySelector("img").alt, cap: cap ? cap.textContent : "" };
  });
  var box = document.createElement("dialog");
  box.className = "viewer";
  box.innerHTML =
    '<figure><img alt=""><figcaption><span class="viewer-cap"></span> <span class="viewer-count"></span></figcaption></figure>' +
    '<button class="viewer-prev" aria-label="Previous screenshot">‹</button>' +
    '<button class="viewer-next" aria-label="Next screenshot">›</button>' +
    '<button class="viewer-close" aria-label="Close">×</button>';
  document.body.appendChild(box);
  var img = box.querySelector("img"), cap = box.querySelector(".viewer-cap"), count = box.querySelector(".viewer-count");
  var at = 0;
  function show(i) {
    at = (i + shots.length) % shots.length;
    img.src = shots[at].src;
    img.alt = shots[at].alt;
    cap.textContent = shots[at].cap;
    count.textContent = (at + 1) + " / " + shots.length;
    new Image().src = shots[(at + 1) % shots.length].src;
  }
  function open(i) { show(i); if (!box.open) box.showModal(); }
  links.forEach(function (a, i) {
    a.addEventListener("click", function (e) { e.preventDefault(); open(i); });
  });
  var intro = document.querySelector(".intro-shot");
  if (intro) intro.addEventListener("click", function (e) { e.preventDefault(); open(0); });
  box.querySelector(".viewer-prev").addEventListener("click", function () { show(at - 1); });
  box.querySelector(".viewer-next").addEventListener("click", function () { show(at + 1); });
  box.querySelector(".viewer-close").addEventListener("click", function () { box.close(); });
  box.addEventListener("click", function (e) { if (e.target === box) box.close(); });
  box.addEventListener("keydown", function (e) {
    if (e.key === "ArrowLeft") show(at - 1);
    if (e.key === "ArrowRight") show(at + 1);
  });
  var x0 = null;
  box.addEventListener("touchstart", function (e) { x0 = e.touches[0].clientX; }, { passive: true });
  box.addEventListener("touchend", function (e) {
    if (x0 === null) return;
    var dx = e.changedTouches[0].clientX - x0;
    if (Math.abs(dx) > 40) show(dx < 0 ? at + 1 : at - 1);
    x0 = null;
  });
})();
