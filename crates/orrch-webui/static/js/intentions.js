var Intentions = (function() {
  var selected = 0;
  var ideas = [];

  function init() {
    WS.onState(render);
  }

  function render(state) {
    var panel = document.getElementById('panel-design');
    if (!panel.classList.contains('active')) return;
    ideas = state.ideas || [];
    if (selected >= ideas.length) selected = Math.max(0, ideas.length - 1);
    ensureLayout(panel);
    renderList();
    renderDetail();
  }

  function ensureLayout(panel) {
    if (panel.querySelector('.master-detail')) return;
    var md = document.createElement('div');
    md.className = 'master-detail';
    var master = document.createElement('div');
    master.className = 'master';
    master.id = 'ideas-list';
    var detail = document.createElement('div');
    detail.className = 'detail';
    detail.id = 'idea-detail';
    md.appendChild(master);
    md.appendChild(detail);
    panel.appendChild(md);
  }

  function renderList() {
    var el = document.getElementById('ideas-list');
    el.textContent = '';
    ideas.forEach(function(idea, i) {
      var item = document.createElement('div');
      item.className = 'list-item' + (i === selected ? ' selected' : '');
      item.addEventListener('click', function() { select(i); });

      var row = document.createElement('div');
      row.className = 'list-item-row';

      var name = document.createElement('span');
      name.className = 'item-name';
      name.textContent = idea.filename.replace(/\.md$/, '');

      var badge = document.createElement('span');
      var pct = idea.progress;
      if (idea.complete) {
        badge.className = 'badge badge-done';
        badge.textContent = 'done';
      } else {
        badge.className = 'badge badge-progress';
        badge.textContent = pct + '%';
      }

      row.appendChild(name);
      row.appendChild(badge);

      var bar = document.createElement('div');
      bar.className = 'progress-bar';
      var fill = document.createElement('div');
      fill.className = 'progress-fill';
      fill.style.width = pct + '%';
      var hue = idea.complete ? 150 : 60 + (pct > 50 ? (pct - 50) * 1.2 : 0);
      fill.style.background = 'hsl(' + hue + ',60%,45%)';
      bar.appendChild(fill);

      item.appendChild(row);
      item.appendChild(bar);
      el.appendChild(item);
    });
  }

  function renderDetail() {
    var el = document.getElementById('idea-detail');
    el.textContent = '';
    var idea = ideas[selected];
    if (!idea) {
      var p = document.createElement('p');
      p.className = 'item-meta';
      p.textContent = 'Select an intention';
      el.appendChild(p);
      return;
    }

    var title = document.createElement('div');
    title.className = 'detail-title';
    title.textContent = idea.filename.replace(/\.md$/, '');
    el.appendChild(title);

    var sub = document.createElement('div');
    sub.className = 'detail-sub';
    sub.textContent = 'Progress: ' + idea.progress + '%';
    el.appendChild(sub);

    if (!idea.submitted) {
      var submitBtn = document.createElement('button');
      submitBtn.className = 'action-btn';
      submitBtn.textContent = 'Submit to pipeline';
      submitBtn.addEventListener('click', function() { WS.key('s'); });
      el.appendChild(submitBtn);
    }

    var editBtn = document.createElement('button');
    editBtn.className = 'action-btn';
    editBtn.textContent = 'Edit in editor';
    editBtn.addEventListener('click', function() { WS.key('\r'); });
    el.appendChild(editBtn);

    if (idea.submitted && !idea.complete) {
      var retractBtn = document.createElement('button');
      retractBtn.className = 'action-btn action-btn-danger';
      retractBtn.textContent = 'Retract';
      var fname = idea.filename;
      retractBtn.addEventListener('click', function() {
        WS.action({ action: 'retract', filename: fname });
      });
      el.appendChild(retractBtn);
    }
  }

  function select(i) {
    selected = i;
    renderList();
    renderDetail();
  }

  return { init: init, render: render, select: select };
})();
