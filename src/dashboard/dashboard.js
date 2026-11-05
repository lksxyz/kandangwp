(() => {
  const token = document.querySelector('meta[name="kandang-token"]').content;
  const $ = (id) => document.getElementById(id);
  const state = { sites: [], trash: [], detail: null, tab: 'overview', logTimer: null, autoScroll: true, cfProjects: null, publishTimer: null, publishSite: null, publishStartedAt: 0 };

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
        el('div', { class: 'fact' }, el('dt', { text: 'Project' }), el('dd', { text: plan.project })),
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

  function route() {
    const hash = location.hash.replace(/^#\/?/, '');
    const parts = hash.split('/').filter(Boolean);
    if (parts[0] === 'sites' && parts[1]) return { view: 'site', name: decodeURIComponent(parts[1]), tab: parts[2] || 'overview' };
    if (parts[0] === 'trash') return { view: 'trash' };
    if (parts[0] === 'cloudflare') return { view: 'cloudflare' };
    if (parts[0] === 'engine') return { view: 'engine' };
    return { view: 'sites' };
  }

  function showView(name) {
    for (const section of ['sites', 'site', 'trash', 'cloudflare', 'engine']) {
      $(`view-${section}`).hidden = section !== name;
    }
    document.querySelectorAll('[data-nav]').forEach((a) => {
      const active = a.dataset.nav === name || (name === 'site' && a.dataset.nav === 'sites');
      if (active) a.setAttribute('aria-current', 'page'); else a.removeAttribute('aria-current');
    });
  }

  async function render() {
    const r = route();
    stopLogs();
    showView(r.view);
    if (r.view === 'sites') { await loadSites(); $('h-sites').focus({ preventScroll: true }); }
    if (r.view === 'site') await openSite(r.name, r.tab);
    if (r.view === 'trash') { await loadTrash(); $('h-trash').focus({ preventScroll: true }); }
    if (r.view === 'cloudflare') { await loadCloudflare(); $('h-cloudflare').focus({ preventScroll: true }); }
    if (r.view === 'engine') await loadEngine();
  }

  async function loadSites() {
    state.sites = await api('GET', '/api/sites');
    const running = state.sites.filter((s) => s.running).length;
    const exported = state.sites.filter((s) => s.preview_url).length;
    $('count-sites').textContent = state.sites.length ? String(state.sites.length) : '';
    $('sites-summary').textContent = state.sites.length
      ? `${state.sites.length} ${state.sites.length === 1 ? 'site' : 'sites'} · ${running} running · ${exported} exported`
      : '';
    const list = $('sites-list');
    list.replaceChildren();
    const hasSites = state.sites.length > 0;
    list.hidden = !hasSites;
    $('sites-empty').hidden = hasSites;
    $('sites-label').hidden = !hasSites;
    for (const site of state.sites) {
      const name = el('a', { href: `#/sites/${encodeURIComponent(site.name)}/overview`, text: site.name });
      const address = el('a', { class: 'address', href: site.url, target: '_blank', rel: 'noopener', text: site.url.replace(/^https?:\/\//, '') }, el('span', { class: 'sr-only', text: ' (opens in a new tab)' }));
      const preview = site.preview_url
        ? el('a', { class: 'address', href: site.preview_url, target: '_blank', rel: 'noopener', text: 'preview' }, el('span', { class: 'sr-only', text: ' (opens in a new tab)' }))
        : null;
      const toggle = el('button', {
        class: site.running ? 'btn sm outline danger' : 'btn sm solid', type: 'button',
        'aria-label': `${site.running ? 'Stop' : 'Start'} ${site.name}`,
        onclick: (event) => toggleSite(site, event.currentTarget),
      }, icon(site.running ? 'stop' : 'play'), site.running ? 'Stop' : 'Start');
      list.append(el('li', { class: 'row' },
        statusNode(site.running),
        el('div', { class: 'row-main' },
          el('div', { class: 'row-title' }, name),
          el('div', { class: 'row-meta' }, address, preview),
        ),
        el('div', { class: 'port', text: `:${site.port}` }),
        el('div', { class: 'row-actions' }, toggle),
      ));
    }
  }

  async function toggleSite(site, button) {
    const action = site.running ? 'stop' : 'start';
    await run(button, () => api('POST', `/api/sites/${encodeURIComponent(site.name)}/${action}`, {}),
      `${site.name} ${site.running ? 'stopped' : 'started'}.`);
    await loadSites();
  }

  async function openSite(name, tab) {
    let detail;
    try {
      detail = await api('GET', `/api/sites/${encodeURIComponent(name)}`);
    } catch (e) {
      showError(e.message);
      $('h-site').textContent = 'Site not found';
      $('crumb-name').textContent = name;
      $('panel').replaceChildren(el('p', { class: 'muted', text: 'This site does not exist or was moved to the trash.' }));
      return;
    }
    state.detail = detail;
    const title = detail.title || detail.name;
    $('h-site').textContent = title;
    $('crumb-name').textContent = title;
    document.title = `${title} · Kandang`;
    renderSiteActions(detail);
    setTab(tab, false);
    renderPanel();
    // Surface an in-flight or just-finished publish when the user returns.
    pollPublishStatus(detail.name).then((status) => { if (status === 'active') watchPublish(detail.name); });
  }

  function renderSiteActions(detail) {
    const actions = $('site-actions');
    actions.replaceChildren(
      ...[
      statusNode(detail.running),
      el('button', {
        class: detail.running ? 'btn outline danger' : 'btn solid', type: 'button',
        'aria-label': `${detail.running ? 'Stop' : 'Start'} ${detail.name}`,
        onclick: async (event) => {
          await run(event.currentTarget, () => api('POST', `/api/sites/${encodeURIComponent(detail.name)}/${detail.running ? 'stop' : 'start'}`, {}),
            `${detail.name} ${detail.running ? 'stopped' : 'started'}.`);
          await openSite(detail.name, state.tab);
        },
      }, icon(detail.running ? 'stop' : 'play'), detail.running ? 'Stop' : 'Start'),
      detail.running ? el('a', { class: 'btn solid', href: detail.url + '/wp-admin', target: '_blank', rel: 'noopener' }, icon('external'), 'Open admin') : null,
      ].filter(Boolean),
    );
  }

  function setTab(tab, focus) {
    const tabs = [...document.querySelectorAll('[role="tab"]')];
    const valid = tabs.some((t) => t.dataset.tab === tab) ? tab : 'overview';
    state.tab = valid;
    for (const t of tabs) {
      const selected = t.dataset.tab === valid;
      t.setAttribute('aria-selected', String(selected));
      t.tabIndex = selected ? 0 : -1;
      t.setAttribute('aria-controls', 'panel');
      if (selected && focus) t.focus();
    }
    const name = state.detail ? state.detail.name : route().name;
    const current = location.hash;
    const target = `#/sites/${encodeURIComponent(name)}/${valid}`;
    if (current !== target && state.detail) history.replaceState(null, '', target);
  }

  function renderPanel() {
    const panel = $('panel');
    panel.setAttribute('aria-labelledby', `tab-${state.tab}`);
    const detail = state.detail;
    if (state.tab === 'overview') panel.replaceChildren(overviewPanel(detail));
    if (state.tab === 'logs') { panel.replaceChildren(logsPanel(detail)); startLogs(detail.name); }
    if (state.tab === 'publish') panel.replaceChildren(publishPanel(detail));
    if (state.tab === 'danger') panel.replaceChildren(dangerPanel(detail));
  }

  function fact(label, ...value) {
    return el('div', { class: 'fact' }, el('dt', { text: label }), el('dd', {}, ...value));
  }

  function openTab(tab) {
    setTab(tab, true);
    renderPanel();
  }

  function missing(text, action, tab) {
    return el('span', { class: 'missing' },
      el('span', { text }),
      action ? el('button', { class: 'text-action', type: 'button', onclick: () => openTab(tab) }, action) : null,
    );
  }

  function addressLink(href) {
    return el('a', { class: 'address', href, target: '_blank', rel: 'noopener', text: href },
      el('span', { class: 'sr-only', text: ' (opens in a new tab)' }));
  }

  /// Where the last publish landed: the two addresses that exist only once the
  /// site has been exported and deployed.
  function lastPublishedRows(d) {
    return [
      fact('Deployed', d.deployed ? addressLink(d.deployed) : missing('Not published yet', null)),
      fact('Preview', d.preview_url ? addressLink(d.preview_url) : missing('Not exported yet', null)),
    ];
  }

  function overviewPanel(d) {
    const publicAddress = d.settings.public || defaultPublic(d.settings.project);
    return el('div', { class: 'panel overview' },
      el('section', { class: 'fact-group' },
        el('h3', { text: 'Authoring' }),
        el('dl', { class: 'facts' },
          fact('Site address', addressLink(d.url)),
          fact('Preview', d.preview_url ? addressLink(d.preview_url) : missing('Not exported yet', 'Publish', 'publish')),
          fact('Admin user', el('code', { class: 'address', text: 'admin' })),
          fact('Admin password', adminPasswordCell(d.name)),
        ),
      ),
      el('section', { class: 'fact-group' },
        el('h3', { text: 'Publishing' }),
        el('dl', { class: 'facts' },
          fact('Public address', publicAddress ? el('span', { class: 'address', text: publicAddress }) : missing('Not set', 'Set up', 'publish')),
          fact('Deployed address', d.deployed ? addressLink(d.deployed) : missing('Not published yet', null)),
          fact('Cloudflare project', d.settings.project ? el('span', { text: d.settings.project }) : missing('Not set', null)),
        ),
      ),
    );
  }

  const PASSWORD_VISIBLE_MS = 30000;
  const PASSWORD_CLIPBOARD_MS = 30000;

  function adminPasswordButtons(name, cell) {
    cell.replaceChildren(
      el('button', { class: 'btn sm soft', type: 'button', onclick: () => revealAdminPassword(name, cell) }, icon('view'), 'Show password'),
      el('button', { class: 'btn sm outline danger', type: 'button', 'data-action': 'reset', onclick: () => resetAdminPassword(name, cell) }, icon('lock'), 'Reset password'),
    );
  }

  function adminPasswordCell(name) {
    const cell = el('div', { class: 'btn-row' });
    cell.dataset.site = name;
    adminPasswordButtons(name, cell);
    return cell;
  }

  function hideAdminPassword(cell) {
    cell._hideTimer = null;
    if (!cell.isConnected) return;
    adminPasswordButtons(cell.dataset.site, cell);
    announce('Admin password hidden.');
  }

  function showAdminPassword(cell, password) {
    if (cell._hideTimer) clearTimeout(cell._hideTimer);
    const name = cell.dataset.site;
    const copy = el('button', { class: 'btn sm white', type: 'button', onclick: () => copyAdminPassword(cell, password) }, icon('copy'), 'Copy');
    const reset = el('button', { class: 'btn sm outline danger', type: 'button', 'data-action': 'reset', onclick: () => resetAdminPassword(name, cell) }, icon('lock'), 'Reset password');
    cell.replaceChildren(el('code', { class: 'secret', text: password }), copy, reset);
    cell._hideTimer = setTimeout(() => hideAdminPassword(cell), PASSWORD_VISIBLE_MS);
  }

  async function copyAdminPassword(cell, password) {
    try {
      await navigator.clipboard.writeText(password);
      announce('Password copied. The clipboard clears in 30 seconds.');
      if (cell._clipTimer) clearTimeout(cell._clipTimer);
      cell._clipTimer = setTimeout(async () => {
        cell._clipTimer = null;
        try {
          if (await navigator.clipboard.readText() === password) {
            await navigator.clipboard.writeText('');
            announce('Password cleared from the clipboard.');
          }
        } catch {
          // The browser cannot confirm the clipboard content, so leave it alone.
        }
      }, PASSWORD_CLIPBOARD_MS);
    } catch {
      showError('The browser blocked clipboard access. Select the password and copy it.');
    }
  }

  async function revealAdminPassword(name, cell) {
    cell.dataset.site = name;
    const ok = await confirmDialog({
      title: `Show the admin password for ${name}?`,
      body: 'The password appears on this page for 30 seconds. Anyone who can see your screen can read it during that time.',
      action: 'Show password',
    });
    if (!ok) return;
    const button = cell.querySelector('button');
    const result = await run(button, () => api('POST', `/api/sites/${encodeURIComponent(name)}/admin-password`, {}));
    if (!result) return;
    showAdminPassword(cell, result.password);
    announce('Admin password shown. It hides in 30 seconds.');
  }

  async function resetAdminPassword(name, cell) {
    cell.dataset.site = name;
    const ok = await confirmDialog({
      title: `Reset the admin password for ${name}?`,
      body: 'Kandang generates a new password, saves it in your desktop keyring, and shows it here. The WordPress admin username stays the same.',
      action: 'Reset password',
    });
    if (!ok) return;
    const button = cell.querySelector('[data-action="reset"]');
    const result = await run(button, () => api('POST', `/api/sites/${encodeURIComponent(name)}/admin-password/reset`, {}));
    if (!result) return;
    showAdminPassword(cell, result.password);
    announce('Admin password reset and saved in the keyring.');
  }

  function logsPanel(d) {
    const scroll = el('input', { type: 'checkbox', id: 'log-scroll', checked: state.autoScroll });
    scroll.checked = state.autoScroll;
    scroll.addEventListener('change', () => { state.autoScroll = scroll.checked; });
    return el('div', {},
      el('div', { class: 'btn-row', style: 'margin-bottom:12px' },
        el('button', { class: 'btn ghost', type: 'button', onclick: () => loadLog(d.name) }, icon('refresh'), 'Refresh log'),
        el('label', { class: 'checkbox', for: 'log-scroll' }, scroll, ' Follow new lines'),
      ),
      el('pre', { class: 'log', id: 'log-output', tabindex: '0', role: 'log', 'aria-label': `Server log for ${d.name}`, 'aria-live': 'off', text: 'Loading…' }),
    );
  }

  async function loadLog(name) {
    const out = $('log-output');
    if (!out) return;
    try {
      const text = await api('GET', `/api/sites/${encodeURIComponent(name)}/logs`);
      out.textContent = text || '(no log output yet)';
      if (state.autoScroll) out.scrollTop = out.scrollHeight;
    } catch (e) {
      out.textContent = e.message;
    }
  }

  function startLogs(name) {
    loadLog(name);
    state.logTimer = setInterval(() => { if (!document.hidden) loadLog(name); }, 5000);
  }

  function stopLogs() {
    if (state.logTimer) clearInterval(state.logTimer);
    state.logTimer = null;
  }

  function defaultPublic(project) {
    return project ? `https://${project}.pages.dev` : '';
  }

  const PROJECT_NAME = /^[A-Za-z0-9 -]+$/;

  /// Cloudflare Pages only accepts lowercase letters, digits, and dashes, so the
  /// name typed in the form is kept human and turned into a slug here.
  function projectSlug(value) {
    return value.trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '');
  }

  function projectSlugError(raw) {
    const value = raw.trim();
    if (!value) return 'Enter a project name first.';
    if (!PROJECT_NAME.test(value)) return 'Use letters, digits, and spaces only.';
    if (!projectSlug(value)) return 'Add at least one letter or digit.';
    return null;
  }

  function publishPanel(d) {
    const projectInput = el('input', { type: 'text', id: 'pub-project', value: d.name, disabled: true, autocomplete: 'off', 'aria-describedby': 'pub-project-hint pub-project-status' });
    const projectStatus = el('span', { class: 'hint', id: 'pub-project-status', role: 'status', text: '' });
    const domainsBlock = el('div', { id: 'pub-domains', class: 'domain-list' });
    function refreshStatus() {
      const project = d.name;
      if (state.cfProjects === null) {
        projectStatus.textContent = `Publishing to ${project}`;
        renderDomains(project, null);
        return;
      }
      const existing = state.cfProjects.find((p) => p.name === project);
      projectStatus.textContent = existing
        ? `Publishing to ${project} — existing project, updates it.`
        : `Publishing to ${project} — created on first publish.`;
      renderDomains(project, existing || null);
    }
    function renderDomains(project, existing) {
      const block = domainsBlock;
      if (!block) return;
      const lines = existing ? [...new Set(existing.domains)] : [];
      block.replaceChildren(
        ...(lines.length
          ? lines.map((domain) => el('p', { class: 'address' },
              el('a', { href: `https://${domain}`, target: '_blank', rel: 'noopener', text: domain })))
          : [el('p', { class: 'muted', text: project ? 'No domains yet. The first publish creates one.' : 'Set a project name to see its domains.' })]),
        project && d.cloudflare_connected && d.cloudflare_account_id
          ? el('p', { class: 'hint' },
              el('a', { href: `https://dash.cloudflare.com/${d.cloudflare_account_id}/pages/view/${project}/domains`, target: '_blank', rel: 'noopener' },
                'Add a custom domain in the Cloudflare dashboard',
                el('span', { class: 'sr-only', text: ' (opens in a new tab)' })))
          : null,
      );
    }
    if (d.cloudflare_connected) {
      loadProjects().then(refreshStatus).catch(() => {});
    }
    refreshStatus();
    const form = el('form', { class: 'stack', id: 'publish-form', novalidate: true },
      el('label', { for: 'pub-project' }, 'Project name', el('span', { class: 'hint', id: 'pub-project-hint', text: 'Follows the site name. The Cloudflare Pages project is created on the first publish.' })),
      projectInput,
      projectStatus,
      el('div', { class: 'btn-row' },
        el('button', { class: 'btn solid', type: 'button', id: 'pub-check' }, icon('rocket'), 'Publish'),
      ),
    );
    form.querySelector('#pub-check').addEventListener('click', () => (d.cloudflare_connected ? reviewPublish(d.name) : exportZip(d.name)));
    if (!d.cloudflare_connected) form.append(el('p', { class: 'hint' }, 'Not connected to Cloudflare: the export is written as a zip. Connect Cloudflare in ', el('a', { href: '#/cloudflare', text: 'Integrations' }), ' to publish online.'));

    return el('div', { class: 'panel' },
      el('div', { class: 'settings' },
        el('div', {}, form),
        el('aside', { class: 'settings-side' },
          el('section', { class: 'side-block' }, el('h3', { text: 'Domains' }), domainsBlock),
          el('section', { class: 'side-block' },
            el('h3', { text: 'Last publish' }),
            el('dl', { class: 'facts', id: 'pub-last' }, lastPublishedRows(d))),
        ),
      ),
    );
  }

  async function loadCloudflare() {
    const s = await api('GET', '/api/cloudflare');
    const connected = s.connected;
    const how = 'Connected to your Cloudflare account. Sign-in renews automatically.';
    const body = $('cloudflare-body');
    body.replaceChildren(
      el('div', { class: 'panel' },
        el('h2', { text: 'Cloudflare Pages' }),
        el('div', { class: 'settings' },
          el('div', {},
            el('p', { style: 'margin:0 0 12px' },
              el('span', { class: `status ${connected ? 'running' : 'stopped'}`, text: connected ? 'Connected' : 'Not connected' })),
            el('p', { class: 'muted', style: 'margin:0', text: connected ? how : 'Publishing uploads the exported files to Cloudflare Pages. Connect once; it applies to every site.' })),
          el('aside', { class: 'settings-side' },
            el('section', { class: 'side-block' },
              el('h3', { text: 'Connection' }),
              el('div', { class: 'btn-row' },
                connected
                  ? el('button', { class: 'btn outline danger', type: 'button', onclick: () => disconnectCloudflareGlobal() }, icon('unlink'), 'Disconnect Cloudflare')
                  : el('button', { class: 'btn solid', type: 'button', onclick: (e) => connectCloudflare(e.currentTarget) }, icon('cloud'), 'Connect Cloudflare')),
              connected ? null : el('p', { class: 'hint', style: 'margin:0', text: 'Without a Cloudflare connection, publishing exports a zip of each site.' }))),
        ),
      ),
    );
  }

  async function disconnectCloudflareGlobal() {
    const ok = await confirmDialog({
      title: 'Disconnect Cloudflare?',
      body: 'Publishing stops until you connect again. Sites export as zip files meanwhile.',
      action: 'Disconnect',
      danger: true,
    });
    if (!ok) return;
    await run(null, () => api('POST', '/api/cloudflare/disconnect', {}), 'Cloudflare disconnected.');
    await loadCloudflare();
    await loadStatusBar();
  }

  async function connectCloudflare(button) {
    const idle = [...button.childNodes];
    button.disabled = true;
    button.setAttribute('aria-busy', 'true');
    button.replaceChildren(icon('cloud'), 'Waiting for sign-in…');
    const restore = () => {
      button.disabled = false;
      button.removeAttribute('aria-busy');
      button.replaceChildren(...idle);
    };
    const started = await run(null, () => api('POST', '/api/cloudflare/connect', {}), 'Finish signing in to Cloudflare in your browser.');
    if (started) waitForCloudflare(restore);
    else restore();
  }

  async function loadProjects() {
    if (state.cfProjects) return state.cfProjects;
    const data = await api('GET', '/api/cloudflare/projects');
    state.cfProjects = data.projects || [];
    return state.cfProjects;
  }

  async function reviewPublish(name) {
    const body = { dry_run: true };
    const plan = await run($('pub-check'), () => api('POST', `/api/sites/${encodeURIComponent(name)}/publish`, body));
    if (!plan) return;
    const dialog = $('dlg-publish');
    const ok = $('dlg-publish-ok');
    $('dlg-publish-body').replaceChildren(...publishDialogChildren(plan));
    ok.hidden = plan.changed === 0;
    const confirmed = await new Promise((resolve) => {
      dialog.addEventListener('close', () => resolve(dialog.returnValue === 'ok'), { once: true });
      dialog.showModal();
    });
    ok.hidden = false;
    if (!confirmed) return;
    renderPublishProgress({ phase: 'starting' });
    watchPublish(name);
    try {
      await api('POST', `/api/sites/${encodeURIComponent(name)}/publish`, { ...body, dry_run: false });
    } catch (e) {
      if (!$('publish-progress').dataset.phase || $('publish-progress').dataset.phase === 'running') {
        hidePublishProgress();
      }
      showError(e.message);
    }
  }

  async function exportZip(name) {
    renderPublishProgress({ phase: 'starting' });
    watchPublish(name);
    try {
      await api('POST', `/api/sites/${encodeURIComponent(name)}/publish`, {});
    } catch (e) {
      if (!$('publish-progress').dataset.phase || $('publish-progress').dataset.phase === 'running') {
        hidePublishProgress();
      }
      showError(e.message);
    }
  }

  function dangerPanel(d) {
    return el('div', { class: 'panel danger-panel' },
      el('h2', { text: 'Move to trash' }),
      el('p', { class: 'muted', text: `The site stops and moves to the trash. You can restore it from Trash. Purging it deletes its files permanently.` }),
      el('div', { class: 'btn-row' },
        el('button', { class: 'btn outline danger', type: 'button', onclick: () => trashSite(d.name) }, icon('trash'), `Move ${d.name} to trash`)),
    );
  }

  async function trashSite(name) {
    const ok = await confirmDialog({
      title: `Move “${name}” to trash?`,
      body: 'The site stops first. You can restore it later from Trash.',
      action: 'Move to trash',
      danger: true,
    });
    if (!ok) return;
    await run(null, () => api('POST', `/api/sites/${encodeURIComponent(name)}/trash`, {}), `${name} moved to trash.`);
    location.hash = '#/trash';
  }

  async function loadTrash() {
    state.trash = await api('GET', '/api/trash');
    $('count-trash').textContent = state.trash.length ? String(state.trash.length) : '';
    const list = $('trash-list');
    list.replaceChildren();
    list.hidden = state.trash.length === 0;
    $('trash-empty').hidden = state.trash.length > 0;
    for (const item of state.trash) {
      list.append(el('li', { class: 'row row-compact' },
        el('span', { class: 'status trashed', text: 'Trashed' }),
        el('div', { class: 'row-main' },
          el('div', { class: 'row-title', text: item.name }),
        ),
        el('div', { class: 'row-actions' },
          el('button', { class: 'btn sm white', type: 'button', 'aria-label': `Restore ${item.name}`,
            onclick: async (event) => {
              await run(event.currentTarget, () => api('POST', `/api/trash/${encodeURIComponent(item.id)}/restore`, {}), `${item.name} restored.`);
              await loadTrash();
            } }, icon('restore'), 'Restore'),
          el('button', { class: 'btn sm outline danger', type: 'button', 'aria-label': `Delete ${item.name} permanently`, onclick: () => purge(item) }, icon('trash'), 'Purge'),
        ),
      ));
    }
  }

  function purge(item) {
    const dialog = $('dlg-purge');
    $('dlg-purge-title').textContent = `Delete “${item.name}” permanently?`;
    $('dlg-purge-body').textContent = 'This removes the site files and database. It cannot be undone.';
    $('purge-label').textContent = `Type ${item.name} to confirm`;
    const input = $('purge-confirm');
    input.value = '';
    input.setAttribute('aria-invalid', 'false');
    $('purge-error').hidden = true;
    const form = $('purge-form');
    const onSubmit = async (event) => {
      if (event.submitter && event.submitter.value === 'cancel') return;
      if (input.value !== item.name) {
        event.preventDefault();
        input.setAttribute('aria-invalid', 'true');
        $('purge-error').textContent = `The text must match ${item.name} exactly.`;
        $('purge-error').hidden = false;
        input.focus();
        return;
      }
      await run(null, () => api('POST', `/api/trash/${encodeURIComponent(item.id)}/purge`, { confirm: input.value }), `${item.name} deleted permanently.`);
    };
    form.onsubmit = onSubmit;
    dialog.showModal();
    input.focus();
  }

  async function loadEngine() {
    const status = await api('GET', '/api/status');
    $('engine-dashboard').textContent = `http://localhost:${status.dashboard_port}/`;
    $('engine-gateway').textContent = `http://<name>.localhost:${status.gateway_port}/`;
  }

  async function loadStatusBar() {
    try {
      const status = await api('GET', '/api/status');
      $('bar-gateway').textContent = `*.localhost:${status.gateway_port}`;
    } catch {
      $('bar-gateway').textContent = 'unavailable';
    }
    try {
      const cloudflare = await api('GET', '/api/cloudflare');
      const connected = cloudflare.connected;
      $('bar-cloudflare').replaceChildren(
        el('span', { class: `status ${connected ? 'running' : 'stopped'}`, text: connected ? 'Cloudflare connected' : 'Cloudflare not connected' }),
      );
    } catch {
      $('bar-cloudflare').textContent = '';
    }
  }

  // The browser completes the sign-in on the engine's callback; this page picks up the result.
  function waitForCloudflare(onDone) {
    const finish = () => { clearInterval(timer); clearTimeout(limit); onDone(); };
    const timer = setInterval(async () => {
      try {
        const s = await api('GET', '/api/cloudflare');
        if (s.connected) {
          finish();
          announce('Cloudflare connected.');
          if (state.detail) await openSite(state.detail.name, 'publish'); else await loadCloudflare();
          await loadStatusBar();
        }
      } catch (e) {
        finish();
        announce(`Error: ${e.message}`);
      }
    }, 1500);
    const limit = setTimeout(() => { finish(); announce('Cloudflare sign-in timed out. Try again.'); }, 5 * 60 * 1000);
  }

  // Create-site dialog
  const createDialog = $('dlg-create');

  /// The dialog takes a readable name; the site itself is the slug of it.
  function refreshCreateName() {
    const raw = $('create-name').value;
    const slug = projectSlug(raw);
    const invalid = raw.trim() && projectSlugError(raw);
    $('create-name-status').textContent = invalid || !slug ? '' : `Site name: ${slug}`;
  }

  function openCreate() {
    $('create-form').reset();
    $('create-form-fields').hidden = false;
    $('create-result').hidden = true;
    $('create-submit').hidden = false;
    $('create-done').hidden = true;
    $('create-name-status').textContent = '';
    for (const id of ['create-name', 'create-port']) {
      $(id).setAttribute('aria-invalid', 'false');
    }
    $('create-name-error').hidden = true;
    $('create-port-error').hidden = true;
    createDialog.showModal();
    $('create-name').focus();
  }
  $('open-create').addEventListener('click', openCreate);
  $('open-create-empty').addEventListener('click', openCreate);
  $('create-cancel').addEventListener('click', () => createDialog.close());
  $('create-name').addEventListener('input', refreshCreateName);

  $('create-form').addEventListener('submit', async (event) => {
    event.preventDefault();
    const rawName = $('create-name').value;
    const name = projectSlug(rawName);
    const portValue = $('create-port').value.trim();
    const nameError = $('create-name-error');
    const slugError = projectSlugError(rawName);
    if (slugError) {
      $('create-name').setAttribute('aria-invalid', 'true');
      nameError.textContent = slugError;
      nameError.hidden = false;
      $('create-name').focus();
      return;
    }
    nameError.hidden = true;
    $('create-name').setAttribute('aria-invalid', 'false');
    const body = { name, project: rawName.trim() };
    if (portValue) body.port = Number(portValue);
    announce(`Creating ${name}. The first run downloads WordPress and PHP and can take a minute.`);
    const submit = $('create-submit');
    submit.disabled = true;
    try {
      const site = await api('POST', '/api/sites', body);
      $('create-password').textContent = site.admin_password;
      $('create-password-note').textContent = site.password_stored
        ? 'Site created. The admin password is saved in your desktop keyring; you can show it again from Overview.'
        : 'Site created. Save the admin password now; the keyring is unavailable so it is shown only once.';
      $('create-form-fields').hidden = true;
      $('create-result').hidden = false;
      submit.hidden = true;
      $('create-done').hidden = false;
      $('create-done').focus();
      announce(`${site.name} created.`);
      await loadSites();
    } catch (e) {
      if (/site named|site names/.test(e.message)) {
        nameError.textContent = e.message;
        nameError.hidden = false;
        $('create-name').setAttribute('aria-invalid', 'true');
      } else {
        $('create-port-error').textContent = e.message;
        $('create-port-error').hidden = false;
        $('create-port').setAttribute('aria-invalid', 'true');
      }
      announce(`Error: ${e.message}`);
    } finally {
      submit.disabled = false;
    }
  });
  $('create-done').addEventListener('click', () => {
    createDialog.close();
    location.hash = `#/sites/${encodeURIComponent(projectSlug($('create-name').value))}/overview`;
  });

  // Tabs: arrow keys, Home, End
  $('tabs').addEventListener('click', (event) => {
    const tab = event.target.closest('[role="tab"]');
    if (!tab) return;
    history.replaceState(null, '', `#/sites/${encodeURIComponent(state.detail.name)}/${tab.dataset.tab}`);
    setTab(tab.dataset.tab, true);
    renderPanel();
  });
  $('tabs').addEventListener('keydown', (event) => {
    const tabs = [...document.querySelectorAll('[role="tab"]')];
    const index = tabs.findIndex((t) => t.getAttribute('aria-selected') === 'true');
    let next = null;
    if (event.key === 'ArrowRight') next = (index + 1) % tabs.length;
    if (event.key === 'ArrowLeft') next = (index - 1 + tabs.length) % tabs.length;
    if (event.key === 'Home') next = 0;
    if (event.key === 'End') next = tabs.length - 1;
    if (next === null) return;
    event.preventDefault();
    const target = tabs[next];
    history.replaceState(null, '', `#/sites/${encodeURIComponent(state.detail.name)}/${target.dataset.tab}`);
    setTab(target.dataset.tab, true);
    renderPanel();
  });

  // Mobile navigation drawer
  const navSidebar = $('sidebar');
  const navToggle = $('nav-open');
  const navClose = $('nav-close');
  const navBackdrop = $('nav-backdrop');
  const mobileNavQuery = window.matchMedia('(max-width: 840px)');

  function openNav() {
    navSidebar.classList.add('is-open');
    navBackdrop.hidden = false;
    navToggle.setAttribute('aria-expanded', 'true');
    navClose.focus();
  }

  function closeNav({ restoreFocus = true } = {}) {
    if (!navSidebar.classList.contains('is-open')) return;
    navSidebar.classList.remove('is-open');
    navBackdrop.hidden = true;
    navToggle.setAttribute('aria-expanded', 'false');
    if (restoreFocus && mobileNavQuery.matches) navToggle.focus();
  }

  navToggle.addEventListener('click', openNav);
  navClose.addEventListener('click', () => closeNav());
  navBackdrop.addEventListener('click', () => closeNav());
  navSidebar.querySelectorAll('nav a').forEach((link) => {
    link.addEventListener('click', () => closeNav({ restoreFocus: false }));
  });
  mobileNavQuery.addEventListener('change', () => closeNav({ restoreFocus: false }));
  document.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') closeNav();
  });

  window.addEventListener('hashchange', () => {
    closeNav({ restoreFocus: false });
    render().catch((e) => showError(e.message));
  });

  // Quiet polling: refresh data, pause while hidden, announce nothing.
  setInterval(() => {
    if (document.hidden) return;
    const r = route();
    if (r.view === 'sites') loadSites().catch(() => {});
    if (r.view === 'trash') loadTrash().catch(() => {});
    if (r.view === 'site' && state.detail && !$('dlg-create').open) {
      api('GET', `/api/sites/${encodeURIComponent(r.name)}`).then((d) => {
        if (d.running !== state.detail.running || d.exported !== state.detail.exported) openSite(r.name, state.tab);
      }).catch(() => {});
    }
  }, 5000);

  // Count badges are visible on every view.
  Promise.all([loadSites(), loadTrash()]).catch(() => {});
  loadStatusBar().catch(() => {});
  render().catch((e) => showError(e.message));
})();
