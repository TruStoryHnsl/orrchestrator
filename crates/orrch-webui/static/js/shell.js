// Page 2 of the swipe terminal: an independent tmux session displayed
// via xterm.js. Streams pane bytes from /shell over a WebSocket and
// forwards keystrokes back. Sized via /shell/size so the inner xterm
// matches the tmux pane dimensions.
//
// Exposes `Shell.init()` and `Shell.sendKey(name)` so the swipe-page
// host can dispatch the mobile gamepad to whichever panel is in view.

var Shell = (function() {
  var term = null;
  var ws = null;
  var container = null;

  function init(opts) {
    container = opts.container;
    fetch('/shell/size')
      .then(function(r) { return r.json(); })
      .then(function(s) {
        var cols = s.cols && s.cols > 0 ? s.cols : 120;
        var rows = s.rows && s.rows > 0 ? s.rows : 40;
        openTerminal(cols, rows);
      })
      .catch(function() { openTerminal(120, 40); });
  }

  function openTerminal(cols, rows) {
    term = new Terminal({
      cursorBlink: true,
      fontSize: 13,
      cols: cols, rows: rows,
      theme: { background: '#050d18', foreground: '#9ad', cursor: '#4af' },
      scrollback: 1000
    });
    term.open(container);
    setTimeout(rescale, 50);
    openWebSocket();
  }

  function openWebSocket() {
    var proto = location.protocol === 'https:' ? 'wss' : 'ws';
    ws = new WebSocket(proto + '://' + location.host + '/shell');
    ws.binaryType = 'arraybuffer';
    ws.onopen = function() {
      // Tell the server our terminal size so vim / tmux / fzf inside
      // the PTY render correctly. xterm.js reports cols/rows after
      // attach; the server's PtyState resizes the master accordingly.
      sendResize();
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
      if (term) term.write('\r\n\x1b[31m[shell disconnected - reconnecting]\x1b[0m\r\n');
      setTimeout(openWebSocket, 2000);
    };
    ws.onerror = function() { try { ws.close(); } catch (_) {} };

    term.onData(function(d) {
      if (ws && ws.readyState === WebSocket.OPEN) {
        ws.send(new TextEncoder().encode(d));
      }
    });
  }

  function sendResize() {
    if (!ws || ws.readyState !== WebSocket.OPEN || !term) return;
    try {
      ws.send(JSON.stringify({ type: 'resize', cols: term.cols, rows: term.rows }));
    } catch (_) {}
  }

  function rescale() {
    if (!term || !container) return;
    var wrap = container.parentElement;
    if (!wrap) return;
    var natW = container.offsetWidth || 1;
    var natH = container.offsetHeight || 1;
    var availW = wrap.clientWidth;
    var availH = wrap.clientHeight;
    container.style.transform = 'none';
    var sx = availW / natW;
    var sy = availH / natH;
    var scale = Math.min(sx, sy, 1);
    container.style.transform = 'scale(' + scale + ')';
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

  return { init: init, sendKey: sendKey, rescale: rescale, focus: focus };
})();
