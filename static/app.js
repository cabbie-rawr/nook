// Nook — Today page interactions: command palette, keyboard shortcuts,
// quick-add NLP, optimistic completion, drag-to-reorder, focus sessions,
// momentum tooltips, and theme toggling. Vanilla JS, no build step, matching
// the rest of this server-rendered app.
(function () {
  'use strict';

  const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  function escapeHtml(s) {
    return String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  }

  function debounce(fn, ms) {
    let t;
    return (...args) => { clearTimeout(t); t = setTimeout(() => fn(...args), ms); };
  }

  function isTypingContext(e) {
    const el = e.target;
    if (!el) return false;
    const tag = el.tagName;
    return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || el.isContentEditable;
  }

  // -------------------------------------------------------------------------
  // Toasts + live-region announcements
  // -------------------------------------------------------------------------
  function showToast(message, opts = {}) {
    const region = document.getElementById('toast-region');
    if (!region) return;
    const el = document.createElement('div');
    el.className = 'toast' + (opts.type === 'error' ? ' toast-error' : '');
    el.setAttribute('role', opts.type === 'error' ? 'alert' : 'status');
    const span = document.createElement('span');
    span.textContent = message;
    el.appendChild(span);
    if (opts.actionLabel) {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.textContent = opts.actionLabel;
      btn.addEventListener('click', () => { if (opts.onAction) opts.onAction(); el.remove(); });
      el.appendChild(btn);
    }
    region.appendChild(el);
    const duration = opts.duration || 4000;
    setTimeout(() => el.remove(), duration);
  }

  function announce(message) {
    const region = document.getElementById('a11y-announcer');
    if (!region) return;
    region.textContent = '';
    requestAnimationFrame(() => { region.textContent = message; });
  }

  window.NookToast = showToast;

  // -------------------------------------------------------------------------
  // Reload a single bento card on demand (e.g. after a mutation elsewhere)
  // -------------------------------------------------------------------------
  function reloadCard(key) {
    const el = document.querySelector('[data-card="' + key + '"]');
    if (!el || typeof htmx === 'undefined') return;
    htmx.ajax('GET', '/partials/' + key, { target: el, swap: 'outerHTML' });
  }
  window.NookReloadCard = reloadCard;

  // -------------------------------------------------------------------------
  // Theme toggle
  // -------------------------------------------------------------------------
  function toggleTheme() {
    const root = document.documentElement;
    const current = root.getAttribute('data-theme') ||
      (window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light');
    const next = current === 'dark' ? 'light' : 'dark';
    root.setAttribute('data-theme', next);
    try { localStorage.setItem('nook-theme', next); } catch (e) { /* ignore */ }
    showToast(next === 'dark' ? 'Switched to dark theme' : 'Switched to light theme');
  }

  // -------------------------------------------------------------------------
  // Command palette
  // -------------------------------------------------------------------------
  const palette = document.getElementById('command-palette');
  const paletteInput = document.getElementById('palette-input');
  const paletteResults = document.getElementById('palette-results');
  let paletteLastFocused = null;
  let paletteItems = [];
  let paletteSelectedIndex = 0;

  const STATIC_COMMANDS = [
    { title: 'New task', run: () => { closePalette(); triggerNewTask(); } },
    { title: 'New space', run: () => { closePalette(); window.location.href = '/spaces'; } },
    { title: 'Go to Today', run: () => { closePalette(); window.location.href = '/'; } },
    { title: 'Go to Calendar', run: () => { closePalette(); window.location.href = '/calendar'; } },
    { title: 'Go to Spaces', run: () => { closePalette(); window.location.href = '/spaces'; } },
    { title: 'Toggle theme', run: () => { closePalette(); toggleTheme(); } },
  ];

  function triggerNewTask() {
    const el = document.getElementById('quick-add-input');
    if (el) { el.focus(); } else { window.location.href = '/'; }
  }

  function openPalette() {
    if (!palette) return;
    paletteLastFocused = document.activeElement;
    palette.hidden = false;
    paletteInput.value = '';
    renderPaletteResults('');
    paletteInput.focus();
  }

  function closePalette() {
    if (!palette || palette.hidden) return;
    palette.hidden = true;
    if (paletteLastFocused && paletteLastFocused.focus) paletteLastFocused.focus();
    paletteLastFocused = null;
  }

  function updatePaletteSelection() {
    paletteItems.forEach((item, i) => {
      const selected = i === paletteSelectedIndex;
      item.el.setAttribute('aria-selected', String(selected));
      if (selected) item.el.scrollIntoView({ block: 'nearest' });
    });
  }

  function addPaletteItem(label, sub, run) {
    const li = document.createElement('li');
    li.setAttribute('role', 'option');
    li.innerHTML = '<span>' + escapeHtml(label) + '</span><span class="palette-kind">' + escapeHtml(sub || '') + '</span>';
    li.addEventListener('click', run);
    li.addEventListener('mousemove', () => {
      const idx = paletteItems.findIndex((it) => it.el === li);
      if (idx >= 0 && idx !== paletteSelectedIndex) { paletteSelectedIndex = idx; updatePaletteSelection(); }
    });
    paletteResults.appendChild(li);
    paletteItems.push({ el: li, run });
  }

  const renderPaletteResults = debounce(async function (query) {
    const q = (query || '').trim();
    let serverResults = [];
    try {
      const res = await fetch('/api/search?q=' + encodeURIComponent(q));
      if (res.ok) serverResults = await res.json();
    } catch (e) { /* command list still works offline */ }

    const commands = STATIC_COMMANDS.filter((c) => !q || c.title.toLowerCase().includes(q.toLowerCase()));

    paletteResults.innerHTML = '';
    paletteItems = [];

    if (serverResults.length) {
      const header = document.createElement('li');
      header.className = 'palette-section';
      header.textContent = q ? 'Results' : 'Recent';
      paletteResults.appendChild(header);
      serverResults.forEach((r) => addPaletteItem(r.title, r.subtitle, () => { closePalette(); window.location.href = r.url; }));
    }
    if (commands.length) {
      const header = document.createElement('li');
      header.className = 'palette-section';
      header.textContent = 'Commands';
      paletteResults.appendChild(header);
      commands.forEach((c) => addPaletteItem(c.title, '', c.run));
    }
    if (!serverResults.length && !commands.length) {
      const empty = document.createElement('li');
      empty.className = 'palette-section';
      empty.textContent = 'No matches';
      paletteResults.appendChild(empty);
    }
    paletteSelectedIndex = 0;
    updatePaletteSelection();
  }, 150);

  if (paletteInput) {
    paletteInput.addEventListener('input', () => renderPaletteResults(paletteInput.value));
    paletteInput.addEventListener('keydown', (e) => {
      if (e.key === 'ArrowDown') { e.preventDefault(); paletteSelectedIndex = Math.min(paletteSelectedIndex + 1, paletteItems.length - 1); updatePaletteSelection(); }
      else if (e.key === 'ArrowUp') { e.preventDefault(); paletteSelectedIndex = Math.max(paletteSelectedIndex - 1, 0); updatePaletteSelection(); }
      else if (e.key === 'Enter') { e.preventDefault(); const item = paletteItems[paletteSelectedIndex]; if (item) item.run(); }
      else if (e.key === 'Tab') { e.preventDefault(); }
      else if (e.key === 'Escape') { e.preventDefault(); closePalette(); }
    });
  }
  document.querySelectorAll('[data-open-palette]').forEach((btn) => btn.addEventListener('click', openPalette));
  document.querySelectorAll('[data-close-palette]').forEach((el) => el.addEventListener('click', closePalette));

  // -------------------------------------------------------------------------
  // Shortcuts sheet
  // -------------------------------------------------------------------------
  const shortcutsSheet = document.getElementById('shortcuts-sheet');
  function openShortcuts() {
    if (!shortcutsSheet) return;
    shortcutsSheet.hidden = false;
    const closeBtn = shortcutsSheet.querySelector('.palette');
    if (closeBtn) closeBtn.setAttribute('tabindex', '-1');
    shortcutsSheet.querySelector('.palette')?.focus();
  }
  function closeShortcuts() { if (shortcutsSheet) shortcutsSheet.hidden = true; }
  document.querySelectorAll('[data-close-shortcuts]').forEach((el) => el.addEventListener('click', closeShortcuts));

  // -------------------------------------------------------------------------
  // Global keyboard shortcuts
  // -------------------------------------------------------------------------
  let awaitingG = false;
  let gTimeout = null;

  document.addEventListener('keydown', (e) => {
    // Cmd/Ctrl+K always opens the palette, even while typing.
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
      e.preventDefault();
      if (palette && !palette.hidden) closePalette(); else openPalette();
      return;
    }

    if (palette && !palette.hidden) return; // palette owns its own keydown
    if (shortcutsSheet && !shortcutsSheet.hidden) {
      if (e.key === 'Escape') { e.preventDefault(); closeShortcuts(); }
      return;
    }
    const focusOverlay = document.getElementById('focus-session');
    if (focusOverlay && !focusOverlay.hidden) {
      if (e.key === 'Escape') { e.preventDefault(); window.NookEndFocusSession && window.NookEndFocusSession(); }
      return;
    }

    if (e.key === 'Escape') { return; }

    if (isTypingContext(e)) return;

    if (awaitingG) {
      awaitingG = false;
      clearTimeout(gTimeout);
      if (e.key.toLowerCase() === 'c') { e.preventDefault(); window.location.href = '/calendar'; return; }
      if (e.key.toLowerCase() === 's') { e.preventDefault(); window.location.href = '/spaces'; return; }
      return;
    }

    if (e.key === 'g' || e.key === 'G') {
      awaitingG = true;
      gTimeout = setTimeout(() => { awaitingG = false; }, 800);
      return;
    }
    if (e.key === 'n' || e.key === 'N') { e.preventDefault(); triggerNewTask(); return; }
    if (e.key === '/') { e.preventDefault(); openPalette(); return; }
    if (e.key === '?') { e.preventDefault(); openShortcuts(); return; }
  });

  // -------------------------------------------------------------------------
  // Nav active-state (works across pages without per-template logic)
  // -------------------------------------------------------------------------
  document.querySelectorAll('.primary-nav a').forEach((a) => {
    const href = a.getAttribute('href');
    const match = href === '/' ? location.pathname === '/' : location.pathname.startsWith(href);
    a.classList.toggle('is-active', match);
    if (match) a.setAttribute('aria-current', 'page'); else a.removeAttribute('aria-current');
  });

  // -------------------------------------------------------------------------
  // Getting Started dismiss
  // -------------------------------------------------------------------------
  document.body.addEventListener('click', (e) => {
    const btn = e.target.closest('[data-dismiss-onboarding]');
    if (!btn) return;
    const card = btn.closest('[data-card]');
    fetch('/onboarding/dismiss', { method: 'POST' }).then(() => { if (card) card.remove(); });
  });

  // -------------------------------------------------------------------------
  // Optimistic task completion — Due Soon's own checkboxes, plus the ones
  // enhanceUpNextCheckboxes() adds to matching Up Next rows.
  // -------------------------------------------------------------------------
  document.body.addEventListener('change', (e) => {
    const cb = e.target.closest('.task-checkbox');
    if (!cb || !cb.checked) return;
    const row = cb.closest('.due-row, .timeline-item');
    if (!row) return;
    const taskId = cb.dataset.taskId;
    const title = (row.querySelector('.due-row-title, .timeline-title') || {}).textContent || 'Task';
    completeTaskOptimistic(taskId, row, title);
  });

  async function completeTaskOptimistic(taskId, row, title) {
    row.classList.add('is-completing');
    announce('Completed ' + title);
    const originCard = (row.closest('[data-card]') || {}).dataset;
    const originKey = originCard && originCard.card;
    try {
      const res = await fetch('/tasks/' + taskId + '/complete-toggle', { method: 'POST' });
      if (!res.ok) throw new Error('request failed');
      const collapse = () => {
        if (reduceMotion) { row.remove(); return; }
        row.style.overflow = 'hidden';
        row.style.height = row.offsetHeight + 'px';
        requestAnimationFrame(() => {
          row.style.transition = 'height 200ms ease-out, opacity 200ms ease-out, padding 200ms ease-out, margin 200ms ease-out';
          row.style.height = '0';
          row.style.opacity = '0';
          row.style.paddingTop = '0';
          row.style.paddingBottom = '0';
          row.style.marginTop = '0';
          row.style.marginBottom = '0';
        });
        setTimeout(() => row.remove(), 220);
      };
      setTimeout(collapse, reduceMotion ? 0 : 350);
      showToast('Completed "' + title + '"', { actionLabel: 'Undo', duration: 5000, onAction: () => undoComplete(taskId) });
      // A task due today can show up in both Due Soon and Up Next. The card
      // the click came from already updated itself locally above, so only
      // reload the *other* one — reloading due_soon unconditionally would
      // wipe anything the user was mid-typing into its quick-add input.
      if (originKey !== 'due_soon') reloadCard('due_soon');
      if (originKey !== 'up_next') reloadCard('up_next');
      reloadCard('momentum');
      reloadCard('space_progress');
    } catch (err) {
      row.classList.remove('is-completing');
      const cbEl = row.querySelector('.task-checkbox');
      if (cbEl) cbEl.checked = false;
      showToast("Couldn't complete that task — try again.", { type: 'error' });
    }
  }

  async function undoComplete(taskId) {
    try {
      await fetch('/tasks/' + taskId + '/complete-toggle', { method: 'POST' });
      reloadCard('due_soon');
      reloadCard('momentum');
      reloadCard('space_progress');
      showToast('Task restored');
    } catch (err) {
      showToast("Couldn't undo — refresh to check.", { type: 'error' });
    }
  }

  // -------------------------------------------------------------------------
  // Quick add — natural language parsing + correctable preview chips
  // -------------------------------------------------------------------------
  const SPACES = (() => {
    const el = document.getElementById('spaces-data');
    try { return el ? JSON.parse(el.textContent) : []; } catch (e) { return []; }
  })();
  const WEEKDAYS = ['sunday', 'monday', 'tuesday', 'wednesday', 'thursday', 'friday', 'saturday'];

  function parseQuickAdd(raw) {
    let text = raw;
    const result = { title: '', priority: 'normal', dueDate: null, hasTime: false, spaceId: null, spaceName: null, spaceColor: null };

    const spaceMatch = text.match(/#(\w+)/);
    if (spaceMatch) {
      const name = spaceMatch[1].toLowerCase();
      const found = SPACES.find((s) => s.name.toLowerCase().startsWith(name));
      if (found) { result.spaceId = found.id; result.spaceName = found.name; result.spaceColor = found.color; }
      else { result.spaceName = spaceMatch[1]; result.spaceUnmatched = true; }
      text = text.replace(spaceMatch[0], ' ');
    }
    if (!result.spaceId && !result.spaceUnmatched && SPACES.length) {
      result.spaceId = SPACES[0].id;
      result.spaceName = SPACES[0].name;
      result.spaceColor = SPACES[0].color;
      result.spaceIsDefault = true;
    }

    const priMatch = text.match(/\b(low|normal|high)\b/i);
    if (priMatch) { result.priority = priMatch[1].toLowerCase(); text = text.replace(priMatch[0], ' '); }

    const now = new Date();
    let targetDate = null;
    const todayMatch = text.match(/\btoday\b/i);
    const tomorrowMatch = text.match(/\btomorrow\b/i);
    const weekdayMatch = !todayMatch && !tomorrowMatch ? text.match(/\b(sun|mon|tue|wed|thu|fri|sat)\w*\b/i) : null;

    if (todayMatch) { targetDate = new Date(now); text = text.replace(todayMatch[0], ' '); }
    else if (tomorrowMatch) { targetDate = new Date(now); targetDate.setDate(targetDate.getDate() + 1); text = text.replace(tomorrowMatch[0], ' '); }
    else if (weekdayMatch) {
      const idx = WEEKDAYS.findIndex((d) => d.startsWith(weekdayMatch[1].toLowerCase()));
      if (idx >= 0) {
        targetDate = new Date(now);
        let delta = (idx - now.getDay() + 7) % 7;
        if (delta === 0) delta = 7;
        targetDate.setDate(targetDate.getDate() + delta);
        text = text.replace(weekdayMatch[0], ' ');
      }
    }

    const timeMatch = text.match(/\b(\d{1,2})(:(\d{2}))?\s*(am|pm)\b/i);
    let hours = null, minutes = 0;
    if (timeMatch) {
      hours = parseInt(timeMatch[1], 10) % 12;
      minutes = timeMatch[3] ? parseInt(timeMatch[3], 10) : 0;
      if (/pm/i.test(timeMatch[4])) hours += 12;
      text = text.replace(timeMatch[0], ' ');
    }

    if (targetDate || hours !== null) {
      if (!targetDate) targetDate = new Date(now);
      if (hours !== null) { targetDate.setHours(hours, minutes, 0, 0); result.hasTime = true; }
      else { targetDate.setHours(23, 59, 0, 0); }
      result.dueDate = targetDate;
    }

    result.title = text.replace(/\s+/g, ' ').trim();
    return result;
  }

  function formatChipDate(date, hasTime) {
    const opts = { weekday: 'short', month: 'short', day: 'numeric' };
    const datePart = date.toLocaleDateString(undefined, opts);
    if (!hasTime) return datePart;
    const timePart = date.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
    return datePart + ' · ' + timePart;
  }

  const quickAddInput = document.getElementById('quick-add-input');
  const quickAddPreview = document.getElementById('quick-add-preview');
  let currentParsed = null;
  let overrideSpaceId = null;
  let overridePriority = null;
  let dateCleared = false;

  function renderQuickAddPreview() {
    if (!quickAddPreview) return;
    quickAddPreview.innerHTML = '';
    if (!currentParsed || !currentParsed.title) return;

    if (currentParsed.dueDate) {
      const chip = document.createElement('button');
      chip.type = 'button';
      chip.className = 'chip';
      chip.textContent = '📅 ' + formatChipDate(currentParsed.dueDate, currentParsed.hasTime);
      chip.title = 'Click to remove the due date';
      chip.addEventListener('click', () => { dateCleared = true; updateQuickAddPreview(); });
      quickAddPreview.appendChild(chip);
    }
    if (currentParsed.spaceName) {
      const chip = document.createElement('button');
      chip.type = 'button';
      chip.className = 'chip' + (currentParsed.spaceUnmatched ? ' chip-error' : '');
      chip.textContent = '🏷 ' + currentParsed.spaceName + (currentParsed.spaceIsDefault ? ' (default)' : '');
      chip.title = SPACES.length > 1 ? 'Click to try another space' : '';
      chip.addEventListener('click', cycleSpace);
      quickAddPreview.appendChild(chip);
    } else if (!SPACES.length) {
      const chip = document.createElement('button');
      chip.type = 'button';
      chip.className = 'chip chip-error';
      chip.textContent = 'Create a space first';
      chip.addEventListener('click', () => { window.location.href = '/spaces'; });
      quickAddPreview.appendChild(chip);
    }
    const priChip = document.createElement('button');
    priChip.type = 'button';
    priChip.className = 'chip';
    priChip.textContent = '⚑ ' + currentParsed.priority;
    priChip.title = 'Click to change priority';
    priChip.addEventListener('click', cyclePriority);
    quickAddPreview.appendChild(priChip);
  }

  function updateQuickAddPreview() {
    if (!quickAddInput) return;
    const parsed = parseQuickAdd(quickAddInput.value);
    if (overrideSpaceId) {
      const s = SPACES.find((sp) => sp.id === overrideSpaceId);
      if (s) { parsed.spaceId = s.id; parsed.spaceName = s.name; parsed.spaceColor = s.color; parsed.spaceIsDefault = false; parsed.spaceUnmatched = false; }
    }
    if (overridePriority) parsed.priority = overridePriority;
    if (dateCleared) parsed.dueDate = null;
    currentParsed = parsed;
    renderQuickAddPreview();
  }

  function cycleSpace() {
    if (!SPACES.length) return;
    const currentId = (currentParsed && currentParsed.spaceId) || SPACES[0].id;
    const idx = SPACES.findIndex((s) => s.id === currentId);
    overrideSpaceId = SPACES[(idx + 1) % SPACES.length].id;
    updateQuickAddPreview();
  }

  function cyclePriority() {
    const order = ['low', 'normal', 'high'];
    const idx = order.indexOf((currentParsed && currentParsed.priority) || 'normal');
    overridePriority = order[(idx + 1) % order.length];
    updateQuickAddPreview();
  }

  if (quickAddInput) {
    quickAddInput.addEventListener('input', () => {
      overrideSpaceId = null; overridePriority = null; dateCleared = false;
      updateQuickAddPreview();
    });
    quickAddInput.addEventListener('keydown', async (e) => {
      if (e.key !== 'Enter') return;
      e.preventDefault();
      if (!currentParsed || !currentParsed.title) return;
      if (!currentParsed.spaceId) { showToast('Create a space first', { type: 'error' }); return; }

      const body = new URLSearchParams();
      body.set('title', currentParsed.title);
      body.set('priority', currentParsed.priority);
      if (currentParsed.dueDate) {
        const d = currentParsed.dueDate;
        const pad = (n) => String(n).padStart(2, '0');
        body.set('due_at', currentParsed.hasTime
          ? `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`
          : `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`);
      }
      try {
        const res = await fetch('/spaces/' + currentParsed.spaceId + '/tasks', {
          method: 'POST',
          headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
          body,
        });
        if (!res.ok) throw new Error('request failed');
        quickAddInput.value = '';
        overrideSpaceId = null; overridePriority = null; dateCleared = false;
        updateQuickAddPreview();
        showToast('Task added');
        announce('Task added: ' + currentParsed.title);
        reloadCard('due_soon');
        reloadCard('up_next');
        reloadCard('focus');
        reloadCard('getting_started');
      } catch (err) {
        showToast("Couldn't add that task — try again.", { type: 'error' });
      }
    });
  }

  // -------------------------------------------------------------------------
  // Momentum tooltip
  // -------------------------------------------------------------------------
  let momentumTooltip = null;
  function showMomentumTooltip(day) {
    hideMomentumTooltip();
    momentumTooltip = document.createElement('div');
    momentumTooltip.className = 'momentum-tooltip';
    momentumTooltip.textContent = day.dataset.date + ' · ' + day.dataset.count + ' completed';
    document.body.appendChild(momentumTooltip);
    const rect = day.getBoundingClientRect();
    momentumTooltip.style.left = rect.left + rect.width / 2 + 'px';
    momentumTooltip.style.top = rect.top - 6 + 'px';
  }
  function hideMomentumTooltip() { if (momentumTooltip) { momentumTooltip.remove(); momentumTooltip = null; } }

  document.body.addEventListener('mouseover', (e) => { const d = e.target.closest('.momentum-day'); if (d) showMomentumTooltip(d); });
  document.body.addEventListener('mouseout', (e) => { if (e.target.closest('.momentum-day')) hideMomentumTooltip(); });

  // -------------------------------------------------------------------------
  // Drag-to-reorder bento cards (+ Alt+Arrow keyboard alternative)
  // -------------------------------------------------------------------------
  const bento = document.getElementById('bento');
  if (bento) {
    let dragEl = null;

    function persistLayout() {
      const order = Array.from(bento.querySelectorAll('[data-card]')).map((el) => el.dataset.card);
      fetch(bento.dataset.layoutEndpoint || '/api/layout', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ order }),
      }).catch(() => { /* best-effort */ });
    }

    bento.addEventListener('dragstart', (e) => {
      const card = e.target.closest('[data-card]');
      if (!card) return;
      dragEl = card;
      card.classList.add('is-dragging');
      e.dataTransfer.effectAllowed = 'move';
      e.dataTransfer.setData('text/plain', card.dataset.card);
    });
    bento.addEventListener('dragend', () => {
      if (dragEl) dragEl.classList.remove('is-dragging');
      bento.querySelectorAll('.is-drop-target').forEach((el) => el.classList.remove('is-drop-target'));
      dragEl = null;
    });
    bento.addEventListener('dragover', (e) => {
      const card = e.target.closest('[data-card]');
      if (!card || card === dragEl) return;
      e.preventDefault();
      bento.querySelectorAll('.is-drop-target').forEach((el) => el.classList.remove('is-drop-target'));
      card.classList.add('is-drop-target');
    });
    bento.addEventListener('drop', (e) => {
      const target = e.target.closest('[data-card]');
      if (!target || !dragEl || target === dragEl) return;
      e.preventDefault();
      target.classList.remove('is-drop-target');
      const cards = Array.from(bento.querySelectorAll('[data-card]'));
      const dragIndex = cards.indexOf(dragEl);
      const targetIndex = cards.indexOf(target);
      if (dragIndex < targetIndex) target.after(dragEl); else target.before(dragEl);
      persistLayout();
    });
    bento.addEventListener('keydown', (e) => {
      if (!e.altKey) return;
      const card = e.target.closest('[data-card]');
      if (!card) return;
      if (e.key === 'ArrowUp' || e.key === 'ArrowLeft') {
        const prev = card.previousElementSibling;
        if (prev) { e.preventDefault(); card.parentElement.insertBefore(card, prev); card.focus(); persistLayout(); }
      } else if (e.key === 'ArrowDown' || e.key === 'ArrowRight') {
        const next = card.nextElementSibling;
        if (next) { e.preventDefault(); card.parentElement.insertBefore(next, card); card.focus(); persistLayout(); }
      }
    });
  }

  // -------------------------------------------------------------------------
  // Focus session overlay
  // -------------------------------------------------------------------------
  const focusOverlay = document.getElementById('focus-session');
  const focusTitleEl = document.getElementById('focus-session-title');
  const focusSpaceEl = document.getElementById('focus-session-space');
  const focusTimerEl = document.getElementById('focus-session-timer');
  const focusStepsEl = document.getElementById('focus-session-steps');
  const focusMinutesInput = document.getElementById('focus-session-minutes');
  const focusStartBtn = document.getElementById('focus-session-start');
  const focusEndBtn = document.getElementById('focus-session-end');
  let focusState = null;
  let focusLastFocused = null;

  function formatClock(totalSeconds) {
    const s = Math.max(0, totalSeconds);
    const m = Math.floor(s / 60);
    const sec = s % 60;
    return String(m).padStart(2, '0') + ':' + String(sec).padStart(2, '0');
  }

  async function openFocusSession(taskId, title) {
    if (!focusOverlay) return;
    focusLastFocused = document.activeElement;
    focusState = { taskId: taskId || null, title, totalSeconds: 0, remaining: 0, intervalId: null };
    focusTitleEl.textContent = title;
    focusSpaceEl.textContent = taskId ? 'Focused work' : 'Freeform timer';
    focusTimerEl.textContent = formatClock(25 * 60);
    focusMinutesInput.value = 25;
    focusMinutesInput.disabled = false;
    focusStepsEl.innerHTML = '';
    focusStartBtn.hidden = false;
    focusEndBtn.hidden = true;
    focusOverlay.hidden = false;
    focusStartBtn.focus();

    if (taskId) {
      try {
        const res = await fetch('/tasks/' + taskId + '/plan-steps.json');
        if (res.ok) {
          const steps = await res.json();
          focusStepsEl.innerHTML = steps.length
            ? steps.map((s) => '<li>' + (s.done ? '✓' : '○') + ' ' + escapeHtml(s.text) + '</li>').join('')
            : '<li class="muted">No plan steps yet.</li>';
        }
      } catch (e) { /* plan steps are optional */ }
    }
  }

  function startFocusTimer() {
    const minutes = Math.max(1, Math.min(180, parseInt(focusMinutesInput.value, 10) || 25));
    focusState.totalSeconds = minutes * 60;
    focusState.remaining = minutes * 60;
    focusStartBtn.hidden = true;
    focusMinutesInput.disabled = true;
    focusEndBtn.hidden = false;
    focusEndBtn.focus();
    focusTimerEl.textContent = formatClock(focusState.remaining);
    focusState.intervalId = setInterval(() => {
      focusState.remaining -= 1;
      focusTimerEl.textContent = formatClock(focusState.remaining);
      if (focusState.remaining <= 0) completeFocusSession();
    }, 1000);
  }

  async function logFocusMinutes(minutes) {
    if (!focusState.taskId || minutes < 1) return;
    try {
      await fetch('/tasks/' + focusState.taskId + '/log-minutes', {
        method: 'POST',
        headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
        body: 'minutes=' + minutes,
      });
      showToast('Logged ' + minutes + ' min on "' + focusState.title + '"');
      reloadCard('focus');
    } catch (e) {
      showToast('Could not log those focus minutes.', { type: 'error' });
    }
  }

  function completeFocusSession() {
    clearInterval(focusState.intervalId);
    const minutes = Math.round(focusState.totalSeconds / 60);
    if (focusState.taskId) logFocusMinutes(minutes); else showToast('Focus session complete');
    closeFocusSessionOverlay();
  }

  function endFocusSession() {
    if (!focusState) return;
    clearInterval(focusState.intervalId);
    if (focusState.totalSeconds > 0) {
      const elapsed = focusState.totalSeconds - Math.max(0, focusState.remaining);
      const minutes = Math.round(elapsed / 60);
      if (minutes >= 1) logFocusMinutes(minutes);
    }
    closeFocusSessionOverlay();
  }
  window.NookEndFocusSession = endFocusSession;

  function closeFocusSessionOverlay() {
    if (focusState && focusState.intervalId) clearInterval(focusState.intervalId);
    if (focusOverlay) focusOverlay.hidden = true;
    if (focusMinutesInput) focusMinutesInput.disabled = false;
    focusState = null;
    if (focusLastFocused && focusLastFocused.focus) focusLastFocused.focus();
    focusLastFocused = null;
  }

  document.body.addEventListener('click', (e) => {
    const trigger = e.target.closest('[data-focus-start]');
    if (!trigger) return;
    openFocusSession(trigger.dataset.taskId || null, trigger.dataset.title || 'Focus session');
  });
  if (focusStartBtn) focusStartBtn.addEventListener('click', startFocusTimer);
  if (focusEndBtn) focusEndBtn.addEventListener('click', endFocusSession);

  // ===========================================================================
  // 2026 redesign — progressive-enhancement layer.
  //
  // This app has no JS build step and (on this machine) no Rust toolchain to
  // recompile the Askama templates the server renders, so the redesign below
  // runs entirely client-side: it reshapes the already-rendered HTML on load
  // (and again after every htmx card swap) rather than requiring template or
  // backend changes. Every piece degrades harmlessly if its expected markup
  // isn't present, so this is safe to run on every page.
  // ===========================================================================

  // -------------------------------------------------------------------------
  // Generic delegated affordances used across the redesign.
  // -------------------------------------------------------------------------
  document.body.addEventListener('click', (e) => {
    const soon = e.target.closest('[data-coming-soon]');
    if (soon) { e.preventDefault(); showToast(soon.dataset.comingSoon, { type: 'error' }); return; }

    const focusTarget = e.target.closest('[data-focus-target]');
    if (focusTarget) {
      const el = document.getElementById(focusTarget.dataset.focusTarget);
      if (el && el.focus) el.focus();
    }
  });

  document.querySelectorAll('.mode-choice').forEach((f) => f.classList.add('segmented'));

  // -------------------------------------------------------------------------
  // Auth view — OAuth entry points + "Forgot password?" next to the label.
  // (Auto-focus on the email field is already handled server-side via the
  // `autofocus` attribute in login.html.)
  // -------------------------------------------------------------------------
  function svgGoogleMark() {
    return '<svg width="18" height="18" viewBox="0 0 18 18" aria-hidden="true">'
      + '<path fill="#4285F4" d="M17.64 9.2c0-.64-.06-1.25-.16-1.84H9v3.48h4.84a4.14 4.14 0 0 1-1.8 2.72v2.26h2.9c1.7-1.57 2.7-3.88 2.7-6.62z"/>'
      + '<path fill="#34A853" d="M9 18c2.43 0 4.47-.8 5.96-2.18l-2.9-2.26c-.8.54-1.84.86-3.06.86-2.35 0-4.34-1.59-5.05-3.72H.96v2.33A9 9 0 0 0 9 18z"/>'
      + '<path fill="#FBBC05" d="M3.95 10.7A5.4 5.4 0 0 1 3.67 9c0-.59.1-1.17.28-1.7V4.96H.96A9 9 0 0 0 0 9c0 1.45.35 2.83.96 4.04l2.99-2.34z"/>'
      + '<path fill="#EA4335" d="M9 3.58c1.32 0 2.51.46 3.44 1.35l2.58-2.58C13.46.89 11.43 0 9 0A9 9 0 0 0 .96 4.96l2.99 2.34C4.66 5.17 6.65 3.58 9 3.58z"/>'
      + '</svg>';
  }
  function svgGithubMark() {
    return '<svg width="18" height="18" viewBox="0 0 16 16" aria-hidden="true" fill="currentColor">'
      + '<path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8z"/>'
      + '</svg>';
  }

  function enhanceAuthForms() {
    document.querySelectorAll('.auth-card form.stack').forEach((form) => {
      if (form.dataset.enhanced) return;
      form.dataset.enhanced = '1';

      // Builds backed by real OAuth (Supabase) already server-render a
      // `.oauth-group` + `.auth-divider` pair ahead of the form, with real
      // /auth/oauth/... links — don't cover those with fake "coming soon"
      // placeholders. Only inject placeholders when the server didn't.
      if (!form.parentElement.querySelector('.oauth-group')) {
        const group = document.createElement('div');
        group.className = 'oauth-group';
        group.innerHTML =
          '<button type="button" class="btn-oauth" data-coming-soon="Google sign-in isn\'t connected yet.">' + svgGoogleMark() + '<span>Continue with Google</span></button>'
          + '<button type="button" class="btn-oauth" data-coming-soon="GitHub sign-in isn\'t connected yet.">' + svgGithubMark() + '<span>Continue with GitHub</span></button>';
        const divider = document.createElement('div');
        divider.className = 'auth-divider';
        divider.innerHTML = '<span>or continue with email</span>';
        form.before(group, divider);
      }

      // "Forgot password?" next to the Password label — login only; a fresh
      // signup form has no password to have forgotten yet.
      if (form.getAttribute('action') === '/login') {
        const pwInput = form.querySelector('input[type="password"]');
        const label = pwInput && pwInput.closest('label');
        const labelText = label && label.firstChild;
        if (label && labelText && labelText.nodeType === Node.TEXT_NODE) {
          const row = document.createElement('span');
          row.className = 'label-row';
          const span = document.createElement('span');
          span.textContent = labelText.textContent.trim();
          const link = document.createElement('button');
          link.type = 'button';
          link.className = 'forgot-link';
          link.textContent = 'Forgot password?';
          link.dataset.comingSoon = "Password reset isn't set up yet — contact support.";
          row.append(span, link);
          labelText.remove();
          label.insertBefore(row, pwInput);
        }
      }
    });
  }

  // -------------------------------------------------------------------------
  // Auth split-screen — rebuilds Login/Signup into a 50/50 branded-hero /
  // form layout. Must run *after* enhanceAuthForms(), since it relocates
  // the already-enhanced `.auth-card` (OAuth buttons, divider, forgot-
  // password already injected) into the right column rather than
  // reconstructing it.
  // -------------------------------------------------------------------------
  function enhanceAuthSplitScreen() {
    const card = document.querySelector('.auth-card');
    const main = document.querySelector('body > main.page');
    if (!card || !main || document.querySelector('.auth-split')) return;

    const h1 = card.querySelector('h1');
    // Literal ask: "Welcome back" -> "Welcome" — login only; signup's own
    // heading ("Create your Nook") isn't the described text, so left as-is.
    if (h1 && h1.textContent.trim() === 'Welcome back') h1.textContent = 'Welcome';
    if (h1 && !card.querySelector('.auth-split-subtext')) {
      const subtext = document.createElement('p');
      subtext.className = 'auth-split-subtext';
      subtext.textContent = 'Sign in to your account or get started below.';
      h1.after(subtext);
    }

    const left = document.createElement('div');
    left.className = 'auth-split-left';
    const featureItems = ['Custom workspaces & task tracking', 'Daily focus and calendar views', 'Effortless team collaboration'];
    left.innerHTML =
      '<div class="auth-split-logo"><span class="brand-mark" aria-hidden="true"></span>Nook</div>'
      + '<p class="auth-split-headline">Organize your work and life with Nook.</p>'
      + '<p class="auth-split-subtitle">Your clean, unified workspace for tasks, spaces, and focus routines.</p>'
      + '<ul class="auth-split-features">'
      + featureItems.map((f) => '<li><span class="auth-split-check" aria-hidden="true">✓</span>' + escapeHtml(f) + '</li>').join('')
      + '</ul>';

    const right = document.createElement('div');
    right.className = 'auth-split-right';

    const split = document.createElement('div');
    split.className = 'auth-split';
    main.before(split);
    right.appendChild(card); // moves the real, already-enhanced auth-card
    split.append(left, right);
    main.remove();

    const topbar = document.querySelector('body > .topbar');
    if (topbar) topbar.remove();
  }

  // -------------------------------------------------------------------------
  // Actionable empty states — a message alone is a dead end; add a CTA that
  // jumps straight to the relevant add-form. Re-run after every htmx swap
  // (see the listener near the bottom) since Today's cards reload in place.
  // -------------------------------------------------------------------------
  function makeCTA(label, opts) {
    opts = opts || {};
    const el = document.createElement(opts.href ? 'a' : 'button');
    el.className = 'btn-secondary';
    el.textContent = label;
    if (opts.href) { el.href = opts.href; }
    else {
      el.type = 'button';
      if (opts.focusTarget) el.dataset.focusTarget = opts.focusTarget;
    }
    return el;
  }

  function enhanceEmptyStates() {
    // Today cards with an existing add-surface elsewhere on the page.
    [
      { sel: '[data-card="due_soon"] .empty-state', label: '+ Add task', focusTarget: 'quick-add-input' },
      { sel: '[data-card="up_next"] .empty-state', label: '+ Add a block', href: '/calendar' },
      { sel: '[data-card="momentum"] .empty-state', label: '+ Add a task', focusTarget: 'quick-add-input' },
    ].forEach(({ sel, label, focusTarget, href }) => {
      const el = document.querySelector(sel);
      if (!el || el.dataset.ctaAdded) return;
      if (el.nextElementSibling && /\bbtn-(primary|secondary)\b/.test(el.nextElementSibling.className)) return;
      el.dataset.ctaAdded = '1';
      el.parentElement.classList.add('empty-state-inline');
      el.after(makeCTA(label, { focusTarget, href }));
    });

    // Calendar — both list empty states point back at the block-title field.
    const titleInput = document.querySelector('.schedule-form input[name="title"]');
    if (titleInput) {
      if (!titleInput.id) titleInput.id = 'block-title-input';
      document.querySelectorAll('.card .empty-state').forEach((el) => {
        if (el.closest('[data-card]') || el.dataset.ctaAdded) return;
        el.dataset.ctaAdded = '1';
        el.parentElement.classList.add('empty-state-inline');
        el.after(makeCTA('+ Add a block', { focusTarget: 'block-title-input' }));
      });
    }

    // Dashboard — "No spaces yet".
    const noSpaces = document.querySelector('.page > p.muted');
    if (noSpaces && !noSpaces.dataset.ctaAdded && /^No spaces yet/.test(noSpaces.textContent.trim())) {
      noSpaces.dataset.ctaAdded = '1';
      const nameInput = document.querySelector('.inline-form input[name="name"]');
      if (nameInput && !nameInput.id) nameInput.id = 'new-space-name';
      const block = document.createElement('div');
      block.className = 'empty-state-block';
      const icon = document.createElement('span');
      icon.className = 'empty-state-icon';
      icon.setAttribute('aria-hidden', 'true');
      icon.textContent = '🗂️';
      const p = document.createElement('p');
      p.textContent = 'No spaces yet — create one to start organizing your tasks.';
      block.append(icon, p, makeCTA('+ Create space', { focusTarget: nameInput ? nameInput.id : null }));
      noSpaces.replaceWith(block);
    }

    // Space detail — "No tasks yet".
    document.querySelectorAll('li.no-tasks.muted').forEach((li) => {
      if (li.dataset.ctaAdded || !/^No tasks yet/.test(li.textContent.trim())) return;
      li.dataset.ctaAdded = '1';
      const titleInput = document.querySelector('.task-form input[name="title"]');
      if (titleInput && !titleInput.id) titleInput.id = 'new-task-title';
      li.classList.add('empty-state-block');
      li.textContent = '';
      const icon = document.createElement('span');
      icon.className = 'empty-state-icon';
      icon.setAttribute('aria-hidden', 'true');
      icon.textContent = '✅';
      const p = document.createElement('p');
      p.textContent = 'No tasks yet — add your first one above.';
      li.append(icon, p, makeCTA('+ Add task', { focusTarget: titleInput ? titleInput.id : null }));
    });
  }

  // -------------------------------------------------------------------------
  // Custom date / time pickers — a styled trigger + popover layered over the
  // real native input, which stays in the DOM (visually hidden) so the form
  // still submits the exact `YYYY-MM-DD` / `HH:MM` values the backend parses.
  // -------------------------------------------------------------------------
  function pad2(n) { return String(n).padStart(2, '0'); }

  function formatDateDisplay(iso) {
    const [y, m, d] = iso.split('-').map(Number);
    return new Date(y, m - 1, d).toLocaleDateString(undefined, { weekday: 'short', month: 'short', day: 'numeric', year: 'numeric' });
  }
  function formatTimeDisplay(hhmm) {
    const [h, m] = hhmm.split(':').map(Number);
    return new Date(2000, 0, 1, h, m).toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
  }

  function closeAllPickers(except) {
    document.querySelectorAll('.picker-popover:not([hidden])').forEach((p) => {
      if (p === except) return;
      p.hidden = true;
      const trigger = p.previousElementSibling;
      if (trigger) trigger.setAttribute('aria-expanded', 'false');
    });
  }
  document.addEventListener('click', (e) => { if (!e.target.closest('.field-picker')) closeAllPickers(); });
  document.addEventListener('keydown', (e) => {
    if (e.key !== 'Escape') return;
    const open = document.querySelector('.picker-popover:not([hidden])');
    if (!open) return;
    const trigger = open.previousElementSibling;
    open.hidden = true;
    if (trigger) { trigger.setAttribute('aria-expanded', 'false'); trigger.focus(); }
  });

  function buildPickerShell(input) {
    const wrap = document.createElement('div');
    wrap.className = 'field-picker';
    input.before(wrap);
    wrap.appendChild(input);
    input.tabIndex = -1;
    input.setAttribute('aria-hidden', 'true');

    const trigger = document.createElement('button');
    trigger.type = 'button';
    trigger.className = 'picker-trigger';
    trigger.setAttribute('aria-haspopup', 'dialog');
    trigger.setAttribute('aria-expanded', 'false');
    wrap.appendChild(trigger);

    const popover = document.createElement('div');
    popover.className = 'picker-popover';
    popover.hidden = true;
    wrap.appendChild(popover);

    return { trigger, popover };
  }

  function enhanceDateField(input) {
    if (input.dataset.pickerEnhanced) return;
    input.dataset.pickerEnhanced = '1';
    const { trigger, popover } = buildPickerShell(input);
    trigger.innerHTML =
      '<span class="picker-value is-placeholder">Select date</span>'
      + '<svg class="picker-icon" width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">'
      + '<rect x="2" y="3" width="12" height="11" rx="2" stroke="currentColor" stroke-width="1.3"/>'
      + '<path d="M2 6.5h12M5 1.5v3M11 1.5v3" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/></svg>';
    const valueEl = trigger.querySelector('.picker-value');
    let viewDate = new Date();

    function selectedDate() {
      if (!input.value) return null;
      const [y, m, d] = input.value.split('-').map(Number);
      return new Date(y, m - 1, d);
    }

    function renderCalendar() {
      const sel = selectedDate();
      const today = new Date(); today.setHours(0, 0, 0, 0);
      const y = viewDate.getFullYear(), m = viewDate.getMonth();
      const startOffset = new Date(y, m, 1).getDay();
      const daysInMonth = new Date(y, m + 1, 0).getDate();
      const monthLabel = viewDate.toLocaleDateString(undefined, { month: 'long', year: 'numeric' });

      let html = '<div class="picker-cal-head">'
        + '<button type="button" data-cal-prev aria-label="Previous month">‹</button>'
        + '<span>' + monthLabel + '</span>'
        + '<button type="button" data-cal-next aria-label="Next month">›</button>'
        + '</div><div class="picker-cal-grid">';
      ['S', 'M', 'T', 'W', 'T', 'F', 'S'].forEach((d) => { html += '<span class="picker-cal-dow">' + d + '</span>'; });
      for (let i = 0; i < startOffset; i++) html += '<span></span>';
      for (let day = 1; day <= daysInMonth; day++) {
        const cellDate = new Date(y, m, day);
        const classes = ['picker-cal-day'];
        if (cellDate.getTime() === today.getTime()) classes.push('is-today');
        if (sel && cellDate.getTime() === sel.getTime()) classes.push('is-selected');
        html += '<button type="button" class="' + classes.join(' ') + '" data-cal-day="' + day + '">' + day + '</button>';
      }
      html += '</div>';
      popover.innerHTML = html;

      popover.querySelector('[data-cal-prev]').addEventListener('click', () => { viewDate = new Date(y, m - 1, 1); renderCalendar(); });
      popover.querySelector('[data-cal-next]').addEventListener('click', () => { viewDate = new Date(y, m + 1, 1); renderCalendar(); });
      popover.querySelectorAll('[data-cal-day]').forEach((btn) => {
        btn.addEventListener('click', () => {
          const picked = new Date(y, m, Number(btn.dataset.calDay));
          input.value = picked.getFullYear() + '-' + pad2(picked.getMonth() + 1) + '-' + pad2(picked.getDate());
          input.dispatchEvent(new Event('change', { bubbles: true }));
          valueEl.textContent = formatDateDisplay(input.value);
          valueEl.classList.remove('is-placeholder');
          closePopover();
        });
      });
    }

    function openPopover() {
      closeAllPickers(popover);
      viewDate = selectedDate() || new Date();
      renderCalendar();
      popover.hidden = false;
      trigger.setAttribute('aria-expanded', 'true');
      const target = popover.querySelector('.is-selected') || popover.querySelector('.is-today') || popover.querySelector('[data-cal-day]');
      if (target) target.focus();
    }
    function closePopover() { popover.hidden = true; trigger.setAttribute('aria-expanded', 'false'); }

    trigger.addEventListener('click', () => { (popover.hidden ? openPopover : closePopover)(); });
    if (input.value) { valueEl.textContent = formatDateDisplay(input.value); valueEl.classList.remove('is-placeholder'); }
  }

  function enhanceTimeField(input) {
    if (input.dataset.pickerEnhanced) return;
    input.dataset.pickerEnhanced = '1';
    const { trigger, popover } = buildPickerShell(input);
    trigger.innerHTML =
      '<span class="picker-value is-placeholder">Select time</span>'
      + '<svg class="picker-icon" width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">'
      + '<circle cx="8" cy="8" r="6.3" stroke="currentColor" stroke-width="1.3"/>'
      + '<path d="M8 4.5V8l2.6 1.5" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/></svg>';
    const valueEl = trigger.querySelector('.picker-value');

    const options = [];
    for (let h = 0; h < 24; h++) { for (let m = 0; m < 60; m += 15) options.push(pad2(h) + ':' + pad2(m)); }

    function renderList() {
      const list = document.createElement('div');
      list.className = 'picker-time-list';
      options.forEach((opt) => {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = 'picker-time-opt' + (opt === input.value ? ' is-selected' : '');
        btn.textContent = formatTimeDisplay(opt);
        btn.dataset.time = opt;
        btn.addEventListener('click', () => {
          input.value = opt;
          input.dispatchEvent(new Event('change', { bubbles: true }));
          valueEl.textContent = formatTimeDisplay(opt);
          valueEl.classList.remove('is-placeholder');
          closePopover();
        });
        list.appendChild(btn);
      });
      popover.innerHTML = '';
      popover.appendChild(list);
    }

    function openPopover() {
      closeAllPickers(popover);
      renderList();
      popover.hidden = false;
      trigger.setAttribute('aria-expanded', 'true');
      const sel = popover.querySelector('.is-selected') || popover.querySelector('.picker-time-opt');
      if (sel) { sel.focus(); if (sel.scrollIntoView) sel.scrollIntoView({ block: 'center' }); }
    }
    function closePopover() { popover.hidden = true; trigger.setAttribute('aria-expanded', 'false'); }

    trigger.addEventListener('click', () => { (popover.hidden ? openPopover : closePopover)(); });
    if (input.value) { valueEl.textContent = formatTimeDisplay(input.value); valueEl.classList.remove('is-placeholder'); }
  }

  // -------------------------------------------------------------------------
  // Calendar page — 2-column layout (form / visual schedule) + a live weekly
  // timetable built from the already-rendered block lists, plus a fix for a
  // pre-existing gap: selecting "One time, on a date" never revealed the
  // Date field (no `data-schedule-kind`/`data-schedule-field` wiring existed).
  // -------------------------------------------------------------------------
  function setupScheduleKindToggle(form) {
    const radios = form.querySelectorAll('[data-schedule-kind]');
    const fields = form.querySelectorAll('[data-schedule-field]');
    if (!radios.length || !fields.length) return;
    function sync() {
      const checked = form.querySelector('[data-schedule-kind]:checked');
      const kind = checked && checked.dataset.scheduleKind;
      fields.forEach((f) => { f.hidden = f.dataset.scheduleField !== kind; });
    }
    radios.forEach((r) => r.addEventListener('change', sync));
    sync();
  }

  function parseTimeRange(text) {
    const m = text.match(/(\d{1,2}):(\d{2})\s*(AM|PM)\s*[–-]\s*(\d{1,2}):(\d{2})\s*(AM|PM)/i);
    if (!m) return null;
    const to24 = (h, mm, ap) => { h = Number(h) % 12; if (/pm/i.test(ap)) h += 12; return h + Number(mm) / 60; };
    return { start: to24(m[1], m[2], m[3]), end: to24(m[4], m[5], m[6]) };
  }

  function resolveOneOffDate(label) {
    const now = new Date();
    let d = new Date(label + ', ' + now.getFullYear());
    if (isNaN(d.getTime())) return null;
    if (d < new Date(now.getFullYear(), now.getMonth(), now.getDate())) d = new Date(label + ', ' + (now.getFullYear() + 1));
    return d;
  }

  function renderWeekGrid() {
    const grid = document.getElementById('week-grid');
    if (!grid) return;
    const DOW_SHORT = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
    const DOW_FULL = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];
    const startHour = 6, endHour = 22;
    const slots = (endHour - startHour) * 4; // 15-minute rows

    const blocks = [];
    document.querySelectorAll('.schedule-list li').forEach((li) => {
      const spans = li.querySelectorAll('span');
      if (spans.length < 3) return;
      const dayText = spans[0].textContent.trim();
      const title = spans[1].textContent.trim();
      const range = parseTimeRange(spans[2].textContent.trim());
      if (!range) return;
      let dow = DOW_FULL.indexOf(dayText);
      if (dow < 0) {
        const resolved = resolveOneOffDate(dayText);
        if (!resolved) return;
        const diffDays = Math.floor((resolved - new Date(new Date().toDateString())) / 86400000);
        if (diffDays < 0 || diffDays > 6) return; // only this week
        dow = resolved.getDay();
      }
      blocks.push({ dow, title, start: range.start, end: range.end });
    });

    if (!blocks.length) {
      grid.parentElement.innerHTML = '<p class="week-grid-empty">Add a block and your week will take shape here.</p>';
      return;
    }

    const now = new Date();
    const todayDow = now.getDay();
    const nowHour = now.getHours() + now.getMinutes() / 60;

    grid.style.gridTemplateRows = '28px repeat(' + slots + ', 9px)';
    let html = '<div class="week-grid-cell-head" style="grid-column:1;grid-row:1;"></div>';
    DOW_SHORT.forEach((d, i) => {
      const isToday = i === todayDow ? ' is-today-col' : '';
      html += '<div class="week-grid-cell-head' + isToday + '" style="grid-column:' + (i + 2) + ';grid-row:1;">' + d + '</div>';
    });

    for (let s = 0; s <= slots; s += 4) {
      const h = startHour + s / 4;
      const label = h === 0 ? '12 AM' : h < 12 ? h + ' AM' : h === 12 ? '12 PM' : (h - 12) + ' PM';
      html += '<div class="week-grid-time" style="grid-column:1;grid-row:' + (s + 2) + ' / span 4;">' + label + '</div>';
    }
    for (let d = 0; d < 7; d++) {
      const isToday = d === todayDow ? ' is-today-col' : '';
      html += '<div class="week-grid-cell' + isToday + '" style="grid-column:' + (d + 2) + ';grid-row:2 / span ' + slots + ';"></div>';
    }

    const colorNames = ['clay', 'sage', 'slate', 'amber', 'plum', 'teal', 'rust', 'denim'];
    blocks.forEach((b, i) => {
      const startSlot = Math.max(0, Math.round((b.start - startHour) * 4));
      const endSlot = Math.min(slots, Math.round((b.end - startHour) * 4));
      if (endSlot <= startSlot) return;
      const color = colorNames[i % colorNames.length];
      html += '<div class="week-block" style="--block-color: var(--' + color + '); grid-column:' + (b.dow + 2) + ';grid-row:' + (startSlot + 2) + ' / ' + (endSlot + 2) + ';" title="' + escapeHtml(b.title) + '">' + escapeHtml(b.title) + '</div>';
    });

    // "Now" line — only meaningful while the current time falls inside the displayed window.
    if (nowHour >= startHour && nowHour <= endHour) {
      const nowSlot = Math.round((nowHour - startHour) * 4);
      html += '<div class="week-grid-now" style="grid-column:2 / 9;grid-row:' + (nowSlot + 2) + ';align-self:start;" aria-hidden="true"></div>';
    }

    grid.innerHTML = html;
  }

  function enhanceCalendarPage() {
    const scheduleForm = document.querySelector('.schedule-form');
    if (!scheduleForm || document.querySelector('.calendar-layout')) return;

    const main = document.querySelector('main.page');
    const cards = Array.from(main.querySelectorAll(':scope > section.card'));
    const formCard = cards.find((c) => c.contains(scheduleForm));
    if (!formCard) return;
    const otherCards = cards.filter((c) => c !== formCard);

    const anchor = document.createComment('calendar-layout');
    main.insertBefore(anchor, cards[0]);

    const layout = document.createElement('div');
    layout.className = 'calendar-layout';
    const leftCol = document.createElement('div');
    leftCol.className = 'calendar-col-form';
    const rightCol = document.createElement('div');
    rightCol.className = 'calendar-col-schedule';

    const gridSection = document.createElement('section');
    gridSection.className = 'card';
    gridSection.innerHTML = '<h2 class="card-label">This week</h2><div class="card-body"><div class="week-grid" id="week-grid" aria-label="Weekly schedule preview"></div></div>';
    rightCol.appendChild(gridSection);
    otherCards.forEach((c) => rightCol.appendChild(c));
    leftCol.appendChild(formCard);

    layout.append(leftCol, rightCol);
    anchor.replaceWith(layout);

    setupScheduleKindToggle(scheduleForm);
    const dateInput = scheduleForm.querySelector('input[type="date"]');
    if (dateInput) enhanceDateField(dateInput);
    scheduleForm.querySelectorAll('input[type="time"]').forEach(enhanceTimeField);
    renderWeekGrid();
  }

  // -------------------------------------------------------------------------
  // Spaces grid — icon badges + a hover/focus-revealed action menu (Edit,
  // Archive, Delete) in place of the always-on text links, plus an inline
  // rename+recolor form. The real Archive/Delete <form>s already in the DOM
  // are moved (not rebuilt) into the menu so their POST wiring is untouched;
  // Edit calls the same `/spaces/:id/edit` endpoint the space-detail page's
  // "Edit space" panel already uses.
  //
  // The true per-space `icon` field isn't exposed in the rendered HTML (it's
  // read by the Rust view model but never rendered into a template), so the
  // badge below is a color-keyed emoji stand-in rather than the real icon.
  // -------------------------------------------------------------------------
  const SPACE_COLOR_ICONS = {
    clay: '🧱', sage: '🌿', slate: '🗂️', amber: '⭐', plum: '🎨', teal: '🌊', rust: '🔥', denim: '📘',
  };

  function closeAllSpaceMenus(except) {
    document.querySelectorAll('.space-card-menu:not([hidden])').forEach((m) => {
      if (m === except) return;
      m.hidden = true;
      const trigger = m.previousElementSibling;
      if (trigger && trigger.classList.contains('space-card-menu-trigger')) trigger.setAttribute('aria-expanded', 'false');
    });
  }

  function enhanceSpacesGrid() {
    const cards = document.querySelectorAll('.space-card:not(.is-archived)');
    if (!cards.length) return;
    const colorSelectTemplate = document.querySelector('.inline-form select[name="color"]');

    cards.forEach((card) => {
      if (card.dataset.enhanced) return;
      card.dataset.enhanced = '1';

      const link = card.querySelector('.space-card-link');
      const heading = link && link.querySelector('h2');
      const archiveForm = card.querySelector('form[action$="/archive"]');
      const deleteForm = card.querySelector('form[action$="/delete"]');
      if (!link || !heading || !archiveForm) return;

      const spaceId = (link.getAttribute('href') || '').split('/').pop();
      const colorMatch = Array.from(card.classList).find((c) => c.startsWith('space-'));
      const color = colorMatch ? colorMatch.slice('space-'.length) : '';
      const name = heading.textContent.trim();

      // Icon badge.
      const icon = document.createElement('span');
      icon.className = 'space-card-icon';
      icon.setAttribute('aria-hidden', 'true');
      icon.textContent = SPACE_COLOR_ICONS[color] || '📦';
      link.insertBefore(icon, heading);

      // Kebab trigger + menu, replacing the always-visible actions row.
      const actionsRow = card.querySelector('.space-card-actions');
      const trigger = document.createElement('button');
      trigger.type = 'button';
      trigger.className = 'space-card-menu-trigger';
      trigger.setAttribute('aria-haspopup', 'true');
      trigger.setAttribute('aria-expanded', 'false');
      trigger.setAttribute('aria-label', 'Actions for ' + name);
      trigger.innerHTML = '<svg width="14" height="14" viewBox="0 0 4 16" fill="currentColor" aria-hidden="true"><circle cx="2" cy="2" r="2"/><circle cx="2" cy="8" r="2"/><circle cx="2" cy="14" r="2"/></svg>';

      const menu = document.createElement('div');
      menu.className = 'space-card-menu';
      menu.hidden = true;
      menu.setAttribute('role', 'menu');

      const editBtn = document.createElement('button');
      editBtn.type = 'button';
      editBtn.setAttribute('role', 'menuitem');
      editBtn.textContent = 'Edit';
      menu.appendChild(editBtn);

      if (archiveForm) { archiveForm.setAttribute('role', 'presentation'); menu.appendChild(archiveForm); }
      if (deleteForm) { deleteForm.setAttribute('role', 'presentation'); menu.appendChild(deleteForm); }

      card.appendChild(trigger);
      card.appendChild(menu);
      if (actionsRow) actionsRow.remove();

      function openMenu() { closeAllSpaceMenus(menu); menu.hidden = false; trigger.setAttribute('aria-expanded', 'true'); }
      function closeMenu() { menu.hidden = true; trigger.setAttribute('aria-expanded', 'false'); }
      trigger.addEventListener('click', (e) => { e.preventDefault(); (menu.hidden ? openMenu : closeMenu)(); });

      // Inline edit — a text input + the same color options as "Add space",
      // submitted via fetch so a full page reload isn't needed to see it land.
      let editForm = null;
      editBtn.addEventListener('click', () => {
        closeMenu();
        if (editForm) { editForm.hidden = false; editForm.querySelector('input[name="name"]').focus(); return; }

        editForm = document.createElement('form');
        editForm.className = 'space-card-edit-form';
        editForm.innerHTML = '<label class="visually-hidden" for="edit-name-' + spaceId + '">Space name</label>'
          + '<input type="text" id="edit-name-' + spaceId + '" name="name" value="' + name.replace(/"/g, '&quot;') + '" required>';
        const row = document.createElement('div');
        row.className = 'task-form-row';
        const colorSelect = colorSelectTemplate ? colorSelectTemplate.cloneNode(true) : document.createElement('select');
        colorSelect.name = 'color';
        colorSelect.value = color;
        const cancelBtn = document.createElement('button');
        cancelBtn.type = 'button';
        cancelBtn.className = 'btn-secondary';
        cancelBtn.textContent = 'Cancel';
        const saveBtn = document.createElement('button');
        saveBtn.type = 'submit';
        saveBtn.textContent = 'Save';
        row.append(colorSelect, cancelBtn, saveBtn);
        editForm.appendChild(row);
        card.appendChild(editForm);

        cancelBtn.addEventListener('click', () => { editForm.hidden = true; });
        editForm.addEventListener('submit', async (e) => {
          e.preventDefault();
          saveBtn.disabled = true;
          try {
            const body = new URLSearchParams({ name: editForm.querySelector('input[name="name"]').value, color: colorSelect.value });
            const res = await fetch('/spaces/' + spaceId + '/edit', { method: 'POST', headers: { 'Content-Type': 'application/x-www-form-urlencoded' }, body });
            if (!res.ok) throw new Error('request failed');
            showToast('Space updated');
            window.location.reload();
          } catch (err) {
            saveBtn.disabled = false;
            showToast("Couldn't save that — try again.", { type: 'error' });
          }
        });
        editForm.querySelector('input[name="name"]').focus();
      });
    });
  }

  document.addEventListener('click', (e) => { if (!e.target.closest('.space-card-menu-trigger, .space-card-menu')) closeAllSpaceMenus(); });
  document.addEventListener('keydown', (e) => {
    if (e.key !== 'Escape') return;
    const openMenu = document.querySelector('.space-card-menu:not([hidden])');
    if (openMenu) { closeAllSpaceMenus(); return; }
    const openEdit = document.querySelector('.space-card-edit-form:not([hidden])');
    if (openEdit) openEdit.hidden = true;
  });

  // -------------------------------------------------------------------------
  // App shell — converts the horizontal `.topbar` into the fixed dark/gold
  // left sidebar on authenticated pages, built from the topbar's existing
  // brand/nav/search/user markup (moved, not rebuilt, so the nav's own
  // active-state logic above and the palette trigger keep working
  // untouched). Login/signup have no nav to move, so they're left with the
  // plain (now warm-toned) topbar instead of an empty sidebar shell.
  // -------------------------------------------------------------------------
  function isDarkTheme() {
    const attr = document.documentElement.getAttribute('data-theme');
    if (attr) return attr === 'dark';
    return window.matchMedia('(prefers-color-scheme: dark)').matches;
  }

  // Best-effort source of {id, name, color} for the sidebar's Spaces
  // accordion, using whatever the current page already has on hand — never
  // fabricated. Returns null (not an empty array) when nothing local is
  // found, so the caller knows to fall back to a fetch.
  function collectSpacesFromPage() {
    const dataScript = document.getElementById('spaces-data');
    if (dataScript) {
      try {
        return JSON.parse(dataScript.textContent).map((s) => ({ id: s.id, name: s.name, color: s.color }));
      } catch (e) { /* fall through */ }
    }
    const cards = document.querySelectorAll('.space-grid .space-card:not(.is-archived)');
    if (cards.length) {
      return Array.from(cards).map((card) => {
        const link = card.querySelector('.space-card-link');
        const id = link && (link.getAttribute('href') || '').split('/').pop();
        const heading = card.querySelector('h2');
        const colorClass = Array.from(card.classList).find((c) => c.startsWith('space-'));
        return { id, name: heading ? heading.textContent.trim() : '', color: colorClass ? colorClass.slice(6) : null };
      });
    }
    const options = document.querySelectorAll('.schedule-form select[name="space_id"] option[value]');
    if (options.length) {
      return Array.from(options).filter((o) => o.value).map((o) => ({ id: o.value, name: o.textContent.trim(), color: null }));
    }
    return null;
  }

  function renderSidebarSpacesList(list, spaces) {
    list.innerHTML = '';
    if (!spaces.length) {
      const empty = document.createElement('p');
      empty.className = 'sidebar-spaces-empty';
      empty.textContent = 'No spaces yet';
      list.appendChild(empty);
      return;
    }
    spaces.forEach((s) => {
      const li = document.createElement('li');
      const a = document.createElement('a');
      a.href = '/spaces/' + s.id;
      if (s.color) a.innerHTML = '<span class="dot dot-' + s.color + '" aria-hidden="true"></span>';
      a.append(document.createTextNode(s.name));
      li.appendChild(a);
      list.appendChild(li);
    });
  }

  function buildSidebarSpacesAccordion(nav) {
    const spacesLink = Array.from(nav.querySelectorAll('a')).find((a) => a.getAttribute('href') === '/spaces');
    if (!spacesLink) return;

    const row = document.createElement('div');
    row.className = 'sidebar-spaces-row';
    spacesLink.replaceWith(row);
    row.appendChild(spacesLink);

    const toggle = document.createElement('button');
    toggle.type = 'button';
    toggle.className = 'sidebar-spaces-toggle';
    toggle.setAttribute('aria-expanded', 'false');
    toggle.setAttribute('aria-label', 'Show spaces');
    toggle.innerHTML = '<svg width="12" height="12" viewBox="0 0 12 12" fill="none" aria-hidden="true"><path d="M4 2l4 4-4 4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>';
    row.appendChild(toggle);

    const list = document.createElement('ul');
    list.className = 'sidebar-spaces-list';
    list.hidden = true;
    row.after(list);

    let loaded = false;
    toggle.addEventListener('click', async () => {
      const expanded = toggle.getAttribute('aria-expanded') === 'true';
      toggle.setAttribute('aria-expanded', String(!expanded));
      list.hidden = expanded;
      if (loaded || expanded) return;
      loaded = true;
      const local = collectSpacesFromPage();
      if (local) { renderSidebarSpacesList(list, local); return; }
      try {
        const res = await fetch('/spaces');
        const html = await res.text();
        const doc = new DOMParser().parseFromString(html, 'text/html');
        const remote = Array.from(doc.querySelectorAll('.space-grid .space-card:not(.is-archived)')).map((card) => {
          const link = card.querySelector('.space-card-link');
          const id = link && (link.getAttribute('href') || '').split('/').pop();
          const heading = card.querySelector('h2');
          const colorClass = Array.from(card.classList).find((c) => c.startsWith('space-'));
          return { id, name: heading ? heading.textContent.trim() : '', color: colorClass ? colorClass.slice(6) : null };
        });
        renderSidebarSpacesList(list, remote);
      } catch (e) {
        list.innerHTML = '<p class="sidebar-spaces-empty">Couldn\'t load spaces</p>';
      }
    });
  }

  function enhanceAppShell() {
    const topbar = document.querySelector('body > .topbar');
    const main = document.querySelector('body > main.page');
    if (!topbar || !main || document.querySelector('.app-sidebar')) return;

    const nav = topbar.querySelector('.primary-nav');
    if (!nav) return; // pre-auth pages (login/signup) keep the plain topbar

    const brand = topbar.querySelector('.brand');
    const paletteTrigger = topbar.querySelector('.palette-trigger');
    const navRight = topbar.querySelector('.nav-right');
    const nameEl = navRight && navRight.querySelector('.muted');
    const logoutForm = navRight && navRight.querySelector('form');

    const sidebar = document.createElement('aside');
    sidebar.className = 'app-sidebar';

    if (brand) sidebar.appendChild(brand);

    // "Inbox" and "Upcoming" aren't real routes in this backend (no such
    // pages exist server-side) — shown for the requested nav shape but
    // wired as honest no-ops (a toast) rather than dead links to a 404.
    const todayLink = nav.querySelector('a[href="/"]');
    ['Inbox', 'Upcoming'].forEach((title) => {
      const inert = document.createElement('a');
      inert.href = '#';
      inert.className = 'is-inert';
      inert.textContent = title;
      inert.dataset.comingSoon = title + " isn't available yet.";
      if (todayLink) todayLink.after(inert); else nav.appendChild(inert);
    });

    nav.classList.add('app-sidebar-nav');
    sidebar.appendChild(nav);
    buildSidebarSpacesAccordion(nav);

    if (paletteTrigger) sidebar.appendChild(paletteTrigger);

    const addTaskBtn = document.createElement('button');
    addTaskBtn.type = 'button';
    addTaskBtn.className = 'btn-primary sidebar-add-task';
    addTaskBtn.textContent = '+ Add task';
    addTaskBtn.addEventListener('click', () => {
      const input = document.getElementById('quick-add-input');
      if (input) { input.focus(); return; }
      window.location.href = '/';
    });
    sidebar.appendChild(addTaskBtn);

    const footer = document.createElement('div');
    footer.className = 'app-sidebar-footer';

    if (nameEl) {
      const name = nameEl.textContent.trim();
      const userRow = document.createElement('div');
      userRow.className = 'app-sidebar-user';
      const avatar = document.createElement('span');
      avatar.className = 'app-sidebar-avatar';
      avatar.setAttribute('aria-hidden', 'true');
      avatar.textContent = name.charAt(0).toUpperCase() || '?';
      const nameSpan = document.createElement('span');
      nameSpan.className = 'app-sidebar-user-name';
      nameSpan.textContent = name;
      userRow.append(avatar, nameSpan);
      footer.appendChild(userRow);
    }

    const toggleRow = document.createElement('div');
    toggleRow.className = 'theme-toggle-row';
    const toggleLabel = document.createElement('span');
    toggleLabel.textContent = 'Dark mode';
    const toggleBtn = document.createElement('button');
    toggleBtn.type = 'button';
    toggleBtn.className = 'theme-toggle-switch';
    toggleBtn.setAttribute('role', 'switch');
    toggleBtn.setAttribute('aria-label', 'Toggle dark mode');
    toggleBtn.setAttribute('aria-checked', String(isDarkTheme()));
    toggleBtn.addEventListener('click', () => {
      toggleTheme();
      toggleBtn.setAttribute('aria-checked', String(isDarkTheme()));
    });
    toggleRow.append(toggleLabel, toggleBtn);
    footer.appendChild(toggleRow);

    if (logoutForm) footer.appendChild(logoutForm);
    sidebar.appendChild(footer);

    topbar.remove();

    const content = document.createElement('div');
    content.className = 'app-content';
    main.before(content);
    content.appendChild(main);

    document.body.insertBefore(sidebar, document.body.firstChild);
    document.body.classList.add('has-app-shell');
  }

  // -------------------------------------------------------------------------
  // Today page — hero banner + right-hand activity panel. Up Next, Jump Back
  // In, and Space Progress move into the sand-toned activity panel; their
  // htmx `hx-trigger="load"` wiring targets the element itself, so moving
  // the (still-skeleton) card before it fires is safe. Runs before any
  // partial has loaded, so the greeting is computed client-side rather than
  // waiting on the Focus card's copy of the same text.
  // -------------------------------------------------------------------------
  function enhanceTodayShell() {
    const bento = document.getElementById('bento');
    if (!bento || document.querySelector('.today-shell')) return;

    const shell = document.createElement('div');
    shell.className = 'today-shell';
    const todayMain = document.createElement('div');
    todayMain.className = 'today-main';
    const panel = document.createElement('aside');
    panel.className = 'activity-panel';
    panel.setAttribute('aria-label', 'Activity');

    // Insert `shell` at bento's current position first, then move bento (and
    // the cards bound for the panel) into it — the DOM reference `bento`
    // stays valid across the move, and htmx's own load-trigger targets that
    // same reference regardless of where it now lives.
    bento.before(shell);
    todayMain.appendChild(bento);
    // "Recent Activity" in the brief is this app's existing Jump Back In widget.
    ['up_next', 'jump_back_in', 'getting_started'].forEach((key) => {
      const card = bento.querySelector('[data-card="' + key + '"]');
      if (card) panel.appendChild(card);
    });
    shell.append(todayMain, panel);

    const name = (document.querySelector('.app-sidebar-user-name') || {}).textContent || '';
    const firstName = name.trim().split(/\s+/)[0] || 'there';
    const initial = firstName.charAt(0).toUpperCase() || '?';
    const dateStr = new Date().toLocaleDateString(undefined, { weekday: 'long', month: 'long', day: 'numeric' });

    const banner = document.createElement('section');
    banner.className = 'hero-banner';
    banner.innerHTML =
      '<div class="hero-banner-identity">'
      + '<span class="hero-banner-avatar" aria-hidden="true">' + escapeHtml(initial) + '</span>'
      + '<div class="hero-banner-text"><h2>Welcome back, ' + escapeHtml(firstName) + '!</h2><p>' + escapeHtml(dateStr) + '</p></div>'
      + '</div>'
      + '<div class="hero-banner-actions">'
      + '<button type="button" class="btn-primary" data-focus-target="quick-add-input">+ New task</button>'
      + '<a class="btn-secondary" href="/calendar">View calendar</a>'
      + '<a class="btn-secondary" href="/spaces">Browse spaces</a>'
      + '</div>';
    todayMain.insertBefore(banner, bento);
  }

  // -------------------------------------------------------------------------
  // Focus card — the hero banner already says "Welcome back, {name}"; drop
  // the Focus card's own "Good morning/afternoon/evening, {name}" line so
  // the two don't repeat the same greeting, and retitle the card toward
  // what it actually shows (today's single most important thing) instead
  // of the generic "Focus" label.
  // -------------------------------------------------------------------------
  function enhanceFocusCard() {
    const card = document.querySelector('[data-card="focus"]');
    if (!card) return;
    const label = card.querySelector('.card-label');
    if (label && label.textContent.trim() === 'Focus') label.textContent = "Today's Focus";
    const greeting = card.querySelector('.focus-greeting');
    if (greeting) greeting.remove();
  }

  // -------------------------------------------------------------------------
  // Up Next — inline "mark done" checkboxes. Up Next's partial never
  // exposes a task id (the schedule blocks mixed into the same list have no
  // task at all), so this cross-references each row's title against Due
  // Soon's rows, which *do* carry `data-task-id` — anything due today is
  // always in both lists (Due Soon covers the next 7 days). A row with no
  // title match (i.e. a schedule block, not a task) is correctly left
  // alone. Best-effort by nature: if Due Soon hasn't loaded yet, or two
  // tasks share an exact title, it may miss — it never guesses wrong.
  // -------------------------------------------------------------------------
  function enhanceUpNextCheckboxes() {
    const dueSoonRows = document.querySelectorAll('[data-card="due_soon"] .due-row[data-task-id]');
    if (!dueSoonRows.length) return;
    const byTitle = new Map();
    dueSoonRows.forEach((row) => {
      const title = (row.querySelector('.due-row-title') || {}).textContent;
      if (title && !byTitle.has(title)) byTitle.set(title, row.dataset.taskId);
    });

    document.querySelectorAll('[data-card="up_next"] .timeline-item').forEach((item) => {
      if (item.querySelector('.task-checkbox')) return;
      const titleEl = item.querySelector('.timeline-title');
      const taskId = titleEl && byTitle.get(titleEl.textContent);
      if (!taskId) return;
      const cb = document.createElement('input');
      cb.type = 'checkbox';
      cb.className = 'task-checkbox';
      cb.dataset.taskId = taskId;
      cb.setAttribute('aria-label', 'Mark done: ' + titleEl.textContent);
      item.insertBefore(cb, item.firstChild);
    });
  }

  // -------------------------------------------------------------------------
  // Getting Started → reframed as the Todoist-style "Finish your setup"
  // drawer widget: same underlying data and the same `data-dismiss-onboarding`
  // endpoint, just retitled, with a segmented step tracker added alongside
  // the existing progress ring (kept, not replaced) and the dismiss button
  // reworded.
  // -------------------------------------------------------------------------
  function enhanceGettingStartedPanel() {
    const card = document.querySelector('[data-card="getting_started"]');
    if (!card) return;
    const label = card.querySelector('.card-label');
    if (label && label.textContent.trim() === 'Getting Started') label.textContent = 'Finish your setup';
    const dismissBtn = card.querySelector('[data-dismiss-onboarding]');
    if (dismissBtn && dismissBtn.textContent.trim() === 'Dismiss') dismissBtn.textContent = "I'll check later";

    const ring = card.querySelector('.progress-ring');
    if (ring && !card.querySelector('.segmented-progress')) {
      const fraction = (ring.querySelector('span') || {}).textContent || '';
      const m = fraction.match(/(\d+)\s*\/\s*(\d+)/);
      if (m) {
        const done = Number(m[1]), total = Number(m[2]);
        const bar = document.createElement('div');
        bar.className = 'segmented-progress';
        bar.setAttribute('role', 'img');
        bar.setAttribute('aria-label', done + ' of ' + total + ' setup steps complete');
        for (let i = 0; i < total; i++) {
          const seg = document.createElement('span');
          seg.className = 'segmented-progress-seg' + (i < done ? ' is-filled' : '');
          bar.appendChild(seg);
        }
        card.querySelector('.getting-started-row').after(bar);
      }
    }
  }

  // -------------------------------------------------------------------------
  // Welcome onboarding — a one-time, full-screen split view for accounts
  // that are still genuinely new. Today page only, and gated on two
  // independent signals so it isn't just a localStorage guess: (1) this
  // browser hasn't dismissed it before, and (2) the server's own onboarding
  // eligibility check (the same one that governs the Getting Started card:
  // fewer than 3 tasks or fewer than 2 spaces, and not already dismissed)
  // still says yes. Fetches its own copies of the getting-started/momentum
  // partials rather than waiting on the dashboard's cards, so it isn't
  // racing their htmx load order.
  // -------------------------------------------------------------------------
  function markWelcomed() {
    try { localStorage.setItem('nook-welcomed', '1'); } catch (e) { /* ignore */ }
  }

  function statTile(value, label) {
    return '<div class="welcome-stat-tile"><div class="welcome-stat-value">' + escapeHtml(String(value)) + '</div><div class="welcome-stat-label">' + escapeHtml(label) + '</div></div>';
  }

  function showWelcomeOnboarding(stats) {
    if (document.querySelector('.welcome-overlay')) return;

    const valueItems = [
      { text: 'Organize your everyday tasks', done: true },
      { text: 'Focus on the right spaces', done: true },
      { text: 'Track momentum & build habits', done: true },
      { text: "Now it's your turn! ✨", done: false },
    ];
    const valueHtml = valueItems.map((i) =>
      '<div class="welcome-value-item' + (i.done ? ' is-done' : '') + '"><span class="welcome-value-check" aria-hidden="true">' + (i.done ? '✓' : '') + '</span><span>' + escapeHtml(i.text) + '</span></div>'
    ).join('');

    const overlay = document.createElement('div');
    overlay.className = 'welcome-overlay';
    overlay.setAttribute('role', 'dialog');
    overlay.setAttribute('aria-modal', 'true');
    overlay.setAttribute('aria-label', 'Welcome to Nook');

    const left = document.createElement('div');
    left.className = 'welcome-left';
    left.innerHTML =
      '<div class="welcome-logo"><span class="brand-mark" aria-hidden="true"></span>Nook</div>'
      + '<h1 class="welcome-heading">Welcome to Nook!</h1>'
      + '<div class="welcome-value-card">' + valueHtml + '</div>'
      + '<button type="button" class="btn-primary welcome-cta">Let’s go!</button>';

    const right = document.createElement('div');
    right.className = 'welcome-right';
    right.innerHTML =
      '<div class="welcome-stats-grid">'
      + statTile(stats.completed, 'Tasks completed')
      + statTile(stats.spaceCount, 'Active spaces')
      + statTile(stats.streak, 'Day streak')
      + statTile(stats.setup, 'Setup progress')
      + '</div>';

    overlay.append(left, right);
    document.body.appendChild(overlay);

    function dismiss() {
      overlay.remove();
      markWelcomed();
      document.removeEventListener('keydown', onKey);
    }
    function onKey(e) { if (e.key === 'Escape') dismiss(); }
    document.addEventListener('keydown', onKey);
    const cta = left.querySelector('.welcome-cta');
    cta.addEventListener('click', dismiss);
    cta.focus();
  }

  async function maybeShowWelcomeOnboarding() {
    if (!document.getElementById('bento')) return; // Today page only
    let seen = null;
    try { seen = localStorage.getItem('nook-welcomed'); } catch (e) { /* ignore */ }
    if (seen) return;

    let gsHtml = '';
    try {
      const gsRes = await fetch('/partials/getting_started');
      gsHtml = gsRes.ok ? await gsRes.text() : '';
    } catch (e) { return; } // no network, no verified "new account" signal — skip rather than guess
    if (!gsHtml.includes('card-getting-started')) return; // server says this account no longer qualifies as new

    const tmp = document.createElement('div');
    tmp.innerHTML = gsHtml;
    const setup = (tmp.querySelector('.progress-ring span') || {}).textContent || '0/?';

    let streak = 0, completed = 0;
    try {
      const momRes = await fetch('/partials/momentum');
      if (momRes.ok) {
        const momTmp = document.createElement('div');
        momTmp.innerHTML = await momRes.text();
        const strongs = momTmp.querySelectorAll('.momentum-summary strong');
        if (strongs.length >= 2) { streak = strongs[0].textContent.trim(); completed = strongs[1].textContent.trim(); }
      }
    } catch (e) { /* stats are a nice-to-have; the overlay still shows without them */ }

    const spaces = collectSpacesFromPage();
    showWelcomeOnboarding({ streak, completed, spaceCount: spaces ? spaces.length : 0, setup });
  }

  // -------------------------------------------------------------------------
  // Wire it all up. Every lazily-loaded Today card re-runs this pass on its
  // own htmx:afterSwap, since due_soon/up_next/momentum/getting_started all
  // load independently and in no guaranteed order.
  // -------------------------------------------------------------------------
  function onCardSwap() {
    enhanceEmptyStates();
    enhanceFocusCard();
    enhanceUpNextCheckboxes();
    enhanceGettingStartedPanel();
  }

  enhanceAppShell();
  enhanceTodayShell();
  enhanceAuthForms();
  enhanceAuthSplitScreen();
  onCardSwap();
  enhanceSpacesGrid();
  enhanceCalendarPage();
  document.body.addEventListener('htmx:afterSwap', onCardSwap);
  maybeShowWelcomeOnboarding();
})();
