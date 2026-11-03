/* KurokaSearch — accent color engine.
   Loaded synchronously in <head> so a saved custom accent paints before
   the first frame (no gold flash). Exposes window.__kAcc for kuroka.js.
   Everything is stored in localStorage only — nothing leaves the browser. */
(function () {
  "use strict";

  var DEFAULT = "#e6b445";
  var GOLD_HUE = 41.4;   /* hue of the default gold, for mascot hue-rotate */
  var GOLD_SAT = 0.763;  /* saturation of the default gold */

  function clamp(v, a, b) { return v < a ? a : (v > b ? b : v); }

  function hexRgb(h) {
    if (typeof h !== "string") return null;
    h = h.replace(/^#/, "").trim();
    if (!/^[0-9a-fA-F]{6}$/.test(h)) return null;
    return [parseInt(h.slice(0, 2), 16), parseInt(h.slice(2, 4), 16), parseInt(h.slice(4, 6), 16)];
  }

  function rgbHsl(r, g, b) {
    r /= 255; g /= 255; b /= 255;
    var mx = Math.max(r, g, b), mn = Math.min(r, g, b);
    var l = (mx + mn) / 2, h = 0, s = 0;
    if (mx !== mn) {
      var d = mx - mn;
      s = l > 0.5 ? d / (2 - mx - mn) : d / (mx + mn);
      if (mx === r) h = ((g - b) / d + (g < b ? 6 : 0));
      else if (mx === g) h = ((b - r) / d + 2);
      else h = ((r - g) / d + 4);
      h *= 60;
    }
    return [h, s, l];
  }

  function h2(n) { n = Math.round(clamp(n, 0, 255)).toString(16); return n.length < 2 ? "0" + n : n; }

  function hslHex(h, s, l) {
    h = ((h % 360) + 360) % 360; s = clamp(s, 0, 1); l = clamp(l, 0, 1);
    var c = (1 - Math.abs(2 * l - 1)) * s;
    var x = c * (1 - Math.abs(((h / 60) % 2) - 1));
    var m = l - c / 2;
    var r = 0, g = 0, b = 0;
    if (h < 60)       { r = c; g = x; }
    else if (h < 120) { r = x; g = c; }
    else if (h < 180) { g = c; b = x; }
    else if (h < 240) { g = x; b = c; }
    else if (h < 300) { r = x; b = c; }
    else              { r = c; b = x; }
    return "#" + h2((r + m) * 255) + h2((g + m) * 255) + h2((b + m) * 255);
  }

  /* full variant set derived from one accent color */
  function compute(hex, light) {
    var c = hexRgb(hex);
    if (!c) return null;
    var hs = rgbHsl(c[0], c[1], c[2]);
    var h = hs[0], s = hs[1], l = hs[2];
    /* light theme needs a darker accent to stay readable on ivory */
    if (light) {
      l = Math.min(l, 0.44);
      if (s > 0.12) s = Math.min(1, s * 1.06);
    }
    var gold = hslHex(h, s, l);
    var rgb = hexRgb(gold);
    /* mascot art hue-rotation: shift gold's hue onto the picked hue */
    var shift, sat;
    if (s < 0.15) {           /* near-neutral pick: desaturate the art instead */
      shift = 0;
      sat = clamp(0.2 + s * 2.2, 0.2, 0.7);
    } else {
      var d = ((h - GOLD_HUE) % 360 + 540) % 360 - 180;
      shift = Math.round(d);
      sat = clamp(s / GOLD_SAT, 0.6, 1.3);
    }
    /* on ivory the emphasis inverts: gold-2 goes darker, gold-3 lighter */
    return {
      gold: gold,
      gold2: light ? hslHex(h, s, clamp(l - 0.12, 0.05, 1))
                   : hslHex(h, s, clamp(l + 0.15, 0, 0.96)),
      gold3: light ? hslHex(h, s, clamp(l + 0.10, 0, 0.92))
                   : hslHex(h, s, clamp(l - 0.17, 0.05, 1)),
      ink: hslHex(h, Math.min(1, s * 0.85), 0.075),
      rgb: rgb ? (rgb[0] + ", " + rgb[1] + ", " + rgb[2]) : "230, 180, 69",
      hue: shift,
      sat: Math.round(sat * 100) / 100,
      base: { h: h, s: s, l: l }
    };
  }

  var PROPS = ["--gold", "--gold-2", "--gold-3", "--gold-rgb", "--gold-ink", "--hue-shift", "--hue-sat"];

  function setOn(el, r) {
    if (!el || !el.style) return;
    if (!r) { PROPS.forEach(function (p) { el.style.removeProperty(p); }); return; }
    el.style.setProperty("--gold", r.gold);
    el.style.setProperty("--gold-2", r.gold2);
    el.style.setProperty("--gold-3", r.gold3);
    el.style.setProperty("--gold-rgb", r.rgb);
    el.style.setProperty("--gold-ink", r.ink);
    el.style.setProperty("--hue-shift", r.hue + "deg");
    el.style.setProperty("--hue-sat", String(r.sat));
  }

  var listeners = [];
  var current = "";   /* currently applied hex ("" = default gold) */

  function apply(hex, light) {
    var r = hex ? compute(hex, light) : null;
    if (hex && !r) return false;
    setOn(document.documentElement, r);
    setOn(document.body, r);          /* overrides body.k-light's own vars */
    current = r ? (r.gold || hex) : "";
    listeners.forEach(function (fn) { try { fn(current); } catch (e) {} });
    return true;
  }

  /* particle palette — six shades of the accent, same spirit as the
     original gold set ["#e6b445","#f0c65e","#d29a2f","#fff3d6","#ffd98a","#f7e6b0"] */
  function pal() {
    if (!current) return ["#e6b445", "#f0c65e", "#d29a2f", "#fff3d6", "#ffd98a", "#f7e6b0"];
    var hs = hexRgb(current);
    if (!hs) return ["#e6b445", "#f0c65e", "#d29a2f", "#fff3d6", "#ffd98a", "#f7e6b0"];
    var t = rgbHsl(hs[0], hs[1], hs[2]);
    var h = t[0], s = t[1], l = t[2];
    return [
      current,
      hslHex(h, s, clamp(l + 0.09, 0, 0.97)),
      hslHex(h, s, clamp(l - 0.12, 0.04, 1)),
      hslHex(h, clamp(s * 0.5, 0, 1), 0.93),
      hslHex(h, clamp(s * 0.8, 0, 1), clamp(l + 0.2, 0, 0.97)),
      hslHex(h, clamp(s * 0.45, 0, 1), clamp(l + 0.3, 0, 0.96))
    ];
  }

  window.__kAcc = {
    def: DEFAULT,
    apply: apply,
    pal: pal,
    get: function () { return current; },
    watch: function (fn) { listeners.push(fn); },
    compute: compute
  };

  /* boot: read saved prefs and paint immediately (head-time, pre-render) */
  var saved = "";
  var light = false;
  try {
    var p = JSON.parse(localStorage.getItem("kuroka.prefs") || "{}");
    if (p && typeof p.accent === "string" && /^#?[0-9a-fA-F]{6}$/.test(p.accent.replace(/^#/, "#"))) {
      saved = p.accent.charAt(0) === "#" ? p.accent : "#" + p.accent;
    }
    light = !!p && p.theme === "light";
  } catch (e) {}
  if (saved && saved.toLowerCase() !== DEFAULT) {
    apply(saved, light);
    /* body appears after this head script — re-apply once it exists, so the
       inline vars also override body.k-light's own definitions */
    document.addEventListener("DOMContentLoaded", function () {
      apply(saved, light);
    });
  }
})();
