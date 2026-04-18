var WS = (function() {
  var socket = null;
  var lastState = {};
  var listeners = [];

  function connect() {
    var proto = location.protocol === 'https:' ? 'wss' : 'ws';
    socket = new WebSocket(proto + '://' + location.host + '/state');
    socket.onmessage = function(e) {
      lastState = JSON.parse(e.data);
      for (var i = 0; i < listeners.length; i++) listeners[i](lastState);
    };
    socket.onclose = function() { setTimeout(connect, 2000); };
    socket.onerror = function() { socket.close(); };
  }

  function onState(fn) {
    listeners.push(fn);
    if (Object.keys(lastState).length) fn(lastState);
  }

  function action(payload) {
    fetch('/action', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload)
    });
  }

  function key(k) { action({ action: 'key', key: k }); }

  function getLastState() { return lastState; }

  return { connect: connect, onState: onState, action: action, key: key, getLastState: getLastState };
})();
