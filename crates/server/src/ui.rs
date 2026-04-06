//! Embedded web UI dashboard.
//!
//! Serves a single-page HTML dashboard at `/` that provides:
//! - Login screen when auth is enabled (API key stored in sessionStorage)
//! - Real-time pipeline visualization via WebSocket
//! - Session history browser
//! - Tool and skill registry viewer
//!
//! The entire UI is embedded as a static string to avoid external
//! file dependencies, following OpenClaw's Control UI pattern.

use crate::state::AppState;
use axum::{
    http::{header, StatusCode},
    response::{Html, IntoResponse},
    routing::get,
    Router,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(dashboard))
        .route("/ui/app.js", get(app_js))
}

async fn dashboard() -> Html<&'static str> {
    Html(DASHBOARD_HTML)
}

async fn app_js() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/javascript")],
        APP_JS,
    )
}

const DASHBOARD_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>AgentDept Gateway</title>
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
           background: #0f172a; color: #e2e8f0; }
    .header { background: #1e293b; padding: 1rem 2rem; border-bottom: 1px solid #334155;
              display: flex; align-items: center; gap: 1rem; }
    .header h1 { font-size: 1.25rem; color: #38bdf8; }
    .header .status { margin-left: auto; display: flex; align-items: center; gap: 0.5rem; }
    .header .auth-info { display: flex; align-items: center; gap: 0.5rem; font-size: 0.8rem; color: #94a3b8; }
    .header .auth-info .role-badge { background: #1d4ed8; color: #bfdbfe; padding: 0.125rem 0.5rem;
                                      border-radius: 9999px; font-size: 0.7rem; font-weight: 600; }
    .header .auth-info button { background: none; border: 1px solid #475569; color: #94a3b8;
                                 padding: 0.25rem 0.5rem; border-radius: 4px; cursor: pointer; font-size: 0.75rem; }
    .header .auth-info button:hover { border-color: #ef4444; color: #ef4444; }
    .dot { width: 8px; height: 8px; border-radius: 50%; }
    .dot.connected { background: #4ade80; }
    .dot.disconnected { background: #ef4444; }
    .container { display: grid; grid-template-columns: 1fr 1fr; gap: 1rem; padding: 1rem 2rem; max-width: 1400px; margin: 0 auto; }
    .card { background: #1e293b; border-radius: 8px; border: 1px solid #334155; overflow: hidden; }
    .card-header { padding: 0.75rem 1rem; background: #334155; font-weight: 600; font-size: 0.875rem;
                   display: flex; align-items: center; justify-content: space-between; }
    .card-body { padding: 1rem; max-height: 400px; overflow-y: auto; }
    .full-width { grid-column: 1 / -1; }
    .event { padding: 0.5rem; margin-bottom: 0.5rem; background: #0f172a; border-radius: 4px;
             font-family: 'SF Mono', Monaco, monospace; font-size: 0.8rem; }
    .event .time { color: #64748b; margin-right: 0.5rem; }
    .event .type { color: #38bdf8; margin-right: 0.5rem; }
    .session-row { padding: 0.5rem; border-bottom: 1px solid #334155; cursor: pointer; }
    .session-row:hover { background: #334155; }
    .session-row .id { color: #38bdf8; font-family: monospace; font-size: 0.8rem; }
    .session-row .meta { color: #94a3b8; font-size: 0.75rem; margin-top: 0.25rem; }
    .badge { display: inline-block; padding: 0.125rem 0.5rem; border-radius: 9999px;
             font-size: 0.7rem; font-weight: 600; }
    .badge.running { background: #1d4ed8; color: #bfdbfe; }
    .badge.completed { background: #166534; color: #bbf7d0; }
    .badge.failed { background: #991b1b; color: #fecaca; }
    .badge.interrupted { background: #854d0e; color: #fef3c7; }
    .tool-item, .skill-item { padding: 0.5rem; border-bottom: 1px solid #334155; }
    .tool-item .name, .skill-item .name { color: #a78bfa; font-weight: 600; }
    .tool-item .desc, .skill-item .desc { color: #94a3b8; font-size: 0.8rem; }
    .submit-form { display: flex; gap: 0.5rem; padding: 1rem; }
    .submit-form input { flex: 1; padding: 0.5rem 0.75rem; background: #0f172a; border: 1px solid #475569;
                          border-radius: 4px; color: #e2e8f0; font-size: 0.875rem; }
    .submit-form button { padding: 0.5rem 1.25rem; background: #2563eb; color: white; border: none;
                           border-radius: 4px; cursor: pointer; font-weight: 600; }
    .submit-form button:hover { background: #1d4ed8; }
    .empty { color: #64748b; text-align: center; padding: 2rem; }
    .count { background: #475569; color: #e2e8f0; padding: 0.125rem 0.5rem; border-radius: 9999px;
             font-size: 0.7rem; }

    /* Session detail overlay */
    .detail-overlay { position: fixed; inset: 0; background: rgba(15,23,42,0.85); z-index: 900;
                      display: flex; align-items: center; justify-content: center; }
    .detail-panel { background: #1e293b; border: 1px solid #334155; border-radius: 12px;
                    width: 95%; max-width: 900px; max-height: 90vh; display: flex; flex-direction: column; }
    .detail-header { display: flex; align-items: center; justify-content: space-between;
                     padding: 1rem 1.5rem; border-bottom: 1px solid #334155; }
    .detail-header h2 { font-size: 1rem; color: #38bdf8; }
    .detail-header .close-btn { background: none; border: 1px solid #475569; color: #94a3b8;
                                 padding: 0.25rem 0.75rem; border-radius: 4px; cursor: pointer; font-size: 0.85rem; }
    .detail-header .close-btn:hover { border-color: #ef4444; color: #ef4444; }
    .detail-meta { padding: 0.75rem 1.5rem; background: #0f172a; font-size: 0.8rem; color: #94a3b8;
                   display: flex; gap: 1.5rem; flex-wrap: wrap; }
    .detail-meta span { display: flex; align-items: center; gap: 0.35rem; }
    .detail-body { padding: 1rem 1.5rem; overflow-y: auto; flex: 1; }

    /* Agent role badges */
    .role-pill { display: inline-block; padding: 0.15rem 0.5rem; border-radius: 9999px;
                 font-size: 0.7rem; font-weight: 700; text-transform: uppercase; letter-spacing: 0.03em; }
    .role-pm { background: #7c3aed; color: #ede9fe; }
    .role-ba { background: #0891b2; color: #cffafe; }
    .role-dev { background: #059669; color: #d1fae5; }
    .role-frontend { background: #d97706; color: #fef3c7; }
    .role-test { background: #dc2626; color: #fee2e2; }

    /* Message card in detail view */
    .msg-card { background: #0f172a; border: 1px solid #334155; border-radius: 6px;
                padding: 0.75rem 1rem; margin-bottom: 0.75rem; }
    .msg-card-header { display: flex; align-items: center; gap: 0.5rem; margin-bottom: 0.5rem; font-size: 0.8rem; }
    .msg-card-header .arrow { color: #64748b; }
    .msg-card-header .kind-badge { background: #334155; color: #cbd5e1; padding: 0.1rem 0.4rem;
                                    border-radius: 4px; font-size: 0.7rem; font-family: monospace; }
    .msg-card-header .priority-badge { font-size: 0.65rem; padding: 0.1rem 0.35rem; border-radius: 4px; }
    .msg-card-header .priority-badge.high { background: #991b1b; color: #fecaca; }
    .msg-card-header .priority-badge.low { background: #1e3a5f; color: #93c5fd; }
    .msg-card-body { font-size: 0.8rem; color: #cbd5e1; font-family: 'SF Mono', Monaco, monospace;
                     white-space: pre-wrap; word-break: break-word; max-height: 200px; overflow-y: auto;
                     background: #1e293b; border-radius: 4px; padding: 0.5rem; }
    .msg-card-body.collapsed { max-height: 80px; cursor: pointer; position: relative; }
    .msg-card-body.collapsed::after { content: 'click to expand'; position: absolute; bottom: 0; left: 0; right: 0;
                                       text-align: center; padding: 0.5rem 0 0.25rem; font-family: sans-serif;
                                       background: linear-gradient(transparent, #1e293b); color: #64748b; font-size: 0.7rem; }

    /* Agent summary cards */
    .agent-summary { display: flex; gap: 0.5rem; flex-wrap: wrap; margin-bottom: 1rem; }
    .agent-chip { display: flex; align-items: center; gap: 0.35rem; padding: 0.35rem 0.65rem;
                  background: #0f172a; border: 1px solid #334155; border-radius: 6px; font-size: 0.75rem; }
    .agent-chip .chip-count { color: #64748b; }

    /* Filter tabs */
    .filter-tabs { display: flex; gap: 0.25rem; margin-bottom: 0.75rem; flex-wrap: wrap; }
    .filter-tab { padding: 0.3rem 0.6rem; border-radius: 4px; border: 1px solid #334155;
                  background: transparent; color: #94a3b8; cursor: pointer; font-size: 0.75rem; }
    .filter-tab:hover { border-color: #475569; color: #e2e8f0; }
    .filter-tab.active { background: #334155; color: #e2e8f0; border-color: #475569; }

    /* Action buttons */
    .action-btn { padding: 0.3rem 0.65rem; border-radius: 4px; border: 1px solid #475569;
                  background: transparent; cursor: pointer; font-size: 0.75rem; font-weight: 600; }
    .action-btn:hover { opacity: 0.85; }
    .action-btn.stop { color: #fbbf24; border-color: #854d0e; }
    .action-btn.stop:hover { background: #854d0e; color: #fef3c7; }
    .action-btn.delete { color: #f87171; border-color: #991b1b; }
    .action-btn.delete:hover { background: #991b1b; color: #fee2e2; }
    .detail-actions { display: flex; gap: 0.5rem; }
    .session-row { position: relative; }
    .session-actions { display: none; position: absolute; right: 0.5rem; top: 50%; transform: translateY(-50%);
                       gap: 0.35rem; }
    .session-row:hover .session-actions { display: flex; }
    .confirm-overlay { position: fixed; inset: 0; background: rgba(15,23,42,0.85); z-index: 950;
                       display: flex; align-items: center; justify-content: center; }
    .confirm-box { background: #1e293b; border: 1px solid #475569; border-radius: 8px; padding: 1.5rem;
                   max-width: 400px; width: 90%; text-align: center; }
    .confirm-box p { margin-bottom: 1rem; color: #cbd5e1; font-size: 0.9rem; }
    .confirm-box .confirm-actions { display: flex; gap: 0.5rem; justify-content: center; }
    .confirm-box .confirm-actions button { padding: 0.5rem 1.25rem; border-radius: 4px; border: none;
                                            cursor: pointer; font-weight: 600; font-size: 0.85rem; }
    .confirm-box .confirm-cancel { background: #334155; color: #e2e8f0; }
    .confirm-box .confirm-cancel:hover { background: #475569; }
    .confirm-box .confirm-danger { background: #dc2626; color: white; }
    .confirm-box .confirm-danger:hover { background: #b91c1c; }

    /* Login overlay */
    .login-overlay { position: fixed; inset: 0; background: #0f172a; display: flex;
                     align-items: center; justify-content: center; z-index: 1000; }
    .login-box { background: #1e293b; border: 1px solid #334155; border-radius: 12px;
                 padding: 2.5rem; width: 100%; max-width: 420px; }
    .login-box h2 { color: #38bdf8; margin-bottom: 0.5rem; }
    .login-box p { color: #94a3b8; font-size: 0.875rem; margin-bottom: 1.5rem; }
    .login-box input { width: 100%; padding: 0.75rem; background: #0f172a; border: 1px solid #475569;
                        border-radius: 6px; color: #e2e8f0; font-size: 0.875rem; margin-bottom: 1rem;
                        font-family: 'SF Mono', Monaco, monospace; }
    .login-box input:focus { outline: none; border-color: #38bdf8; }
    .login-box button { width: 100%; padding: 0.75rem; background: #2563eb; color: white; border: none;
                         border-radius: 6px; cursor: pointer; font-weight: 600; font-size: 0.9rem; }
    .login-box button:hover { background: #1d4ed8; }
    .login-box .error { color: #ef4444; font-size: 0.8rem; margin-bottom: 1rem; display: none; }
    .login-box .skip { text-align: center; margin-top: 1rem; }
    .login-box .skip a { color: #64748b; font-size: 0.8rem; cursor: pointer; text-decoration: underline; }
    .login-box .skip a:hover { color: #94a3b8; }
  </style>
</head>
<body>
  <!-- Login overlay (hidden if auth not needed) -->
  <div class="login-overlay" id="login-overlay" style="display:none">
    <div class="login-box">
      <h2>AgentDept Gateway</h2>
      <p>Enter your API key to access the dashboard. Keys are created with <code>POST /api/keys</code> using an admin key.</p>
      <div class="error" id="login-error"></div>
      <input type="password" id="login-key" placeholder="agd_..." autocomplete="off" />
      <button onclick="doLogin()">Sign In</button>
      <div class="skip"><a onclick="skipLogin()">Continue without authentication</a></div>
    </div>
  </div>

  <!-- Main app -->
  <div id="app" style="display:none">
    <div class="header">
      <h1>AgentDept Gateway</h1>
      <div class="auth-info" id="auth-info" style="display:none">
        <span id="auth-label"></span>
        <span class="role-badge" id="auth-role"></span>
        <button onclick="doLogout()">Logout</button>
      </div>
      <div class="status">
        <span class="dot disconnected" id="ws-dot"></span>
        <span id="ws-status">Connecting...</span>
      </div>
    </div>

    <div class="submit-form">
      <input type="text" id="requirement-input" placeholder="Enter a requirement (e.g., Build a login page with email + password)..." />
      <button onclick="submitRequirement()">Submit</button>
    </div>

    <div class="container">
      <div class="card">
        <div class="card-header">Live Events <span class="count" id="event-count">0</span></div>
        <div class="card-body" id="events-list">
          <div class="empty">No events yet. Submit a requirement or wait for activity.</div>
        </div>
      </div>

      <div class="card">
        <div class="card-header">Sessions <span class="count" id="session-count">0</span></div>
        <div class="card-body" id="sessions-list">
          <div class="empty">Loading...</div>
        </div>
      </div>

      <div class="card">
        <div class="card-header">Tools <span class="count" id="tool-count">0</span></div>
        <div class="card-body" id="tools-list">
          <div class="empty">Loading...</div>
        </div>
      </div>

      <div class="card">
        <div class="card-header">Skills <span class="count" id="skill-count">0</span></div>
        <div class="card-body" id="skills-list">
          <div class="empty">Loading...</div>
        </div>
      </div>
    </div>
  </div>

  <!-- Session detail overlay -->
  <div class="detail-overlay" id="detail-overlay" style="display:none" onclick="if(event.target===this)closeDetail()">
    <div class="detail-panel">
      <div class="detail-header">
        <h2 id="detail-title">Session Detail</h2>
        <div class="detail-actions">
          <button class="action-btn stop" id="detail-stop-btn" style="display:none" onclick="stopSessionFromDetail()">Stop</button>
          <button class="action-btn delete" id="detail-delete-btn" onclick="deleteSessionFromDetail()">Delete</button>
          <button class="close-btn" onclick="closeDetail()">Close</button>
        </div>
      </div>
      <div class="detail-meta" id="detail-meta"></div>
      <div class="detail-body" id="detail-body">
        <div class="empty">Loading...</div>
      </div>
    </div>
  </div>

  <script src="/ui/app.js"></script>
</body>
</html>
"##;

const APP_JS: &str = r##"
// ─── Auth State ───

function getApiKey() {
  return sessionStorage.getItem('agentdept_api_key') || '';
}

function setApiKey(key) {
  if (key) {
    sessionStorage.setItem('agentdept_api_key', key);
  } else {
    sessionStorage.removeItem('agentdept_api_key');
  }
}

// Add auth header to fetch requests.
function authHeaders(extra = {}) {
  const key = getApiKey();
  const headers = { ...extra };
  if (key) {
    headers['Authorization'] = `Bearer ${key}`;
  }
  return headers;
}

// ─── Login Flow ───

async function checkAuthRequired() {
  // Try hitting /api/health (public) then /api/auth/me to see if auth is enforced.
  try {
    const resp = await fetch('/api/auth/me', { headers: authHeaders() });
    if (resp.status === 401) {
      // Auth is enabled and we have no valid key.
      if (!getApiKey()) {
        showLogin();
        return;
      }
    }
    if (resp.ok) {
      const data = await resp.json();
      showApp(data);
      return;
    }
  } catch {}
  // If /api/auth/me fails for any reason, just show the app (auth might be disabled).
  showApp(null);
}

function showLogin() {
  document.getElementById('login-overlay').style.display = 'flex';
  document.getElementById('app').style.display = 'none';
  document.getElementById('login-key').focus();
}

function showApp(authData) {
  document.getElementById('login-overlay').style.display = 'none';
  document.getElementById('app').style.display = 'block';

  if (authData && authData.role) {
    document.getElementById('auth-info').style.display = 'flex';
    document.getElementById('auth-label').textContent = authData.label || '';
    document.getElementById('auth-role').textContent = authData.role;
  } else if (getApiKey()) {
    document.getElementById('auth-info').style.display = 'flex';
    document.getElementById('auth-label').textContent = 'authenticated';
    document.getElementById('auth-role').textContent = '?';
  }

  initApp();
}

async function doLogin() {
  const input = document.getElementById('login-key');
  const key = input.value.trim();
  const errorEl = document.getElementById('login-error');

  if (!key) {
    errorEl.textContent = 'Please enter an API key.';
    errorEl.style.display = 'block';
    return;
  }

  // Validate the key by calling /api/auth/me.
  try {
    const resp = await fetch('/api/auth/me', {
      headers: { 'Authorization': `Bearer ${key}` },
    });

    if (resp.ok) {
      const data = await resp.json();
      setApiKey(key);
      errorEl.style.display = 'none';
      showApp(data);
    } else if (resp.status === 401) {
      errorEl.textContent = 'Invalid or expired API key.';
      errorEl.style.display = 'block';
    } else {
      const body = await resp.json().catch(() => ({}));
      errorEl.textContent = body.error || `Error: ${resp.status}`;
      errorEl.style.display = 'block';
    }
  } catch (err) {
    errorEl.textContent = `Connection error: ${err.message}`;
    errorEl.style.display = 'block';
  }
}

function skipLogin() {
  setApiKey('');
  showApp(null);
}

function doLogout() {
  setApiKey('');
  document.getElementById('auth-info').style.display = 'none';
  // Check if auth is required; if so, show login again.
  checkAuthRequired();
}

// Enter key on login input.
document.getElementById('login-key').addEventListener('keydown', (e) => {
  if (e.key === 'Enter') doLogin();
});

// ─── Main App ───

let ws = null;
let eventCount = 0;
let appInitialized = false;

function initApp() {
  if (appInitialized) return;
  appInitialized = true;

  connectWs();
  loadSessions();
  loadTools();
  loadSkills();
  setInterval(loadSessions, 10000);

  document.getElementById('requirement-input').addEventListener('keydown', (e) => {
    if (e.key === 'Enter') submitRequirement();
  });
}

function connectWs() {
  const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
  // Pass API key as query param for WebSocket (can't set headers on WS).
  const key = getApiKey();
  const params = key ? `?api_key=${encodeURIComponent(key)}` : '';
  ws = new WebSocket(`${proto}//${location.host}/ws${params}`);

  ws.onopen = () => {
    document.getElementById('ws-dot').className = 'dot connected';
    document.getElementById('ws-status').textContent = 'Connected';
  };

  ws.onclose = () => {
    document.getElementById('ws-dot').className = 'dot disconnected';
    document.getElementById('ws-status').textContent = 'Disconnected';
    setTimeout(connectWs, 3000);
  };

  ws.onmessage = (e) => {
    try {
      const data = JSON.parse(e.data);
      addEvent(data);
    } catch {}
  };
}

function addEvent(data) {
  const list = document.getElementById('events-list');
  if (eventCount === 0) list.innerHTML = '';
  eventCount++;
  document.getElementById('event-count').textContent = eventCount;

  const el = document.createElement('div');
  el.className = 'event';
  const time = new Date().toLocaleTimeString();
  const type_ = data.event_type || data.type || 'event';
  el.innerHTML = `<span class="time">${time}</span><span class="type">${type_}</span>${JSON.stringify(data.data || data.message || data).substring(0, 200)}`;
  list.prepend(el);

  while (list.children.length > 100) list.removeChild(list.lastChild);
}

async function submitRequirement() {
  const input = document.getElementById('requirement-input');
  const text = input.value.trim();
  if (!text) return;

  try {
    const resp = await fetch('/api/run', {
      method: 'POST',
      headers: authHeaders({ 'Content-Type': 'application/json' }),
      body: JSON.stringify({ requirement: text }),
    });
    if (resp.status === 401) { showLogin(); return; }
    if (resp.status === 403) {
      addEvent({ event_type: 'error', data: { message: 'Insufficient permissions (need operator role)' } });
      return;
    }
    const data = await resp.json();
    addEvent({ event_type: 'submitted', data });
    input.value = '';
    loadSessions();
  } catch (err) {
    addEvent({ event_type: 'error', data: { message: err.message } });
  }
}

async function loadSessions() {
  try {
    const resp = await fetch('/api/sessions?limit=20', { headers: authHeaders() });
    if (resp.status === 401) return;
    const data = await resp.json();
    const list = document.getElementById('sessions-list');
    document.getElementById('session-count').textContent = data.count;

    if (data.count === 0) {
      list.innerHTML = '<div class="empty">No sessions yet.</div>';
      return;
    }

    list.innerHTML = data.sessions.map(s => {
      const stopBtn = s.status === 'running'
        ? `<button class="action-btn stop" onclick="event.stopPropagation();stopSession('${s.id}')">Stop</button>`
        : '';
      return `
      <div class="session-row" onclick="openSessionDetail('${s.id}')">
        <div class="id">${s.id} <span class="badge ${s.status}">${s.status}</span></div>
        <div class="meta">${s.requirement ? s.requirement.substring(0, 80) : '-'}</div>
        <div class="session-actions">
          ${stopBtn}
          <button class="action-btn delete" onclick="event.stopPropagation();confirmDeleteSession('${s.id}')">Delete</button>
        </div>
      </div>`;
    }).join('');
  } catch {}
}

async function loadTools() {
  try {
    const resp = await fetch('/api/tools', { headers: authHeaders() });
    if (resp.status === 401) return;
    const data = await resp.json();
    const list = document.getElementById('tools-list');
    document.getElementById('tool-count').textContent = data.count;

    if (data.count === 0) {
      list.innerHTML = '<div class="empty">No tools registered.</div>';
      return;
    }

    list.innerHTML = data.tools.map(t => `
      <div class="tool-item">
        <div class="name">${t.name}</div>
        <div class="desc">${t.description}</div>
      </div>
    `).join('');
  } catch {}
}

async function loadSkills() {
  try {
    const resp = await fetch('/api/skills', { headers: authHeaders() });
    if (resp.status === 401) return;
    const data = await resp.json();
    const list = document.getElementById('skills-list');
    document.getElementById('skill-count').textContent = data.count;

    if (data.count === 0) {
      list.innerHTML = '<div class="empty">No skills registered. Add SKILL.md files to the skills/ directory.</div>';
      return;
    }

    list.innerHTML = data.skills.map(s => `
      <div class="skill-item">
        <div class="name">${s.name} <span style="color:#64748b;font-size:0.7rem">v${s.version}</span></div>
        <div class="desc">${s.description}</div>
      </div>
    `).join('');
  } catch {}
}

// ─── Session Detail ───

const ROLE_COLORS = { pm: 'role-pm', ba: 'role-ba', dev: 'role-dev', frontend: 'role-frontend', test: 'role-test' };
const ROLE_LABELS = { pm: 'PM', ba: 'BA', dev: 'Dev', frontend: 'Frontend', test: 'Test' };

function rolePill(role) {
  const cls = ROLE_COLORS[role] || '';
  const label = ROLE_LABELS[role] || role;
  return `<span class="role-pill ${cls}">${label}</span>`;
}

function kindLabel(kind) {
  if (!kind) return 'unknown';
  // kind comes as {kind: "requirement"} or similar tagged enum
  if (typeof kind === 'object') return kind.kind || JSON.stringify(kind);
  return kind;
}

let currentFilter = 'all';

function openSessionDetail(id) {
  document.getElementById('detail-overlay').style.display = 'flex';
  document.getElementById('detail-body').innerHTML = '<div class="empty">Loading...</div>';
  document.getElementById('detail-meta').innerHTML = '';
  document.getElementById('detail-title').textContent = 'Session Detail';
  currentFilter = 'all';
  loadSessionDetail(id);
}

function closeDetail() {
  document.getElementById('detail-overlay').style.display = 'none';
}

// Close on Escape key
document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') closeDetail();
});

async function loadSessionDetail(id) {
  try {
    const resp = await fetch(`/api/sessions/${id}`, { headers: authHeaders() });
    if (resp.status === 401) { showLogin(); return; }
    if (!resp.ok) {
      document.getElementById('detail-body').innerHTML = `<div class="empty">Error loading session: ${resp.status}</div>`;
      return;
    }
    const data = await resp.json();
    renderSessionDetail(data);
  } catch (err) {
    document.getElementById('detail-body').innerHTML = `<div class="empty">Error: ${err.message}</div>`;
  }
}

function renderSessionDetail(data) {
  const session = data.session;
  const messages = data.messages || [];

  // Title
  document.getElementById('detail-title').textContent = `Session ${session.id.substring(0, 8)}...`;

  // Meta bar
  const meta = document.getElementById('detail-meta');
  meta.innerHTML = `
    <span><strong>Status:</strong> <span class="badge ${session.status}">${session.status}</span></span>
    <span><strong>Created:</strong> ${new Date(session.created_at).toLocaleString()}</span>
    <span><strong>Messages:</strong> ${data.message_count}</span>
    ${session.requirement ? `<span><strong>Requirement:</strong> ${session.requirement.substring(0, 120)}</span>` : ''}
  `;

  // Count messages per agent (from role)
  const agentCounts = {};
  messages.forEach(m => {
    const from = m.from || 'unknown';
    agentCounts[from] = (agentCounts[from] || 0) + 1;
  });

  // Build body content
  const body = document.getElementById('detail-body');

  // Agent summary chips
  const roles = ['pm', 'ba', 'dev', 'frontend', 'test'];
  const summaryHtml = roles
    .filter(r => agentCounts[r])
    .map(r => `<div class="agent-chip">${rolePill(r)} <span class="chip-count">${agentCounts[r]} msg${agentCounts[r] > 1 ? 's' : ''}</span></div>`)
    .join('');

  // Filter tabs
  const filterRoles = ['all', ...roles.filter(r => agentCounts[r])];
  const tabsHtml = filterRoles.map(r => {
    const label = r === 'all' ? 'All' : (ROLE_LABELS[r] || r);
    const count = r === 'all' ? messages.length : (agentCounts[r] || 0);
    return `<button class="filter-tab ${currentFilter === r ? 'active' : ''}" onclick="filterMessages('${r}', this)">${label} (${count})</button>`;
  }).join('');

  // Message cards
  const msgsHtml = renderMessages(messages, currentFilter);

  body.innerHTML = `
    <div class="agent-summary">${summaryHtml}</div>
    <div class="filter-tabs" id="detail-filters">${tabsHtml}</div>
    <div id="detail-messages">${msgsHtml}</div>
  `;

  // Store messages for filtering
  body.dataset.messages = JSON.stringify(messages);
}

function filterMessages(role, btnEl) {
  currentFilter = role;
  const body = document.getElementById('detail-body');
  const messages = JSON.parse(body.dataset.messages || '[]');

  // Update active tab
  document.querySelectorAll('.filter-tab').forEach(t => t.classList.remove('active'));
  if (btnEl) btnEl.classList.add('active');

  document.getElementById('detail-messages').innerHTML = renderMessages(messages, role);
}

function renderMessages(messages, filter) {
  const filtered = filter === 'all' ? messages : messages.filter(m => m.from === filter);

  if (filtered.length === 0) {
    return '<div class="empty">No messages from this agent.</div>';
  }

  return filtered.map((m, idx) => {
    const knd = kindLabel(m.kind);
    const from = m.from || '?';
    const to = m.to || '?';
    const priorityCls = m.priority && m.priority.toLowerCase && m.priority.toLowerCase() !== 'normal' ? m.priority.toLowerCase() : '';
    const priorityBadge = priorityCls ? `<span class="priority-badge ${priorityCls}">${priorityCls}</span>` : '';

    // Format payload - try to pretty-print JSON
    let payloadStr = '';
    if (m.payload !== undefined && m.payload !== null) {
      payloadStr = typeof m.payload === 'object' ? JSON.stringify(m.payload, null, 2) : String(m.payload);
    } else if (m.kind) {
      payloadStr = typeof m.kind === 'object' ? JSON.stringify(m.kind, null, 2) : String(m.kind);
    }

    // Check if content is long enough to collapse
    const isLong = payloadStr.length > 300;
    const collapsedCls = isLong ? 'collapsed' : '';

    return `
      <div class="msg-card">
        <div class="msg-card-header">
          <span class="step-num" style="color:#64748b;font-size:0.7rem">#${idx + 1}</span>
          ${rolePill(from)}
          <span class="arrow">&rarr;</span>
          ${rolePill(to)}
          <span class="kind-badge">${knd}</span>
          ${priorityBadge}
        </div>
        <div class="msg-card-body ${collapsedCls}" onclick="this.classList.toggle('collapsed')">${escapeHtml(payloadStr)}</div>
      </div>
    `;
  }).join('');
}

function escapeHtml(str) {
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}

// ─── Session Actions (Stop / Delete) ───

let currentDetailSessionId = null;

// Override openSessionDetail to track the current session ID.
const _origOpenSessionDetail = openSessionDetail;
openSessionDetail = function(id) {
  currentDetailSessionId = id;
  _origOpenSessionDetail(id);
};

// Show/hide the Stop button based on session status.
const _origRenderSessionDetail = renderSessionDetail;
renderSessionDetail = function(data) {
  _origRenderSessionDetail(data);
  const stopBtn = document.getElementById('detail-stop-btn');
  if (data.session && data.session.status === 'running') {
    stopBtn.style.display = 'inline-block';
  } else {
    stopBtn.style.display = 'none';
  }
};

async function stopSession(id) {
  try {
    const resp = await fetch(`/api/sessions/${id}/stop`, {
      method: 'POST',
      headers: authHeaders(),
    });
    if (resp.status === 401) { showLogin(); return; }
    if (resp.status === 403) {
      addEvent({ event_type: 'error', data: { message: 'Insufficient permissions (need operator role)' } });
      return;
    }
    const data = await resp.json();
    if (!resp.ok) {
      addEvent({ event_type: 'error', data: { message: data.error || 'Failed to stop session' } });
      return;
    }
    addEvent({ event_type: 'session_stopped', data });
    loadSessions();
  } catch (err) {
    addEvent({ event_type: 'error', data: { message: err.message } });
  }
}

function stopSessionFromDetail() {
  if (currentDetailSessionId) {
    stopSession(currentDetailSessionId).then(() => {
      // Reload the detail view.
      loadSessionDetail(currentDetailSessionId);
    });
  }
}

async function deleteSessionApi(id) {
  try {
    const resp = await fetch(`/api/sessions/${id}`, {
      method: 'DELETE',
      headers: authHeaders(),
    });
    if (resp.status === 401) { showLogin(); return; }
    if (resp.status === 403) {
      addEvent({ event_type: 'error', data: { message: 'Insufficient permissions (need operator role)' } });
      return;
    }
    const data = await resp.json();
    if (!resp.ok) {
      addEvent({ event_type: 'error', data: { message: data.error || 'Failed to delete session' } });
      return;
    }
    addEvent({ event_type: 'session_deleted', data });
    closeDetail();
    loadSessions();
  } catch (err) {
    addEvent({ event_type: 'error', data: { message: err.message } });
  }
}

function deleteSessionFromDetail() {
  if (currentDetailSessionId) {
    showConfirm(
      'Are you sure you want to delete this session? This action cannot be undone.',
      () => deleteSessionApi(currentDetailSessionId)
    );
  }
}

function confirmDeleteSession(id) {
  showConfirm(
    'Are you sure you want to delete this session? This action cannot be undone.',
    () => deleteSessionApi(id)
  );
}

function showConfirm(message, onConfirm) {
  // Remove any existing confirm overlay.
  const existing = document.getElementById('confirm-overlay');
  if (existing) existing.remove();

  const overlay = document.createElement('div');
  overlay.className = 'confirm-overlay';
  overlay.id = 'confirm-overlay';
  overlay.innerHTML = `
    <div class="confirm-box">
      <p>${message}</p>
      <div class="confirm-actions">
        <button class="confirm-cancel" id="confirm-cancel">Cancel</button>
        <button class="confirm-danger" id="confirm-ok">Delete</button>
      </div>
    </div>
  `;
  document.body.appendChild(overlay);

  overlay.addEventListener('click', (e) => {
    if (e.target === overlay) { overlay.remove(); }
  });
  document.getElementById('confirm-cancel').onclick = () => overlay.remove();
  document.getElementById('confirm-ok').onclick = () => {
    overlay.remove();
    onConfirm();
  };
}

// ─── Boot ───

// Start by checking if auth is required.
checkAuthRequired();
"##;
