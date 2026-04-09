// orrchestrator - web node editor (vanilla JS, no external deps).
//
// State shape matches the Workforce struct serialized by orrch-workforce:
//   { name, description, agents: [{id, agent_profile, user_facing, nested_workforce?}],
//     connections: [{from, to, data_type}], operations: [string] }
//
// Node positions live client-side only (not persisted yet) - they're laid
// out on a grid when a workforce is loaded and can be dragged around.

"use strict";

const NODE_W = 180;
const NODE_H = 60;
const GRID_COLS = 3;
const GRID_MARGIN_X = 80;
const GRID_MARGIN_Y = 80;
const GRID_PITCH_X = NODE_W + 80;
const GRID_PITCH_Y = NODE_H + 80;

const state = {
  workforces: [],       // list of summaries
  current: null,        // currently loaded Workforce object
  positions: {},        // { nodeId: {x, y} }
  selected: null,       // selected node id
  edgeStart: null,      // id of node shift-clicked to begin an edge
  dragging: null,       // { id, offsetX, offsetY }
};

const canvas = document.getElementById("editor");
const ctx = canvas.getContext("2d");
const statusEl = document.getElementById("status");
const selectEl = document.getElementById("workforce-select");
const inspectorBody = document.getElementById("inspector-body");

function setStatus(msg, isError) {
  statusEl.textContent = msg;
  statusEl.style.color = isError ? "#e77" : "";
}

function clearChildren(el) {
  while (el.firstChild) el.removeChild(el.firstChild);
}

// ---- Network ----

async function fetchWorkforces() {
  try {
    const r = await fetch("/api/workforces");
    if (!r.ok) throw new Error("HTTP " + r.status);
    state.workforces = await r.json();
    clearChildren(selectEl);
    state.workforces.forEach((wf) => {
      const opt = document.createElement("option");
      opt.value = wf.name;
      opt.textContent = wf.name + "  (" + wf.agent_count + " agents)";
      selectEl.appendChild(opt);
    });
    setStatus(state.workforces.length + " workforce(s) loaded");
    if (state.workforces.length > 0) {
      await loadWorkforce(state.workforces[0].name);
    }
  } catch (e) {
    setStatus("failed to load list: " + e, true);
  }
}

async function loadWorkforce(name) {
  try {
    const r = await fetch("/api/workforce/" + encodeURIComponent(name));
    if (!r.ok) throw new Error("HTTP " + r.status);
    state.current = await r.json();
    state.positions = {};
    state.current.agents.forEach((a, i) => {
      const col = i % GRID_COLS;
      const row = Math.floor(i / GRID_COLS);
      state.positions[a.id] = {
        x: GRID_MARGIN_X + col * GRID_PITCH_X,
        y: GRID_MARGIN_Y + row * GRID_PITCH_Y,
      };
    });
    state.selected = null;
    state.edgeStart = null;
    setStatus("loaded " + name);
    renderInspector();
    draw();
  } catch (e) {
    setStatus("load failed: " + e, true);
  }
}

async function saveWorkforce() {
  if (!state.current) {
    setStatus("nothing to save", true);
    return;
  }
  try {
    const r = await fetch(
      "/api/workforce/" + encodeURIComponent(state.current.name),
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(state.current),
      }
    );
    if (!r.ok) {
      const body = await r.text();
      throw new Error("HTTP " + r.status + ": " + body);
    }
    const j = await r.json();
    setStatus("saved to " + (j.path || "disk"));
  } catch (e) {
    setStatus("save failed: " + e, true);
  }
}

// ---- Rendering ----

function draw() {
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  if (!state.current) {
    ctx.fillStyle = "#555";
    ctx.font = "14px monospace";
    ctx.fillText("(no workforce loaded)", 40, 40);
    return;
  }

  // Edges first so nodes draw over them.
  state.current.connections.forEach((c) => drawEdge(c));
  state.current.agents.forEach((a) => drawNode(a));
}

function drawNode(agent) {
  const pos = state.positions[agent.id];
  if (!pos) return;
  const selected = state.selected === agent.id;
  const edgeStarting = state.edgeStart === agent.id;

  ctx.fillStyle = agent.user_facing ? "#5a7f3f" : "#2a3f55";
  if (selected) ctx.fillStyle = "#3c5a7e";
  if (edgeStarting) ctx.fillStyle = "#7fb3d5";

  ctx.strokeStyle = "#a8d0e6";
  ctx.lineWidth = selected || edgeStarting ? 2 : 1;

  roundRect(ctx, pos.x, pos.y, NODE_W, NODE_H, 6);
  ctx.fill();
  ctx.stroke();

  ctx.fillStyle = "#e0e0e0";
  ctx.font = "bold 12px monospace";
  ctx.fillText(truncate(agent.id, 22), pos.x + 10, pos.y + 22);
  ctx.font = "11px monospace";
  ctx.fillStyle = "#bdd";
  ctx.fillText(truncate(agent.agent_profile, 22), pos.x + 10, pos.y + 42);
}

function drawEdge(conn) {
  const from = state.positions[conn.from];
  const to = state.positions[conn.to];
  if (!from || !to) return;
  const x1 = from.x + NODE_W / 2;
  const y1 = from.y + NODE_H / 2;
  const x2 = to.x + NODE_W / 2;
  const y2 = to.y + NODE_H / 2;

  ctx.strokeStyle = "#7fb3d5";
  ctx.lineWidth = 1.5;
  ctx.beginPath();
  ctx.moveTo(x1, y1);
  ctx.lineTo(x2, y2);
  ctx.stroke();

  // Arrowhead
  const angle = Math.atan2(y2 - y1, x2 - x1);
  const ax = x2 - Math.cos(angle) * (NODE_W / 2);
  const ay = y2 - Math.sin(angle) * (NODE_H / 2);
  const head = 8;
  ctx.beginPath();
  ctx.moveTo(ax, ay);
  ctx.lineTo(
    ax - head * Math.cos(angle - Math.PI / 6),
    ay - head * Math.sin(angle - Math.PI / 6)
  );
  ctx.lineTo(
    ax - head * Math.cos(angle + Math.PI / 6),
    ay - head * Math.sin(angle + Math.PI / 6)
  );
  ctx.closePath();
  ctx.fillStyle = "#7fb3d5";
  ctx.fill();

  // Label
  const mx = (x1 + x2) / 2;
  const my = (y1 + y2) / 2;
  ctx.fillStyle = "#888";
  ctx.font = "10px monospace";
  ctx.fillText(conn.data_type || "message", mx + 4, my - 4);
}

function roundRect(ctx, x, y, w, h, r) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}

function truncate(s, n) {
  if (!s) return "";
  return s.length > n ? s.slice(0, n - 1) + "\u2026" : s;
}

// ---- Hit testing ----

function nodeAt(x, y) {
  if (!state.current) return null;
  for (let i = state.current.agents.length - 1; i >= 0; i--) {
    const a = state.current.agents[i];
    const p = state.positions[a.id];
    if (!p) continue;
    if (x >= p.x && x <= p.x + NODE_W && y >= p.y && y <= p.y + NODE_H) {
      return a;
    }
  }
  return null;
}

// ---- Mouse events ----

canvas.addEventListener("mousedown", (ev) => {
  const rect = canvas.getBoundingClientRect();
  const x = ev.clientX - rect.left;
  const y = ev.clientY - rect.top;
  const hit = nodeAt(x, y);

  if (ev.button === 2) {
    // right click -> delete node
    if (hit) deleteNode(hit.id);
    return;
  }

  if (ev.shiftKey && hit) {
    // start or complete an edge
    if (state.edgeStart && state.edgeStart !== hit.id) {
      addConnection(state.edgeStart, hit.id);
      state.edgeStart = null;
    } else {
      state.edgeStart = hit.id;
    }
    draw();
    return;
  }

  if (hit) {
    state.selected = hit.id;
    state.dragging = {
      id: hit.id,
      offsetX: x - state.positions[hit.id].x,
      offsetY: y - state.positions[hit.id].y,
    };
    renderInspector();
    draw();
  } else {
    state.selected = null;
    state.edgeStart = null;
    renderInspector();
    draw();
  }
});

canvas.addEventListener("mousemove", (ev) => {
  if (!state.dragging) return;
  const rect = canvas.getBoundingClientRect();
  const x = ev.clientX - rect.left;
  const y = ev.clientY - rect.top;
  const p = state.positions[state.dragging.id];
  if (!p) return;
  p.x = x - state.dragging.offsetX;
  p.y = y - state.dragging.offsetY;
  draw();
});

canvas.addEventListener("mouseup", () => {
  state.dragging = null;
});

canvas.addEventListener("contextmenu", (ev) => ev.preventDefault());

// ---- Inspector ----

function renderInspector() {
  clearChildren(inspectorBody);
  if (!state.selected || !state.current) {
    const p = document.createElement("p");
    p.className = "muted";
    p.textContent = "Click a node to edit it.";
    inspectorBody.appendChild(p);
    return;
  }
  const agent = state.current.agents.find((a) => a.id === state.selected);
  if (!agent) return;

  inspectorBody.appendChild(
    fieldText("ID", agent.id, (v) => {
      // Update ID in connections too
      state.current.connections.forEach((c) => {
        if (c.from === agent.id) c.from = v;
        if (c.to === agent.id) c.to = v;
      });
      state.positions[v] = state.positions[agent.id];
      delete state.positions[agent.id];
      agent.id = v;
      state.selected = v;
      draw();
    })
  );
  inspectorBody.appendChild(
    fieldText("Profile", agent.agent_profile, (v) => {
      agent.agent_profile = v;
      draw();
    })
  );
  inspectorBody.appendChild(
    fieldCheckbox("User-facing", agent.user_facing, (v) => {
      agent.user_facing = v;
      draw();
    })
  );
  inspectorBody.appendChild(
    fieldText(
      "Nested workforce",
      agent.nested_workforce || "",
      (v) => {
        agent.nested_workforce = v || null;
      }
    )
  );
}

function fieldText(label, value, onChange) {
  const wrap = document.createElement("div");
  wrap.className = "field";
  const lab = document.createElement("label");
  lab.textContent = label;
  const input = document.createElement("input");
  input.type = "text";
  input.value = value || "";
  input.addEventListener("input", () => onChange(input.value));
  wrap.appendChild(lab);
  wrap.appendChild(input);
  return wrap;
}

function fieldCheckbox(label, value, onChange) {
  const wrap = document.createElement("div");
  wrap.className = "field";
  const lab = document.createElement("label");
  lab.textContent = label;
  const input = document.createElement("input");
  input.type = "checkbox";
  input.checked = !!value;
  input.addEventListener("change", () => onChange(input.checked));
  wrap.appendChild(lab);
  wrap.appendChild(input);
  return wrap;
}

// ---- Mutations ----

function addNode() {
  if (!state.current) {
    setStatus("load a workforce first", true);
    return;
  }
  // Find a fresh id
  let n = state.current.agents.length + 1;
  let id = "agent" + n;
  while (state.current.agents.some((a) => a.id === id)) {
    n++;
    id = "agent" + n;
  }
  state.current.agents.push({
    id,
    agent_profile: "Developer",
    user_facing: false,
    nested_workforce: null,
  });
  const i = state.current.agents.length - 1;
  state.positions[id] = {
    x: GRID_MARGIN_X + (i % GRID_COLS) * GRID_PITCH_X,
    y: GRID_MARGIN_Y + Math.floor(i / GRID_COLS) * GRID_PITCH_Y,
  };
  state.selected = id;
  renderInspector();
  draw();
}

function deleteNode(id) {
  if (!state.current) return;
  state.current.agents = state.current.agents.filter((a) => a.id !== id);
  state.current.connections = state.current.connections.filter(
    (c) => c.from !== id && c.to !== id
  );
  delete state.positions[id];
  if (state.selected === id) state.selected = null;
  if (state.edgeStart === id) state.edgeStart = null;
  renderInspector();
  draw();
}

function addConnection(from, to) {
  if (!state.current) return;
  if (state.current.connections.some((c) => c.from === from && c.to === to)) {
    setStatus("connection already exists", true);
    return;
  }
  state.current.connections.push({ from, to, data_type: "message" });
  setStatus("added connection " + from + " -> " + to);
  draw();
}

// ---- Wire up UI ----

document.getElementById("save-btn").addEventListener("click", saveWorkforce);
document.getElementById("reload-btn").addEventListener("click", () => {
  if (state.current) loadWorkforce(state.current.name);
});
document.getElementById("add-node-btn").addEventListener("click", addNode);
selectEl.addEventListener("change", () => loadWorkforce(selectEl.value));

fetchWorkforces();
