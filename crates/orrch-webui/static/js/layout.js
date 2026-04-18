var Layout = (function() {
  var panels = ['design', 'oversee', 'hypervise', 'analyze'];
  var active = 'design';

  function init() {
    buildNav();
    WS.onState(syncFromServer);
    Intentions.init();
    Sessions.init();
  }

  function buildNav() {
    var nav = document.getElementById('top-nav');
    panels.forEach(function(name) {
      var btn = document.createElement('button');
      btn.className = 'nav-tab' + (name === active ? ' active' : '');
      btn.textContent = name.charAt(0).toUpperCase() + name.slice(1);
      btn.addEventListener('click', function() { switchPanel(name); });
      nav.appendChild(btn);
    });
  }

  function switchPanel(name) {
    active = name;
    document.querySelectorAll('.nav-tab').forEach(function(b, i) {
      b.classList.toggle('active', panels[i] === name);
    });
    document.querySelectorAll('.panel').forEach(function(p) {
      p.classList.remove('active');
    });
    var el = document.getElementById('panel-' + name);
    if (el) el.classList.add('active');
    var ls = WS.getLastState();
    if (name === 'design' && ls && Object.keys(ls).length) Intentions.render(ls);
  }

  function syncFromServer(state) {
    if (state.active_panel && state.active_panel !== active) {
      switchPanel(state.active_panel);
    }
  }

  return { init: init, switchPanel: switchPanel };
})();
