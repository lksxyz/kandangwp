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
          el('button', { class: 'btn sm white', type: 'button', 'aria-label': `Restore ${item.name}`, disabled: runtimeReady() ? null : true,
            onclick: async (event) => {
              await run(event.currentTarget, () => api('POST', `/api/trash/${encodeURIComponent(item.id)}/restore`, {}), `${item.name} restored.`);
              await loadTrash();
            } }, icon('restore'), 'Restore'),
          el('button', { class: 'btn sm outline danger', type: 'button', 'aria-label': `Delete ${item.name} permanently`, disabled: runtimeReady() ? null : true, onclick: () => purge(item) }, icon('trash'), 'Purge'),
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
    if (document.hidden || !runtimeReady()) return;
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

  // Runtime setup: the user starts the download from this page. Progress and
  // failures show here; the rest of the dashboard appears once the runtime is ready.
  const SETUP_VIEWS = ['view-sites', 'view-site', 'view-trash', 'view-cloudflare', 'view-engine'];

  function updateRuntimeUI() {
    const r = state.runtime;
    const setup = $('runtime-setup');
    if (r && runtimeReady()) {
      setup.hidden = true;
      return;
    }
    setup.hidden = false;
    for (const id of SETUP_VIEWS) $(id).hidden = true;
    if (!r) {
      $('runtime-checking').hidden = false;
      $('runtime-intro').hidden = true;
      $('runtime-start').hidden = true;
      $('runtime-progress').hidden = true;
      $('runtime-error').hidden = true;
      return;
    }
    const preparing = r.status === 'preparing';
    const failed = r.status === 'failed';
    const percent = Math.max(0, Math.min(100, r.percent || 0));
    $('runtime-checking').hidden = true;
    $('runtime-intro').hidden = preparing;
    $('runtime-start').hidden = preparing;
    $('runtime-start').textContent = failed ? 'Try again' : 'Download runtime';
    $('runtime-progress').hidden = !preparing;
    $('runtime-bar').value = percent;
    $('runtime-percent').textContent = `${percent}% downloaded`;
    $('runtime-file').textContent = r.file ? `Downloading ${r.file}` : 'Starting download…';
    $('runtime-error').hidden = !failed;
    $('runtime-error').textContent = failed ? `Download failed: ${r.error || 'unknown error'}` : '';
  }

  async function refreshRuntime() {
    try {
      const next = await api('GET', '/api/runtime');
      const was = state.runtime && state.runtime.status;
      state.runtime = next;
      updateRuntimeUI();
      if (next.status === 'ready') {
        if (was !== 'ready') await render();
        return;
      }
      if (next.status === 'preparing') setTimeout(refreshRuntime, 1000);
    } catch {
      setTimeout(refreshRuntime, 3000);
    }
  }

  $('runtime-start').addEventListener('click', async () => {
    const button = $('runtime-start');
    button.disabled = true;
    try {
      await api('POST', '/api/runtime/provision', {});
      state.runtime = { status: 'preparing', percent: 0 };
      updateRuntimeUI();
      refreshRuntime();
    } catch (e) {
      showError(e.message);
    } finally {
      button.disabled = false;
    }
  });

  refreshRuntime();
  render().catch((e) => showError(e.message));
})();
