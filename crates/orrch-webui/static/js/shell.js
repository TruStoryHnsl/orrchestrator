// Page 2 of the swipe terminal: an independent tmux session displayed
// via xterm.js. Streams pane bytes from /shell over a WebSocket and
// forwards keystrokes back.
//
// Sizing flow (the bit that fixes the mobile-portrait black-void bug):
//
//   1. We open xterm.js at a tiny placeholder size (80×24).
//   2. We attach FitAddon and call fit.proposeDimensions() against the
//      actual rendered DOM box. That gives us the cols × rows that
//      will exactly fill the viewport at the current font-size.
//   3. We resize the xterm grid to those dims AND push the same dims
//      to the server as a JSON control frame: {"type":"resize",...}.
//      The server runs `tmux resize-window` so the underlying pane
//      matches what the browser is rendering.
//   4. On window resize / orientation change / soft-keyboard show, we
//      debounce-redo steps 2-3.
//
// Without step 3 the tmux pane stays at its hardcoded creation dims
// (200×50), which on a 390px portrait phone produces the horizontal
// smear bug — the prompt and command output land in absolute columns
// far wider than the viewport, cropped, with the bottom 70% empty.

var Shell = (function() {
  var term = null;
  var ws = null;
  var fit = null;
  var container = null;
  var lastSentCols = 0;
  var lastSentRows = 0;
  var resizeDebounce = null;
  var queuedResize = null; // resize attempted before WS opened

  function init(opts) {
    container = opts.container;
    openTerminal();
  }

  function openTerminal() {
    var fontSize = parseInt(getCSSVar('--term-font-size'), 10) || 13;
    term = new Terminal({
      cursorBlink: true,
      fontSize: fontSize,
      fontFamily: getCSSVar('--term-font-family') || 'monospace',
      // Placeholder grid; replaced immediately by fitToViewport().
      cols: 80, rows: 24,
      theme: { background: '#050d18', foreground: '#9ad', cursor: '#4af' },
      scrollback: 1000,
      allowProposedApi: true
    });
    term.open(container);
    try {
      fit = new FitAddon.FitAddon();
      term.loadAddon(fit);
    } catch (e) {
      console.warn('FitAddon unavailable; falling back to fixed grid', e);
    }
    // Wait one frame so the container has its real layout box, then
    // fit-and-send.
    requestAnimationFrame(function() { fitToViewport(); });
    openWebSocket();
  }

  // Compute cols × rows that exactly fill the panel-body box at the
  // current font size, resize the xterm grid locally, then push the
  // same dims to the server. Idempotent within a frame; suppressed
  // when nothing changed since the last send.
  function fitToViewport() {
    if (!term || !container) return;
    var dims = null;
    if (fit) {
      try {
        // fit.proposeDimensions can return null in rare layout
        // states (zero-size container during a transition).
        dims = fit.proposeDimensions();
      } catch (e) { /* ignore */ }
    }
    if (!dims || !dims.cols || !dims.rows) {
      dims = computeDimsManually();
    }
    if (!dims) return;
    var cols = Math.max(2, dims.cols | 0);
    var rows = Math.max(2, dims.rows | 0);
    if (cols !== term.cols || rows !== term.rows) {
      try { term.resize(cols, rows); } catch (e) {}
    }
    sendResize(cols, rows);
  }

  // Fallback dimension calculator for when FitAddon is unavailable or
  // returns null. Reads the rendered char cell from xterm's internal
  // helper if possible, else estimates from font-size.
  function computeDimsManually() {
    var box = container.parentElement;
    if (!box) return null;
    var w = box.clientWidth, h = box.clientHeight;
    if (w <= 0 || h <= 0) return null;
    var fontSize = parseInt(getCSSVar('--term-font-size'), 10) || 13;
    // Empirically: monospace cell width ~= 0.6 * font-size px,
    //              cell height ~= 1.2 * font-size px (xterm default).
    var cellW = fontSize * 0.6;
    var cellH = fontSize * 1.2;
    var cols = Math.floor(w / cellW);
    var rows = Math.floor(h / cellH);
    if (cols < 2 || rows < 2) return null;
    return { cols: cols, rows: rows };
  }

  function sendResize(cols, rows) {
    if (cols === lastSentCols && rows === lastSentRows) return;
    lastSentCols = cols;
    lastSentRows = rows;
    var frame = JSON.stringify({ type: 'resize', cols: cols, rows: rows });
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(frame);
    } else {
      // WS not yet open; cache the latest desired size and let the
      // onopen handler flush it.
      queuedResize = frame;
    }
  }

  function openWebSocket() {
    var proto = location.protocol === 'https:' ? 'wss' : 'ws';
    ws = new WebSocket(proto + '://' + location.host + '/shell');
    ws.binaryType = 'arraybuffer';
    ws.onopen = function() {
      // Push the most recent fit dims so tmux matches the browser.
      if (queuedResize) {
        try { ws.send(queuedResize); } catch (_) {}
        queuedResize = null;
      } else if (lastSentCols && lastSentRows) {
        try {
          ws.send(JSON.stringify({
            type: 'resize',
            cols: lastSentCols, rows: lastSentRows,
          }));
        } catch (_) {}
      }
    };
    ws.onmessage = function(e) {
      if (e.data instanceof ArrayBuffer) {
        term.write(new Uint8Array(e.data));
      } else if (e.data instanceof Blob) {
        e.data.arrayBuffer().then(function(buf) {
          term.write(new Uint8Array(buf));
        });
      } else if (typeof e.data === 'string') {
        term.write(e.data);
      }
    };
    ws.onclose = function() {
      if (term) term.write('\r\n\x1b[31m[shell disconnected — reconnecting]\x1b[0m\r\n');
      // Force re-send on next open.
      lastSentCols = 0; lastSentRows = 0;
      setTimeout(openWebSocket, 2000);
    };
    ws.onerror = function() { try { ws.close(); } catch (_) {} };

    term.onData(function(d) {
      if (ws && ws.readyState === WebSocket.OPEN) {
        ws.send(new TextEncoder().encode(d));
      }
    });
  }

  // Public hook the swipe page calls on viewport change. Debounced so
  // a flurry of orientationchange/resize events doesn't pummel the
  // server with /shell/resize calls.
  function rescale() {
    clearTimeout(resizeDebounce);
    resizeDebounce = setTimeout(function() { fitToViewport(); }, 120);
  }

  function sendKey(key) {
    if (!ws || ws.readyState !== WebSocket.OPEN) return;
    var seq = {
      Enter: '\r', Tab: '\t', Escape: '\x1b', Backspace: '\x7f',
      ArrowUp: '\x1b[A', ArrowDown: '\x1b[B',
      ArrowRight: '\x1b[C', ArrowLeft: '\x1b[D'
    }[key] || key;
    ws.send(new TextEncoder().encode(seq));
  }

  function focus() {
    if (term) term.focus();
  }

  function getCSSVar(name) {
    return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  }

  return { init: init, sendKey: sendKey, rescale: rescale, focus: focus };
})();
