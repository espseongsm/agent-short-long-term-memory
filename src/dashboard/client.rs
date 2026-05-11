pub(super) fn dashboard_script() -> &'static str {
    r#"
const filter = document.getElementById('filter');
const groupedView = document.getElementById('grouped-view');
const controls = [...document.querySelectorAll('[data-group-level]')];
const dimensions = ['date', 'session', 'user'];
const messages = [...document.querySelectorAll('.message')].map((message) => {
  const content = message.querySelector('.content')?.textContent || '';
  const role = message.dataset.role || '';
  const timestamp = Number(message.dataset.ts || 0);
  const session = message.dataset.session || 'unknown session';
  const user = message.dataset.user || 'unknown user';
  const date = message.dataset.date || 'unknown date';
  const time = timestamp > 0 ? new Date(timestamp * 1000).toLocaleString() : 'unknown';
  return {
    role,
    content,
    timestamp,
    time,
    session,
    user,
    date,
    search: `${role} ${content} ${session} ${user} ${date}`.toLowerCase(),
  };
});

function escapeHtml(value) {
  return value.replace(/[&<>"']/g, (character) => ({
    '&': '&amp;',
    '<': '&lt;',
    '>': '&gt;',
    '"': '&quot;',
    "'": '&#39;',
  }[character]));
}

function sessionAnchor(value) {
  const slug = value
    .split('')
    .map((character) => /[a-z0-9]/i.test(character) ? character.toLowerCase() : '-')
    .join('')
    .replace(/^-+|-+$/g, '');
  return `session-${slug || 'session'}`;
}

function normalizeOrder() {
  const selected = controls.map((control) => control.value);
  const unique = [];
  for (const value of selected) {
    if (dimensions.includes(value) && !unique.includes(value)) unique.push(value);
  }
  for (const value of dimensions) {
    if (!unique.includes(value)) unique.push(value);
  }
  controls.forEach((control, index) => {
    control.value = unique[index];
  });
  return unique;
}

function groupLabel(dimension, value, count) {
  const label = {
    date: 'Date',
    session: 'Session',
    user: 'User',
  }[dimension];
  return `${label}: ${value} · ${count} messages`;
}

function groupMessages(items, order, depth = 0) {
  if (depth >= order.length) return renderMessages(items);

  const dimension = order[depth];
  const groups = new Map();
  for (const item of items) {
    const key = item[dimension] || `unknown ${dimension}`;
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key).push(item);
  }

  return [...groups.entries()].map(([key, group], index) => {
    const open = depth === 0 && index === 0 ? ' open' : '';
    const anchor = dimension === 'session' ? ` id="${sessionAnchor(key)}"` : '';
    return `<details class="group-card depth-${depth}"${anchor}${open}><summary>${escapeHtml(groupLabel(dimension, key, group.length))}</summary>${groupMessages(group, order, depth + 1)}</details>`;
  }).join('');
}

function renderMessages(items) {
  return items.map((item) => `
    <div class="message" data-role="${escapeHtml(item.role)}" data-ts="${item.timestamp}">
      <div>
        <div class="role ${escapeHtml(item.role)}">${escapeHtml(item.role)}</div>
        <div class="message-meta">${escapeHtml(item.time)} · ${escapeHtml(item.user)}</div>
      </div>
      <div class="content">${escapeHtml(item.content)}</div>
    </div>
  `).join('');
}

function renderGroupedView() {
  const query = filter.value.trim().toLowerCase();
  const visible = query ? messages.filter((message) => message.search.includes(query)) : messages;
  if (visible.length === 0) {
    groupedView.innerHTML = '<div class="empty">No messages match the current filter.</div>';
    return;
  }
  groupedView.innerHTML = groupMessages(visible, normalizeOrder());
}

filter.addEventListener('input', renderGroupedView);
for (const control of controls) {
  control.addEventListener('change', renderGroupedView);
}
renderGroupedView();
"#
}
