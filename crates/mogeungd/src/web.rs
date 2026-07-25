//! A thin web client, served by the daemon itself. Roadmap `R-C3`.
//!
//! The daemon already carries the whole product: state, ranking, diffs, review
//! marks. A client is a projection with no local authority
//! ([ADR-0001](../../../docs/decisions/0001-rust-core-with-egui-ui.md)), which
//! is precisely what makes this cheap — one HTML file over the same WebSocket
//! the desktop UI uses.
//!
//! Scope is deliberately "triage from the sofa", not a second full UI: see the
//! queue, read a diff, mark hunks read, snooze. Anything needing a keyboard and
//! a real screen stays in the desktop client.
//!
//! No build step, no framework, no CDN — a single self-contained string. A
//! phone client that needs `npm install` would never get maintained.

pub const INDEX_HTML: &str = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover">
<title>mogeung</title>
<style>
  :root {
    --bg:#16171b; --panel:#1e2026; --line:#2c2f36; --text:#e6e6ea; --dim:#8a8a90;
    --red:#e5484f; --amber:#e09b24; --blue:#4c8eda; --green:#3fa85e; --purple:#9a6fd0;
    --add:#143a21; --del:#451a1d;
  }
  * { box-sizing:border-box; -webkit-tap-highlight-color:transparent; }
  body {
    margin:0; background:var(--bg); color:var(--text);
    font:15px/1.45 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;
    padding-bottom:env(safe-area-inset-bottom);
  }
  header {
    position:sticky; top:0; z-index:5; background:var(--panel);
    border-bottom:1px solid var(--line); padding:10px 14px;
    display:flex; align-items:center; gap:10px;
    padding-top:calc(10px + env(safe-area-inset-top));
  }
  header h1 { font-size:16px; margin:0; font-weight:600; }
  #dot { width:9px; height:9px; border-radius:50%; background:var(--red); flex:none; }
  #dot.on { background:var(--green); }
  #summary { color:var(--dim); font-size:13px; margin-left:auto; text-align:right; }
  main { padding:10px; max-width:900px; margin:0 auto; }

  .card {
    background:var(--panel); border:1px solid var(--line); border-left-width:3px;
    border-radius:8px; padding:11px 12px; margin-bottom:9px; cursor:pointer;
  }
  .card:active { background:#24262d; }
  .row { display:flex; align-items:center; gap:8px; }
  .badge {
    font:600 10px/1.6 ui-monospace,SFMono-Regular,Menlo,monospace;
    padding:1px 6px; border-radius:4px; color:#fff; flex:none; letter-spacing:.4px;
  }
  .label { font-weight:500; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .detail, .meta { color:var(--dim); font-size:13px; margin-top:3px; }
  .meta { font-size:12px; }
  .warn { color:var(--amber); font-size:12.5px; margin-top:4px; }

  #detail { display:none; }
  #detail.open { display:block; }
  .back {
    background:none; border:1px solid var(--line); color:var(--text);
    border-radius:6px; padding:6px 12px; font-size:14px;
  }
  .actions { display:flex; gap:8px; flex-wrap:wrap; margin:10px 0; }
  .actions button {
    background:#2a2d35; border:1px solid var(--line); color:var(--text);
    border-radius:6px; padding:7px 12px; font-size:13px;
  }
  .actions button:active { background:#343842; }

  .file { border:1px solid var(--line); border-radius:8px; margin-bottom:10px; overflow:hidden; }
  .file > summary {
    padding:9px 11px; background:var(--panel); cursor:pointer;
    display:flex; align-items:center; gap:8px; font-size:13.5px;
  }
  .file > summary::-webkit-details-marker { display:none; }
  .path { font-family:ui-monospace,SFMono-Regular,Menlo,monospace; font-size:12.5px;
          overflow:hidden; text-overflow:ellipsis; white-space:nowrap; direction:rtl; text-align:left; }
  .hunk { border-top:1px solid var(--line); }
  .hunk-bar {
    display:flex; align-items:center; gap:8px; padding:6px 10px;
    background:#191b20; font-size:12px; color:var(--dim);
  }
  .hunk-bar button {
    margin-left:auto; background:#2a2d35; border:1px solid var(--line);
    color:var(--text); border-radius:5px; padding:4px 10px; font-size:12px; flex:none;
  }
  .hunk.read .hunk-bar button { background:var(--green); border-color:var(--green); }
  .hunk.read pre { opacity:.4; }
  pre {
    margin:0; padding:8px 10px; overflow-x:auto; font-size:11.5px; line-height:1.4;
    font-family:ui-monospace,SFMono-Regular,Menlo,monospace;
  }
  pre div { white-space:pre; }
  .a { background:var(--add); } .d { background:var(--del); }
  .empty { color:var(--dim); text-align:center; padding:40px 20px; }
</style>
</head>
<body>
<header>
  <div id="dot"></div>
  <h1>mogeung</h1>
  <div id="summary">connecting…</div>
</header>
<main>
  <div id="queue"></div>
  <div id="detail"></div>
</main>
<script>
(() => {
  const COLORS = {
    awaiting_permission:'var(--red)', awaiting_input:'var(--red)', failed:'var(--red)',
    needs_review:'var(--amber)', stalled:'var(--purple)', running:'var(--blue)', idle:'var(--dim)'
  };
  const LABELS = {
    awaiting_permission:'APPROVE', awaiting_input:'WAITING', failed:'FAILED',
    needs_review:'REVIEW', stalled:'STALLED', running:'running', idle:'idle'
  };
  const NEEDS = new Set(['awaiting_permission','awaiting_input','failed','needs_review','stalled']);

  let sessions = {}, queue = [], changes = {}, selected = null, ws = null;
  const $ = id => document.getElementById(id);
  const esc = s => (s ?? '').replace(/[&<>"]/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}[c]));

  function connect() {
    ws = new WebSocket(`ws://${location.host}/ws`);
    ws.onopen = () => { $('dot').classList.add('on'); };
    ws.onclose = () => { $('dot').classList.remove('on'); setTimeout(connect, 2000); };
    ws.onmessage = e => { ingest(JSON.parse(e.data)); render(); };
  }
  const send = m => ws && ws.readyState === 1 && ws.send(JSON.stringify(m));

  function ingest(m) {
    switch (m.ev) {
      case 'snapshot':
        sessions = {}; m.sessions.forEach(s => sessions[s.id] = s); queue = m.queue; break;
      case 'session_updated': sessions[m.session.id] = m.session; break;
      case 'session_removed': delete sessions[m.session_id]; delete changes[m.session_id];
        if (selected === m.session_id) selected = null; break;
      case 'queue': queue = m.queue; break;
      case 'change_updated': changes[m.session_id] = m.change; break;
    }
  }

  function render() { selected ? renderDetail() : renderQueue(); }

  function renderQueue() {
    $('detail').classList.remove('open');
    const need = queue.filter(q => NEEDS.has(q.reason)).length;
    const live = Object.values(sessions).filter(s => s.alive).length;
    $('summary').textContent = `${need} need you · ${live} live`;

    const el = $('queue');
    el.style.display = '';
    if (!queue.length) { el.innerHTML = '<div class="empty">No sessions yet.</div>'; return; }

    el.innerHTML = queue.map(q => {
      const s = sessions[q.session_id]; if (!s) return '';
      const c = COLORS[q.reason] || 'var(--dim)';
      const snoozed = s.snoozed_until && new Date(s.snoozed_until) > new Date();
      const collide = (s.collisions || []).length
        ? `<div class="warn">⚠ also being edited by ${esc(s.collisions[0].other_label)}</div>` : '';
      const loop = s.loop_signal ? `<div class="warn">↻ ${esc(s.loop_signal)}</div>` : '';
      return `<div class="card" style="border-left-color:${c}" data-id="${esc(s.id)}">
        <div class="row">
          <span class="badge" style="background:${c}">${LABELS[q.reason] || q.reason}</span>
          ${snoozed ? '<span class="badge" style="background:var(--dim)">ZZZ</span>' : ''}
          <span class="label">${esc(s.title || s.last_prompt || s.name || s.id.slice(0,8))}</span>
        </div>
        <div class="detail">${esc(q.detail)}</div>
        <div class="meta">${esc(repoOf(s))}${s.git_branch ? ' · ' + esc(s.git_branch) : ''}${
          s.files_changed ? ` · ${s.files_changed} file(s) +${s.insertions} −${s.deletions}` : ''}</div>
        ${collide}${loop}
      </div>`;
    }).join('');

    el.querySelectorAll('.card').forEach(c => c.onclick = () => {
      selected = c.dataset.id;
      send({cmd:'refresh_change', session_id:selected});
      render();
    });
  }

  const repoOf = s => (s.repo_root || s.cwd || '').split('/').pop() || '—';

  function renderDetail() {
    $('queue').style.display = 'none';
    const s = sessions[selected];
    const d = $('detail'); d.classList.add('open');
    if (!s) { selected = null; render(); return; }

    const ch = changes[selected];
    const snoozed = s.snoozed_until && new Date(s.snoozed_until) > new Date();

    d.innerHTML = `
      <button class="back">← queue</button>
      <h2 style="font-size:16px;margin:12px 0 4px">${esc(s.label || s.title || s.id.slice(0,8))}</h2>
      <div class="meta">${esc(s.cwd)}</div>
      <div class="actions">
        <button data-act="review-all">Mark all read</button>
        <button data-act="snooze">${snoozed ? 'Wake' : 'Snooze 30m'}</button>
        <button data-act="refresh">Refresh diff</button>
      </div>
      ${ch ? renderFiles(ch) : '<div class="empty">Loading diff…</div>'}`;

    d.querySelector('.back').onclick = () => { selected = null; render(); };
    d.querySelectorAll('[data-act]').forEach(b => b.onclick = () => {
      const a = b.dataset.act;
      if (a === 'review-all') send({cmd:'review_all', session_id:selected});
      if (a === 'refresh') send({cmd:'refresh_change', session_id:selected});
      if (a === 'snooze') send({cmd:'snooze', session_id:selected, minutes: snoozed ? 0 : 30});
    });
    d.querySelectorAll('[data-anchor]').forEach(b => b.onclick = ev => {
      ev.preventDefault(); ev.stopPropagation();
      send({cmd:'set_hunk_reviewed', session_id:selected,
            anchor:b.dataset.anchor, reviewed:b.dataset.read !== 'true'});
    });
  }

  function renderFiles(ch) {
    if (ch.error) return `<div class="empty">${esc(ch.error)}</div>`;
    if (!ch.files.length) return '<div class="empty">No changes.</div>';
    return ch.files.map(f => {
      const unread = f.hunks.filter(h => !h.reviewed).length;
      return `<details class="file"${unread ? ' open' : ''}>
        <summary>
          <span class="badge" style="background:${
            f.score >= 70 ? 'var(--red)' : f.score >= 30 ? 'var(--amber)' : 'var(--dim)'}">${
            f.score >= 70 ? 'HIGH' : f.score >= 30 ? 'MED' : 'low'}</span>
          <span class="path">${esc(f.path)}</span>
          <span style="margin-left:auto;color:var(--dim);font-size:12px;flex:none">${
            unread ? unread + ' unread' : '✓'}</span>
        </summary>
        ${f.hunks.map(h => `
          <div class="hunk${h.reviewed ? ' read' : ''}">
            <div class="hunk-bar">
              <span>+${h.insertions} −${h.deletions}</span>
              <span>${h.flags.map(esc).join(' ')}</span>
              <button data-anchor="${esc(h.anchor)}" data-read="${h.reviewed}">${
                h.reviewed ? '✓ read' : 'mark read'}</button>
            </div>
            <pre>${h.lines.map(l =>
              `<div class="${l[0] === '+' ? 'a' : l[0] === '-' ? 'd' : ''}">${esc(l)}</div>`
            ).join('')}</pre>
          </div>`).join('')}
      </details>`;
    }).join('');
  }

  connect();
  setInterval(() => { if (!selected) render(); }, 5000);
})();
</script>
</body>
</html>
"##;
