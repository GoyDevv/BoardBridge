/* Injected into every page before its own scripts run.

   Three jobs:
   1. answer navigator.getGamepads() straight out of the native touch state
   2. when PC mode is on, make the page believe it is on a desktop
   3. when mouse mode is on, turn the camera zone into a real mouse

   The gamepad hot path uses one synchronous native call and a compact UTF-16 snapshot.
   That keeps the input path off the frame queue while avoiding split()/substring() churn. */
(function () {
  if (window.__XTGA) return; window.__XTGA = true;
  var NB = 17;

  /* ======================= 1. desktop disguise ======================= */
  function def(o, k, v) {
    try { Object.defineProperty(o, k, { configurable: true, get: function () { return v; } }); } catch (e) {}
  }
  if (window.__XTG_PC) {
    var n = navigator;
    def(n, 'maxTouchPoints', 0);
    def(n, 'msMaxTouchPoints', 0);
    def(n, 'platform', 'Win32');
    try {
      if (n.userAgentData) def(n, 'userAgentData', {
        brands: n.userAgentData.brands, mobile: false, platform: 'Windows',
        getHighEntropyValues: function () {
          return Promise.resolve({ platform: 'Windows', platformVersion: '15.0.0', mobile: false,
            architecture: 'x86', bitness: '64', model: '' });
        },
        toJSON: function () { return { brands: n.userAgentData.brands, mobile: false, platform: 'Windows' }; }
      });
    } catch (e) {}
    try { delete window.ontouchstart; delete window.ontouchmove; delete window.ontouchend; } catch (e) {}
    var mm = window.matchMedia;
    if (mm) try {
      window.matchMedia = function (q) {
        var s = String(q), f = null;
        if (/pointer\s*:\s*coarse/i.test(s) || /hover\s*:\s*none/i.test(s)) f = false;
        else if (/pointer\s*:\s*fine/i.test(s) || /hover\s*:\s*hover/i.test(s)) f = true;
        var r = mm.call(window, s);
        if (f === null) return r;
        try { def(r, 'matches', f); return r; } catch (e) {}
        return { media: s, matches: f, onchange: null, addListener: function () {},
          removeListener: function () {}, addEventListener: function () {},
          removeEventListener: function () {}, dispatchEvent: function () { return false; } };
      };
    } catch (e) {}
  }

  /* ======================= 2. the gamepad ======================= */
  var axes = [0, 0, 0, 0];
  var buttons = new Array(NB);
  for (var i = 0; i < NB; i++) buttons[i] = { pressed: false, touched: false, value: 0 };
  var connected = false, last = null;
  var useFast = !!(window.XTGN && window.XTGN.stateFast);

  var pad = {
    id: 'Xbox Wireless Controller (STANDARD GAMEPAD Vendor: 045e Product: 02ea)',
    index: 0, connected: true, mapping: 'standard', timestamp: 0,
    axes: axes, buttons: buttons, vibrationActuator: null,
    hand: '', pose: null, displayId: 0, axesLength: 4
  };
  try { pad[Symbol.toStringTag] = 'Gamepad'; } catch (e) {}
  var list = [pad, null, null, null], none = [null, null, null, null];

  function decodeFast(s) {
    var ax0 = s.charCodeAt(1), ax1 = s.charCodeAt(2), ax2 = s.charCodeAt(3), ax3 = s.charCodeAt(4);
    axes[0] = ax0 / 2047.5 - 1;
    axes[1] = ax1 / 2047.5 - 1;
    axes[2] = ax2 / 2047.5 - 1;
    axes[3] = ax3 / 2047.5 - 1;

    buttons[6].value = s.charCodeAt(5) / 1023;
    buttons[7].value = s.charCodeAt(6) / 1023;
    buttons[6].pressed = buttons[6].value > 0.1; buttons[6].touched = buttons[6].pressed;
    buttons[7].pressed = buttons[7].value > 0.1; buttons[7].touched = buttons[7].pressed;

    var lo = s.charCodeAt(7), hi = s.charCodeAt(8);
    for (var j = 0; j < 16; j++) {
      if (j === 6 || j === 7) continue; // LT/RT stay analog
      var o = buttons[j], p = ((lo >>> j) & 1) !== 0;
      o.value = p ? 1 : 0; o.pressed = p; o.touched = p;
    }
    var g = buttons[16], gp = (hi & 1) !== 0;
    g.value = gp ? 1 : 0; g.pressed = gp; g.touched = gp;
    connected = s.charCodeAt(0) === 1;
  }

  function sync() {
    var s;
    try { s = useFast ? window.XTGN.stateFast() : window.XTGN.state(); } catch (e) { return; }
    if (!s) return;
    if (s === last) { pad.timestamp = performance.now(); return; }
    last = s;

    if (useFast && s.length >= 9) {
      decodeFast(s);
      pad.timestamp = performance.now();
      return;
    }

    var s1 = s.indexOf(';'), s2 = s.indexOf(';', s1 + 1);
    connected = s.charCodeAt(0) === 49;
    var a = s.substring(s1 + 1, s2).split(',');
    for (var i = 0; i < 4; i++) axes[i] = (+a[i] || 0) / 1000;
    var b = s.substring(s2 + 1).split(',');
    for (var j = 0; j < NB; j++) {
      var v = (+b[j] || 0) / 1000, o = buttons[j];
      o.value = v; o.pressed = v > 0.1; o.touched = o.pressed;
    }
    pad.timestamp = performance.now();
  }

  function getGamepads() { sync(); return connected ? list : none; }
  function install(t) {
    try {
      Object.defineProperty(t, 'getGamepads', { value: getGamepads, writable: true, configurable: true });
      Object.defineProperty(t, 'webkitGetGamepads', { value: getGamepads, writable: true, configurable: true });
    } catch (e) {}
  }
  if (window.Navigator && Navigator.prototype) install(Navigator.prototype);
  install(navigator);

  window.__xtgConn = function (v) {
    connected = !!v;
    var ev;
    try { ev = new window.GamepadEvent(v ? 'gamepadconnected' : 'gamepaddisconnected', { gamepad: pad }); }
    catch (e) {
      ev = new Event(v ? 'gamepadconnected' : 'gamepaddisconnected');
      try { Object.defineProperty(ev, 'gamepad', { value: pad }); } catch (e2) {}
    }
    try { window.dispatchEvent(ev); } catch (e3) {}
  };

  /* ======================= 3. mouse and keyboard ======================= */
  var lockEl = null, lockWanted = null, kbmOn = false;
  try {
    var natReq = Element.prototype.requestPointerLock;
    Element.prototype.requestPointerLock = function () {
      lockWanted = this;
      if (kbmOn) {
        lockEl = this;
        setTimeout(function () {
          try { document.dispatchEvent(new Event('pointerlockchange', { bubbles: true })); } catch (e) {}
        }, 0);
        return Promise.resolve();
      }
      try { return natReq.apply(this, arguments); } catch (e) { return Promise.resolve(); }
    };
    var d0 = Object.getOwnPropertyDescriptor(Document.prototype, 'pointerLockElement');
    Object.defineProperty(Document.prototype, 'pointerLockElement', {
      configurable: true,
      get: function () {
        if (lockEl) return lockEl;
        try { return d0 && d0.get ? d0.get.call(this) : null; } catch (e) { return null; }
      }
    });
  } catch (e) {}

  var surf = null, surfAt = 0;
  function surface() {
    var now = Date.now();
    if (surf && surf.isConnected && now - surfAt < 2000) return surf;
    surfAt = now;
    var best = null, area = 0, i, r, a;
    try {
      var vs = document.getElementsByTagName('video');
      for (i = 0; i < vs.length; i++) { r = vs[i].getBoundingClientRect(); a = r.width * r.height;
        if (a > area) { area = a; best = vs[i]; } }
      if (!best) {
        var cs = document.getElementsByTagName('canvas');
        for (i = 0; i < cs.length; i++) { r = cs[i].getBoundingClientRect(); a = r.width * r.height;
          if (a > area) { area = a; best = cs[i]; } }
      }
    } catch (e) {}
    surf = lockEl || best || document.body || document.documentElement;
    return surf;
  }

  var mx = 0, my = 0, mask = 0, held = {};
  var KEYS = {
    KeyW:['w',87],KeyA:['a',65],KeyS:['s',83],KeyD:['d',68],KeyQ:['q',81],KeyE:['e',69],
    KeyR:['r',82],KeyF:['f',70],KeyG:['g',71],KeyC:['c',67],KeyV:['v',86],KeyX:['x',88],
    KeyZ:['z',90],KeyB:['b',66],KeyH:['h',72],KeyT:['t',84],KeyM:['m',77],KeyN:['n',78],
    Space:[' ',32],ShiftLeft:['Shift',16],ControlLeft:['Control',17],AltLeft:['Alt',18],
    Tab:['Tab',9],Escape:['Escape',27],Enter:['Enter',13],
    Digit1:['1',49],Digit2:['2',50],Digit3:['3',51],Digit4:['4',52],Digit5:['5',53],
    ArrowUp:['ArrowUp',38],ArrowDown:['ArrowDown',40],ArrowLeft:['ArrowLeft',37],ArrowRight:['ArrowRight',39]
  };

  function mouseMove(dx, dy) {
    var t = surface(); if (!t) return;
    mx += dx; my += dy;
    var ev;
    try {
      ev = new MouseEvent('mousemove', { bubbles: true, cancelable: true, composed: true, view: window,
        clientX: mx, clientY: my, screenX: mx, screenY: my, movementX: dx, movementY: dy,
        buttons: mask, button: 0 });
    } catch (e) { return; }
    try {
      if (ev.movementX !== dx) {
        Object.defineProperty(ev, 'movementX', { value: dx, configurable: true });
        Object.defineProperty(ev, 'movementY', { value: dy, configurable: true });
      }
    } catch (e) {}
    try { t.dispatchEvent(ev); } catch (e) {}
    try {
      t.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, cancelable: true,
        composed: true, view: window, pointerId: 1, pointerType: 'mouse', isPrimary: true,
        clientX: mx, clientY: my, screenX: mx, screenY: my, movementX: dx, movementY: dy,
        buttons: mask }));
    } catch (e) {}
  }

  function mouseBtn(which, down) {
    var t = surface(); if (!t) return;
    var bit = which === 2 ? 2 : (which === 1 ? 4 : 1);
    if (down) mask |= bit; else mask &= ~bit;
    var init = { bubbles: true, cancelable: true, composed: true, view: window, detail: 1,
      clientX: mx, clientY: my, screenX: mx, screenY: my, button: which, buttons: mask };
    try { t.dispatchEvent(new MouseEvent(down ? 'mousedown' : 'mouseup', init)); } catch (e) {}
    try {
      t.dispatchEvent(new PointerEvent(down ? 'pointerdown' : 'pointerup', { bubbles: true,
        cancelable: true, composed: true, view: window, pointerId: 1, pointerType: 'mouse',
        isPrimary: true, clientX: mx, clientY: my, button: which, buttons: mask }));
    } catch (e) {}
    if (!down && which === 0) { try { t.dispatchEvent(new MouseEvent('click', init)); } catch (e) {} }
    if (which === 2 && down) { try { t.dispatchEvent(new MouseEvent('contextmenu', init)); } catch (e) {} }
  }

  function key(code, down) {
    var m = KEYS[code]; if (!m) return;
    var t = surface(); if (!t) return;
    if (down) held[code] = 1; else delete held[code];
    var ev;
    try {
      ev = new KeyboardEvent(down ? 'keydown' : 'keyup', { bubbles: true, cancelable: true,
        composed: true, view: window, key: m[0], code: code, keyCode: m[1], which: m[1],
        charCode: 0, repeat: false, location: 0,
        shiftKey: !!held.ShiftLeft, ctrlKey: !!held.ControlLeft, altKey: !!held.AltLeft });
    } catch (e) { return; }
    try {
      if (ev.keyCode !== m[1]) {
        Object.defineProperty(ev, 'keyCode', { value: m[1], configurable: true });
        Object.defineProperty(ev, 'which', { value: m[1], configurable: true });
      }
    } catch (e) {}
    try { t.dispatchEvent(ev); } catch (e) {}
  }

  function releaseAll() {
    for (var c in held) key(c, false);
    if (mask & 1) mouseBtn(0, false);
    if (mask & 2) mouseBtn(2, false);
    if (mask & 4) mouseBtn(1, false);
    mask = 0;
  }

  function drain() {
    var s;
    try { s = window.XTGN.kbm(); } catch (e) { return; }
    if (!s) return;
    var parts = s.split('|');
    var m = parts[0].split(',');
    var dx = +m[0] || 0, dy = +m[1] || 0;
    if (dx || dy) mouseMove(dx, dy);
    for (var i = 1; i < parts.length; i++) {
      var p = parts[i];
      if (!p) continue;
      if (p === 'r') { releaseAll(); continue; }
      var c = p.charCodeAt(0), down = p.charCodeAt(1) === 43;
      if (c === 107) key(p.substring(2), down);
      else if (c === 98) mouseBtn(+p.substring(2) || 0, down);
    }
  }

  var raf = null;
  function loop() { drain(); raf = requestAnimationFrame(loop); }
  window.__xtgKbm = function (v) {
    kbmOn = !!v;
    if (kbmOn) {
      if (lockWanted && !lockEl) {
        lockEl = lockWanted;
        try { document.dispatchEvent(new Event('pointerlockchange', { bubbles: true })); } catch (e) {}
      }
      mx = Math.round((window.innerWidth || 1280) / 2);
      my = Math.round((window.innerHeight || 720) / 2);
      if (raf === null) raf = requestAnimationFrame(loop);
    } else {
      if (raf !== null) { cancelAnimationFrame(raf); raf = null; }
      releaseAll();
      if (lockEl) { lockEl = null;
        try { document.dispatchEvent(new Event('pointerlockchange', { bubbles: true })); } catch (e) {} }
    }
  };
  try { window.addEventListener('pagehide', releaseAll, true); } catch (e) {}
})();
