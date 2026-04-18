var Sessions = (function() {
  function init() {
    WS.onState(render);
  }

  function render(state) {
    var panel = document.getElementById('panel-hypervise');
    if (!panel.classList.contains('active')) return;
    panel.textContent = '';

    var sessions = state.sessions || [];
    if (sessions.length === 0) {
      var p = document.createElement('p');
      p.className = 'item-meta';
      p.style.padding = '16px';
      p.textContent = 'No active sessions';
      panel.appendChild(p);
      return;
    }

    var wrapper = document.createElement('div');
    wrapper.style.padding = '8px';

    sessions.forEach(function(s) {
      var item = document.createElement('div');
      item.className = 'list-item';

      var row = document.createElement('div');
      row.className = 'list-item-row';

      var name = document.createElement('span');
      name.className = 'item-name';
      name.textContent = s.name;

      var cat = document.createElement('span');
      cat.className = 'badge badge-progress';
      cat.textContent = s.category;

      row.appendChild(name);
      row.appendChild(cat);
      item.appendChild(row);

      var goal = document.createElement('div');
      goal.className = 'item-meta';
      goal.textContent = s.goal.length > 80 ? s.goal.slice(0, 80) + '\u2026' : s.goal;
      item.appendChild(goal);

      var attachDiv = document.createElement('div');
      attachDiv.className = 'item-meta';
      var code = document.createElement('code');
      code.className = 'attach-cmd';
      code.textContent = s.attach_cmd;
      code.title = 'Click to copy';
      var cmd = s.attach_cmd;
      code.addEventListener('click', function() {
        navigator.clipboard.writeText(cmd);
      });
      attachDiv.appendChild(code);
      item.appendChild(attachDiv);

      wrapper.appendChild(item);
    });

    panel.appendChild(wrapper);
  }

  return { init: init, render: render };
})();
