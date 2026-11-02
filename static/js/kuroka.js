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
   * search box: clear button + focus on results page
   * ------------------------------------------------------------------ */

  var qInput = $("#kQ"), clearBtn = $("#kClear");
  function syncClear() {
    if (!clearBtn || !qInput) return;
    clearBtn.hidden = qInput.value.length === 0;
  }
  if (qInput && clearBtn) {
    qInput.addEventListener("input", syncClear);
    syncClear();
    clearBtn.addEventListener("click", function () {
      qInput.value = "";
      syncClear();
      qInput.focus();
    });
    if (document.body.classList.contains("k-page-results") && window.matchMedia("(min-width: 761px)").matches) {
      var v = qInput.value.length;
      qInput.focus();
      qInput.setSelectionRange(v, v);
    }
  }

  /* home form: carry preferences into the first search */
  var homeForm = $(".k-srow-home");
  if (homeForm) {
    homeForm.addEventListener("submit", function () {
      function addHidden(name, value) {
        if (value === "" || value === "auto" || value === "0") return;
        var i = document.createElement("input");
        i.type = "hidden"; i.name = name; i.value = value;
        homeForm.appendChild(i);
      }
      var c = homeForm.querySelector('input[name="c"]');
      if (c && prefs.cat && prefs.cat !== "all") c.value = prefs.cat;
      addHidden("lang", prefs.lang);
      addHidden("time", prefs.time);
      if (prefs.ss && prefs.ss !== "0") addHidden("ss", String(prefs.ss));
    });
  }

  /* open results in a new tab (preference) */
  if (prefs.newtab) {
    $$(".k-main a[href^='http']").forEach(function (a) {
      a.target = "_blank";
      a.rel = "noopener noreferrer";
    });
  }

  /* ------------------------------------------------------------------ *
   * autocomplete (server-proxied — the provider never sees the user)
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
      list.forEach(function (s, i) {
        var d = document.createElement("div");
        d.className = "k-ac-it";
        d.textContent = s;
        function apply() {
          qInput.value = s;
          acHide();
          if (qInput.form) qInput.form.submit();
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
   * results filters: selects navigate, buttons toggle panels
   * ------------------------------------------------------------------ */

  function searchUrl(over) {
    var sp = new URLSearchParams(window.location.search);
    var q = sp.get("q") || "";
    var url = "/search?q=" + encodeURIComponent(q);
    var c = over.c !== undefined ? over.c : (sp.get("c") || "all");
    var lang = over.lang !== undefined ? over.lang : (sp.get("lang") || prefs.lang || "auto");
    var time = over.time !== undefined ? over.time : (sp.get("time") || "");
    var ss = over.ss !== undefined ? String(over.ss) : (sp.get("ss") || "0");
    if (c && c !== "all") url += "&c=" + c;
    if (lang && lang !== "auto") url += "&lang=" + lang;
    if (time) url += "&time=" + time;
    if (ss && ss !== "0") url += "&ss=" + ss;
    if (over.e) url += "&e=" + over.e;
    if (over.size && over.size !== "any") url += "&size=" + over.size;
    if (over.layout && over.layout !== "any") url += "&layout=" + over.layout;
    return url;
  }

  $$(".k-filters .k-sel").forEach(function (sel) {
    sel.addEventListener("change", function () {
      var over = {};
      over[sel.name] = sel.value;
      window.location.href = searchUrl(over);
    });
  });

  var extrasBtn = $("#kExtrasBtn"), extrasPanel = $("#kExtras"), infoBtn = $("#kInfoBtn"), infoPanel = $("#kInfoPanel");

  function togglePanel(btn, panel) {
    if (!btn || !panel) return;
    btn.addEventListener("click", function () {
      var open = !panel.hidden;
      panel.hidden = open;
      btn.setAttribute("aria-expanded", String(!open));
    });
  }
  togglePanel(extrasBtn, extrasPanel);
  togglePanel(infoBtn, infoPanel);

  var extrasApply = $("#kExtrasApply");
  if (extrasApply) {
    extrasApply.addEventListener("click", function () {
      var engines = $$('input[name="k-engine"]:checked').map(function (i) { return i.value; });
      var size = $('.k-sel[name="x-size"]');
      var layout = $('.k-sel[name="x-layout"]');
      window.location.href = searchUrl({
        e: engines.join(","),
        size: size ? size.value : "",
        layout: layout ? layout.value : ""
      });
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
      if (!prefs.aiKey && !(prefs.aiProv === "gemini" && prefs.aiKey === "")) {
        if (!prefs.aiKey) {
          toast("Add your API key in Preferences \u2192 AI Report");
          return;
        }
      }
      var model = prefs.aiModel || prov.model;
      if (!model) { toast("Set a model name in Preferences"); return; }
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

  /* ------------------------------------------------------------------ *
   * preferences page
   * ------------------------------------------------------------------ */

  var saveBtn = $("#p-save");
  if (saveBtn) {
    var LANGS = [["auto", "Auto-detect"], ["en", "English"], ["az", "Az\u0259rbaycanca"], ["tr", "T\u00fcrk\u00e7e"],
      ["ru", "\u0420\u0443\u0441\u0441\u043a\u0438\u0439"], ["de", "Deutsch"], ["fr", "Fran\u00e7ais"], ["es", "Espa\u00f1ol"],
      ["ar", "\u0627\u0644\u0639\u0631\u0628\u064a\u0629"], ["ja", "\u65e5\u672c\u8a9e"], ["zh", "\u4e2d\u6587"]];
    var TIMES = [["", "Anytime"], ["d", "Past 24 hours"], ["w", "Past week"], ["m", "Past month"], ["y", "Past year"]];
    var SS = [["0", "Off"], ["1", "Moderate"], ["2", "Strict"]];
    var CATS = [["all", "All"], ["images", "Images"], ["videos", "Videos"], ["news", "News"], ["maps", "Maps"],
      ["music", "Music"], ["it", "IT"], ["science", "Science"], ["files", "Files"], ["social", "Social Media"]];
    var PROVS = [["gemini", "Google Gemini"], ["openai", "OpenAI"], ["groq", "Groq"],
      ["openrouter", "OpenRouter"], ["custom", "Custom (OpenAI-compatible)"]];

    function fill(id, options, value) {
      var sel = $("#" + id);
      if (!sel) return;
      sel.innerHTML = "";
      options.forEach(function (o) {
        var op = document.createElement("option");
        op.value = o[0]; op.textContent = o[1];
        if (o[0] === value) op.selected = true;
        sel.appendChild(op);
      });
    }

    fill("p-lang", LANGS, prefs.lang);
    fill("p-time", TIMES, prefs.time);
    fill("p-ss", SS, String(prefs.ss));
    fill("p-cat", CATS, prefs.cat);
    fill("p-ai-prov", PROVS, prefs.aiProv);
    var pm = $("#p-ai-model"); if (pm) pm.value = prefs.aiModel || "";
    var pk = $("#p-ai-key"); if (pk) pk.value = prefs.aiKey || "";
    var pb = $("#p-ai-base"); if (pb) pb.value = prefs.aiBase || "";
    var pn = $("#p-newtab"); if (pn) pn.checked = !!prefs.newtab;
    var pa = $("#p-ac"); if (pa) pa.checked = prefs.ac !== false;

    saveBtn.addEventListener("click", function () {
      prefs = {
        lang: ($("#p-lang") || {}).value || "auto",
        time: ($("#p-time") || {}).value || "",
        ss: ($("#p-ss") || {}).value || "0",
        cat: ($("#p-cat") || {}).value || "all",
        newtab: !!($("#p-newtab") || {}).checked,
        ac: ($("#p-ac") || {}).checked !== false,
        aiProv: ($("#p-ai-prov") || {}).value || "gemini",
        aiModel: ($("#p-ai-model") || {}).value || "",
        aiKey: ($("#p-ai-key") || {}).value || "",
        aiBase: ($("#p-ai-base") || {}).value || ""
      };
      savePrefs(prefs);
      toast("Preferences saved \u2014 stored only in this browser");
    });
  }
})();
