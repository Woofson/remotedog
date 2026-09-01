/**
 * 🐕 RemoteDog — High-Performance Remote Gateway Client
 * Part of the Woofson Suite (CommanderDog, NoteDog, DotDog)
 */

// ================= Global State =================
const state = {
  currentUser: null,
  jwtToken: localStorage.getItem('remotedog_token') || '',
  activePaneIndex: 1,
  paneLayout: 1,
  connections: [],
  stagedFiles: [],
  panes: {
    1: { id: 1, socket: null, conn: null, type: null, canvas: null, ctx: null, terminal: null },
    2: { id: 2, socket: null, conn: null, type: null, canvas: null, ctx: null, terminal: null },
    3: { id: 3, socket: null, conn: null, type: null, canvas: null, ctx: null, terminal: null },
    4: { id: 4, socket: null, conn: null, type: null, canvas: null, ctx: null, terminal: null },
  },
};

// ================= Initialization & Lifecycle =================
document.addEventListener('DOMContentLoaded', async () => {
  setupKeyboardShortcuts();
  setupGlobalDragAndDrop();
  await checkAuth();
});

// ================= Authentication =================
async function checkAuth() {
  if (!state.jwtToken) {
    showLoginModal();
    return;
  }

  try {
    const res = await fetch('/api/auth/me', {
      headers: { Authorization: `Bearer ${state.jwtToken}` },
    });

    if (res.ok) {
      state.currentUser = await res.json();
      onAuthenticated();
    } else {
      showLoginModal();
    }
  } catch (err) {
    console.error('Auth check error:', err);
    showLoginModal();
  }
}

function onAuthenticated() {
  const modal = document.getElementById('login-modal');
  if (modal) {
    modal.classList.remove('active');
    modal.style.display = 'none';
  }
  const name = state.currentUser.display_name || state.currentUser.username;
  const initial = (state.currentUser.username[0] || 'U').toUpperCase();
  const role = (state.currentUser.role || 'operator').toUpperCase();

  const navName = document.getElementById('nav-user-name');
  if (navName) navName.textContent = name;
  const navAvatar = document.getElementById('nav-user-avatar');
  if (navAvatar) navAvatar.textContent = initial;
  const navRole = document.getElementById('nav-user-role');
  if (navRole) navRole.textContent = role;

  const menuName = document.getElementById('menu-user-name');
  if (menuName) menuName.textContent = name;
  const menuAvatar = document.getElementById('menu-avatar-large');
  if (menuAvatar) menuAvatar.textContent = initial;
  const menuEmail = document.getElementById('menu-user-email');
  if (menuEmail) menuEmail.textContent = `${state.currentUser.username}@remotedog.local`;

  const adminItem = document.getElementById('admin-nav-item');
  if (adminItem) {
    adminItem.style.display = state.currentUser.role === 'admin' ? 'flex' : 'none';
  }

  loadConnections();
  showToast(`Welcome back, ${name}!`);
}

function showLoginModal() {
  const modal = document.getElementById('login-modal');
  modal.classList.add('active');
  modal.style.display = 'flex';
  fetchAuthProviders();
  setTimeout(() => {
    const userInput = document.getElementById('login-username');
    if (userInput) userInput.focus();
  }, 100);
}

async function fetchAuthProviders() {
  try {
    const res = await fetch('/api/auth/providers');
    if (res.ok) {
      const data = await res.json();
      if (data.oidc && data.oidc.enabled) {
        document.getElementById('oidc-login-section').style.display = 'block';
        document.getElementById('oidc-provider-btn-label').textContent = `Sign in with ${data.oidc.provider_name || 'Authentik'}`;
      } else {
        document.getElementById('oidc-login-section').style.display = 'none';
      }
    }
  } catch (e) {}
}

async function handleLoginSubmit() {
  const username = document.getElementById('login-username').value.trim();
  const password = document.getElementById('login-password').value;
  const errEl = document.getElementById('login-error');
  errEl.style.display = 'none';

  if (!username) {
    errEl.textContent = 'Please enter your username';
    errEl.style.display = 'block';
    return;
  }

  try {
    const res = await fetch('/api/auth/login', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ username, password }),
    });

    const data = await res.json();
    if (res.ok) {
      state.jwtToken = data.token;
      state.currentUser = data.user;
      localStorage.setItem('remotedog_token', data.token);
      document.cookie = `remotedog_token=${data.token}; path=/; max-age=86400; SameSite=Lax`;
      onAuthenticated();
    } else {
      errEl.textContent = data.error || 'Invalid username or password';
      errEl.style.display = 'block';
    }
  } catch (err) {
    errEl.textContent = 'Server connection error. Please try again.';
    errEl.style.display = 'block';
  }
}

const handleLogin = (e) => {
  if (e) e.preventDefault();
  handleLoginSubmit();
};

function loginWithOidc() {
  window.location.href = '/api/auth/oidc/login';
}

function logout() {
  state.jwtToken = '';
  state.currentUser = null;
  localStorage.removeItem('remotedog_token');
  document.cookie = 'remotedog_token=; path=/; max-age=0; SameSite=Lax';
  window.location.reload();
}

function toggleProfileMenu(e) {
  if (e) e.stopPropagation();
  const menu = document.getElementById('profile-dropdown-menu');
  if (menu) {
    const isVisible = menu.classList.contains('active') || menu.style.display === 'block';
    if (isVisible) {
      menu.classList.remove('active');
      menu.style.display = 'none';
    } else {
      menu.classList.add('active');
      menu.style.display = 'block';
    }
  }
}

document.addEventListener('click', (e) => {
  if (!e.target.closest('.profile-dropdown-wrapper')) {
    const menu = document.getElementById('profile-dropdown-menu');
    if (menu) {
      menu.classList.remove('active');
      menu.style.display = 'none';
    }
  }
});

// ================= Multi-Pane Layout Controls =================
function setPaneLayout(layout) {
  state.paneLayout = layout;
  const grid = document.getElementById('viewport-grid');
  if (grid) {
    grid.className = `viewport-grid layout-${layout}`;
  }

  // Update layout button styles
  ['1', '2v', '2h', '3', '4'].forEach(id => {
    const btn = document.getElementById(`layout-${id}`);
    if (btn) btn.classList.toggle('active', String(layout) === id || (layout === 2 && id === '2v'));
  });

  const maxPanes = (layout === 1) ? 1 : (layout === 2 || layout === '2v' || layout === '2h') ? 2 : (layout === 3) ? 3 : 4;

  // Show/Hide Panes
  for (let i = 1; i <= 4; i++) {
    const paneEl = document.getElementById(`pane-${i}`);
    if (paneEl) {
      paneEl.style.display = i <= maxPanes ? 'flex' : 'none';
    }
  }

  if (state.activePaneIndex > maxPanes) {
    activatePane(1);
  }
}

// ================= About & Profile Modals =================
function openAboutModal() {
  const menu = document.getElementById('profile-dropdown-menu');
  if (menu) { menu.classList.remove('active'); menu.style.display = 'none'; }
  const m = document.getElementById('about-modal');
  if (m) { m.classList.add('active'); m.style.display = 'flex'; }
}

function closeAboutModal() {
  const m = document.getElementById('about-modal');
  if (m) { m.classList.remove('active'); m.style.display = 'none'; }
}

function openUserProfileModal() {
  const menu = document.getElementById('profile-dropdown-menu');
  if (menu) { menu.classList.remove('active'); menu.style.display = 'none'; }
  if (state.currentUser) {
    const uEl = document.getElementById('profile-modal-username');
    if (uEl) uEl.textContent = state.currentUser.username;
    const rEl = document.getElementById('profile-modal-role');
    if (rEl) rEl.textContent = (state.currentUser.role || 'Operator').toUpperCase();
    const aEl = document.getElementById('profile-modal-avatar');
    if (aEl) aEl.textContent = (state.currentUser.username[0] || 'U').toUpperCase();
    const dEl = document.getElementById('profile-display-name');
    if (dEl) dEl.value = state.currentUser.display_name || '';
  }
  const m = document.getElementById('user-profile-modal');
  if (m) { m.classList.add('active'); m.style.display = 'flex'; }
}

function closeUserProfileModal() {
  const m = document.getElementById('user-profile-modal');
  if (m) { m.classList.remove('active'); m.style.display = 'none'; }
}

async function handleUpdateProfile(e) {
  e.preventDefault();
  const displayName = document.getElementById('profile-display-name').value.trim();
  const newPassword = document.getElementById('profile-new-password').value;
  const msgEl = document.getElementById('profile-modal-msg');
  
  try {
    const payload = { display_name: displayName };
    if (newPassword) payload.password = newPassword;
    const res = await apiFetch(`/api/users/${state.currentUser.id}`, {
      method: 'PUT',
      body: JSON.stringify(payload),
    });
    if (res.ok) {
      state.currentUser.display_name = displayName;
      const navName = document.getElementById('nav-user-name');
      if (navName) navName.textContent = displayName || state.currentUser.username;
      const menuName = document.getElementById('menu-user-name');
      if (menuName) menuName.textContent = displayName || state.currentUser.username;
      msgEl.textContent = 'Profile updated successfully!';
      msgEl.style.display = 'block';
      setTimeout(() => { closeUserProfileModal(); msgEl.style.display = 'none'; }, 1000);
    } else {
      const d = await res.json();
      alert(d.error || 'Failed to update profile');
    }
  } catch (err) {
    alert('Error updating profile');
  }
}

function activatePane(index) {
  state.activePaneIndex = index;
  for (let i = 1; i <= 4; i++) {
    const p = document.getElementById(`pane-${i}`);
    if (p) p.classList.toggle('active', i === index);
  }
  document.getElementById('active-pane-info').textContent = `Active Pane: [ ${index} ]`;

  const activePane = state.panes[index];
  if (activePane && activePane.conn && activePane.conn.protocol === 'ssh') {
    document.getElementById('sftp-explorer-section').style.display = 'block';
  } else {
    document.getElementById('sftp-explorer-section').style.display = 'none';
  }
}

// ================= Connection Management =================
async function loadConnections() {
  try {
    const res = await fetch('/api/connections', {
      headers: { Authorization: `Bearer ${state.jwtToken}` },
    });
    if (res.ok) {
      state.connections = await res.json();
      renderConnectionsTable();
    }
  } catch (err) {
    console.error('Failed to load connections:', err);
  }
}

function openConnectionsModal() {
  document.getElementById('connections-modal').style.display = 'flex';
  renderConnectionsTable();
}

function closeConnectionsModal() {
  document.getElementById('connections-modal').style.display = 'none';
}

function renderConnectionsTable() {
  const tbody = document.getElementById('connections-table-body');
  if (!tbody) return;

  if (state.connections.length === 0) {
    tbody.innerHTML = `<tr><td colspan="6" class="text-center text-muted p-3">No connections configured yet. Click "+ New Connection" to create one.</td></tr>`;
    return;
  }

  tbody.innerHTML = state.connections
    .map(c => `
      <tr>
        <td><strong>${escapeHtml(c.name)}</strong></td>
        <td><span class="pane-protocol-badge">${c.protocol.toUpperCase()}</span></td>
        <td><code>${c.protocol === 'local_pty' ? 'Local Host' : `${escapeHtml(c.host)}:${c.port}`}</code></td>
        <td>${escapeHtml(c.username || '—')}</td>
        <td>${c.tags ? `<span class="badge-count">${escapeHtml(c.tags)}</span>` : '—'}</td>
        <td>
          <div style="display:flex;gap:4px;">
            <button class="btn btn-primary btn-sm" onclick="connectToTarget('${c.id}')">Connect [ ${state.activePaneIndex} ]</button>
            ${c.user_permissions.can_edit ? `
              <button class="btn btn-secondary btn-sm" onclick="openEditConnectionModal('${c.id}')">Edit</button>
              <button class="btn btn-secondary btn-sm text-danger" onclick="deleteConnection('${c.id}')">Delete</button>
            ` : ''}
          </div>
        </td>
      </tr>
    `)
    .join('');
}

function openAddConnectionModal() {
  document.getElementById('connection-edit-title').textContent = 'New Connection';
  document.getElementById('conn-id').value = '';
  document.getElementById('conn-name').value = '';
  document.getElementById('conn-protocol').value = 'ssh';
  document.getElementById('conn-host').value = '';
  document.getElementById('conn-port').value = '22';
  document.getElementById('conn-username').value = '';
  document.getElementById('conn-password').value = '';
  document.getElementById('conn-private-key').value = '';
  document.getElementById('conn-tags').value = '';
  onProtocolChanged();
  document.getElementById('connection-edit-modal').style.display = 'flex';
}

function openEditConnectionModal(id) {
  const c = state.connections.find(item => item.id === id);
  if (!c) return;

  document.getElementById('connection-edit-title').textContent = `Edit Connection — ${c.name}`;
  document.getElementById('conn-id').value = c.id;
  document.getElementById('conn-name').value = c.name;
  document.getElementById('conn-protocol').value = c.protocol;
  document.getElementById('conn-host').value = c.host;
  document.getElementById('conn-port').value = c.port;
  document.getElementById('conn-username').value = c.username || '';
  document.getElementById('conn-password').value = '';
  document.getElementById('conn-private-key').value = '';
  document.getElementById('conn-tags').value = c.tags || '';
  onProtocolChanged();
  document.getElementById('connection-edit-modal').style.display = 'flex';
}

function closeConnectionEditModal() {
  document.getElementById('connection-edit-modal').style.display = 'none';
}

function onProtocolChanged() {
  const proto = document.getElementById('conn-protocol').value;
  const isLocal = proto === 'local_pty';
  const isSsh = proto === 'ssh';

  document.getElementById('group-host').style.display = isLocal ? 'none' : 'block';
  document.getElementById('group-port').style.display = isLocal ? 'none' : 'block';
  document.getElementById('group-auth-user').style.display = isLocal ? 'none' : 'flex';
  document.getElementById('group-ssh-key').style.display = isSsh ? 'block' : 'none';

  if (proto === 'vnc') document.getElementById('conn-port').value = '5900';
  if (proto === 'rdp') document.getElementById('conn-port').value = '3389';
  if (proto === 'ssh') document.getElementById('conn-port').value = '22';
}

async function handleSaveConnection(e) {
  e.preventDefault();
  const id = document.getElementById('conn-id').value;
  const payload = {
    name: document.getElementById('conn-name').value.trim(),
    protocol: document.getElementById('conn-protocol').value,
    host: document.getElementById('conn-host').value.trim() || 'localhost',
    port: parseInt(document.getElementById('conn-port').value, 10) || 22,
    username: document.getElementById('conn-username').value.trim() || null,
    password: document.getElementById('conn-password').value || null,
    private_key: document.getElementById('conn-private-key').value || null,
    tags: document.getElementById('conn-tags').value.trim() || null,
  };

  const url = id ? `/api/connections/${id}` : '/api/connections';
  const method = id ? 'PUT' : 'POST';

  try {
    const res = await fetch(url, {
      method,
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${state.jwtToken}`,
      },
      body: JSON.stringify(payload),
    });

    if (res.ok) {
      showToast(id ? 'Connection updated!' : 'Connection created!');
      closeConnectionEditModal();
      await loadConnections();
    } else {
      const err = await res.json();
      alert(`Error saving connection: ${err.error}`);
    }
  } catch (err) {
    alert(`Failed to save connection: ${err}`);
  }
}

async function deleteConnection(id) {
  if (!confirm('Are you sure you want to delete this connection?')) return;
  try {
    const res = await fetch(`/api/connections/${id}`, {
      method: 'DELETE',
      headers: { Authorization: `Bearer ${state.jwtToken}` },
    });
    if (res.ok) {
      showToast('Connection deleted');
      await loadConnections();
    }
  } catch (err) {
    alert(`Failed to delete connection: ${err}`);
  }
}

// ================= Remote Gateway WebSocket Tunnel & Protocol Handlers =================
function connectToTarget(connectionId) {
  const conn = state.connections.find(c => c.id === connectionId);
  if (!conn) return;

  closeConnectionsModal();
  const paneIndex = state.activePaneIndex;
  const pane = state.panes[paneIndex];

  disconnectPane(paneIndex);

  const titleEl = document.getElementById(`pane-${paneIndex}-title`);
  const protoBadge = document.getElementById(`pane-${paneIndex}-proto`);
  const bodyEl = document.getElementById(`pane-${paneIndex}-body`);
  const statsEl = document.getElementById(`pane-${paneIndex}-stats`);

  titleEl.textContent = conn.name;
  protoBadge.textContent = conn.protocol.toUpperCase();
  protoBadge.style.display = 'inline-block';
  statsEl.style.display = 'inline-block';
  statsEl.textContent = 'Connecting...';

  bodyEl.innerHTML = '';
  pane.conn = conn;
  pane.type = conn.protocol;

  const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
  const wsUrl = `${protocol}//${location.host}/ws/tunnel/${conn.id}?token=${encodeURIComponent(state.jwtToken)}&cols=120&rows=32`;

  const ws = new WebSocket(wsUrl);
  ws.binaryType = 'arraybuffer';
  pane.socket = ws;

  const startTime = Date.now();

  ws.onopen = () => {
    const latency = Date.now() - startTime;
    statsEl.textContent = `${latency} ms`;
    showToast(`Connected to ${conn.name} [Pane ${paneIndex}]`);

    if (conn.protocol === 'ssh') {
      loadSftpDirectory(paneIndex, '.');
    }
  };

  ws.onerror = (e) => {
    statsEl.textContent = 'Error';
    showToast(`Connection error on ${conn.name}`, 'danger');
  };

  ws.onclose = () => {
    statsEl.textContent = 'Closed';
    showToast(`Disconnected from ${conn.name}`);
  };

  // Branch by Protocol
  if (conn.protocol === 'vnc' || conn.protocol === 'rdp') {
    setupGraphicsProtocol(pane, ws, bodyEl);
  } else {
    setupTerminalProtocol(pane, ws, bodyEl);
  }
}

function setupTerminalProtocol(pane, ws, bodyEl) {
  const term = document.createElement('div');
  term.className = 'pane-terminal';
  term.tabIndex = 0;
  bodyEl.appendChild(term);
  pane.terminal = term;

  let buffer = '';

  ws.onmessage = (e) => {
    if (typeof e.data === 'string') {
      try {
        const msg = JSON.parse(e.data);
        if (msg.type === 'error') {
          term.textContent += `\r\n[RemoteDog Error]: ${msg.message}\r\n`;
          return;
        }
      } catch (err) {}
      appendTerminalText(term, e.data);
    } else {
      const decoder = new TextDecoder('utf-8');
      const text = decoder.decode(e.data);
      appendTerminalText(term, text);
    }
  };

  // Terminal Keyboard Capture
  term.addEventListener('keydown', (e) => {
    if (ws.readyState !== WebSocket.OPEN) return;

    if (e.key === 'Enter') {
      ws.send('\r');
      e.preventDefault();
    } else if (e.key === 'Backspace') {
      ws.send('\x7f');
      e.preventDefault();
    } else if (e.key === 'Tab') {
      ws.send('\t');
      e.preventDefault();
    } else if (e.key === 'ArrowUp') {
      ws.send('\x1b[A');
      e.preventDefault();
    } else if (e.key === 'ArrowDown') {
      ws.send('\x1b[B');
      e.preventDefault();
    } else if (e.key === 'ArrowRight') {
      ws.send('\x1b[C');
      e.preventDefault();
    } else if (e.key === 'ArrowLeft') {
      ws.send('\x1b[D');
      e.preventDefault();
    } else if (e.ctrlKey && e.key.toLowerCase() === 'c') {
      ws.send('\x03');
      e.preventDefault();
    } else if (e.ctrlKey && e.key.toLowerCase() === 'd') {
      ws.send('\x04');
      e.preventDefault();
    } else if (e.key.length === 1 && !e.ctrlKey && !e.altKey && !e.metaKey) {
      ws.send(e.key);
      e.preventDefault();
    }
  });

  // Global Clipboard paste into terminal
  term.addEventListener('paste', (e) => {
    e.preventDefault();
    const text = (e.clipboardData || window.clipboardData).getData('text');
    if (text && ws.readyState === WebSocket.OPEN) {
      ws.send(text);
    }
  });

  term.focus();
}

function appendTerminalText(termEl, text) {
  // Strip or basic ANSI escape code handler
  const clean = text
    .replace(/\x1b\[\?25[hl]/g, '')
    .replace(/\x1b\[[0-9;]*[mGKH]/g, '')
    .replace(/\x1b\[[0-9;]*[ABCD]/g, '');

  termEl.textContent += clean;
  termEl.scrollTop = termEl.scrollHeight;
}

function setupGraphicsProtocol(pane, ws, bodyEl) {
  const canvas = document.createElement('canvas');
  canvas.className = 'pane-canvas';
  canvas.width = 1280;
  canvas.height = 720;
  canvas.tabIndex = 0;
  bodyEl.appendChild(canvas);

  const ctx = canvas.getContext('2d');
  pane.canvas = canvas;
  pane.ctx = ctx;

  // Background black fill
  ctx.fillStyle = '#000000';
  ctx.fillRect(0, 0, canvas.width, canvas.height);

  let mouseMask = 0;

  ws.onmessage = (e) => {
    if (typeof e.data === 'string') {
      try {
        const msg = JSON.parse(e.data);
        if (msg.type === 'init') {
          canvas.width = msg.width;
          canvas.height = msg.height;
          ctx.fillStyle = '#000000';
          ctx.fillRect(0, 0, canvas.width, canvas.height);
        } else if (msg.type === 'clipboard_sync') {
          onRemoteClipboardSync(msg.text);
        } else if (msg.type === 'error') {
          showToast(`RFB Error: ${msg.message}`, 'danger');
        }
      } catch (err) {}
    } else {
      // Binary Tile Packet: [type: 0x01, x: u16, y: u16, w: u16, h: u16, rgba...]
      const dv = new DataView(e.data);
      const type = dv.getUint8(0);

      if (type === 0x01) {
        // Frame Tile
        const x = dv.getUint16(1, false);
        const y = dv.getUint16(3, false);
        const w = dv.getUint16(5, false);
        const h = dv.getUint16(7, false);

        const pixelBytes = new Uint8ClampedArray(e.data, 9, w * h * 4);
        const imgData = new ImageData(pixelBytes, w, h);
        ctx.putImageData(imgData, x, y);
      } else if (type === 0x02) {
        // CopyRect
        const x = dv.getUint16(1, false);
        const y = dv.getUint16(3, false);
        const w = dv.getUint16(5, false);
        const h = dv.getUint16(7, false);
        const srcX = dv.getUint16(9, false);
        const srcY = dv.getUint16(11, false);

        ctx.drawImage(canvas, srcX, srcY, w, h, x, y, w, h);
      }
    }
  };

  // Mouse Inputs
  function sendPointer(mask, x, y) {
    if (ws.readyState !== WebSocket.OPEN) return;
    const buf = new ArrayBuffer(6);
    const dv = new DataView(buf);
    dv.setUint8(0, 0x02); // POINTER_EVENT
    dv.setUint8(1, mask);
    dv.setUint16(2, Math.max(0, Math.min(canvas.width, x)), false);
    dv.setUint16(4, Math.max(0, Math.min(canvas.height, y)), false);
    ws.send(buf);
  }

  function getCanvasCoords(evt) {
    const rect = canvas.getBoundingClientRect();
    const scaleX = canvas.width / rect.width;
    const scaleY = canvas.height / rect.height;
    return {
      x: Math.round((evt.clientX - rect.left) * scaleX),
      y: Math.round((evt.clientY - rect.top) * scaleY),
    };
  }

  canvas.addEventListener('mousemove', (e) => {
    const pos = getCanvasCoords(e);
    sendPointer(mouseMask, pos.x, pos.y);
  });

  canvas.addEventListener('mousedown', (e) => {
    canvas.focus();
    if (e.button === 0) mouseMask |= 1; // Left
    if (e.button === 1) mouseMask |= 2; // Middle
    if (e.button === 2) mouseMask |= 4; // Right
    const pos = getCanvasCoords(e);
    sendPointer(mouseMask, pos.x, pos.y);
    e.preventDefault();
  });

  canvas.addEventListener('mouseup', (e) => {
    if (e.button === 0) mouseMask &= ~1;
    if (e.button === 1) mouseMask &= ~2;
    if (e.button === 2) mouseMask &= ~4;
    const pos = getCanvasCoords(e);
    sendPointer(mouseMask, pos.x, pos.y);
    e.preventDefault();
  });

  canvas.addEventListener('contextmenu', (e) => e.preventDefault());

  canvas.addEventListener('wheel', (e) => {
    const pos = getCanvasCoords(e);
    const wheelMask = e.deltaY < 0 ? (mouseMask | 8) : (mouseMask | 16);
    sendPointer(wheelMask, pos.x, pos.y);
    sendPointer(mouseMask, pos.x, pos.y); // release wheel
    e.preventDefault();
  }, { passive: false });

  // Key Inputs (RFB Keysyms)
  function sendKey(down, keysym) {
    if (ws.readyState !== WebSocket.OPEN) return;
    const buf = new ArrayBuffer(6);
    const dv = new DataView(buf);
    dv.setUint8(0, 0x04); // KEY_EVENT
    dv.setUint8(1, down ? 1 : 0);
    dv.setUint32(2, keysym, false);
    ws.send(buf);
  }

  function getKeySym(e) {
    switch (e.key) {
      case 'Backspace': return 0xff08;
      case 'Tab': return 0xff09;
      case 'Enter': return 0xff0d;
      case 'Escape': return 0xff1b;
      case 'Delete': return 0xffff;
      case 'Home': return 0xff50;
      case 'ArrowLeft': return 0xff51;
      case 'ArrowUp': return 0xff52;
      case 'ArrowRight': return 0xff53;
      case 'ArrowDown': return 0xff54;
      case 'PageUp': return 0xff55;
      case 'PageDown': return 0xff56;
      case 'End': return 0xff57;
      case 'Shift': return 0xffe1;
      case 'Control': return 0xffe3;
      case 'Meta': return 0xffeb;
      case 'Alt': return 0xffe9;
      default:
        if (e.key.length === 1) return e.key.charCodeAt(0);
        return 0;
    }
  }

  canvas.addEventListener('keydown', (e) => {
    const keysym = getKeySym(e);
    if (keysym !== 0) {
      sendKey(true, keysym);
      e.preventDefault();
    }
  });

  canvas.addEventListener('keyup', (e) => {
    const keysym = getKeySym(e);
    if (keysym !== 0) {
      sendKey(false, keysym);
      e.preventDefault();
    }
  });

  canvas.focus();
}

function disconnectPane(paneIndex) {
  const pane = state.panes[paneIndex];
  if (pane.socket) {
    pane.socket.close();
    pane.socket = null;
  }
  pane.conn = null;

  document.getElementById(`pane-${paneIndex}-title`).textContent = 'Disconnected';
  document.getElementById(`pane-${paneIndex}-proto`).style.display = 'none';
  document.getElementById(`pane-${paneIndex}-stats`).style.display = 'none';
  document.getElementById(`pane-${paneIndex}-body`).innerHTML = `
    <div class="pane-empty-state">
      <div class="empty-icon"><img src="assets/Remotedogiconsmall.png" alt="RemoteDog" class="empty-brand-img" /></div>
      <h3>Disconnected</h3>
      <button class="btn btn-primary" onclick="openConnectionsModal()">Connect Pane ${paneIndex}</button>
    </div>
  `;
}

// ================= Bi-Directional Clipboard Engine =================
function toggleClipboardDrawer() {
  const drawer = document.getElementById('clipboard-drawer');
  drawer.style.display = drawer.style.display === 'none' ? 'flex' : 'none';
}

function onRemoteClipboardSync(text) {
  document.getElementById('clipboard-buffer').value = text;
  const autoClip = document.getElementById('chk-auto-clip').checked;

  if (autoClip && navigator.clipboard && navigator.clipboard.writeText) {
    navigator.clipboard.writeText(text).then(() => {
      showToast('📋 Remote clipboard copied to your device!');
    }).catch(() => {});
  } else {
    showToast('📋 Remote clipboard updated in drawer');
  }
}

function pushClipboardToRemote() {
  const text = document.getElementById('clipboard-buffer').value;
  const pane = state.panes[state.activePaneIndex];

  if (!pane || !pane.socket || pane.socket.readyState !== WebSocket.OPEN) {
    showToast('No active remote connection on this pane', 'danger');
    return;
  }

  pane.socket.send(JSON.stringify({
    type: 'clipboard_push',
    text: text,
  }));

  showToast('Clipboard sent to remote host!');
}

async function copyLocalClipboardToBuffer() {
  try {
    const text = await navigator.clipboard.readText();
    document.getElementById('clipboard-buffer').value = text;
    showToast('Pasted from local device clipboard!');
  } catch (err) {
    showToast('Could not read clipboard. Please paste manually.', 'danger');
  }
}

async function copyBufferToLocalDevice() {
  const text = document.getElementById('clipboard-buffer').value;
  try {
    await navigator.clipboard.writeText(text);
    showToast('Copied buffer to your device clipboard!');
  } catch (err) {
    showToast('Failed to copy to clipboard', 'danger');
  }
}

// ================= File Dropbox & SFTP Transfers =================
function toggleTransferDrawer() {
  const drawer = document.getElementById('transfer-drawer');
  drawer.style.display = drawer.style.display === 'none' ? 'flex' : 'none';
}

function openPaneDropbox(paneIndex) {
  activatePane(paneIndex);
  toggleTransferDrawer();
}

function triggerFileInput() {
  document.getElementById('file-input').click();
}

async function handleFileInputChange(e) {
  const files = e.target.files;
  if (!files || files.length === 0) return;
  await uploadFilesToStaging(files);
}

function setupGlobalDragAndDrop() {
  for (let i = 1; i <= 4; i++) {
    const paneEl = document.getElementById(`pane-${i}`);
    const dropzone = document.getElementById(`pane-${i}-dropzone`);
    if (!paneEl || !dropzone) continue;

    paneEl.addEventListener('dragover', (e) => {
      e.preventDefault();
      dropzone.classList.add('active');
    });

    paneEl.addEventListener('dragleave', (e) => {
      e.preventDefault();
      if (!paneEl.contains(e.relatedTarget)) {
        dropzone.classList.remove('active');
      }
    });

    paneEl.addEventListener('drop', async (e) => {
      e.preventDefault();
      dropzone.classList.remove('active');
      const files = e.dataTransfer.files;
      if (files.length > 0) {
        activatePane(i);
        await uploadFilesToStaging(files);
      }
    });
  }
}

async function uploadFilesToStaging(files) {
  const formData = new FormData();
  for (let i = 0; i < files.length; i++) {
    formData.append(`file_${i}`, files[i]);
  }

  showToast(`Uploading ${files.length} file(s) to staging...`);

  try {
    const res = await fetch('/api/transfers/upload', {
      method: 'POST',
      headers: { Authorization: `Bearer ${state.jwtToken}` },
      body: formData,
    });

    if (res.ok) {
      const data = await res.json();
      state.stagedFiles.push(...data.files);
      renderStagedFiles();
      showToast('Files staged in Dropbox!');
      toggleTransferDrawer();

      // Check if current pane is SSH, prompt to push to remote
      const pane = state.panes[state.activePaneIndex];
      if (pane && pane.conn && pane.conn.protocol === 'ssh') {
        if (confirm(`Push uploaded file "${data.files[0].original_name}" directly to remote SSH host?`)) {
          await pushStagedFileToSftp(pane.conn.id, data.files[0].id, `./${data.files[0].original_name}`);
        }
      }
    }
  } catch (err) {
    showToast('Failed to upload file to staging', 'danger');
  }
}

function renderStagedFiles() {
  const listEl = document.getElementById('staged-files-list');
  const countBadge = document.getElementById('badge-transfer-count');

  if (state.stagedFiles.length === 0) {
    listEl.innerHTML = `<div class="text-muted text-sm p-3 text-center">No files staged in current session.</div>`;
    countBadge.style.display = 'none';
    return;
  }

  countBadge.style.display = 'inline-block';
  countBadge.textContent = state.stagedFiles.length;

  listEl.innerHTML = state.stagedFiles
    .map(f => `
      <div class="transfer-item">
        <div class="transfer-info">
          <span class="transfer-name">${escapeHtml(f.original_name)}</span>
          <span class="transfer-meta">${formatBytes(f.file_size)} • ${new Date(f.uploaded_at).toLocaleTimeString()}</span>
        </div>
        <div style="display:flex;gap:4px;">
          <a href="/api/transfers/download/${f.id}" class="btn btn-secondary btn-sm" download>Download</a>
        </div>
      </div>
    `)
    .join('');
}

async function loadSftpDirectory(paneIndex, path) {
  const pane = state.panes[paneIndex];
  if (!pane || !pane.conn || pane.conn.protocol !== 'ssh') return;

  try {
    const res = await fetch(`/api/transfers/sftp/${pane.conn.id}/list`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${state.jwtToken}`,
      },
      body: JSON.stringify({ path: path }),
    });

    if (res.ok) {
      const data = await res.json();
      document.getElementById('sftp-current-path').textContent = path;
      renderSftpFiles(paneIndex, data.entries, path);
    }
  } catch (err) {
    console.error('SFTP load error:', err);
  }
}

function renderSftpFiles(paneIndex, entries, currentPath) {
  const listEl = document.getElementById('sftp-files-list');
  if (!entries || entries.length === 0) {
    listEl.innerHTML = `<div class="text-muted text-sm p-2 text-center">Empty directory</div>`;
    return;
  }

  let html = '';
  if (currentPath !== '.' && currentPath !== '/') {
    const parentPath = currentPath.substring(0, currentPath.lastIndexOf('/')) || '/';
    html += `
      <div class="sftp-item" onclick="loadSftpDirectory(${paneIndex}, '${parentPath}')">
        <div class="sftp-item-left">
          <span>📁</span>
          <strong>.. (Up one level)</strong>
        </div>
      </div>
    `;
  }

  html += entries.map(item => `
    <div class="sftp-item" onclick="${item.is_dir ? `loadSftpDirectory(${paneIndex}, '${item.path}')` : ''}">
      <div class="sftp-item-left">
        <span>${item.is_dir ? '📁' : '📄'}</span>
        <span>${escapeHtml(item.name)}</span>
      </div>
      <div class="text-muted text-sm">
        ${item.is_dir ? 'DIR' : formatBytes(item.size)}
      </div>
    </div>
  `).join('');

  listEl.innerHTML = html;
}

async function pushStagedFileToSftp(connectionId, stagedId, remotePath) {
  showToast('Transferring file to remote server via SFTP...');
  try {
    const res = await fetch(`/api/transfers/sftp/${connectionId}/upload`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${state.jwtToken}`,
      },
      body: JSON.stringify({ staged_id: stagedId, remote_path: remotePath }),
    });

    if (res.ok) {
      showToast('File transfer to remote SSH host complete!', 'success');
      loadSftpDirectory(state.activePaneIndex, '.');
    }
  } catch (err) {
    showToast('Failed to push file via SFTP', 'danger');
  }
}

// ================= Users & RBAC Modal =================
async function openUsersModal() {
  document.getElementById('users-modal').style.display = 'flex';
  try {
    const res = await fetch('/api/users', {
      headers: { Authorization: `Bearer ${state.jwtToken}` },
    });
    if (res.ok) {
      const users = await res.json();
      renderUsersTable(users);
    }
  } catch (err) {
    console.error('Failed to load users:', err);
  }
}

function closeUsersModal() {
  document.getElementById('users-modal').style.display = 'none';
}

function renderUsersTable(users) {
  const tbody = document.getElementById('users-table-body');
  tbody.innerHTML = users.map(u => `
    <tr>
      <td><strong>${escapeHtml(u.username)}</strong> ${u.display_name ? `(${escapeHtml(u.display_name)})` : ''}</td>
      <td><span class="user-role-badge">${u.role.toUpperCase()}</span></td>
      <td><span class="pane-protocol-badge">${u.auth_provider.toUpperCase()}</span></td>
      <td>${u.is_active ? '<span class="text-success">Active</span>' : '<span class="text-danger">Disabled</span>'}</td>
      <td class="text-muted">${new Date(u.created_at).toLocaleDateString()}</td>
      <td>
        <button class="btn btn-secondary btn-sm text-danger" onclick="deleteUser('${u.id}')">Delete</button>
      </td>
    </tr>
  `).join('');
}

function openAddUserModal() {
  document.getElementById('user-edit-form').reset();
  document.getElementById('user-edit-modal').style.display = 'flex';
}

function closeUserEditModal() {
  document.getElementById('user-edit-modal').style.display = 'none';
}

async function handleSaveUser(e) {
  e.preventDefault();
  const username = document.getElementById('edit-user-username').value.trim();
  const password = document.getElementById('edit-user-password').value;
  const role = document.getElementById('edit-user-role').value;

  try {
    const res = await fetch('/api/users', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${state.jwtToken}`,
      },
      body: JSON.stringify({ username, password, role }),
    });

    if (res.ok) {
      showToast('User created successfully!');
      closeUserEditModal();
      openUsersModal();
    } else {
      const data = await res.json();
      alert(`Error creating user: ${data.error}`);
    }
  } catch (err) {
    alert(`Failed: ${err}`);
  }
}

async function deleteUser(id) {
  if (!confirm('Are you sure you want to delete this user?')) return;
  try {
    const res = await fetch(`/api/users/${id}`, {
      method: 'DELETE',
      headers: { Authorization: `Bearer ${state.jwtToken}` },
    });
    if (res.ok) {
      showToast('User deleted');
      openUsersModal();
    }
  } catch (err) {
    alert(`Failed: ${err}`);
  }
}

// ================= Settings & Audit Modals =================
async function openSettingsModal() {
  document.getElementById('settings-modal').style.display = 'flex';
  try {
    const res = await fetch('/api/settings');
    if (res.ok) {
      const data = await res.json();
      renderSettings(data);
    }
  } catch (e) {}
}

function closeSettingsModal() {
  document.getElementById('settings-modal').style.display = 'none';
}

function renderSettings(s) {
  const oidcEl = document.getElementById('settings-oidc-details');
  oidcEl.innerHTML = `
    <div class="form-group"><label>OIDC SSO Status</label><input class="form-input" disabled value="${s.oidc.enabled ? 'Enabled' : 'Disabled (Configured in config.toml)'}" /></div>
    <div class="form-group"><label>Identity Provider</label><input class="form-input" disabled value="${s.oidc.provider_name}" /></div>
    <div class="form-group"><label>Issuer URL</label><input class="form-input" disabled value="${s.oidc.issuer_url}" /></div>
    <div class="form-group"><label>Redirect URI</label><input class="form-input" disabled value="${s.oidc.redirect_uri}" /></div>
  `;

  const genEl = document.getElementById('settings-general-details');
  genEl.innerHTML = `
    <div class="form-group"><label>Default Clipboard Sync</label><input class="form-input" disabled value="${s.clipboard.default_mode}" /></div>
    <div class="form-group"><label>Staging Directory</label><input class="form-input" disabled value="${s.storage.staging_dir}" /></div>
  `;
}

async function openAuditModal() {
  document.getElementById('audit-modal').style.display = 'flex';
  try {
    const res = await fetch('/api/audit-logs', {
      headers: { Authorization: `Bearer ${state.jwtToken}` },
    });
    if (res.ok) {
      const logs = await res.json();
      const tbody = document.getElementById('audit-table-body');
      tbody.innerHTML = logs.map(l => `
        <tr>
          <td class="text-muted">${new Date(l.timestamp).toLocaleString()}</td>
          <td><strong>${escapeHtml(l.username)}</strong></td>
          <td><span class="pane-protocol-badge">${l.action}</span></td>
          <td>${l.connection_name ? escapeHtml(l.connection_name) : '—'}</td>
          <td><span class="text-muted">${escapeHtml(l.details || '')}</span></td>
        </tr>
      `).join('');
    }
  } catch (e) {}
}

function closeAuditModal() {
  document.getElementById('audit-modal').style.display = 'none';
}

// ================= Keyboard Shortcuts =================
function setupKeyboardShortcuts() {
  document.addEventListener('keydown', (e) => {
    // Alt+1 to Alt+4 -> Multi-Pane Switching
    if (e.altKey && e.key >= '1' && e.key <= '4') {
      setPaneLayout(parseInt(e.key, 10));
      e.preventDefault();
    }
    // Ctrl+Shift+V -> Clipboard Drawer
    if (e.ctrlKey && e.shiftKey && e.key.toLowerCase() === 'v') {
      toggleClipboardDrawer();
      e.preventDefault();
    }
  });
}

// ================= Toast Notifications & Helpers =================
function showToast(msg, type = 'info') {
  const container = document.getElementById('toast-container');
  const toast = document.createElement('div');
  toast.className = 'toast';
  toast.innerHTML = `<span>${escapeHtml(msg)}</span>`;
  container.appendChild(toast);

  setTimeout(() => {
    toast.style.opacity = '0';
    setTimeout(() => toast.remove(), 200);
  }, 3500);
}

function formatBytes(bytes, decimals = 2) {
  if (!+bytes) return '0 Bytes';
  const k = 1024;
  const dm = decimals < 0 ? 0 : decimals;
  const sizes = ['Bytes', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(dm))} ${sizes[i]}`;
}

function escapeHtml(str) {
  if (!str) return '';
  return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}
