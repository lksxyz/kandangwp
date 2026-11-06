(() => {
  const token = document.querySelector('meta[name="kandang-token"]').content;
  const $ = (id) => document.getElementById(id);
  const state = { sites: [], trash: [], detail: null, tab: 'overview', logTimer: null, autoScroll: true, cfProjects: null, publishTimer: null, publishSite: null, publishStartedAt: 0, runtime: null };
  const runtimeReady = () => !!state.runtime && state.runtime.status === 'ready';

  const PUBLISH_PHASES = [
    { id: 'starting', label: 'Starting site' },
    { id: 'exporting', label: 'Exporting' },
    { id: 'deploying', label: 'Deploying to Cloudflare' },
  ];

  /// A finished result is worth showing on return, but only while it is fresh.
  const PUBLISH_RESULT_MS = 2 * 60 * 1000;
  /// The publish request claims its job server-side a moment after the click,
  /// so an idle status right after starting is not proof that it never began.
  const PUBLISH_START_GRACE_MS = 8000;

  function stopPublishPolling() {
    if (state.publishTimer) clearInterval(state.publishTimer);
    state.publishTimer = null;
  }

  function hidePublishProgress() {
    stopPublishPolling();
    const bar = $('publish-progress');
    bar.dataset.phase = 'idle';
    bar.hidden = true;
    $('publish-progress-steps').replaceChildren();
    $('publish-progress-actions').replaceChildren();
  }

  function dismissPublishButton() {
    return el('button', { class: 'btn sm ghost', type: 'button', onclick: hidePublishProgress }, 'Dismiss');
  }

  function openPublishLog() {
    const name = state.publishSite;
    if (!name) return;
    const target = `#/sites/${encodeURIComponent(name)}/logs`;
    if (location.hash === target) openTab('logs'); else location.hash = target;
  }

  function renderPublishProgress(job) {
    const bar = $('publish-progress');
    const steps = $('publish-progress-steps');
    const actions = $('publish-progress-actions');
    bar.hidden = false;
    bar.dataset.phase = job.phase === 'done' || job.phase === 'error' ? job.phase : 'running';
    if (job.phase === 'error') {
      steps.replaceChildren(
        el('span', { class: 'error-text', text: job.error || 'Publish failed.' }),
        el('span', { class: 'hint', text: 'The deploy log has the full output.' }),
      );
      actions.replaceChildren(
        el('button', { class: 'btn sm white', type: 'button', onclick: openPublishLog }, icon('refresh'), 'View log'),
        dismissPublishButton(),
      );
      announce(`Publish failed: ${job.error || 'unknown error'}`);
      return;
    }
    if (job.phase === 'done') {
      steps.replaceChildren(
        el('span', { class: 'status running', text: 'Published' }),
        el('span', {
          class: 'publish-meta',
          text: job.deployed === false
            ? 'Everything was already up to date'
            : `${job.changed} of ${job.files} files changed`,
        }),
      );
      actions.replaceChildren(
        job.public
          ? el('a', { class: 'btn sm white', href: job.public, target: '_blank', rel: 'noopener' },
              icon('external'), 'Open live site', el('span', { class: 'sr-only', text: ' (opens in a new tab)' }))
          : null,
        dismissPublishButton(),
      );
      announce('Publish finished. The live site is up to date.');
      setTimeout(() => { if (bar.dataset.phase === 'done') hidePublishProgress(); }, 20000);
      return;
    }
    actions.replaceChildren();
    const current = Math.max(0, PUBLISH_PHASES.findIndex((p) => p.id === job.phase));
    steps.replaceChildren(
      ...PUBLISH_PHASES.map((p, i) => el('span', {
        class: `step ${i < current ? 'done' : i === current ? 'current' : ''}`,
        text: p.label,
      })),
    );
    announce(`Publishing: ${PUBLISH_PHASES[current].label}.`);
  }

  /// Reports what the strip should do next: `active` covers every in-flight
  /// phase, `done` and `error` are terminal, and `idle`/`stale`/`unknown` mean
  /// there is nothing left to watch.
  async function pollPublishStatus(name) {
    try {
      const job = await api('GET', `/api/sites/${encodeURIComponent(name)}/publish/status`);
      if (job.phase === 'idle') {
        if (state.publishSite === name && Date.now() - state.publishStartedAt < PUBLISH_START_GRACE_MS) return 'active';
        hidePublishProgress();
        return 'idle';
      }
      if (job.finished_at && job.finished_at * 1000 < Date.now() - PUBLISH_RESULT_MS) {
        hidePublishProgress();
        return 'stale';
      }
      state.publishSite = name;
      renderPublishProgress(job);
      return job.phase === 'done' || job.phase === 'error' ? job.phase : 'active';
    } catch {
      return 'unknown';
    }
  }

  function watchPublish(name) {
    stopPublishPolling();
    state.publishSite = name;
    state.publishStartedAt = Date.now();
    const settle = (status) => {
      if (status === 'active') return;
      stopPublishPolling();
      refreshAfterPublish(name);
    };
    pollPublishStatus(name).then(settle);
    state.publishTimer = setInterval(async () => {
      if (document.hidden) return;
      settle(await pollPublishStatus(name));
    }, 1500);
  }

  /// A finished publish changes the deployed address, the preview, and the
  /// saved project, so the panels holding them are updated without touching
  /// the rest of the form the user may be editing.
  async function refreshAfterPublish(name) {
    const r = route();
    if (r.view !== 'site' || r.name !== name) return;
    let detail;
    try {
      detail = await api('GET', `/api/sites/${encodeURIComponent(name)}`);
    } catch {
      return;
    }
    state.detail = detail;
    renderSiteActions(detail);
    const block = $('pub-last');
    if (block) block.replaceChildren(...lastPublishedRows(detail));
    if (state.tab === 'overview') renderPanel();
  }

  async function api(method, path, body) {
    const headers = {};
    if (method !== 'GET') headers['X-Kandang-Token'] = token;
    if (body !== undefined) headers['Content-Type'] = 'application/json';
    const res = await fetch(path, { method, headers, body: body === undefined ? undefined : JSON.stringify(body) });
    const text = await res.text();
    if (path.endsWith('/logs')) {
      if (!res.ok) throw new Error(text || res.statusText);
      return text;
    }
    let data = {};
    try { data = text ? JSON.parse(text) : {}; } catch { /* plain-text error body */ }
    if (res.status === 403 && /wrong token|missing/.test(text)) {
      // The engine restarts regenerate the per-run token; this page is stale.
      location.reload();
      await new Promise(() => {});
    }
    if (!res.ok) throw new Error(data.error || text || res.statusText);
    return data;
  }

  function announce(message) {
    const el = $('status');
    el.textContent = '';
    requestAnimationFrame(() => { el.textContent = message; });
  }

  function showError(message) {
    const box = $('error');
    $('error-text').textContent = message;
    box.hidden = !message;
  }

  function dismissError() {
    $('error').hidden = true;
    $('error-text').textContent = '';
  }

  function el(tag, props = {}, ...children) {
    const node = document.createElement(tag);
    for (const [key, value] of Object.entries(props)) {
      if (key === 'class') node.className = value;
      else if (key === 'text') node.textContent = value;
      else if (key.startsWith('on')) node.addEventListener(key.slice(2), value);
      else if (value !== undefined && value !== null && value !== false) node.setAttribute(key, value === true ? '' : value);
    }
    const append = (child) => {
      if (child === null || child === undefined || child === false || child === true) return;
      if (Array.isArray(child)) child.forEach(append);
      else if (child instanceof Node) node.append(child);
      else node.append(document.createTextNode(String(child)));
    };
    children.forEach(append);
    return node;
  }

  // Hugeicons, 24px grid, 1.5 stroke: inner markup for buttons rendered at runtime.
  const ICONS = {
    play: '<path fill="none" stroke="currentColor" stroke-linejoin="round" stroke-width="1.5" d="M18.89 12.846c-.353 1.343-2.023 2.292-5.364 4.19c-3.23 1.835-4.845 2.752-6.146 2.384a3.25 3.25 0 0 1-1.424-.841C5 17.614 5 15.743 5 12s0-5.614.956-6.579a3.25 3.25 0 0 1 1.424-.84c1.301-.37 2.916.548 6.146 2.383c3.34 1.898 5.011 2.847 5.365 4.19a3.3 3.3 0 0 1 0 1.692Z"/>',
    stop: '<path fill="none" stroke="currentColor" stroke-width="1.5" d="M4 12c0-3.28 0-4.919.814-6.081a4.5 4.5 0 0 1 1.105-1.105C7.08 4 8.72 4 12 4s4.919 0 6.081.814a4.5 4.5 0 0 1 1.105 1.105C20 7.08 20 8.72 20 12s0 4.919-.814 6.081a4.5 4.5 0 0 1-1.105 1.105C16.92 20 15.28 20 12 20s-4.919 0-6.081-.814a4.5 4.5 0 0 1-1.105-1.105C4 16.92 4 15.28 4 12Z"/>',
    external: '<path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="m13 11l-7 7m9.54-11.917l-1.241.113c-2.012.183-3.018.274-3.255.928c-.236.654.478 1.368 1.906 2.796l1.13 1.13c1.428 1.428 2.142 2.142 2.796 1.905s.745-1.242.928-3.254l.113-1.242c.11-1.211.165-1.817-.197-2.18c-.362-.361-.968-.306-2.18-.196"/>',
    view: '<g fill="none" stroke="currentColor" stroke-width="1.5"><path d="M21.544 11.045c.304.426.456.64.456.955c0 .316-.152.529-.456.955C20.178 14.871 16.689 19 12 19c-4.69 0-8.178-4.13-9.544-6.045C2.152 12.529 2 12.315 2 12c0-.316.152-.529.456-.955C3.822 9.129 7.311 5 12 5c4.69 0 8.178 4.13 9.544 6.045Z"/><path d="M15 12a3 3 0 1 0-6 0a3 3 0 0 0 6 0Z"/></g>',
    copy: '<g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5"><path d="M7.5 14.5c0-3.3 0-4.95 1.025-5.975S11.2 7.5 14.5 7.5s4.95 0 5.975 1.025S21.5 11.2 21.5 14.5s0 4.95-1.025 5.975S17.8 21.5 14.5 21.5s-4.95 0-5.975-1.025S7.5 17.8 7.5 14.5"/><path d="M7.5 16.5c-1.396 0-2.095 0-2.656-.196a3.5 3.5 0 0 1-2.148-2.148C2.5 13.595 2.5 12.896 2.5 11.5v-2c0-3.3 0-4.95 1.025-5.975S6.2 2.5 9.5 2.5h2c1.396 0 2.095 0 2.656.196a3.5 3.5 0 0 1 2.148 2.148c.196.561.196 1.26.196 2.656"/></g>',
    lock: '<g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5"><path d="M4.268 18.845c.225 1.67 1.608 2.979 3.292 3.056c1.416.065 2.855.099 4.44.099s3.024-.034 4.44-.1c1.684-.076 3.067-1.385 3.292-3.055c.147-1.09.268-2.207.268-3.345s-.121-2.255-.268-3.345c-.225-1.67-1.608-2.979-3.292-3.056A95 95 0 0 0 12 9c-1.585 0-3.024.034-4.44.1c-1.684.076-3.067 1.385-3.292 3.055C4.12 13.245 4 14.362 4 15.5s.121 2.255.268 3.345"/><path d="M7.5 9V6.5a4.5 4.5 0 0 1 9 0V9m-4.375 6.5H12m.25 0a.25.25 0 1 1-.5 0a.25.25 0 0 1 .5 0m-4.125 0H8m.25 0a.25.25 0 1 1-.5 0a.25.25 0 0 1 .5 0m7.875 0H16m.25 0a.25.25 0 1 1-.5 0a.25.25 0 0 1 .5 0"/></g>',
    refresh: '<path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M20.01 2v3.132a.314.314 0 0 1-.556.201A9.98 9.98 0 0 0 12 2C6.477 2 2 6.477 2 12s4.477 10 10 10s10-4.477 10-10"/>',
    save: '<g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5"><path d="M15.879 3H11C7.229 3 5.343 3 4.172 4.172S3 7.229 3 11v2c0 3.771 0 5.657 1.172 6.828S7.229 21 11 21h2c3.771 0 5.657 0 6.828-1.172S21 16.771 21 13V8.121c0-.456 0-.685-.024-.904a4 4 0 0 0-.854-2.06c-.138-.173-.3-.334-.622-.657c-.323-.323-.484-.484-.656-.622a4 4 0 0 0-2.061-.854C16.563 3 16.335 3 15.879 3"/><path d="M17 3.5V4c0 1.886 0 2.828-.586 3.414S14.886 8 13 8h-2c-1.886 0-2.828 0-3.414-.586S7 5.886 7 4v-.5m10 17V17c0-1.886 0-2.828-.586-3.414S14.886 13 13 13h-2c-1.886 0-2.828 0-3.414.586S7 15.114 7 17v3.5"/></g>',
    rocket: '<g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5"><path d="m6.219 11.618l-2.907-.38c-.596-.077-.993-.654-.729-1.192C3.425 8.334 5.62 6.354 9.74 6.68m-3.521 4.937c1.055-1.764 2.353-3.614 3.52-4.937m-3.52 4.937l5.663 5.663M9.74 6.681c3.67-4.091 7.434-4.953 9.854-4.614a2.156 2.156 0 0 1 1.84 1.84c.338 2.419-.524 6.182-4.615 9.853m-4.937 3.521l.38 2.907c.077.596.654.993 1.192.729c1.712-.842 3.692-3.036 3.365-7.157m-4.937 3.521c1.764-1.055 3.614-2.353 4.937-3.52"/><path d="M17.5 8a2 2 0 1 0-4 0a2 2 0 0 0 4 0M4 22l4-4m-4-1l1.5-1.5"/></g>',
    cloud: '<path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M17.478 10h.022a4.5 4.5 0 1 1 0 9H7a5 5 0 0 1-.48-9.977M17.478 10q.021-.247.022-.5a5.5 5.5 0 0 0-10.98-.477M17.478 10a5.5 5.5 0 0 1-1.235 3M6.52 9.023Q6.757 9 7 9c1.126 0 2.165.372 3 1"/>',
    unlink: '<g fill="none" stroke="currentColor" stroke-linecap="round" stroke-width="1.5"><path d="m1.996 2l20 20"/><path stroke-linejoin="round" d="M8.996 17h-2a5 5 0 0 1 0-10m14.001 8a5 5 0 0 0-4-8h-3.5m3.499 10h-2m-6.5-5h3.5"/></g>',
    trash: '<path fill="none" stroke="currentColor" stroke-linecap="round" stroke-width="1.5" d="m19.5 5.5l-.62 10.025c-.158 2.561-.237 3.842-.88 4.763a4 4 0 0 1-1.2 1.128c-.957.584-2.24.584-4.806.584c-2.57 0-3.855 0-4.814-.585a4 4 0 0 1-1.2-1.13c-.642-.922-.72-2.205-.874-4.77L4.5 5.5M3 5.5h18m-4.944 0l-.683-1.408c-.453-.936-.68-1.403-1.071-1.695a2 2 0 0 0-.275-.172C13.594 2 13.074 2 12.035 2c-1.066 0-1.599 0-2.04.234a2 2 0 0 0-.278.18c-.395.303-.616.788-1.058 1.757L8.053 5.5m1.447 11v-6m5 6v-6"/>',
    restore: '<g fill="none" stroke="currentColor" stroke-linecap="round" stroke-width="1.5"><path d="m19.5 5.5l-.5 6m-14.5-6l.605 10.037c.154 2.57.232 3.855.874 4.78a4 4 0 0 0 1.2 1.132c.582.356 1.284.496 2.321.551"/><path stroke-linejoin="round" d="m11 15.5l1.136 1.466a4 4 0 0 1 7.364-.901M21 20.5l-1.136-1.464a4 4 0 0 1-7.328.965"/><path d="M3 5.5h18m-4.944 0l-.683-1.408c-.453-.936-.68-1.403-1.071-1.695a2 2 0 0 0-.275-.172C13.594 2 13.074 2 12.035 2c-1.066 0-1.599 0-2.04.234a2 2 0 0 0-.278.18c-.395.303-.616.788-1.058 1.757L8.053 5.5"/></g>',
  };

  function icon(name) {
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    for (const [key, value] of Object.entries({
      viewBox: '0 0 24 24', fill: 'none', stroke: 'currentColor', 'stroke-width': '1.5',
      'stroke-linecap': 'round', 'stroke-linejoin': 'round', 'aria-hidden': 'true', focusable: 'false',
    })) svg.setAttribute(key, value);
    svg.innerHTML = ICONS[name];
    return svg;
  }

  function statusNode(running) {
    return el('span', { class: `status ${running ? 'running' : 'stopped'}`, text: running ? 'Running' : 'Stopped' });
  }

  function confirmDialog({ title, body, action, danger = false }) {
    const dialog = $('dlg-confirm');
    $('dlg-confirm-title').textContent = title;
    $('dlg-confirm-body').textContent = body;
    const ok = $('dlg-confirm-ok');
    ok.textContent = action;
    ok.className = danger ? 'btn solid danger' : 'btn solid';
    return new Promise((resolve) => {
      dialog.addEventListener('close', () => resolve(dialog.returnValue === 'ok'), { once: true });
      dialog.showModal();
    });
  }

  function publishDialogChildren(plan) {
    const children = [
      el('dl', { class: 'facts' },
        el('div', { class: 'fact' }, el('dt', { text: 'Changed files' }), el('dd', { text: plan.changed === 0 ? 'None' : `${plan.changed} of ${plan.files}` })),
        el('div', { class: 'fact' }, el('dt', { text: 'Pages project' }), el('dd', { text: plan.project })),
        el('div', { class: 'fact' }, el('dt', { text: 'Destination' }), el('dd', {}, el('span', { class: 'address', text: plan.published }))),
      ),
    ];
    if (plan.changed === 0) {
      children.push(el('p', { class: 'notice-line' },
        'Nothing changed since the last publish, so nothing will be uploaded. ',
        el('a', { href: plan.published, target: '_blank', rel: 'noopener' },
          'Open the live site', el('span', { class: 'sr-only', text: ' (opens in a new tab)' }))));
    } else {
      children.push(el('p', { class: 'hint', style: 'margin:0', text: 'Kandang starts the site if it is not running, exports it, then deploys. The site stays running afterwards, and the result stays at the top of the page.' }));
    }
    return children;
  }

  async function run(button, task, successMessage) {
    if (button) button.disabled = true;
    showError('');
    try {
      const result = await task();
      if (successMessage) announce(successMessage);
      return result;
    } catch (e) {
      showError(e.message);
      announce(`Error: ${e.message}`);
      return undefined;
    } finally {
      if (button) button.disabled = false;
    }
  }

