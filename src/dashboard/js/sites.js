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
          fact('Pages project', d.settings.project ? el('span', { text: d.settings.project }) : missing('Not set', null)),
        ),
      ),
    );
  }

