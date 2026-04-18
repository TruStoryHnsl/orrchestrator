var Mobile = (function() {
  var hints = [
    { label: 'n new', key: 'n' },
    { label: 's submit', key: 's' },
    { label: 'r rename', key: 'r' },
    { label: 'X retract', key: 'X' }
  ];
  var holdTimers = {};

  function init() {
    if (window.innerWidth > 768) return;
    buildPad();
  }

  function buildPad() {
    var pad = document.getElementById('mobile-pad');
    if (!pad) return;

    var strip = document.createElement('div');
    strip.className = 'hint-strip';
    hints.forEach(function(h) {
      var btn = document.createElement('div');
      btn.className = 'hint-btn';
      btn.textContent = h.label;
      attachHold(btn, h.key);
      strip.appendChild(btn);
    });
    var toggle = document.createElement('div');
    toggle.className = 'hint-btn toggle-btn';
    toggle.textContent = '\u229f';
    toggle.addEventListener('click', function() {
      var pad = document.getElementById('mobile-pad');
      pad.style.position = pad.style.position === 'relative' ? '' : 'relative';
    });
    strip.appendChild(toggle);
    pad.appendChild(strip);

    var row = document.createElement('div');
    row.className = 'controls-row';

    // Action cluster LEFT
    var cluster = document.createElement('div');
    cluster.className = 'action-cluster';
    var clusterTop = document.createElement('div');
    clusterTop.className = 'action-cluster-top';
    clusterTop.appendChild(makeBtn('Tab', 'pad-btn pad-tab', 'Tab'));
    clusterTop.appendChild(makeBtn('Esc', 'pad-btn pad-esc', 'Escape'));
    cluster.appendChild(clusterTop);
    cluster.appendChild(makeBtn('Enter', 'pad-btn pad-enter', 'Enter'));
    row.appendChild(cluster);

    var spacer = document.createElement('div');
    spacer.className = 'spacer';
    row.appendChild(spacer);

    // D-pad RIGHT
    row.appendChild(buildDpad());

    pad.appendChild(row);
  }

  function buildDpad() {
    var grid = document.createElement('div');
    grid.className = 'dpad';
    var cells = [
      null, { label: '\u2191', key: 'ArrowUp' }, null,
      { label: '\u2190', key: 'ArrowLeft' }, null, { label: '\u2192', key: 'ArrowRight' },
      null, { label: '\u2193', key: 'ArrowDown' }, null
    ];
    cells.forEach(function(cell) {
      var el = document.createElement('div');
      if (cell) {
        el.className = 'dpad-btn';
        el.textContent = cell.label;
        attachHold(el, cell.key);
      } else {
        el.className = 'dpad-center';
      }
      grid.appendChild(el);
    });
    return grid;
  }

  function makeBtn(label, cls, key) {
    var el = document.createElement('div');
    el.className = cls;
    el.textContent = label;
    attachHold(el, key);
    return el;
  }

  function attachHold(el, key) {
    function press() { WS.key(key); }
    function startHold() {
      press();
      holdTimers[key] = setTimeout(function() {
        holdTimers[key] = setInterval(press, 80);
      }, 150);
    }
    function endHold() {
      clearTimeout(holdTimers[key]);
      clearInterval(holdTimers[key]);
    }
    el.addEventListener('touchstart', function(e) { e.preventDefault(); startHold(); }, { passive: false });
    el.addEventListener('touchend', function(e) { e.preventDefault(); endHold(); }, { passive: false });
    el.addEventListener('mousedown', startHold);
    el.addEventListener('mouseup', endHold);
    el.addEventListener('mouseleave', endHold);
  }

  return { init: init };
})();
