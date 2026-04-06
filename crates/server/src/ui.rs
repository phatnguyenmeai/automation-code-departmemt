//! Embedded web UI dashboard.
//!
//! Serves a single-page HTML dashboard at `/` that provides:
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
  </style>
</head>
<body>
  <div class="header">
    <h1>AgentDept Gateway</h1>
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

  <script src="/ui/app.js"></script>
</body>
</html>
"##;

const APP_JS: &str = r##"
// WebSocket connection
let ws = null;
let eventCount = 0;

function connectWs() {
  const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
  ws = new WebSocket(`${proto}//${location.host}/ws`);

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

  // Keep max 100 events
  while (list.children.length > 100) list.removeChild(list.lastChild);
}

async function submitRequirement() {
  const input = document.getElementById('requirement-input');
  const text = input.value.trim();
  if (!text) return;

  try {
    const resp = await fetch('/api/run', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ requirement: text }),
    });
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
    const resp = await fetch('/api/sessions?limit=20');
    const data = await resp.json();
    const list = document.getElementById('sessions-list');
    document.getElementById('session-count').textContent = data.count;

    if (data.count === 0) {
      list.innerHTML = '<div class="empty">No sessions yet.</div>';
      return;
    }

    list.innerHTML = data.sessions.map(s => `
      <div class="session-row">
        <div class="id">${s.id} <span class="badge ${s.status}">${s.status}</span></div>
        <div class="meta">${s.requirement ? s.requirement.substring(0, 80) : '-'}</div>
      </div>
    `).join('');
  } catch {}
}

async function loadTools() {
  try {
    const resp = await fetch('/api/tools');
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
    const resp = await fetch('/api/skills');
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

// Enter key to submit
document.getElementById('requirement-input').addEventListener('keydown', (e) => {
  if (e.key === 'Enter') submitRequirement();
});

// Initialize
connectWs();
loadSessions();
loadTools();
loadSkills();

// Refresh sessions periodically
setInterval(loadSessions, 10000);
"##;
