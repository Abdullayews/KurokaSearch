/* KurokaSearch — client behaviors (original code).
   Everything degrades gracefully: no JS -> plain links still work. */
(function () {
  "use strict";

  var $ = function (s, r) { return (r || document).querySelector(s); };
  var $$ = function (s, r) { return Array.prototype.slice.call((r || document).querySelectorAll(s)); };

  /* ------------------------------------------------------------------ *
   * preferences (localStorage only — nothing leaves the browser)
   * ------------------------------------------------------------------ */

  var PREF_KEY = "kuroka.prefs";
  var DEFAULT_PREFS = {
    lang: "auto", time: "", ss: "0", cat: "all", newtab: false, ac: true,
    theme: "dark", glow: true, engines: {},
    aiProv: "gemini", aiModel: "", aiKey: "", aiBase: ""
  };

  function loadPrefs() {
    var p = {};
    try { p = JSON.parse(localStorage.getItem(PREF_KEY)) || {}; } catch (e) { p = {}; }
    var out = {};
    Object.keys(DEFAULT_PREFS).forEach(function (k) {
      out[k] = (p[k] === undefined || p[k] === null) ? DEFAULT_PREFS[k] : p[k];
    });
    return out;
  }

  function savePrefs(p) {
    try { localStorage.setItem(PREF_KEY, JSON.stringify(p)); } catch (e) { /* private mode */ }
  }

  var prefs = loadPrefs();

  function applyTheme() {
    document.body.classList.toggle("k-light", prefs.theme === "light");
    document.body.classList.toggle("k-noglow", prefs.glow === false);
  }
  applyTheme();

  /* engine ids saved for a category (or null = use defaults) */
  function enginesFor(cat) {
    var list = prefs.engines && prefs.engines[cat];
    return Array.isArray(list) ? list : null;
  }

  /* ------------------------------------------------------------------ *
   * toast
   * ------------------------------------------------------------------ */

  var toastEl = $("#kToast"), toastTimer = null;
  function toast(msg, ms) {
    if (!toastEl) return;
    toastEl.textContent = msg;
    toastEl.hidden = false;
    requestAnimationFrame(function () { toastEl.classList.add("show"); });
    clearTimeout(toastTimer);
    toastTimer = setTimeout(function () {
      toastEl.classList.remove("show");
      setTimeout(function () { toastEl.hidden = true; }, 350);
    }, ms || 2600);
  }

  /* ------------------------------------------------------------------ *
   * particles — every action sparks: clicks, taps, key presses, scroll.
   * Mouse movement is deliberately excluded. Respects reduced motion.
   * ------------------------------------------------------------------ */

  (function () {
    try {
      if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
      var cv = document.createElement("canvas");
      cv.id = "kFx";
      cv.setAttribute("aria-hidden", "true");
      document.body.appendChild(cv);
      var ctx = cv.getContext("2d");
      if (!ctx) return;
      var W = 0, H = 0;
      var DPR = Math.min(window.devicePixelRatio || 1, 2);
      function resize() {
        W = window.innerWidth; H = window.innerHeight;
        cv.width = Math.round(W * DPR); cv.height = Math.round(H * DPR);
        ctx.setTransform(DPR, 0, 0, DPR, 0, 0);
      }
      resize();
      window.addEventListener("resize", resize);

      var parts = [];
      var COLORS = ["#e6b445", "#f0c65e", "#d29a2f", "#fff3d6"];

      function spawn(x, y, n, spread, lift) {
        for (var i = 0; i < n; i++) {
          var a = Math.random() * Math.PI * 2;
          var v = Math.random() * spread + 0.35;
          parts.push({
            x: x, y: y,
            vx: Math.cos(a) * v,
            vy: Math.sin(a) * v - (lift || 0),
            r: Math.random() * 2.1 + 0.7,
            c: COLORS[(Math.random() * COLORS.length) | 0],
            life: 1,
            decay: 0.016 + Math.random() * 0.026
          });
        }
        if (parts.length > 350) parts.splice(0, parts.length - 350);
      }

      var raf = 0;
      function tick() {
        raf = 0;
        ctx.clearRect(0, 0, W, H);
        for (var i = parts.length - 1; i >= 0; i--) {
          var p = parts[i];
          p.x += p.vx; p.y += p.vy;
          p.vy += 0.05; p.vx *= 0.985; p.vy *= 0.985;
          p.life -= p.decay;
          if (p.life <= 0) { parts.splice(i, 1); continue; }
          ctx.globalAlpha = p.life > 0 ? p.life : 0;
          ctx.fillStyle = p.c;
          ctx.beginPath();
          ctx.arc(p.x, p.y, p.r * (0.45 + p.life * 0.55), 0, 6.28318);
          ctx.fill();
        }
        ctx.globalAlpha = 1;
        if (parts.length) raf = requestAnimationFrame(tick);
        else ctx.clearRect(0, 0, W, H);
      }
      function wake() { if (!raf) raf = requestAnimationFrame(tick); }

      /* click / tap — burst right under the pointer */
      document.addEventListener("click", function (ev) {
        spawn(ev.clientX, ev.clientY, 12, 2.3, 0.5);
        wake();
      }, { passive: true });
      document.addEventListener("touchstart", function (ev) {
        var t = ev.touches && ev.touches[0];
        if (t) { spawn(t.clientX, t.clientY, 10, 2.1, 0.4); wake(); }
      }, { passive: true });

      /* key presses — small sparks along the focused element's edge */
      document.addEventListener("keydown", function (ev) {
        if (ev.metaKey || ev.ctrlKey || ev.altKey) return;
        var el = ev.target;
        var r = (el && el.getBoundingClientRect) ? el.getBoundingClientRect() : null;
        var x, y;
        if (r && r.width > 2) { x = r.left + Math.random() * r.width; y = r.bottom - 2; }
        else { x = W / 2 + (Math.random() - 0.5) * 240; y = H * 0.55; }
        spawn(x, y, 3, 1.1, 1.7);
        wake();
      }, { passive: true });

      /* scrolling — occasional embers rising from the bottom edge */
      var lastSpark = 0;
      window.addEventListener("scroll", function () {
        var now = Date.now();
        if (now - lastSpark < 150) return;
        lastSpark = now;
        spawn(Math.random() * W, H - 6, 2, 0.7, 1.9);
        wake();
      }, { passive: true });
    } catch (e) { /* particles are decorative — never break the page */ }
  })();

  /* ------------------------------------------------------------------ *
   * scroll: collapse header, show back-to-top
   * ------------------------------------------------------------------ */

  var topBtn = $("#kTop");
  var lastScroll = -1;
  function onScroll() {
    var y = window.scrollY || document.documentElement.scrollTop || 0;
    if (y === lastScroll) return;
    lastScroll = y;
    document.body.classList.toggle("k-scrolled", y > 140);
    if (topBtn) topBtn.classList.toggle("show", y > 480);
  }
  window.addEventListener("scroll", function () {
    window.requestAnimationFrame(onScroll);
  }, { passive: true });
  onScroll();

  if (topBtn) {
    topBtn.addEventListener("click", function () {
      window.scrollTo({ top: 0, behavior: "smooth" });
    });
  }

  /* ------------------------------------------------------------------ *
   * infinite scroll — ALWAYS ON: fetch the next page and append results
   * ------------------------------------------------------------------ */

  (function () {
    var main = $("#kMain");
    if (!main) return;
    var busy = false, done = false;
    var page = parseInt(main.getAttribute("data-page") || "1", 10) || 1;
    var more = main.getAttribute("data-more") === "1";

    function nextLink() {
      if (!more || page >= 50) return null;
      return searchUrl({ page: page + 1 });
    }

    function sentinel() {
      var s = document.createElement("div");
      s.id = "kInfSentinel";
      s.style.height = "1px";
      s.setAttribute("aria-hidden", "true");
      main.appendChild(s);
      return s;
    }

    var sent = sentinel();
    var io = new IntersectionObserver(function (entries) {
      if (!entries[0].isIntersecting || busy || done) return;
      var href = nextLink();
      if (!href) { done = true; io.disconnect(); return; }
      busy = true;
      fetch(href, { headers: { "X-Requested-With": "kuroka" } })
        .then(function (r) { return r.text(); })
        .then(function (html) {
          var doc = new DOMParser().parseFromString(html, "text/html");
          var nm = doc.getElementById("kMain");
          if (!nm || $(".k-nors", nm)) { done = true; return; }
          var add = [];
          for (var i = 0; i < nm.children.length; i++) add.push(nm.children[i]);
          if (!add.length) { done = true; return; }
          add.forEach(function (el) { main.insertBefore(el, sent); });
          page = parseInt(nm.getAttribute("data-page") || "1", 10) || (page + 1);
          more = nm.getAttribute("data-more") === "1";
          if (!nextLink()) { done = true; io.disconnect(); }
          busy = false;
          hookDeadImages(add);
        })
        .catch(function () { busy = false; });
    }, { rootMargin: "900px 0px" });
    io.observe(sent);

    function hookDeadImages(els) {
      els.forEach ? els.forEach(function (e) {
        $$("img[onerror]", e).forEach(function (img) {
          // re-arm the onerror fallback for injected nodes
          img.onerror = function () {
            var card = img.closest(".k-img");
            if (card) card.classList.add("k-img-dead");
          };
        });
        // favicons: a failed icon removes itself, the letter avatar shows
        $$("img.k-fav, .k-ava img", e).forEach(function (img) {
          img.onerror = function () { img.remove(); };
        });
      }) : null;
    }
  })();

  /* ------------------------------------------------------------------ *
   * search URL helpers — keep params, honor saved engine choices
   * ------------------------------------------------------------------ */

  function searchUrl(over) {
    var sp = new URLSearchParams(window.location.search);
    var q = (over.q !== undefined) ? over.q : (sp.get("q") || "");
    var url = "/search?q=" + encodeURIComponent(q);
    var c = over.c !== undefined ? over.c : (sp.get("c") || "all");
    var lang = over.lang !== undefined ? over.lang : (sp.get("lang") || prefs.lang || "auto");
    var time = over.time !== undefined ? over.time : (sp.get("time") || "");
    var ss = over.ss !== undefined ? String(over.ss) : (sp.get("ss") || "0");
    var e = over.e !== undefined ? over.e : sp.get("e");
    if (c && c !== "all") url += "&c=" + c;
    if (lang && lang !== "auto") url += "&lang=" + lang;
    if (time) url += "&time=" + time;
    if (ss && ss !== "0") url += "&ss=" + ss;
    if (over.page) url += "&page=" + over.page;
    if (e) url += "&e=" + e;
    if (over.size && over.size !== "any") url += "&size=" + over.size;
    if (over.layout && over.layout !== "any") url += "&layout=" + over.layout;
    return url;
  }

  /* category tabs / pagination: attach saved engines for the target category */
  document.addEventListener("click", function (ev) {
    var a = ev.target.closest ? ev.target.closest("a[href^='/search']") : null;
    if (!a) return;
    var href = a.getAttribute("href") || "";
    if (href.indexOf("e=") !== -1) return;
    var m = href.match(/[?&]c=([a-z]+)/);
    var cat = m ? m[1] : "all";
    var ids = enginesFor(cat);
    if (ids && ids.length) {
      ev.preventDefault();
      window.location.href = href + (href.indexOf("?") === -1 ? "?" : "&") + "e=" + ids.join(",");
    }
  });

  /* ------------------------------------------------------------------ *
   * search box
   * ------------------------------------------------------------------ */

  var qInput = $("#kQ");
  if (qInput && document.body.classList.contains("k-page-results") &&
      window.matchMedia("(min-width: 761px)").matches) {
    var v = qInput.value.length;
    qInput.focus();
    qInput.setSelectionRange(v, v);
  }

  /* home + results forms: carry preferences into the search.
     (programmatic .submit() skips the submit event, so this is a plain
     function called from both the listener and the autocomplete picker) */
  function carryPrefs(form) {
    var catIn = form.querySelector('input[name="c"]');
    var cat = catIn ? catIn.value : "all";
    if (catIn && prefs.cat && cat === "all" && prefs.cat !== "all") {
      cat = prefs.cat;
      catIn.value = prefs.cat;
    }
    function addHidden(name, value) {
      if (value === "" || value === "auto" || value === "0") return;
      var i = document.createElement("input");
      i.type = "hidden"; i.name = name; i.value = value;
      form.appendChild(i);
    }
    addHidden("lang", prefs.lang);
    addHidden("time", prefs.time);
    if (prefs.ss && prefs.ss !== "0") addHidden("ss", String(prefs.ss));
    var ids = enginesFor(cat);
    if (ids && ids.length) addHidden("e", ids.join(","));
  }

  $$("form.k-srow").forEach(function (form) {
    form.addEventListener("submit", function () { carryPrefs(form); });
  });

  /* open results in a new tab (preference) */
  if (prefs.newtab) {
    $$(".k-main a[href^='http']").forEach(function (a) {
      a.target = "_blank";
      a.rel = "noopener noreferrer";
    });
  }

  /* ------------------------------------------------------------------ *
   * autocomplete (server-proxied; overlay — never shifts the layout)
   * ------------------------------------------------------------------ */

  var acBox = $("#kAc");
  if (qInput && acBox && prefs.ac) {
    var acItems = [], acSel = -1, acReq = null, acTimer = null;

    function acHide() {
      acBox.hidden = true;
      acBox.innerHTML = "";
      acItems = []; acSel = -1;
    }

    function acRender(list) {
      acItems = list; acSel = -1;
      if (!list.length) { acHide(); return; }
      acBox.innerHTML = "";
      list.forEach(function (s) {
        var d = document.createElement("div");
        d.className = "k-ac-it";
        d.textContent = s;
        function apply() {
          qInput.value = s;
          acHide();
          if (qInput.form) {
            carryPrefs(qInput.form);
            qInput.form.submit();
          }
        }
        d.addEventListener("mousedown", function (ev) { ev.preventDefault(); apply(); });
        d.addEventListener("click", function (ev) { ev.preventDefault(); apply(); });
        acBox.appendChild(d);
      });
      acBox.hidden = false;
    }

    function acHighlight() {
      $$(".k-ac-it", acBox).forEach(function (el, i) {
        el.classList.toggle("on", i === acSel);
      });
    }

    qInput.addEventListener("input", function () {
      clearTimeout(acTimer);
      var v = qInput.value.trim();
      if (v.length < 2) { acHide(); return; }
      acTimer = setTimeout(function () {
        if (acReq) acReq.abort();
        acReq = new XMLHttpRequest();
        acReq.open("GET", "/ac?q=" + encodeURIComponent(v));
        acReq.onload = function () {
          if (qInput.value.trim() !== v) return;
          try {
            var list = JSON.parse(acReq.responseText) || [];
            if (!Array.isArray(list)) list = [];
            acRender(list.slice(0, 8));
          } catch (e) { acHide(); }
        };
        acReq.onerror = function () { acHide(); };
        acReq.send();
      }, 150);
    });

    qInput.addEventListener("keydown", function (ev) {
      if (acBox.hidden || !acItems.length) {
        if (ev.key === "Escape") acHide();
        return;
      }
      if (ev.key === "ArrowDown") {
        ev.preventDefault();
        acSel = (acSel + 1) % acItems.length; acHighlight();
      } else if (ev.key === "ArrowUp") {
        ev.preventDefault();
        acSel = (acSel - 1 + acItems.length) % acItems.length; acHighlight();
      } else if (ev.key === "Escape") {
        acHide();
      } else if ((ev.key === "Enter" || ev.key === "Tab") && acSel >= 0) {
        ev.preventDefault();
        qInput.value = acItems[acSel];
        acHide();
        if (ev.key === "Enter" && qInput.form) qInput.form.submit();
      }
    });

    document.addEventListener("click", function (ev) {
      if (!acBox.hidden && !acBox.contains(ev.target) && ev.target !== qInput) acHide();
    });
  }

  /* ------------------------------------------------------------------ *
   * custom dropdowns — styled menus, never the browser default
   * ------------------------------------------------------------------ */

  function closeAllDds(except) {
    $$(".k-dd.open").forEach(function (dd) {
      if (dd === except) return;
      dd.classList.remove("open");
      var m = $(".k-dd-menu", dd);
      if (m) m.hidden = true;
      var b = $(".k-dd-btn", dd);
      if (b) b.setAttribute("aria-expanded", "false");
    });
  }

  document.addEventListener("click", function (ev) {
    var btn = ev.target.closest ? ev.target.closest(".k-dd-btn") : null;
    if (btn) {
      var dd = btn.closest(".k-dd");
      var menu = $(".k-dd-menu", dd);
      var willOpen = menu.hidden;
      closeAllDds(dd);
      menu.hidden = !willOpen;
      dd.classList.toggle("open", willOpen);
      btn.setAttribute("aria-expanded", String(willOpen));
      ev.preventDefault();
      return;
    }
    var opt = ev.target.closest ? ev.target.closest(".k-dd-opt") : null;
    if (opt) {
      var dd2 = opt.closest(".k-dd");
      var val = opt.getAttribute("data-v") || "";
      var lbl = opt.textContent;
      var cur = $(".k-dd-cur", dd2);
      if (cur) cur.textContent = lbl;
      closeAllDds();
      if (dd2.getAttribute("data-nav") === "1") {
        var over = {};
        over[dd2.getAttribute("data-k")] = val;
        window.location.href = searchUrl(over);
      }
      ev.preventDefault();
      return;
    }
    if (!ev.target.closest || !ev.target.closest(".k-dd")) closeAllDds();
  });

  document.addEventListener("keydown", function (ev) {
    if (ev.key === "Escape") {
      closeAllDds();
      var open = $(".k-dd.open .k-dd-btn");
      if (open) open.focus();
    }
  });

  /* ------------------------------------------------------------------ *
   * extras — one piece: image filters + engines + telemetry
   * ------------------------------------------------------------------ */

  var extrasBtn = $("#kExtrasBtn"), extrasPanel = $("#kExtras");
  if (extrasBtn && extrasPanel) {
    extrasBtn.addEventListener("click", function () {
      var open = extrasPanel.hidden;
      extrasPanel.hidden = !open;
      extrasBtn.setAttribute("aria-expanded", String(open));
    });
  }

  var extrasApply = $("#kExtrasApply");
  if (extrasApply) {
    extrasApply.addEventListener("click", function () {
      var engines = $$('input[name="k-engine"]:checked').map(function (i) { return i.value; });
      var sizeDd = $('#ddXSize');
      var layoutDd = $('#ddXLayout');
      var cur = function (dd) {
        if (!dd) return "";
        var c = $(".k-dd-cur", dd);
        var m = c ? c.textContent : "";
        var map = { "Any": "any", "Small": "small", "Medium": "medium", "Large": "large", "Wallpaper": "wallpaper",
                    "Wide": "wide", "Tall": "tall", "Square": "square" };
        return map[m] || "any";
      };
      window.location.href = searchUrl({
        e: engines.join(","),
        size: cur(sizeDd),
        layout: cur(layoutDd)
      });
    });
  }

  /* ------------------------------------------------------------------ *
   * settings popup — blurred backdrop, animated, on any page
   * ------------------------------------------------------------------ */

  var setEl = $("#kSet");
  function openSettings() {
    if (!setEl) return;
    fillSettings();
    setEl.hidden = false;
    requestAnimationFrame(function () {
      setEl.classList.add("show");
      document.body.classList.add("k-modal");
    });
    var x = $("#kSetX");
    if (x) x.focus();
  }
  function closeSettings() {
    if (!setEl) return;
    setEl.classList.remove("show");
    document.body.classList.remove("k-modal");
    setTimeout(function () { setEl.hidden = true; }, 240);
  }

  var gear = $("#kGear");
  if (gear) gear.addEventListener("click", openSettings);
  var aiOpenSet = $("#kAiOpenSet");
  if (aiOpenSet) aiOpenSet.addEventListener("click", openSettings);
  var setX = $("#kSetX");
  if (setX) setX.addEventListener("click", closeSettings);
  var setBg = $("#kSetBg");
  if (setBg) setBg.addEventListener("click", closeSettings);
  document.addEventListener("keydown", function (ev) {
    if (ev.key === "Escape" && setEl && !setEl.hidden) closeSettings();
  });

  /* tabs */
  $$(".k-set-nav button").forEach(function (b) {
    b.addEventListener("click", function () {
      $$(".k-set-nav button").forEach(function (x) { x.classList.remove("on"); });
      b.classList.add("on");
      var t = b.getAttribute("data-t");
      $$(".k-set-pane").forEach(function (p) {
        p.classList.toggle("on", p.getAttribute("data-pane") === t);
      });
    });
  });

  function setDdCur(id, val) {
    var dd = $("#" + id);
    if (!dd) return;
    var cur = $(".k-dd-cur", dd);
    var opt = $(".k-dd-opt[data-v='" + val + "']", dd);
    if (cur && opt) cur.textContent = opt.textContent;
  }

  function fillSettings() {
    setDdCur("s-dd-lang", prefs.lang);
    setDdCur("s-dd-cat", prefs.cat);
    setDdCur("s-dd-time", prefs.time || "");
    setDdCur("s-dd-ss", String(prefs.ss));
    setDdCur("s-dd-theme", prefs.theme);
    setDdCur("s-dd-prov", prefs.aiProv);
    var m = $("#s-ai-model"); if (m) m.value = prefs.aiModel || "";
    var k = $("#s-ai-key"); if (k) k.value = prefs.aiKey || "";
    var b = $("#s-ai-base"); if (b) b.value = prefs.aiBase || "";
    var ac = $("#s-ac"); if (ac) ac.checked = prefs.ac !== false;
    var nt = $("#s-newtab"); if (nt) nt.checked = !!prefs.newtab;
    var gl = $("#s-glow"); if (gl) gl.checked = prefs.glow !== false;
    /* engine toggles */
    $$("input[data-eng]").forEach(function (i) {
      var cat = i.getAttribute("data-cat");
      var ids = enginesFor(cat);
      if (ids) i.checked = ids.indexOf(i.getAttribute("data-eng")) !== -1;
      else i.checked = i.hasAttribute("checked");
    });
  }

  var setSave = $("#kSetSave");
  if (setSave) {
    setSave.addEventListener("click", function () {
      var langDd = $("#s-dd-lang");
      function ddVal(id) {
        var dd = $("#" + id);
        if (!dd) return "";
        var opt = $(".k-dd-opt.on", dd);
        return opt ? (opt.getAttribute("data-v") || "") : "";
      }
      /* remember .on for chosen options */
      function ddValByCur(id) {
        var dd = $("#" + id);
        if (!dd) return "";
        var cur = $(".k-dd-cur", dd);
        var chosen = null;
        $$(".k-dd-opt", dd).forEach(function (o) {
          if (cur && o.textContent === cur.textContent) chosen = o.getAttribute("data-v");
        });
        return chosen || "";
      }
      var engines = {};
      $$("input[data-eng]").forEach(function (i) {
        var cat = i.getAttribute("data-cat");
        if (!engines[cat]) engines[cat] = [];
        if (i.checked) engines[cat].push(i.getAttribute("data-eng"));
      });
      Object.keys(engines).forEach(function (cat) {
        if (engines[cat].length === 0) delete engines[cat];
      });
      prefs = {
        lang: ddValByCur("s-dd-lang") || "auto",
        time: ddValByCur("s-dd-time") || "",
        ss: ddValByCur("s-dd-ss") || "0",
        cat: ddValByCur("s-dd-cat") || "all",
        newtab: !!($("#s-newtab") || {}).checked,
        ac: ($("#s-ac") || {}).checked !== false,
        theme: ddValByCur("s-dd-theme") || "dark",
        glow: ($("#s-glow") || {}).checked !== false,
        engines: engines,
        aiProv: ddValByCur("s-dd-prov") || "gemini",
        aiModel: ($("#s-ai-model") || {}).value || "",
        aiKey: ($("#s-ai-key") || {}).value || "",
        aiBase: ($("#s-ai-base") || {}).value || ""
      };
      savePrefs(prefs);
      applyTheme();
      closeSettings();
      toast("Settings saved — stored only in this browser");
    });
  }

  var setReset = $("#kSetReset");
  if (setReset) {
    setReset.addEventListener("click", function () {
      prefs = JSON.parse(JSON.stringify(DEFAULT_PREFS));
      savePrefs(prefs);
      applyTheme();
      fillSettings();
      toast("Settings reset to defaults");
    });
  }

  var wipe = $("#s-wipe");
  if (wipe) {
    wipe.addEventListener("click", function () {
      try { localStorage.removeItem(PREF_KEY); } catch (e) {}
      toast("Local data cleared");
      setTimeout(function () { window.location.reload(); }, 700);
    });
  }

  /* ------------------------------------------------------------------ *
   * music previews — one shared player
   * ------------------------------------------------------------------ */

  var audio = null, playingBtn = null;
  $$(".k-mus-play").forEach(function (btn) {
    btn.addEventListener("click", function (ev) {
      ev.preventDefault();
      var src = btn.getAttribute("data-pv");
      if (!src) return;
      if (playingBtn === btn) {
        audio.pause();
        audio.currentTime = 0;
        return;
      }
      if (audio) { audio.pause(); }
      if (playingBtn) playingBtn.classList.remove("playing");
      audio = new Audio(src);
      playingBtn = btn;
      btn.classList.add("playing");
      audio.addEventListener("ended", function () {
        btn.classList.remove("playing");
        if (playingBtn === btn) playingBtn = null;
      });
      audio.addEventListener("error", function () {
        btn.classList.remove("playing");
        if (playingBtn === btn) playingBtn = null;
        toast("Preview unavailable");
      });
      audio.play().catch(function () {
        btn.classList.remove("playing");
        playingBtn = null;
        toast("Preview unavailable");
      });
    });
  });

  /* ------------------------------------------------------------------ *
   * dates
   * ------------------------------------------------------------------ */

  $$(".k-r-date[data-d]").forEach(function (el) {
    var d = new Date(el.getAttribute("data-d"));
    if (!isNaN(d.getTime())) {
      el.textContent = d.toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
    }
  });

  /* ------------------------------------------------------------------ *
   * AI report (bring-your-own-key; the key stays in localStorage)
   * ------------------------------------------------------------------ */

  var aiGo = $("#kAiGo");
  if (aiGo) {
    var AI_PROVIDERS = {
      gemini:     { label: "Google Gemini", base: "", model: "gemini-flash-lite-latest" },
      openai:     { label: "OpenAI", base: "https://api.openai.com/v1", model: "gpt-4o-mini" },
      groq:       { label: "Groq", base: "https://api.groq.com/openai/v1", model: "llama-3.1-8b-instant" },
      openrouter: { label: "OpenRouter", base: "https://openrouter.ai/api/v1", model: "openrouter/auto" },
      custom:     { label: "Custom", base: "", model: "" }
    };
    var prov = AI_PROVIDERS[prefs.aiProv] || AI_PROVIDERS.gemini;
    var provEl = $("#kAiProv");
    if (provEl) provEl.textContent = prov.label;

    var out = $("#kAiOut");

    function aiRender(text) {
      if (!out) return;
      out.hidden = false;
      out.textContent = "";
      var i = 0;
      (function step() {
        if (i >= text.length) return;
        out.textContent += text.slice(i, i + 6);
        i += 6;
        setTimeout(step, 8);
      })();
    }

    aiGo.addEventListener("click", function () {
      if (!prefs.aiKey) {
        toast("Add your API key in Settings → AI Report");
        openSettings();
        return;
      }
      var model = prefs.aiModel || prov.model;
      if (!model) { toast("Set a model name in Settings"); openSettings(); return; }
      var base = prefs.aiBase || prov.base;
      if (prefs.aiProv !== "gemini" && !base) { toast("Set a Base URL for the custom provider"); return; }

      var q = aiGo.getAttribute("data-q") || "";
      var ctx = "";
      try { ctx = JSON.parse(aiGo.getAttribute("data-ctx") || '""'); } catch (e) { ctx = ""; }
      var prompt =
        "You are Kuroka, the AI librarian of the KurokaSearch metasearch engine. " +
        "Write a concise, factual briefing (max ~180 words) answering the user's query, " +
        "based ONLY on the provided search-result snippets. Cite the domains you used " +
        "inline like (example.com). If the snippets are insufficient, say so plainly.\n\n" +
        "Query: " + q + "\n\nSearch result snippets:\n" + ctx;

      aiGo.disabled = true;
      aiGo.textContent = "Thinking\u2026";
      if (out) { out.hidden = false; out.textContent = "\u2026"; }

      var done = function (text) {
        aiGo.disabled = false;
        aiGo.textContent = "Generate";
        aiRender(String(text || "").trim() || "(empty response)");
      };
      var fail = function (msg) {
        aiGo.disabled = false;
        aiGo.textContent = "Generate";
        if (out) out.hidden = true;
        toast("AI error: " + msg);
      };

      if (prefs.aiProv === "gemini") {
        var u = "https://generativelanguage.googleapis.com/v1beta/models/" +
                encodeURIComponent(model) + ":generateContent?key=" + encodeURIComponent(prefs.aiKey);
        fetch(u, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ contents: [{ role: "user", parts: [{ text: prompt }] }] })
        }).then(function (r) {
          return r.json().then(function (j) { return { ok: r.ok, j: j }; });
        }).then(function (res) {
          if (!res.ok) throw new Error((res.j && res.j.error && res.j.error.message) || ("HTTP " + res.status));
          var parts = res.j && res.j.candidates && res.j.candidates[0] &&
                      res.j.candidates[0].content && res.j.candidates[0].content.parts;
          var text = (parts || []).map(function (p) { return p.text || ""; }).join("");
          done(text);
        }).catch(function (e) { fail(e.message || "network"); });
      } else {
        fetch(base.replace(/\/+$/, "") + "/chat/completions", {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Authorization": "Bearer " + prefs.aiKey
          },
          body: JSON.stringify({
            model: model,
            messages: [
              { role: "system", content: "You are Kuroka, a concise and factual search summarizer." },
              { role: "user", content: prompt }
            ]
          })
        }).then(function (r) {
          return r.json().then(function (j) { return { ok: r.ok, j: j }; });
        }).then(function (res) {
          if (!res.ok) throw new Error((res.j && res.j.error && res.j.error.message) || ("HTTP " + res.status));
          var text = res.j && res.j.choices && res.j.choices[0] &&
                     res.j.choices[0].message && res.j.choices[0].message.content;
          done(text);
        }).catch(function (e) { fail(e.message || "network"); });
      }
    });
  }
})();
