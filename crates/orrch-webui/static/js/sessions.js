// Hypervise panel — interactive session list + drill-in focus view.
// Lives entirely in panel-hypervise; no client-side route changes.
//
// State pushed via WS at ~1Hz includes preview lines per session. Re-renders
// preserve typed text and focus by capturing the active element + selection
// before wiping the panel and restoring afterward.
var Sessions = (function() {
  var expanded = {};        // name -> bool
  var promptOpen = {};      // name -> bool (inline prompt textarea visible)
  var promptBuffers = {};   // name -> string
  var focused = null;       // name | null
  var INLINE_COLLAPSED = 6;
  var INLINE_EXPANDED = 30;
  var FOCUS_LINES = 200;

  function init() {
    WS.onState(render);
  }

  function captureFocusState(panel) {
    var act = document.activeElement;
    if (!act || !panel.contains(act)) return null;
    var sel = null;
    if (act.tagName === 'TEXTAREA' || act.tagName === 'INPUT') {
      sel = { start: act.selectionStart, end: act.selectionEnd };
    }
    return { id: act.dataset.sid || null, kind: act.dataset.kind || null, sel: sel };
  }

  function restoreFocusState(panel, snap) {
    if (!snap) return;
    var sel = '[data-sid="' + snap.id + '"][data-kind="' + snap.kind + '"]';
    var node = panel.querySelector(sel);
    if (!node) return;
    node.focus({ preventScroll: true });
    if (snap.sel && (node.tagName === 'TEXTAREA' || node.tagName === 'INPUT')) {
      try { node.setSelectionRange(snap.sel.start, snap.sel.end); } catch (e) {}
    }
  }

  function el(tag, opts) {
    var n = document.createElement(tag);
    if (opts) {
      if (opts.cls) n.className = opts.cls;
      if (opts.text != null) n.textContent = opts.text;
      if (opts.attrs) for (var k in opts.attrs) n.setAttribute(k, opts.attrs[k]);
      if (opts.style) for (var k2 in opts.style) n.style[k2] = opts.style[k2];
      if (opts.data) for (var k3 in opts.data) n.dataset[k3] = opts.data[k3];
    }
    return n;
  }

  function previewBlock(s, maxLines) {
    var pre = el('pre', { cls: 'session-preview', data: { sid: s.name, kind: 'preview' } });
    pre.setAttribute('aria-label', 'session preview');
    var lines = (s.preview || []);
    if (maxLines > 0 && lines.length > maxLines) {
      lines = lines.slice(lines.length - maxLines);
    }
    pre.textContent = lines.length === 0 ? '(no output yet)' : lines.join('\n');
    return pre;
  }

  function makePromptArea(s, isFocus) {
    var wrap = el('div', { cls: 'session-prompt-wrap' });
    var ta = el('textarea', {
      cls: 'session-prompt-input',
      attrs: { rows: isFocus ? 4 : 2, placeholder: 'Type a prompt — Enter to send, Shift+Enter for newline' },
      data: { sid: s.name, kind: 'prompt' }
    });
    ta.value = promptBuffers[s.name] || '';
    autosize(ta);
    ta.addEventListener('input', function() {
      promptBuffers[s.name] = ta.value;
      autosize(ta);
    });
    ta.addEventListener('keydown', function(e) {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        submitPrompt(s.name);
      } else if (e.key === 'Escape' && !isFocus) {
        e.preventDefault();
        promptOpen[s.name] = false;
        render(WS.getLastState());
      }
    });
    var bar = el('div', { cls: 'session-prompt-bar' });
    var hint = el('span', { cls: 'session-prompt-hint', text: 'Enter sends · Shift+Enter newline' });
    var send = el('button', { cls: 'session-prompt-send', text: 'send →' });
    send.addEventListener('click', function() { submitPrompt(s.name); });
    bar.appendChild(hint);
    bar.appendChild(send);
    wrap.appendChild(ta);
    wrap.appendChild(bar);
    return wrap;
  }

  function autosize(ta) {
    ta.style.height = 'auto';
    var h = Math.min(ta.scrollHeight, 320);
    ta.style.height = h + 'px';
  }

  function submitPrompt(name) {
    var text = (promptBuffers[name] || '').trim();
    if (!text) return;
    WS.action({ action: 'send_prompt', session_name: name, text: text });
    promptBuffers[name] = '';
  }

  function rawKey(name, key) {
    WS.action({ action: 'send_raw_key', session_name: name, key: key });
  }

  function renderListItem(parent, s) {
    var item = el('div', { cls: 'session-card' + (expanded[s.name] ? ' expanded' : '') });

    var header = el('div', { cls: 'session-card-header' });
    var nameWrap = el('div', { cls: 'session-card-name' });
    var caret = el('span', { cls: 'session-card-caret', text: expanded[s.name] ? '▾' : '▸' });
    var name = el('span', { cls: 'item-name', text: s.name });
    nameWrap.appendChild(caret);
    nameWrap.appendChild(name);

    var badges = el('div', { cls: 'session-card-badges' });
    if (s.status) {
      var statusBadgeCls = 'badge ';
      if (s.status === 'working') statusBadgeCls += 'badge-progress';
      else if (s.status === 'waiting') statusBadgeCls += 'badge-pending';
      else statusBadgeCls += 'badge-done';
      badges.appendChild(el('span', { cls: statusBadgeCls, text: s.status }));
    }
    if (s.category) {
      badges.appendChild(el('span', { cls: 'badge badge-progress', text: s.category }));
    }
    var openBtn = el('button', { cls: 'session-open-btn', text: 'open ⤢', attrs: { title: 'Open as a focused page' } });
    openBtn.addEventListener('click', function(ev) {
      ev.stopPropagation();
      focused = s.name;
      expanded[s.name] = true;
      render(WS.getLastState());
    });
    badges.appendChild(openBtn);

    header.appendChild(nameWrap);
    header.appendChild(badges);

    header.addEventListener('click', function() {
      expanded[s.name] = !expanded[s.name];
      render(WS.getLastState());
    });
    item.appendChild(header);

    if (s.cwd) {
      item.appendChild(el('div', { cls: 'item-meta session-cwd', text: s.cwd }));
    }

    item.appendChild(previewBlock(s, expanded[s.name] ? INLINE_EXPANDED : INLINE_COLLAPSED));

    var promptRow = el('div', { cls: 'session-prompt-row' });
    if (!promptOpen[s.name]) {
      promptRow.classList.add('clickable');
      promptRow.textContent = '✎  click to send a prompt to ' + s.name;
      promptRow.addEventListener('click', function() {
        promptOpen[s.name] = true;
        expanded[s.name] = true;
        render(WS.getLastState());
        var ta = parent.querySelector('[data-sid="' + s.name + '"][data-kind="prompt"]');
        if (ta) ta.focus();
      });
    } else {
      promptRow.appendChild(makePromptArea(s, false));
    }
    item.appendChild(promptRow);

    var attachDiv = el('div', { cls: 'item-meta session-attach' });
    var code = el('code', { cls: 'attach-cmd', text: s.attach_cmd, attrs: { title: 'Click to copy' } });
    code.addEventListener('click', function(ev) {
      ev.stopPropagation();
      navigator.clipboard.writeText(s.attach_cmd).catch(function() {});
    });
    attachDiv.appendChild(code);
    item.appendChild(attachDiv);

    parent.appendChild(item);
  }

  function renderFocusView(parent, s) {
    var view = el('div', { cls: 'session-focus-view' });

    var bar = el('div', { cls: 'session-focus-bar' });
    var back = el('button', { cls: 'session-focus-back', text: '← back to list' });
    back.addEventListener('click', function() {
      focused = null;
      render(WS.getLastState());
    });
    var title = el('div', { cls: 'session-focus-title' });
    title.appendChild(el('span', { cls: 'item-name', text: s.name }));
    if (s.status) title.appendChild(el('span', { cls: 'badge badge-progress', text: s.status, style: { marginLeft: '8px' } }));
    var live = el('span', { cls: 'session-focus-live', text: '● live' });
    bar.appendChild(back);
    bar.appendChild(title);
    bar.appendChild(live);
    view.appendChild(bar);

    if (s.cwd) view.appendChild(el('div', { cls: 'item-meta session-cwd', text: s.cwd }));

    var pre = previewBlock(s, FOCUS_LINES);
    pre.classList.add('session-focus-preview');
    view.appendChild(pre);

    var keyRow = el('div', { cls: 'session-key-row' });
    [
      ['↑', 'Up'], ['↓', 'Down'], ['←', 'Left'], ['→', 'Right'],
      ['Enter', 'Enter'], ['Esc', 'Escape'], ['Tab', 'Tab'], ['Ctrl+C', 'C-c']
    ].forEach(function(pair) {
      var b = el('button', { cls: 'session-key-btn', text: pair[0] });
      b.addEventListener('click', function() { rawKey(s.name, pair[1]); });
      keyRow.appendChild(b);
    });
    view.appendChild(keyRow);

    view.appendChild(makePromptArea(s, true));

    var attach = el('div', { cls: 'item-meta session-attach' });
    var code = el('code', { cls: 'attach-cmd', text: s.attach_cmd });
    code.addEventListener('click', function() { navigator.clipboard.writeText(s.attach_cmd).catch(function() {}); });
    attach.appendChild(code);
    view.appendChild(attach);

    parent.appendChild(view);

    requestAnimationFrame(function() {
      if (pre.scrollHeight > pre.clientHeight) {
        pre.scrollTop = pre.scrollHeight;
      }
    });
  }

  function render(state) {
    var panel = document.getElementById('panel-hypervise');
    if (!panel || !panel.classList.contains('active')) return;
    if (!state) state = {};
    var sessions = state.sessions || [];

    var snap = captureFocusState(panel);
    panel.textContent = '';

    if (focused) {
      var s = null;
      for (var i = 0; i < sessions.length; i++) {
        if (sessions[i].name === focused) { s = sessions[i]; break; }
      }
      if (!s) {
        focused = null;
      } else {
        renderFocusView(panel, s);
        restoreFocusState(panel, snap);
        return;
      }
    }

    if (sessions.length === 0) {
      panel.appendChild(el('p', { cls: 'item-meta', text: 'No active sessions', style: { padding: '16px' } }));
      return;
    }

    var wrapper = el('div', { cls: 'session-list-wrap' });
    sessions.forEach(function(sess) { renderListItem(wrapper, sess); });
    panel.appendChild(wrapper);

    restoreFocusState(panel, snap);
  }

  return { init: init, render: render };
})();
