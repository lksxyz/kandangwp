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
        ? `Publishing to ${project} — existing Pages project, updates it.`
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
      el('label', { for: 'pub-project' }, 'Pages project', el('span', { class: 'hint', id: 'pub-project-hint', text: 'Follows the site name. The Cloudflare Pages project is created on the first publish.' })),
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

