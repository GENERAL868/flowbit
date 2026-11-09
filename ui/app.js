'use strict'

// Flowbit's window. Plain JavaScript, no build step: the Rust side does
// the torrenting (librqbit), playback (mpv), the watch library and the subtitle
// translator; this draws them. Texts come from i18n.js (English and Arabic).

const { invoke } = window.__TAURI__.core
const { listen } = window.__TAURI__.event

const FILTERS = ['all', 'downloading', 'seeding', 'completed', 'paused']
const LANGUAGES = ['ar', 'en', 'fr', 'es', 'de', 'tr', 'fa', 'ur', 'hi', 'id', 'ms', 'pt', 'ru', 'it', 'ja', 'ko', 'zh-CN', 'bn', 'nl', 'pl', 'he']
const MEDIA = /\.(mkv|mp4|m4v|webm|avi|mov|wmv|flv|ts|m2ts|mts|mpg|mpeg|ogv|3gp|vob|rmvb|hevc|mp3|flac|m4a|aac|ogg|opus|wav|wma|alac|ape|mka|ac3|dts)$/i
const SUBTITLE = /\.(srt|ass|ssa|vtt)$/i

const state = {
  view: 'home', // home, library, translator, preferences
  filter: 'all',
  search: '',
  snapshot: null,
  info: null, // version, prefs, isDefaultApp, startOnLogin
  expanded: null, // id of the torrent whose files are shown
  files: {}, // torrent id -> file list
  menu: null, // id of the torrent whose "more" menu is open
  selected: new Set(), // ids of the torrents ticked in the list
  modal: null,
  visible: true,
  engineError: null,
  posters: {}, // info hash -> true once its CSS class exists
  posterPending: new Set(),
  library: null, // items from library_items
  libFilter: 'all',
  libSearch: '',
  tr: {
    file: null, // { path, name, format, cues, preview }
    running: false,
    progress: null,
    result: null,
    models: null,
    modelsError: null,
    torrentSubs: null
  }
}

const $ = (id) => document.getElementById(id)

// ---------------------------------------------------------------------------
// Icons: strokes on a 24px grid

const ICONS = {
  play: '<path d="M7 4.5v15l12-7.5z" fill="currentColor" stroke="none"/>',
  pause: '<path d="M8 5v14M16 5v14"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  search: '<circle cx="11" cy="11" r="7"/><path d="M20 20l-3.5-3.5"/>',
  arrowDown: '<path d="M12 5v14M6 13l6 6 6-6"/>',
  arrowUp: '<path d="M12 19V5M6 11l6-6 6 6"/>',
  chevronDown: '<path d="M6 9l6 6 6-6"/>',
  folder: '<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
  folderPlus: '<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><path d="M12 11v5M9.5 13.5h5"/>',
  sliders: '<path d="M4 7h10M18 7h2M4 17h4M12 17h8"/><circle cx="16" cy="7" r="2"/><circle cx="10" cy="17" r="2"/>',
  link: '<path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1"/><path d="M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1"/>',
  users: '<circle cx="9" cy="8" r="3.5"/><path d="M2.5 20a6.5 6.5 0 0 1 13 0"/><path d="M16 4.5a3.5 3.5 0 0 1 0 7M18.5 20a6.5 6.5 0 0 0-2.5-5.1"/>',
  x: '<path d="M6 6l12 12M18 6L6 18"/>',
  trash: '<path d="M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3"/>',
  download: '<path d="M12 4v11M7 10l5 5 5-5M4 19h16"/>',
  upload: '<path d="M12 15V4M7 9l5-5 5 5M4 19h16"/>',
  file: '<path d="M6 3h8l4 4v14H6z"/><path d="M14 3v4h4"/>',
  check: '<path d="M5 12l5 5 9-10"/>',
  alert: '<circle cx="12" cy="12" r="9"/><path d="M12 8v5M12 16h.01"/>',
  more: '<circle cx="5" cy="12" r="1.4" fill="currentColor"/><circle cx="12" cy="12" r="1.4" fill="currentColor"/><circle cx="19" cy="12" r="1.4" fill="currentColor"/>',
  save: '<path d="M5 3h11l3 3v15H5z"/><path d="M8 3v5h8V3M8 21v-7h8v7"/>',
  film: '<rect x="3" y="5" width="18" height="14" rx="2"/><path d="M10 9.5v5l4-2.5z"/>',
  subtitles: '<rect x="3" y="5" width="18" height="14" rx="2"/><path d="M7 15h4M13 15h4M7 11h2M11 11h6"/>',
  eye: '<path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12z"/><circle cx="12" cy="12" r="3"/>',
  skipNext: '<path d="M18 5v14M6 6l9 6-9 6z"/>'
}

function icon (name, size = 18) {
  return `<svg class="icon" viewBox="0 0 24 24" width="${size}" height="${size}" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${ICONS[name]}</svg>`
}

// Six pieces forming a play button; the last one, still arriving, is lit
function logo (size = 30) {
  const pieces = [[15, 14], [15, 27], [15, 40], [28, 20.5], [28, 33.5]]
    .map(([x, y]) => `<rect x="${x}" y="${y}" width="10" height="10" rx="2.5" class="logo-piece"/>`).join('')
  return `<svg class="logo" viewBox="0 0 64 64" width="${size}" height="${size}" aria-hidden="true"><rect width="64" height="64" rx="16" class="logo-tile"/>${pieces}<rect x="41" y="27" width="10" height="10" rx="2.5" class="logo-tip"/></svg>`
}

// ---------------------------------------------------------------------------
// Formatting. Everything from torrents goes through esc() before it's HTML.

function esc (s) {
  return String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]))
}

// Translated text, escaped for HTML
function tt (key, vars) {
  return esc(t(key, vars))
}

// A size or speed, kept left to right inside Arabic text
function num (text) {
  return `<bdi dir="ltr">${text}</bdi>`
}

function bytes (n) {
  if (!n || n < 1) return '0 B'
  const units = ['B', 'kB', 'MB', 'GB', 'TB']
  const exp = Math.min(Math.floor(Math.log10(n) / 3), units.length - 1)
  const value = n / Math.pow(1000, exp)
  return (exp === 0 ? value : value.toFixed(value < 10 ? 1 : 0)) + ' ' + units[exp]
}

function eta (seconds) {
  if (seconds == null) return ''
  const h = Math.floor(seconds / 3600)
  const m = Math.floor(seconds % 3600 / 60)
  const s = Math.floor(seconds % 60)
  if (h > 24) return t('eta.long')
  if (h > 0) return t('eta.hours', { h, m })
  if (m > 0) return t('eta.minutes', { m, s })
  return t('eta.seconds', { s })
}

function clock (seconds) {
  seconds = Math.max(0, Math.floor(seconds || 0))
  const h = Math.floor(seconds / 3600)
  const m = Math.floor(seconds % 3600 / 60)
  const s = String(seconds % 60).padStart(2, '0')
  return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${s}` : `${m}:${s}`
}

function baseName (path) {
  return path.split(/[\\/]/).pop()
}

// Things we can add from text: magnet links, http(s) links, and info hashes
function isTorrentLink (s) {
  return /^magnet:\?/i.test(s) || /^https?:\/\/\S+$/i.test(s) || /^[0-9a-f]{40}$/i.test(s) || /^[a-z2-7]{32}$/i.test(s)
}

// ---------------------------------------------------------------------------
// Torrent status and filters

function status (t) {
  if (t.error || t.state === 'error') return { key: 'status.error', cls: 'error' }
  if (t.state === 'initializing') return { key: 'status.checking', cls: 'downloading' }
  if (t.state === 'paused') return t.finished ? { key: 'status.completed', cls: 'done' } : { key: 'status.paused', cls: 'paused' }
  if (!t.hasMetadata) return { key: 'status.info', cls: 'downloading' }
  return t.finished ? { key: 'status.seeding', cls: 'seeding' } : { key: 'status.downloading', cls: 'downloading' }
}

function matchesFilter (t, filter) {
  const active = t.state !== 'paused' && t.state !== 'error'
  switch (filter) {
    case 'downloading': return active && !t.finished
    case 'seeding': return t.state === 'live' && t.finished
    case 'completed': return t.finished
    case 'paused': return t.state === 'paused' && !t.finished
    default: return true
  }
}

function visibleTorrents () {
  const torrents = state.snapshot ? state.snapshot.torrents : []
  const search = state.search.trim().toLowerCase()
  return torrents
    .filter((t) => matchesFilter(t, state.filter))
    .filter((t) => !search || (t.name || '').toLowerCase().includes(search))
    .sort((a, b) => b.id - a.id)
}

// Texts that mention Windows have a ".linux" variant
function forOs (key) {
  return state.info && state.info.platform === 'linux' ? key + '.linux' : key
}

function isPlayable (t) {
  const files = state.files[t.id]
  return !!files && files.some((f) => !f.padding && MEDIA.test(f.name))
}

// ---------------------------------------------------------------------------
// Posters: one CSS class per torrent, so the list's HTML stays small. They go in a
// constructed stylesheet: a <style> element in the page would make Tauri add a
// nonce to the CSP, which turns off style="" attributes (like progress widths).

const posterSheet = new CSSStyleSheet()
document.adoptedStyleSheets = [...document.adoptedStyleSheets, posterSheet]

function posterClass (infoHash) {
  return state.posters[infoHash] ? ` has-image poster-${infoHash}` : ''
}

function loadPosters (torrents) {
  for (const t of torrents) {
    if (!t.hasPoster || state.posters[t.infoHash] || state.posterPending.has(t.infoHash)) continue
    loadPoster(t.infoHash)
  }
}

async function loadPoster (infoHash) {
  if (!/^[0-9a-f]+$/i.test(infoHash)) return
  state.posterPending.add(infoHash)
  try {
    const url = await invoke('poster', { infoHash })
    if (url && url.startsWith('data:image/jpeg;base64,') && /^[A-Za-z0-9+/=]+$/.test(url.slice(23))) {
      posterSheet.insertRule(`.poster-${infoHash}{background-image:url("${url}")}`, posterSheet.cssRules.length)
      state.posters[infoHash] = true
      render()
    }
  } catch (err) {}
  state.posterPending.delete(infoHash)
}

// ---------------------------------------------------------------------------
// Rendering

function render () {
  const prefs = state.info ? state.info.prefs : null
  $('app').className = `app view-${state.view} theme-${theme(prefs)}`
  renderSidebar()
  if (state.view === 'home') renderHome()
  else if (state.view === 'library') renderLibrary()
  else if (state.view === 'translator') renderTranslator()
  renderStatusBar()
}

function theme (prefs) {
  const choice = prefs ? prefs.theme : 'system'
  if (choice === 'light' || choice === 'dark') return choice
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
}

// Only touches the DOM when the HTML changed, so hover states and scrolling survive
function setHTML (el, html) {
  if (el._html !== html) {
    el._html = html
    el.innerHTML = html
  }
}

// Replaces the page when switching views; keeps it otherwise
function page (name, html) {
  const content = $('content')
  if (content.dataset.page !== name) {
    content.dataset.page = name
    content._html = null
    content.innerHTML = html
    content.scrollTop = 0
    return true
  }
  return false
}

function renderSidebar () {
  const torrents = state.snapshot ? state.snapshot.torrents : []
  const filters = FILTERS.map((f) => {
    const count = torrents.filter((t) => matchesFilter(t, f)).length
    const current = state.view === 'home' && state.filter === f
    return `<button type="button" class="nav-item" data-action="filter" data-filter="${f}" aria-current="${current}">
      <span class="dot ${f}" aria-hidden="true"></span>${tt('filter.' + f)}<span class="count mono">${count}</span></button>`
  }).join('')

  let disk = ''
  const snap = state.snapshot
  if (snap && snap.diskTotal) {
    const used = Math.round(100 * (1 - snap.diskFree / snap.diskTotal))
    disk = `<div class="disk"><div class="row"><span>${tt('disk.title')}</span><span class="mono">${tt('disk.free', { size: '\u0001' }).replace('\u0001', num(bytes(snap.diskFree)))}</span></div>
      <div class="progress thin" role="progressbar" aria-label="${tt('disk.title')}" aria-valuenow="${used}" aria-valuemin="0" aria-valuemax="100"><div class="fill" style="width:${used}%"></div></div></div>`
  }

  const nav = (view, iconName, label) =>
    `<button type="button" class="nav-item" data-action="view" data-view="${view}" aria-current="${state.view === view}">${icon(iconName)}${tt(label)}</button>`

  setHTML($('sidebar'), `
    <div class="brand">${logo()}Flowbit</div>
    <div class="group"><span class="group-title">${tt('nav.library')}</span>${filters}</div>
    <div class="group"><span class="group-title">${tt('nav.watch')}</span>${nav('library', 'film', 'nav.myLibrary')}</div>
    <div class="group"><span class="group-title">${tt('nav.tools')}</span>${nav('translator', 'subtitles', 'nav.translator')}</div>
    <div class="group"><span class="group-title">${tt('nav.folders')}</span>
      <button type="button" class="nav-item" data-action="open-downloads">${icon('folder')}${tt('nav.downloads')}</button></div>
    <div class="bottom">
      ${nav('preferences', 'sliders', 'nav.preferences')}
      ${disk}
    </div>`)
}

function renderHome () {
  page('home', `
    <div class="page home">
      <header class="page-header">
        <div class="page-title"><h1 id="page-title"></h1><span class="count mono" id="page-count"></span></div>
        <label class="input-group search">${icon('search')}<span class="visually-hidden">${tt('home.search')}</span>
          <input id="search" type="search" placeholder="${tt('home.search')}" spellcheck="false"></label>
        <button type="button" class="icon-btn" data-action="add-dialog" aria-label="${tt('home.paste')}" title="${tt('home.paste')}">${icon('link')}</button>
        <button type="button" class="btn primary" data-action="add-dialog">${icon('plus')}${tt('home.add')}</button>
      </header>
      <div id="engine-notice"></div>
      <div id="list"></div>
      <div class="drop-zone">${icon('download')}<span>${tt('home.drop')}</span></div>
    </div>`) && ($('search').value = state.search)

  const torrents = visibleTorrents()
  $('page-title').textContent = t('filter.' + state.filter)
  $('page-count').textContent = torrents.length

  setHTML($('engine-notice'), state.engineError
    ? `<div class="notice" role="alert">${icon('alert')}<div><p>${tt('home.engineError')} <span class="mono">${esc(state.engineError)}</span></p></div></div>`
    : '')

  let html
  if (!state.snapshot && !state.engineError) {
    html = `<div class="loading">${tt('home.starting')}</div>`
  } else if (!torrents.length) {
    const message = state.search
      ? tt('home.noMatch', { query: state.search })
      : state.filter === 'all' ? tt('home.empty') : tt('home.emptyFilter')
    html = `<div class="empty"><strong>${message}</strong><span>${tt('home.emptyHint')}</span></div>`
  } else {
    html = `${renderBulkBar(torrents)}<ul class="torrent-list">${torrents.map(renderTorrent).join('')}</ul>`
  }
  setHTML($('list'), html)

  const selectAll = $('select-all')
  if (selectAll) {
    const count = selectedAmong(torrents).length
    selectAll.indeterminate = count > 0 && count < torrents.length
  }
}

// Only what's on screen counts, so a filter or search can't hide what gets removed
function selectedAmong (torrents) {
  return torrents.filter((t) => state.selected.has(t.id))
}

function renderBulkBar (torrents) {
  const count = selectedAmong(torrents).length
  const remove = count
    ? `<span class="count mono">${tt('bulk.selected', { count })}</span>
      <button type="button" class="btn small danger" data-action="remove-selected">${icon('trash', 16)}${tt('bulk.remove')}</button>`
    : ''
  return `<div class="bulk-bar">
    <label class="check-row"><input type="checkbox" class="checkbox" id="select-all"${count === torrents.length ? ' checked' : ''}>${tt('bulk.selectAll')}</label>
    ${remove}
  </div>`
}

// Torrents already drawn once, so only new ones fade in
const seenTorrents = new Set()

function renderTorrent (t) {
  const st = status(t)
  const name = t.name || '…'
  const expanded = state.expanded === t.id
  const percent = t.totalBytes ? Math.floor(100 * t.progressBytes / t.totalBytes) : 0
  const active = t.state === 'live' || t.state === 'initializing'

  const meta = []
  if (t.error) {
    meta.push(esc(t.error))
  } else {
    if (t.totalBytes && t.progressBytes < t.totalBytes) meta.push(tt('meta.of', { done: '\u0001' + bytes(t.progressBytes) + '\u0002', total: '\u0001' + bytes(t.totalBytes) + '\u0002' }).replace(/\u0001/g, '<bdi dir="ltr">').replace(/\u0002/g, '</bdi>'))
    else if (t.totalBytes) meta.push(num(bytes(t.totalBytes)))
    if (t.fileCount) meta.push(t.fileCount === 1 ? tt('meta.file') : tt('meta.files', { count: t.fileCount }))
    if (active && !t.finished && t.etaSecs != null) meta.push(esc(eta(t.etaSecs)))
  }

  const stats = active && !t.error
    ? `<div class="torrent-stats mono">
        <span class="stat down" title="${tt('stat.down')}">${icon('arrowDown', 16)}${num(bytes(t.downSpeed) + '/s')}</span>
        <span class="stat up" title="${tt('stat.up')}">${icon('arrowUp', 16)}${num(bytes(t.upSpeed) + '/s')}</span>
        <span class="stat peers" title="${tt('stat.peers')}">${icon('users', 16)}${t.peers}</span></div>`
    : ''

  let toggleIcon = 'download'
  let toggleLabel = tt('act.resume')
  if (active) {
    toggleIcon = 'pause'
    toggleLabel = t.finished ? tt('act.stopSeeding') : tt('act.pause')
  } else if (t.finished) {
    toggleIcon = 'upload'
    toggleLabel = tt('act.startSeeding')
  }

  const play = isPlayable(t)
    ? `<button type="button" class="icon-btn primary play" data-action="play" data-id="${t.id}" aria-label="${tt('act.play')}: ${esc(name)}" title="${tt('act.play')}">${icon('play')}</button>`
    : ''

  const menu = state.menu === t.id
    ? `<div class="menu" role="menu">
        <button type="button" data-action="show-folder" data-id="${t.id}">${icon('folder')}${tt('menu.showFolder')}</button>
        <button type="button" data-action="copy-magnet" data-id="${t.id}">${icon('link')}${tt('menu.copyMagnet')}</button>
        <button type="button" data-action="save-torrent" data-id="${t.id}"${t.hasMetadata ? '' : ' disabled'}>${icon('save')}${tt('menu.saveTorrent')}</button>
        <hr>
        <button type="button" data-action="remove" data-id="${t.id}">${icon('x')}${tt('menu.removeList')}</button>
        <button type="button" class="danger" data-action="remove-files" data-id="${t.id}">${icon('trash')}${tt('menu.removeFiles')}</button>
      </div>`
    : ''

  const isNew = !seenTorrents.has(t.id)
  seenTorrents.add(t.id)
  const initial = state.posters[t.infoHash] ? '' : esc(name.trim().charAt(0).toUpperCase())

  return `<li class="torrent${expanded ? ' selected' : ''}${isNew ? ' new' : ''}${state.menu === t.id ? ' menu-open' : ''}" data-id="${t.id}">
    <div class="torrent-row" data-action="toggle-files" data-id="${t.id}">
      <input type="checkbox" class="checkbox" data-action="select-torrent" data-id="${t.id}" aria-label="${tt('bulk.selectOne', { name })}"${state.selected.has(t.id) ? ' checked' : ''}>
      <div class="poster${posterClass(t.infoHash)}" aria-hidden="true">${initial}</div>
      <div class="torrent-info">
        <div class="torrent-name"><span class="name ellipsis" title="${esc(name)}">${esc(name)}</span><span class="chip ${st.cls}">${tt(st.key)}</span></div>
        ${t.error ? '' : `<div class="torrent-progress"><div class="progress ${st.cls}" role="progressbar" aria-valuenow="${percent}" aria-valuemin="0" aria-valuemax="100"><div class="fill" style="width:${percent}%"></div></div><span class="percent mono">${percent}%</span></div>`}
        ${meta.length ? `<span class="torrent-meta ${t.error ? 'error' : 'mono'}">${meta.join(' · ')}</span>` : ''}
      </div>
      ${stats}
      <div class="torrent-actions" data-stop>
        ${play}
        <button type="button" class="icon-btn download" data-action="toggle" data-id="${t.id}" aria-label="${toggleLabel}" title="${toggleLabel}">${icon(toggleIcon)}</button>
        <button type="button" class="icon-btn${expanded ? ' rotated' : ''}" data-action="toggle-files" data-id="${t.id}" aria-expanded="${expanded}" aria-label="${tt('act.files')}" title="${tt('act.files')}">${icon('chevronDown')}</button>
        <button type="button" class="icon-btn ghost delete" data-action="remove" data-id="${t.id}" aria-label="${tt('act.remove')}" title="${tt('act.remove')}">${icon('trash')}</button>
        <button type="button" class="icon-btn ghost" data-action="menu" data-id="${t.id}" aria-haspopup="menu" aria-label="${tt('act.more')}" title="${tt('act.more')}">${icon('more')}</button>
        ${menu}
      </div>
    </div>
    ${expanded ? renderFiles(t) : ''}
  </li>`
}

function renderFiles (t) {
  const files = state.files[t.id]
  if (!t.hasMetadata) return `<div class="torrent-files"><div class="files-header">${tt('files.gettingInfo')}</div></div>`
  if (!files) return `<div class="torrent-files"><div class="files-header">${tt('files.loading')}</div></div>`

  let rows = files.filter((f) => !f.padding)
  if (!state.info || state.info.prefs.sortByName) {
    const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' })
    rows = rows.slice().sort((a, b) => collator.compare(a.name, b.name))
  }

  const html = rows.map((f) => {
    const done = f.length > 0 && f.progress >= f.length
    const percent = f.length ? Math.floor(100 * f.progress / f.length) : 100
    const media = MEDIA.test(f.name)
    const canOpen = media || done
    const translate = SUBTITLE.test(f.name) && done
      ? `<button type="button" class="btn small" data-action="translate-torrent-sub" data-id="${t.id}" data-index="${f.index}">${icon('subtitles', 16)}${tt('files.translate')}</button>`
      : ''
    // A file unchecked after it finished is still on disk: it isn't "skipped"
    const skipped = !f.included && !done
    return `<div class="file-row${skipped ? ' skipped' : ''}">
      <input type="checkbox" class="checkbox" data-action="include-file" data-id="${t.id}" data-index="${f.index}" aria-label="${tt('files.download', { name: f.name })}"${f.included ? ' checked' : ''}>
      <button type="button" class="file-open" data-action="open-file" data-id="${t.id}" data-index="${f.index}"${canOpen ? '' : ' disabled'} title="${media ? tt('act.play') : tt('files.open')}">
        ${icon(media ? 'play' : 'file', 16)}<span class="ellipsis" dir="auto">${esc(f.name)}</span></button>
      ${translate}
      <span class="size mono">${num(bytes(f.length))}</span>
      <div class="progress thin ${done ? 'done' : 'downloading'}" aria-hidden="true"><div class="fill" style="width:${skipped ? 0 : percent}%"></div></div>
      <span class="percent mono">${skipped ? tt('files.skip') : percent + '%'}</span>
    </div>`
  }).join('')

  return `<div class="torrent-files">
    <div class="files-header"><span>${tt('files.header')}</span><span class="mono ellipsis path" dir="ltr">${esc(t.outputFolder)}</span></div>
    ${html}</div>`
}

function renderStatusBar () {
  const bar = $('status-bar')
  if (state.view !== 'home' || !state.snapshot) {
    setHTML(bar, '')
    bar.style.display = state.view === 'home' ? '' : 'none'
    return
  }
  bar.style.display = ''
  const snap = state.snapshot
  const prefs = state.info.prefs
  const active = snap.torrents.filter((x) => x.state === 'live').length
  const limits = []
  if (prefs.downloadLimit > 0) limits.push('↓ ' + prefs.downloadLimit + ' KB/s')
  if (prefs.uploadLimit > 0) limits.push('↑ ' + prefs.uploadLimit + ' KB/s')
  setHTML(bar, `
    <span class="down">${num('↓ ' + bytes(snap.downSpeed) + '/s')}</span>
    <span class="up">${num('↑ ' + bytes(snap.upSpeed) + '/s')}</span>
    <span>${tt('bar.torrents', { count: snap.torrents.length, active })}</span>
    <span>${tt('bar.limit', { limit: limits.length ? limits.join(' ') : t('bar.off') })}</span>
    ${snap.listenPort ? `<span class="push">${tt('bar.port', { port: snap.listenPort })}</span>` : ''}`)
}

// ---------------------------------------------------------------------------
// Watch library: episodes grouped into shows by their names

// "[Erai-raws] Show Name - 05 [1080p][ABCD1234].mkv" -> { show: "Show Name", episode: 5 }
function parseEpisode (name) {
  const base = baseName(name).replace(/\.[^.]+$/, '')
  const clean = base
    .replace(/\[[^\]]*\]|\([^)]*\)|\{[^}]*\}/g, ' ')
    .replace(/[._]/g, ' ')
    .replace(/\b(1080p|720p|480p|2160p|4k|x264|x265|hevc|web-?dl|webrip|bluray|aac|flac|multi ?sub)\b/gi, ' ')
    .replace(/\s+/g, ' ')
    .trim()
  const patterns = [
    /^(.*?)\s*S(\d{1,2})\s*E(\d{1,4})\b/i,
    /^(.*?)\s+-\s+(?:E|EP)?(\d{1,4})(?:v\d)?(?:\s|$)/i,
    /^(.*?)\s+(?:EP?|Episode)\s*(\d{1,4})\b/i
  ]
  for (const re of patterns) {
    const m = clean.match(re)
    if (m && m[1].trim()) {
      const hasSeason = re === patterns[0]
      return {
        show: m[1].replace(/[\s-]+$/, '').trim(),
        season: hasSeason ? Number(m[2]) : null,
        episode: Number(hasSeason ? m[3] : m[2])
      }
    }
  }
  return { show: null, season: null, episode: null }
}

function libraryGroups (items) {
  const groups = new Map()
  for (const item of items) {
    const ep = parseEpisode(item.name)
    // Shows by name across torrents; anything else stays with its torrent
    const key = ep.show ? ep.show.toLowerCase() : 'torrent:' + item.infoHash
    const title = ep.show || item.torrentName || t('lib.other')
    if (!groups.has(key)) groups.set(key, { key, title, items: [] })
    groups.get(key).items.push(Object.assign({}, item, ep))
  }
  const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' })
  const list = [...groups.values()]
  for (const g of list) {
    g.items.sort((a, b) => (a.season ?? 0) - (b.season ?? 0) || (a.episode ?? 0) - (b.episode ?? 0) || collator.compare(a.name, b.name))
    g.lastPlayed = Math.max(0, ...g.items.map((i) => i.lastPlayed || 0))
  }
  // Recently watched first, then by name
  return list.sort((a, b) => b.lastPlayed - a.lastPlayed || collator.compare(a.title, b.title))
}

async function loadLibrary () {
  try {
    state.library = await invoke('library_items')
  } catch (err) {
    state.library = []
  }
  if (state.view === 'library') renderLibrary()
}

function renderLibrary () {
  if (page('library', `
    <div class="page library">
      <header class="page-header">
        <div class="page-title"><h1>${tt('lib.title')}</h1></div>
        <div class="segmented" role="group" id="lib-filters"></div>
        <label class="input-group search">${icon('search')}<span class="visually-hidden">${tt('lib.search')}</span>
          <input id="lib-search" type="search" placeholder="${tt('lib.search')}" spellcheck="false"></label>
      </header>
      <div id="lib-content"></div>
    </div>`)) $('lib-search').value = state.libSearch

  setHTML($('lib-filters'), ['all', 'unwatched', 'watched']
    .map((f) => `<button type="button" data-action="lib-filter" data-filter="${f}" aria-pressed="${state.libFilter === f}">${tt('lib.' + f)}</button>`).join(''))

  if (!state.library) {
    setHTML($('lib-content'), `<div class="loading">${tt('home.starting')}</div>`)
    return
  }
  const search = state.libSearch.trim().toLowerCase()
  const items = state.library.filter((i) => !search || i.name.toLowerCase().includes(search) || (i.torrentName || '').toLowerCase().includes(search))
  if (!items.length) {
    setHTML($('lib-content'), `<div class="empty"><strong>${tt('lib.empty')}</strong><span>${tt('lib.emptyHint')}</span></div>`)
    return
  }

  // Continue watching: started, not finished, most recent first
  const started = items.filter((i) => i.position > 5 && !i.watched).sort((a, b) => b.lastPlayed - a.lastPlayed).slice(0, 6)
  const continueRow = started.length && state.libFilter !== 'watched'
    ? `<section class="lib-section"><h2>${tt('lib.continue')}</h2><div class="continue-row">${started.map(renderContinueCard).join('')}</div></section>`
    : ''

  const groups = libraryGroups(items)
    .map((g) => Object.assign(g, {
      shown: g.items.filter((i) => state.libFilter === 'all' || (state.libFilter === 'watched') === !!i.watched)
    }))
    .filter((g) => g.shown.length)

  setHTML($('lib-content'), continueRow + groups.map(renderShow).join(''))
}

function renderContinueCard (item) {
  const ep = parseEpisode(item.name)
  const title = ep.show ? `${ep.show}${ep.episode != null ? ' · ' + t('lib.episode', { n: ep.episode }) : ''}` : baseName(item.name)
  const percent = item.duration ? Math.min(100, Math.round(100 * item.position / item.duration)) : 0
  return `<button type="button" class="continue-card" data-action="lib-play" data-id="${item.torrentId}" data-index="${item.fileIndex}">
    <div class="poster wide${posterClass(item.infoHash)}" aria-hidden="true">${state.posters[item.infoHash] ? '' : icon('film', 28)}<span class="play-badge">${icon('play', 16)}</span></div>
    <span class="title ellipsis" dir="auto">${esc(title)}</span>
    <span class="sub mono">${tt('lib.resumeAt', { time: '\u0001' }).replace('\u0001', num(clock(item.position)))}</span>
    <div class="progress thin done"><div class="fill" style="width:${percent}%"></div></div>
  </button>`
}

function renderShow (g) {
  const watched = g.items.filter((i) => i.watched).length
  const next = g.items.find((i) => !i.watched)
  const posterHash = (g.items.find((i) => state.posters[i.infoHash]) || g.items[0]).infoHash
  const header = `<div class="show-header">
    <div class="poster${posterClass(posterHash)}" aria-hidden="true">${state.posters[posterHash] ? '' : esc(g.title.charAt(0).toUpperCase())}</div>
    <div class="show-info">
      <h2 dir="auto">${esc(g.title)}</h2>
      <span class="sub">${tt('lib.watchedCount', { watched, total: g.items.length })}</span>
      <div class="progress thin done"><div class="fill" style="width:${Math.round(100 * watched / g.items.length)}%"></div></div>
    </div>
    ${next
      ? `<button type="button" class="btn primary" data-action="lib-play" data-id="${next.torrentId}" data-index="${next.fileIndex}">${icon('play', 16)}${tt('lib.playNext')}${next.episode != null ? ` <span class="mono">${next.episode}</span>` : ''}</button>`
      : `<span class="chip done">${tt('lib.allWatched')}</span>`}
  </div>`
  return `<section class="show">${header}<div class="episodes">${g.shown.map(renderEpisode).join('')}</div></section>`
}

function renderEpisode (item) {
  const downloaded = item.length ? Math.floor(100 * item.downloaded / item.length) : 0
  let detail
  if (item.watched) detail = `<span class="chip done">${icon('check', 14)}${tt('lib.watchedBadge')}</span>`
  else if (item.position > 5) detail = `<span class="sub mono">${tt('lib.resumeAt', { time: '\u0001' }).replace('\u0001', num(clock(item.position)))}</span>`
  else if (downloaded < 100) detail = `<span class="sub mono">${tt('lib.downloading', { percent: downloaded })}</span>`
  else detail = ''
  const label = item.episode != null
    ? `<span class="ep-number mono">${item.season != null ? 'S' + item.season + ' ' : ''}E${item.episode}</span>`
    : ''
  const subs = item.subtitles && item.subtitles.length
    ? `<button type="button" class="icon-btn ghost" data-action="translate-torrent-sub" data-id="${item.torrentId}" data-index="${item.subtitles[0]}" title="${tt('lib.translateSubs')}" aria-label="${tt('lib.translateSubs')}">${icon('subtitles', 16)}</button>`
    : ''
  return `<div class="episode${item.watched ? ' watched' : ''}">
    <input type="checkbox" class="checkbox" data-action="set-watched" data-hash="${esc(item.infoHash)}" data-index="${item.fileIndex}"
      title="${item.watched ? tt('lib.markUnwatched') : tt('lib.markWatched')}" aria-label="${item.watched ? tt('lib.markUnwatched') : tt('lib.markWatched')}"${item.watched ? ' checked' : ''}>
    ${label}
    <button type="button" class="file-open" data-action="lib-play" data-id="${item.torrentId}" data-index="${item.fileIndex}" title="${tt('act.play')}">
      ${icon('play', 16)}<span class="ellipsis" dir="auto">${esc(baseName(item.name))}</span></button>
    ${detail}
    ${subs}
  </div>`
}

// ---------------------------------------------------------------------------
// Subtitle translator

// AI services (same list as src-tauri/src/translate.rs): free tiers first
const AI_SERVICES = [
  { id: 'gemini', model: 'gemini-2.5-flash', key: true, free: true, help: ['gemini-key'] },
  { id: 'groq', model: 'llama-3.3-70b-versatile', key: true, free: true, help: ['groq-key'] },
  { id: 'openrouter', model: 'meta-llama/llama-3.3-70b-instruct:free', key: true, free: true, help: ['openrouter-key', 'openrouter-models'] },
  { id: 'openai', model: 'gpt-4o-mini', key: true, free: false, help: ['openai-key'] },
  { id: 'ollama', model: '', key: false, free: true, help: ['ollama', 'ollama-models'] },
  { id: 'custom', model: '', key: false, free: true, help: [] }
]

const HELP_LABELS = {
  'gemini-key': 'tr2.getKey',
  'groq-key': 'tr2.getKey',
  'openrouter-key': 'tr2.getKey',
  'openrouter-models': 'help.models',
  'openai-key': 'tr2.getKeyPaid',
  'deepl-signup': 'tr2.getKey',
  ollama: 'Ollama',
  'ollama-models': 'help.models',
  python: 'help.python',
  docker: 'help.docker'
}

function helpButtons (pages) {
  return pages.length
    ? `<div class="button-row start">${pages.map((page) => {
        const label = HELP_LABELS[page]
        return `<button type="button" class="btn small" data-action="open-help" data-page="${page}">${icon('link', 14)}${label.includes('.') ? tt(label) : esc(label)}</button>`
      }).join('')}</div>`
    : ''
}

// A setup guide: the steps are fixed HTML from i18n.js, nothing from outside
function guide (key, pages) {
  return `<details class="guide"><summary>${tt('tr2.howTo')}</summary>${helpButtons(pages)}<ol>${t(key)}</ol></details>`
}

function keyField (service, serviceName) {
  const saved = (state.tr.savedKeys || []).includes(service)
  const control = saved
    ? `<span class="key-saved">${icon('check', 16)}${tt('tr2.keySaved')}</span>
       <button type="button" class="btn small" data-action="remove-key" data-service="${service}">${tt('tr2.remove')}</button>`
    : `<input type="password" class="input mono medium" dir="ltr" id="key-input" placeholder="${tt('tr2.keyPlaceholder')}" autocomplete="off" spellcheck="false">
       <button type="button" class="btn small primary" data-action="save-key" data-service="${service}">${tt('tr2.save')}</button>`
  return `<div class="pref-row"><span class="text"><span class="label">${tt('tr2.key')}</span>
      <span class="hint">${tt(forOs('tr2.keyHint'), { service: serviceName })}</span></span>
      <span class="key-control">${control}</span></div>`
}

function textPref (pref, label, hint, placeholder) {
  return `<label class="pref-row"><span class="text"><span class="label">${tt(label)}</span>${hint ? `<span class="hint">${hint}</span>` : ''}</span>
    <input type="text" class="input mono medium" dir="ltr" data-pref="${pref}" value="${esc(state.info.prefs[pref])}" placeholder="${esc(placeholder || '')}" spellcheck="false"></label>`
}

function engineDetails () {
  const p = state.info.prefs
  const tr = state.tr
  switch (p.subEngine) {
    case 'google':
      return `<p class="engine-about">${tt('tr2.googleHint')}</p>${guide('guide.google', [])}`
    case 'deepl':
      return `<p class="engine-about">${tt('tr2.deeplHint')}</p>${keyField('deepl', 'DeepL')}${guide('guide.deepl', ['deepl-signup'])}`
    case 'libre':
      return `<p class="engine-about">${tt('tr2.libreHint')}</p>
        ${textPref('subLibreUrl', 'tr2.libreUrl', tt('tr2.libreUrlHint'), 'http://127.0.0.1:5000')}
        ${guide('guide.libre', ['python', 'docker'])}`
    case 'ai': {
      const svc = AI_SERVICES.find((s) => s.id === p.subAiProvider) || AI_SERVICES[0]
      const services = AI_SERVICES.map((s) => `<option value="${s.id}"${s.id === svc.id ? ' selected' : ''}>${tt('svc.' + s.id)}</option>`).join('')
      let model
      if (svc.id === 'ollama') {
        const models = tr.models || []
        model = tr.modelsError
          ? `<div class="notice" role="alert">${icon('alert')}<div><p>${tt('tr.noOllama')}</p>
              <button type="button" class="btn small" data-action="refresh-models">${tt('tr2.refresh')}</button></div></div>`
          : `<label class="pref-row"><span class="text"><span class="label">${tt('tr2.model')}</span><span class="hint">${tt('tr2.modelOllamaHint')}</span></span>
              <span class="key-control"><select class="input medium" data-pref="subAiModel">${models.map((m) => `<option value="${esc(m)}"${p.subAiModel === m ? ' selected' : ''}>${esc(m)}</option>`).join('')}</select>
              <button type="button" class="btn small" data-action="refresh-models">${tt('tr2.refresh')}</button></span></label>`
      } else {
        model = textPref('subAiModel', 'tr2.model', svc.model ? tt('tr2.modelHint', { model: svc.model }) : '', svc.model)
      }
      return `<p class="engine-about">${tt('tr2.aiHint')}</p>
        <label class="pref-row"><span class="text"><span class="label">${tt('tr2.service')}</span><span class="hint">${tt('tr2.serviceHint')}</span></span>
          <select class="input medium" data-pref="subAiProvider">${services}</select></label>
        ${svc.id === 'custom' ? textPref('subAiUrl', 'tr2.url', tt('tr2.urlHint'), 'http://127.0.0.1:1234/v1/chat/completions') : ''}
        ${model}
        ${svc.key || svc.id === 'custom' ? keyField(svc.id, t('svc.' + svc.id)) : ''}
        ${guide('guide.' + svc.id, svc.help)}`
    }
  }
  return ''
}

function renderTranslator () {
  const p = state.info.prefs
  const tr = state.tr
  page('translator', `
    <div class="page translator">
      <div class="page-title"><h1>${tt('tr.title')}</h1></div>
      <p class="intro">${tt('tr.intro')}</p>
      <div id="tr-content"></div>
    </div>`)

  // Left column: the file
  let source
  if (tr.file) {
    source = `<section class="pref-section"><h2>${tt('tr2.source')}</h2>
      <div class="file-title">${icon('subtitles', 18)}<span dir="auto">${esc(tr.file.name)}</span></div>
      <div class="pref-row"><span class="text"><span class="label mono">${tt('tr.fileInfo', { format: tr.file.format, count: tr.file.cues })}</span>
        <span class="path mono" dir="ltr">${esc(tr.file.path)}</span></span>
        <button type="button" class="btn" data-action="tr-pick"${tr.running ? ' disabled' : ''}>${tt('tr.another')}</button></div>
      <div class="tr-preview"><span class="field-hint">${tt('tr.preview')}</span>${tr.file.preview.map((l) => `<p dir="auto">${esc(l)}</p>`).join('')}</div>
    </section>`
  } else {
    const subs = tr.torrentSubs || []
    const list = subs.length
      ? subs.map((s) => `<button type="button" class="file-open" data-action="translate-torrent-sub" data-id="${s.torrentId}" data-index="${s.fileIndex}"${s.complete ? '' : ' disabled'}>
          ${icon('subtitles', 16)}<span class="ellipsis" dir="auto">${esc(s.name)}</span></button>`).join('')
      : `<span class="field-hint">${tt('tr.noneInTorrents')}</span>`
    source = `<section class="pref-section"><h2>${tt('tr2.source')}</h2>
        <p class="field-hint">${tt('tr2.sourceHint')}</p>
        <div class="dashed">${icon('subtitles', 32)}
          <button type="button" class="btn primary" data-action="tr-pick">${tt('tr.choose')}</button>
          <span class="hint">${tt('tr.dropHint')}</span></div>
      </section>
      <section class="pref-section"><h2>${tt('tr.fromTorrents')}</h2><div class="tr-torrent-subs">${list}</div></section>`
  }

  // Right column: language, engine, output, and the button
  const languages = LANGUAGES.map((code) => `<option value="${code}"${p.subTarget === code ? ' selected' : ''}>${tt('lang.' + code)}</option>`).join('')
  const styles = ['msa', 'gulf', 'egyptian', 'levantine']
    .map((s) => `<option value="${s}"${p.subArabicStyle === s ? ' selected' : ''}>${tt('tr2.' + s)}</option>`).join('')
  const engines = [
    ['google', 'tr2.googleTag'], ['deepl', 'tr2.deeplTag'], ['ai', 'tr2.aiTag'], ['libre', 'tr2.libreTag']
  ].map(([id, tag]) => `<label class="engine${p.subEngine === id ? ' selected' : ''}">
      <input type="radio" name="tr-engine" data-pref="subEngine" value="${id}"${p.subEngine === id ? ' checked' : ''}>
      <span class="text"><span class="label">${tt('tr2.' + id)}</span><span class="hint">${tt(tag)}</span></span></label>`).join('')
  const check = (pref, label, hint) => `<label class="pref-row"><span class="text"><span class="label">${tt(label)}</span>${hint ? `<span class="hint">${tt(hint)}</span>` : ''}</span>
      <input type="checkbox" class="checkbox" data-pref="${pref}"${p[pref] ? ' checked' : ''}></label>`

  let action
  if (tr.running) {
    const pr = tr.progress || { done: 0, total: 0 }
    const percent = pr.total ? Math.round(100 * pr.done / pr.total) : 0
    action = `<div class="tr-progress"><div class="progress"><div class="fill" style="width:${percent}%"></div></div>
      <span class="mono">${tt('tr.progress', { done: pr.done, total: pr.total })}</span>
      <button type="button" class="btn" data-action="tr-cancel">${tt('common.cancel')}</button></div>`
  } else {
    action = `<div class="modal-actions"><button type="button" class="btn primary large" data-action="tr-run"${tr.file ? '' : ' disabled'}>${icon('subtitles', 18)}${tt('tr.translate')}</button></div>`
  }
  const result = tr.result
    ? `<div class="notice ok" role="status">${icon('check')}<div><p>${tt('tr.saved', { path: '' })}<span class="mono" dir="ltr">${esc(tr.result)}</span></p>
        <button type="button" class="btn small" data-action="tr-reveal">${tt('menu.showFolder')}</button></div></div>`
    : ''

  setHTML($('tr-content'), `<div class="tr-layout">
    <div class="tr-column">${source}</div>
    <div class="tr-column">
      <section class="pref-section"><h2>${tt('tr2.language')}</h2>
        <label class="pref-row"><span class="text"><span class="label">${tt('tr.target')}</span><span class="hint">${tt('tr2.targetHint')}</span></span>
          <select class="input medium" data-pref="subTarget">${languages}</select></label>
        ${p.subTarget === 'ar' && p.subEngine === 'ai'
          ? `<label class="pref-row"><span class="text"><span class="label">${tt('tr2.style')}</span><span class="hint">${tt('tr2.styleHint')}</span></span>
              <select class="input medium" data-pref="subArabicStyle">${styles}</select></label>`
          : ''}
      </section>
      <section class="pref-section"><h2>${tt('tr2.engine')}</h2>
        <p class="field-hint">${tt('tr2.engineHint')}</p>
        <div class="engines">${engines}</div>
        ${engineDetails()}
      </section>
      <section class="pref-section"><h2>${tt('tr2.options')}</h2>
        ${check('subRtlFix', 'tr.rtl', 'tr.rtlHint')}
        ${check('subBilingual', 'tr.bilingual', '')}
        <label class="pref-row"><span class="text"><span class="label">${tt('tr.saveAs')}</span><span class="hint">${tt('tr.saveAsHint')}</span></span>
          <input type="checkbox" class="checkbox" id="tr-save-as"${tr.saveAs ? ' checked' : ''}></label>
        ${p.subEngine === 'ai'
          ? `<label class="field glossary"><span class="label">${tt('tr2.glossary')}</span><span class="field-hint">${tt('tr2.glossaryHint')}</span>
              <textarea class="input" rows="3" dir="auto" data-pref="subGlossary" spellcheck="false">${esc(p.subGlossary)}</textarea></label>`
          : ''}
        <p class="field-hint">${tt('tr.privacy')}</p>
      </section>
      ${result}
      ${action}
    </div>
  </div>`)
}

async function openTranslator (args) {
  state.view = 'translator'
  state.tr.result = null
  render()
  if (args) {
    try {
      state.tr.file = await invoke('open_subtitle', args)
    } catch (err) {
      showError(err)
    }
  }
  invoke('saved_api_keys').then((keys) => { state.tr.savedKeys = keys; render() }, () => {})
  if (!state.tr.torrentSubs) invoke('torrent_subtitles').then((s) => { state.tr.torrentSubs = s; render() }, () => {})
  if (state.info.prefs.subEngine === 'ai' && state.info.prefs.subAiProvider === 'ollama') loadModels()
  render()
}

async function loadModels () {
  try {
    state.tr.models = await invoke('ollama_models')
    state.tr.modelsError = null
    const p = state.info.prefs
    if (state.tr.models.length && !state.tr.models.includes(p.subAiModel)) updatePref('subAiModel', state.tr.models[0])
  } catch (err) {
    state.tr.models = null
    state.tr.modelsError = String(err)
  }
  render()
}

async function saveKey (service) {
  const input = $('key-input')
  const key = input ? input.value.trim() : ''
  if (!key) return
  try {
    state.tr.savedKeys = await invoke('set_api_key', { service, key })
    showInfo(t('tr2.keySavedMsg'))
  } catch (err) {
    showError(err)
  }
  render()
}

async function removeKey (service) {
  try {
    state.tr.savedKeys = await invoke('set_api_key', { service, key: '' })
    showInfo(t('tr2.keyRemovedMsg'))
  } catch (err) {
    showError(err)
  }
  render()
}

async function runTranslation () {
  const tr = state.tr
  if (!tr.file || tr.running) return
  tr.running = true
  tr.result = null
  tr.progress = { done: 0, total: tr.file.cues }
  render()
  try {
    // The engine and options are read from the saved preferences
    clearTimeout(saveTimer)
    state.info.prefs = await invoke('set_prefs', { prefs: state.info.prefs })
    const saved = await invoke('translate_subtitle', { request: { path: tr.file.path, saveAs: !!tr.saveAs } })
    if (saved) {
      tr.result = saved
      sound('done')
    }
  } catch (err) {
    if (String(err) !== 'cancelled') showError(err)
  }
  tr.running = false
  render()
}

// ---------------------------------------------------------------------------
// Preferences

function renderPreferences () {
  const info = state.info
  const p = info.prefs
  const segmented = (pref, options) => options
    .map(([id, label]) => `<button type="button" data-action="set-pref" data-pref="${pref}" data-value="${id}" aria-pressed="${p[pref] === id}">${label}</button>`).join('')

  const check = (pref, label, hint, checked) => `
    <label class="pref-row"><span class="text"><span class="label">${tt(label)}</span>${hint ? `<span class="hint">${tt(hint)}</span>` : ''}</span>
      <input type="checkbox" class="checkbox" data-pref="${pref}"${checked ? ' checked' : ''}></label>`

  const defaultApp = info.isDefaultApp
    ? `<div class="pref-row"><span class="pref-warning label">${icon('check')}${tt('prefs.isDefault')}</span></div>`
    : `<div class="pref-row"><span class="pref-warning label" style="margin-inline-end:auto">${icon('alert')}${tt('prefs.notDefault')}</span>
        <button type="button" class="btn" data-action="make-default">${tt('prefs.makeDefault')}</button></div>`

  const limit = (pref, label) => `
    <label class="field"><span class="label" style="font-size:15px;font-weight:600">${tt(label)}</span>
      <span class="input-group"><input type="number" min="0" step="50" class="mono" data-pref="${pref}" placeholder="${tt('prefs.unlimited')}" value="${Number(p[pref]) > 0 ? Number(p[pref]) : ''}">
      <span class="unit mono">KB/s</span></span></label>`

  const port = state.snapshot && state.snapshot.listenPort ? ' ' + t('prefs.portNow', { port: state.snapshot.listenPort }) : ''

  $('content').dataset.page = 'preferences'
  $('content')._html = null
  $('content').innerHTML = `
    <div class="page preferences">
      <div class="page-title"><h1>${tt('prefs.title')}</h1></div>
      <section class="pref-section"><h2>${tt('prefs.general')}</h2>
        <div class="pref-row"><div class="text"><span class="label">${tt('prefs.language')}</span><span class="hint">${tt(forOs('prefs.languageHint'))}</span></div>
          <div class="segmented" role="group" aria-label="${tt('prefs.language')}">${segmented('language', [['system', tt('prefs.system')], ['en', 'English'], ['ar', 'العربية']])}</div></div>
        <div class="pref-row"><div class="text"><span class="label">${tt('prefs.theme')}</span><span class="hint">${tt('prefs.themeHint')}</span></div>
          <div class="segmented" role="group" aria-label="${tt('prefs.theme')}">${segmented('theme', [['light', tt('prefs.light')], ['dark', tt('prefs.dark')], ['system', tt('prefs.system')]])}</div></div>
        ${check('startOnLogin', 'prefs.startOnLogin', 'prefs.startOnLoginHint', info.startOnLogin)}
        ${check('minimizeToTray', 'prefs.tray', 'prefs.trayHint', p.minimizeToTray)}
        ${check('notifications', 'prefs.notifications', '', p.notifications)}
        ${check('sounds', 'prefs.sounds', '', p.sounds)}
        ${defaultApp}
      </section>
      <section class="pref-section"><h2>${tt('prefs.downloads')}</h2>
        <div class="pref-row path-selector"><div class="text"><span class="label">${tt('prefs.location')}</span><span class="path mono" dir="ltr">${esc(p.downloadDir)}</span></div>
          <button type="button" class="btn" data-action="choose-download-dir">${tt('prefs.change')}</button></div>
        ${check('stopSeedingWhenDone', 'prefs.stopSeeding', 'prefs.stopSeedingHint', p.stopSeedingWhenDone)}
        ${check('sortByName', 'prefs.sortByName', 'prefs.sortByNameHint', p.sortByName)}
      </section>
      <section class="pref-section"><h2>${tt('prefs.playback')}</h2>
        <div class="pref-row"><div class="text"><span class="label">${tt('prefs.player')}</span><span class="hint">${tt('prefs.playerHint')}</span></div></div>
        <label class="pref-row"><span class="text"><span class="label">${tt('prefs.subLangs')}</span><span class="hint">${tt('prefs.subLangsHint')}</span></span>
          <input type="text" class="input mono medium" dir="ltr" data-pref="subtitleLanguages" value="${esc(p.subtitleLanguages)}" spellcheck="false"></label>
        <label class="pref-row"><span class="text"><span class="label">${tt('prefs.audioLangs')}</span><span class="hint">${tt('prefs.audioLangsHint')}</span></span>
          <input type="text" class="input mono medium" dir="ltr" data-pref="audioLanguages" value="${esc(p.audioLanguages)}" spellcheck="false"></label>
      </section>
      <section class="pref-section"><h2>${tt('prefs.network')}</h2>
        <div class="pref-grid">${limit('downloadLimit', 'prefs.downLimit')}${limit('uploadLimit', 'prefs.upLimit')}</div>
        <label class="pref-row"><span class="text"><span class="label">${tt('prefs.port')}</span><span class="hint">${tt('prefs.portHint')}${esc(port)}</span></span>
          <input type="number" min="0" max="65535" class="input mono short" data-pref="listenPort" value="${Number(p.listenPort) || 0}"></label>
      </section>
      <section class="pref-section"><h2>${tt('prefs.trackers')}</h2>
        <label class="field" style="padding:6px 0 18px"><span class="field-hint">${tt('prefs.trackersHint')}</span>
          <textarea class="input mono torrent-trackers" dir="ltr" rows="4" data-pref="trackers" spellcheck="false">${esc(p.trackers.join('\n'))}</textarea></label>
      </section>
      <p class="about">${tt('prefs.about', { version: info.version })}</p>
    </div>`
}

let saveTimer = null
function updatePref (pref, value) {
  const prefs = Object.assign({}, state.info.prefs, { [pref]: value })
  state.info.prefs = prefs
  clearTimeout(saveTimer)
  saveTimer = setTimeout(() => {
    invoke('set_prefs', { prefs }).then((saved) => { state.info.prefs = saved }, showError)
  }, 300)
  if (pref === 'language') {
    setLanguage(value)
    // Redraw everything in the new language
    $('content').dataset.page = ''
    $('sidebar')._html = null
    $('status-bar')._html = null
  }
  render()
  if (state.view === 'preferences') renderPreferences()
}

function prefValue (el) {
  if (el.type === 'checkbox') return el.checked
  if (el.type === 'number') return Math.max(0, Math.round(Number(el.value) || 0))
  if (el.dataset.pref === 'trackers') return el.value.split('\n').map((s) => s.trim()).filter(Boolean)
  if (el.dataset.pref === 'subGlossary') return el.value
  return el.value.trim()
}

function setPrefQuietly (el) {
  const prefs = Object.assign({}, state.info.prefs, { [el.dataset.pref]: prefValue(el) })
  state.info.prefs = prefs
  clearTimeout(saveTimer)
  saveTimer = setTimeout(() => {
    invoke('set_prefs', { prefs }).then((saved) => { state.info.prefs = saved }, showError)
  }, 400)
}

function onPrefInput (el) {
  const pref = el.dataset.pref
  if (pref === 'startOnLogin') {
    invoke('set_start_on_login', { enabled: el.checked }).then((on) => { state.info.startOnLogin = on }, showError)
    return
  }
  const value = prefValue(el)
  if (pref === 'subAiProvider') {
    // Each service has its own models: back to its recommended one
    state.info.prefs.subAiModel = ''
  }
  updatePref(pref, value)
  const p = state.info.prefs
  if ((pref === 'subAiProvider' || pref === 'subEngine') && p.subEngine === 'ai' && p.subAiProvider === 'ollama') loadModels()
}

// ---------------------------------------------------------------------------
// Modals

function openModal (modal) {
  state.modal = modal
  state.menu = null
  renderModal()
}

function closeModal () {
  state.modal = null
  renderModal()
}

function renderModal () {
  const root = $('modal')
  const m = state.modal
  if (!m) {
    root.innerHTML = ''
    return
  }
  let body = ''
  if (m.id === 'add') {
    const tabs = [['magnet', 'add.tabMagnet'], ['file', 'add.tabFile'], ['create', 'add.tabShare']]
      .map(([id, label]) => `<button type="button" data-action="add-tab" data-tab="${id}" aria-pressed="${m.tab === id}">${tt(label)}</button>`).join('')
    let tab
    if (m.tab === 'magnet') {
      tab = `<form class="field" id="magnet-form" style="gap:16px">
          <label class="field"><span class="field-label">${tt('add.magnetLabel')}</span>
            <input id="magnet-input" class="input mono" dir="ltr" type="text" placeholder="magnet:?xt=urn:btih:…" spellcheck="false"></label>
          <div class="modal-actions"><button type="button" class="btn cancel" data-action="close-modal">${tt('common.cancel')}</button>
            <button type="submit" class="btn primary ok">${tt('home.add')}</button></div></form>`
    } else if (m.tab === 'file') {
      tab = `<div class="dashed">${icon('upload', 32)}<span class="title">${tt('add.fileTitle')}</span>
          <span class="hint">${tt('add.fileHint')}</span>
          <button type="button" class="btn" data-action="choose-torrent-file">${tt('add.chooseFile')}</button></div>`
    } else {
      tab = `<div class="dashed">${icon('folderPlus', 32)}<span class="title">${tt('add.shareTitle')}</span>
          <span class="hint">${tt('add.shareHint')}</span>
          <div class="button-row"><button type="button" class="btn" data-action="create" data-folder="true">${tt('add.chooseFolder')}</button>
            <button type="button" class="btn" data-action="create" data-folder="false">${tt('add.chooseFile')}</button></div></div>`
    }
    body = `<div class="modal-header"><h1>${tt('add.title')}</h1>
        <button type="button" class="icon-btn ghost" aria-label="${tt('common.close')}" data-action="close-modal">${icon('x')}</button></div>
      <div class="segmented wide" role="group">${tabs}</div>${tab}`
  } else if (m.id === 'share') {
    const names = m.paths.map((p) => `<li class="mono" dir="auto">${esc(baseName(p))}</li>`).join('')
    body = `<h1>${m.paths.length === 1 ? tt('share.titleOne') : tt('share.titleMany', { count: m.paths.length })}</h1>
      <p>${tt('share.text')}</p>
      <ul style="margin:0;padding-inline-start:20px;color:var(--text-2);max-height:160px;overflow:auto">${names}</ul>
      <div class="modal-actions"><button type="button" class="btn cancel" data-action="close-modal">${tt('common.cancel')}</button>
        <button type="button" class="btn primary" data-action="confirm-share">${tt('share.ok')}</button></div>`
  } else if (m.id === 'remove') {
    const torrent = state.snapshot.torrents.find((x) => x.id === m.torrent)
    const name = torrent ? (torrent.name || '') : ''
    body = `<h1>${tt('remove.title')}</h1>
      <p dir="auto">${esc(name)}</p>
      <label class="check-row"><input type="checkbox" class="checkbox" id="delete-files"${m.deleteFiles ? ' checked' : ''}>
        ${tt('remove.deleteFiles')}</label>
      <div class="modal-actions"><button type="button" class="btn cancel" data-action="close-modal">${tt('common.cancel')}</button>
        <button type="button" class="btn danger" data-action="confirm-remove">${tt('remove.ok')}</button></div>`
  } else if (m.id === 'remove-many') {
    const names = state.snapshot.torrents
      .filter((x) => m.torrents.includes(x.id))
      .map((x) => `<li dir="auto">${esc(x.name || '…')}</li>`).join('')
    body = `<h1>${tt('remove.titleMany', { count: m.torrents.length })}</h1>
      <ul style="margin:0;padding:0;list-style:none;color:var(--text-2);max-height:160px;overflow:auto;overflow-wrap:anywhere">${names}</ul>
      <label class="check-row"><input type="checkbox" class="checkbox" id="delete-files">
        ${tt('remove.deleteFilesMany')}</label>
      <div class="modal-actions"><button type="button" class="btn cancel" data-action="close-modal">${tt('common.cancel')}</button>
        <button type="button" class="btn danger" data-action="confirm-remove-many">${tt('remove.ok')}</button></div>`
  }
  root.innerHTML = `<div class="modal"><div class="modal-background" data-action="close-modal"></div>
    <section class="modal-content" role="dialog" aria-modal="true">${body}</section></div>`

  const input = $('magnet-input')
  if (input) {
    input.focus()
    // Fill in a magnet link from the clipboard
    if (navigator.clipboard && navigator.clipboard.readText) {
      navigator.clipboard.readText().then((text) => {
        text = (text || '').trim()
        if (/^magnet:/i.test(text) && $('magnet-input') === input && !input.value) {
          input.value = text
          input.select()
        }
      }, () => {})
    }
    $('magnet-form').addEventListener('submit', (e) => {
      e.preventDefault()
      const source = input.value.trim()
      if (!source) return
      closeModal()
      addSource(source)
    })
  }
}

// ---------------------------------------------------------------------------
// Actions

async function refresh () {
  try {
    const snapshot = await invoke('snapshot')
    state.snapshot = snapshot
    state.engineError = null
    // File lists: once per torrent for the play buttons, and live for the open one
    for (const x of snapshot.torrents) {
      if (x.hasMetadata && (!state.files[x.id] || state.expanded === x.id)) {
        state.files[x.id] = await invoke('files', { id: x.id }).catch(() => state.files[x.id])
      }
    }
    for (const id of Object.keys(state.files)) {
      if (!snapshot.torrents.some((x) => String(x.id) === id)) delete state.files[id]
    }
    for (const id of state.selected) {
      if (!snapshot.torrents.some((x) => x.id === id)) state.selected.delete(id)
    }
    loadPosters(snapshot.torrents)
  } catch (err) {
    state.engineError = String(err)
  }
  render()
}

async function addSource (source) {
  try {
    await invoke('add', { source })
    sound('add')
    state.filter = 'all'
    state.view = 'home'
    await refresh()
  } catch (err) {
    showError(err)
  }
}

// Dropped .torrent files are added right away, subtitle files open in the
// translator, and anything else would be shared with everyone, so ask first.
async function addPaths (paths) {
  const torrents = paths.filter((p) => /\.torrent$/i.test(p))
  const subtitles = paths.filter((p) => SUBTITLE.test(p))
  const shares = paths.filter((p) => !/\.torrent$/i.test(p) && !SUBTITLE.test(p))
  if (torrents.length) await addPathList(torrents)
  if (subtitles.length) openTranslator({ path: subtitles[0], torrent: null })
  else if (shares.length) openModal({ id: 'share', paths: shares })
}

async function addPathList (paths) {
  try {
    await invoke('add_paths', { paths })
    sound('add')
    await refresh()
  } catch (err) {
    showError(err)
  }
}

function onSelectTorrent (el) {
  const id = Number(el.dataset.id)
  if (el.checked) state.selected.add(id)
  else state.selected.delete(id)
  renderHome()
}

function onSelectAll (el) {
  for (const t of visibleTorrents()) {
    if (el.checked) state.selected.add(t.id)
    else state.selected.delete(t.id)
  }
  renderHome()
}

// Keeps going if one fails, then reports the first error
async function removeMany (ids, deleteFiles) {
  let failure = null
  for (const id of ids) {
    try {
      await invoke('remove', { id, deleteFiles })
    } catch (err) {
      failure = failure ?? err
    }
  }
  if (failure !== null) throw failure
}

async function act (promise, after) {
  try {
    const result = await promise
    if (after) after(result)
  } catch (err) {
    showError(err)
  }
  await refresh()
}

async function copyText (text) {
  try {
    await navigator.clipboard.writeText(text)
  } catch (err) {
    const area = document.createElement('textarea')
    area.value = text
    document.body.appendChild(area)
    area.select()
    document.execCommand('copy')
    area.remove()
  }
}

function handleAction (el, event) {
  const action = el.dataset.action
  const id = el.dataset.id != null ? Number(el.dataset.id) : null

  // Clicks inside the action buttons shouldn't also toggle the file list
  if (action !== 'menu' && state.menu !== null && !el.closest('.menu')) state.menu = null

  switch (action) {
    case 'filter':
      state.filter = el.dataset.filter
      state.view = 'home'
      render()
      break
    case 'view':
      state.view = el.dataset.view
      if (state.view === 'preferences') {
        render()
        renderPreferences()
      } else if (state.view === 'library') {
        render()
        loadLibrary()
      } else if (state.view === 'translator') {
        openTranslator(null)
      } else {
        render()
      }
      break
    case 'open-downloads':
      act(invoke('open_downloads'))
      break
    case 'add-dialog':
      openModal({ id: 'add', tab: 'magnet' })
      break
    case 'add-tab':
      state.modal.tab = el.dataset.tab
      renderModal()
      break
    case 'close-modal':
      closeModal()
      break
    case 'choose-torrent-file':
      closeModal()
      act(invoke('choose_torrent_file'), (added) => { if (added != null) sound('add') })
      break
    case 'create':
      closeModal()
      act(invoke('create_torrent', { folder: el.dataset.folder === 'true' }), (magnet) => {
        if (!magnet) return
        copyText(magnet)
        sound('add')
        showInfo(t('msg.sharingCopied'))
      })
      break
    case 'toggle-files':
      if (event.target.closest('[data-stop]') && el.classList.contains('torrent-row')) return
      state.expanded = state.expanded === id ? null : id
      render()
      if (state.expanded != null) refresh()
      break
    case 'toggle': {
      const x = state.snapshot.torrents.find((y) => y.id === id)
      const active = x && (x.state === 'live' || x.state === 'initializing')
      sound(active ? 'disable' : 'enable')
      act(invoke(active ? 'pause' : 'resume', { id }))
      break
    }
    case 'play':
      sound('play')
      act(invoke('play', { id, file: null }))
      break
    case 'lib-play':
      sound('play')
      act(invoke('play', { id, file: Number(el.dataset.index) }), () => setTimeout(loadLibrary, 1500))
      break
    case 'open-file':
      act(invoke('open_file', { id, file: Number(el.dataset.index) }), (result) => {
        if (result === 'revealed') showInfo(t('msg.revealed'))
      })
      break
    case 'translate-torrent-sub':
      openTranslator({ path: null, torrent: [id, Number(el.dataset.index)] })
      break
    case 'menu':
      state.menu = state.menu === id ? null : id
      render()
      break
    case 'show-folder':
      state.menu = null
      act(invoke('show_in_folder', { id }))
      break
    case 'copy-magnet':
      state.menu = null
      act(invoke('magnet_link', { id }), (link) => { copyText(link); showInfo(t('msg.magnetCopied')) })
      break
    case 'save-torrent':
      state.menu = null
      act(invoke('save_torrent_file', { id }), (saved) => { if (saved) showInfo(t('msg.torrentSaved')) })
      break
    case 'remove':
    case 'remove-files':
      openModal({ id: 'remove', torrent: id, deleteFiles: action === 'remove-files' })
      break
    case 'confirm-share': {
      const paths = state.modal.paths
      closeModal()
      addPathList(paths).then(() => showInfo(t('msg.sharing', { names: paths.map(baseName).join(', ') })))
      break
    }
    case 'confirm-remove': {
      const deleteFiles = $('delete-files').checked
      const torrent = state.modal.torrent
      closeModal()
      sound('delete')
      if (state.expanded === torrent) state.expanded = null
      act(invoke('remove', { id: torrent, deleteFiles }))
      break
    }
    case 'remove-selected':
      openModal({ id: 'remove-many', torrents: selectedAmong(visibleTorrents()).map((x) => x.id) })
      break
    case 'confirm-remove-many': {
      const deleteFiles = $('delete-files').checked
      const ids = state.modal.torrents
      closeModal()
      sound('delete')
      if (ids.includes(state.expanded)) state.expanded = null
      act(removeMany(ids, deleteFiles))
      break
    }
    case 'set-pref':
      updatePref(el.dataset.pref, el.dataset.value)
      break
    case 'choose-download-dir':
      act(invoke('choose_download_dir'), (prefs) => {
        if (prefs) {
          state.info.prefs = prefs
          renderPreferences()
        }
      })
      break
    case 'make-default':
      act(invoke('make_default_app'), (isDefault) => {
        state.info.isDefaultApp = isDefault
        renderPreferences()
      })
      break
    case 'lib-filter':
      state.libFilter = el.dataset.filter
      renderLibrary()
      break
    case 'tr-pick':
      invoke('pick_subtitle').then((file) => {
        if (file) {
          state.tr.file = file
          state.tr.result = null
          render()
        }
      }, showError)
      break
    case 'tr-run':
      runTranslation()
      break
    case 'tr-cancel':
      invoke('cancel_translation')
      break
    case 'tr-reveal':
      if (state.tr.result) invoke('reveal_path', { path: state.tr.result }).catch(showError)
      break
    case 'open-help':
      invoke('open_help', { page: el.dataset.page }).catch(showError)
      break
    case 'save-key':
      saveKey(el.dataset.service)
      break
    case 'remove-key':
      removeKey(el.dataset.service)
      break
    case 'refresh-models':
      loadModels()
      break
  }
}

function onIncludeFile (el) {
  const id = Number(el.dataset.id)
  const files = state.files[id] || []
  const index = Number(el.dataset.index)
  const included = files.filter((f) => !f.padding && (f.index === index ? el.checked : f.included)).map((f) => f.index)
  const file = files.find((f) => f.index === index)
  if (file) file.included = el.checked
  act(invoke('set_files', { id, files: included }))
}

function onSetWatched (el) {
  const infoHash = el.dataset.hash
  const file = Number(el.dataset.index)
  invoke('set_watched', { infoHash, file, watched: el.checked }).then(loadLibrary, showError)
}

// ---------------------------------------------------------------------------
// Messages and sounds

function showPopover (message, kind) {
  const popover = $('popover')
  const el = document.createElement('div')
  el.className = kind
  el.setAttribute('role', kind === 'error' ? 'alert' : 'status')
  el.innerHTML = `${icon(kind === 'error' ? 'alert' : 'check')}<span dir="auto">${esc(message)}</span>`
  popover.appendChild(el)
  popover.classList.remove('hidden')
  setTimeout(() => {
    el.remove()
    if (!popover.children.length) popover.classList.add('hidden')
  }, kind === 'error' ? 6000 : 3000)
}

function showError (err) {
  sound('error')
  showPopover(String(err && err.message ? err.message : err), 'error')
}

function showInfo (message) {
  showPopover(message, 'info')
}

function sound (name) {
  if (!state.info || !state.info.prefs.sounds) return
  const audio = new Audio(`sound/${name}.wav`)
  audio.volume = name === 'startup' ? 0.25 : 0.35
  audio.play().catch(() => {})
}

// ---------------------------------------------------------------------------
// Start

async function start () {
  state.info = await invoke('app_info')
  setLanguage(state.info.prefs.language)
  render()

  const content = $('content')
  document.addEventListener('click', (e) => {
    const el = e.target.closest('[data-action]')
    if (el && el.tagName !== 'INPUT') {
      handleAction(el, e)
    } else if (state.menu !== null && !e.target.closest('.menu')) {
      state.menu = null
      render()
    }
  })
  document.addEventListener('change', (e) => {
    const el = e.target
    if (el.dataset.action === 'include-file') onIncludeFile(el)
    else if (el.dataset.action === 'select-torrent') onSelectTorrent(el)
    else if (el.id === 'select-all') onSelectAll(el)
    else if (el.dataset.action === 'set-watched') onSetWatched(el)
    else if (el.id === 'tr-save-as') state.tr.saveAs = el.checked
    else if (el.dataset.pref) onPrefInput(el)
  })
  content.addEventListener('input', (e) => {
    if (e.target.id === 'search') {
      state.search = e.target.value
      renderHome()
    } else if (e.target.id === 'lib-search') {
      state.libSearch = e.target.value
      renderLibrary()
    } else if (e.target.dataset.pref && e.target.type !== 'checkbox' && e.target.type !== 'radio' && e.target.tagName !== 'SELECT') {
      // While typing: save quietly, without redrawing (that would lose the cursor);
      // the change event redraws when the field is left
      setPrefQuietly(e.target)
    }
  })
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && e.target.id === 'key-input') {
      const button = document.querySelector('[data-action="save-key"]')
      if (button) saveKey(button.dataset.service)
    } else if (e.key === 'Escape') {
      if (state.modal) closeModal()
      else if (state.menu !== null) { state.menu = null; render() }
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'o') {
      e.preventDefault()
      act(invoke('choose_torrent_file'))
    }
  })
  // Pasting a magnet link anywhere outside a text field adds it
  document.addEventListener('paste', (e) => {
    if (e.target.closest && e.target.closest('input, textarea')) return
    const text = (e.clipboardData.getData('text') || '').trim()
    if (text && isTorrentLink(text)) addSource(text)
  })
  window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', render)

  // Dropped files and links: WebView2 takes the drop, files go to the app, which
  // reads their paths (see src-tauri/src/drop.rs) and sends back 'files-dropped'
  const hasDropData = (e) => e.dataTransfer && Array.from(e.dataTransfer.types)
    .some((x) => x === 'Files' || x === 'text/uri-list' || x === 'text/plain')
  let dragDepth = 0
  document.addEventListener('dragenter', (e) => {
    if (!hasDropData(e)) return
    dragDepth++
    document.body.classList.add('drag')
  })
  document.addEventListener('dragleave', (e) => {
    if (!hasDropData(e)) return
    dragDepth = Math.max(0, dragDepth - 1)
    if (!dragDepth) document.body.classList.remove('drag')
  })
  document.addEventListener('dragover', (e) => {
    if (!hasDropData(e)) return
    e.preventDefault()
    e.dataTransfer.dropEffect = 'copy'
  })
  document.addEventListener('drop', (e) => {
    e.preventDefault()
    dragDepth = 0
    document.body.classList.remove('drag')
    const files = e.dataTransfer && e.dataTransfer.files
    if (!files || !files.length) {
      const links = (e.dataTransfer.getData('text/uri-list') || e.dataTransfer.getData('text/plain') || '')
        .split(/\r?\n/).map((s) => s.trim()).filter((s) => s && !s.startsWith('#') && isTorrentLink(s))
      if (links.length) links.forEach(addSource)
      else showError(t('msg.dropHint'))
      return
    }
    if (window.chrome && window.chrome.webview && window.chrome.webview.postMessageWithAdditionalObjects) {
      window.chrome.webview.postMessageWithAdditionalObjects('flowbit-drop', files)
    } else {
      // Other webviews (Linux) don't say where dropped files are: .torrent files are
      // read instead, and sharing goes through Add torrent
      const torrents = Array.from(files).filter((file) => /\.torrent$/i.test(file.name))
      if (!torrents.length) showError(t('msg.dropShareElsewhere'))
      for (const file of torrents) {
        file.arrayBuffer().then((buf) => invoke('add_bytes', { bytes: Array.from(new Uint8Array(buf)) }))
          .then(() => { sound('add'); refresh() }, showError)
      }
    }
  })
  listen('files-dropped', (e) => addPaths(e.payload))

  listen('torrent-done', () => sound('done'))
  listen('torrent-added', () => { sound('add'); refresh() })
  listen('error', (e) => showError(e.payload))
  listen('info', (e) => { showInfo(e.payload); refresh() })
  listen('engine-error', (e) => { state.engineError = e.payload; render() })
  listen('poster', (e) => loadPoster(e.payload))
  listen('library-changed', () => { if (state.view === 'library') loadLibrary() })
  listen('subtitle-progress', (e) => {
    state.tr.progress = e.payload
    if (state.view === 'translator') renderTranslator()
  })

  sound('startup')
  await refresh()
  // Update once a second while the window shows
  setInterval(() => {
    if (!document.hidden) refresh()
  }, 1000)
  setInterval(() => {
    if (!document.hidden && state.view === 'library') loadLibrary()
  }, 5000)
}

start().catch(showError)
