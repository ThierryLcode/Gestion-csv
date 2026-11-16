"use strict";

// ===========================================================================
// Utilitaires
// ===========================================================================

const $ = (sel, root = document) => root.querySelector(sel);
const $$ = (sel, root = document) => [...root.querySelectorAll(sel)];

/** Crée un élément DOM : h("div", {class: "x", onclick: fn}, "texte", enfant…) */
function h(tag, attrs = {}, ...children) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v === null || v === undefined || v === false) continue;
    if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
    else if (k === "class") el.className = v;
    else if (k in el && typeof v !== "string") el[k] = v;
    else el.setAttribute(k, v === true ? "" : v);
  }
  for (const c of children.flat(Infinity)) {
    if (c === null || c === undefined || c === false) continue;
    el.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
  return el;
}

const fmt = (n) => Number(n).toLocaleString("fr-FR");
function fmtBytes(n) {
  const u = ["o", "Ko", "Mo", "Go", "To"];
  let i = 0;
  while (n >= 1024 && i < u.length - 1) { n /= 1024; i++; }
  return `${n.toLocaleString("fr-FR", { maximumFractionDigits: i ? 1 : 0 })} ${u[i]}`;
}
const DELIM_LABELS = { ",": "virgule", ";": "point-virgule", "\t": "tabulation", "|": "barre", ":": "deux-points", " ": "espace" };
const delimLabel = (d) => DELIM_LABELS[d] || d;

async function api(method, url, body) {
  const opts = { method, headers: {} };
  if (body instanceof FormData) opts.body = body;
  else if (body !== undefined) {
    opts.headers["Content-Type"] = "application/json";
    opts.body = JSON.stringify(body);
  }
  const res = await fetch(url, opts);
  if (res.status === 204) return null;
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(data.error || `Erreur ${res.status}`);
  return data;
}

/** Affiche un voile d'attente pendant fn(setLabel) ; les erreurs sont affichées. */
async function busy(label, fn) {
  const text = h("span", {}, label);
  const timer = h("small", {});
  const overlay = h("div", { class: "busy" }, h("div", { class: "busy-box" }, h("div", { class: "spinner" }), text, timer));
  document.body.append(overlay);
  const start = Date.now();
  const tick = setInterval(() => {
    const s = Math.round((Date.now() - start) / 1000);
    if (s >= 2) timer.textContent = `${s} s`;
  }, 500);
  try {
    return await fn((l) => (text.textContent = l));
  } catch (e) {
    showMessage(e.message, "error");
    return undefined;
  } finally {
    clearInterval(tick);
    overlay.remove();
  }
}

function showMessage(text, kind = "ok") {
  const m = $("#message");
  m.textContent = text;
  m.className = `message ${kind}`;
  m.hidden = false;
  clearTimeout(showMessage.timer);
  if (kind === "ok") showMessage.timer = setTimeout(() => (m.hidden = true), 10000);
}

function download(filename, content, type = "text/plain") {
  const a = h("a", { href: URL.createObjectURL(new Blob([content], { type })), download: filename });
  document.body.append(a);
  a.click();
  setTimeout(() => { URL.revokeObjectURL(a.href); a.remove(); }, 1000);
}

// ===========================================================================
// État
// ===========================================================================

const state = {
  files: [],
  active: null,
  previewOffset: 0,
  pageSize: 100,
  valuesCache: new Map(),
  profiles: new Map(),
  recipes: [],
  tab: "preview",
};

const fileById = (id) => state.files.find((f) => f.id === id);
const activeFile = () => fileById(state.active);

function exportParams() {
  return new URLSearchParams({
    delimiter: $("#ex-delim").value,
    encoding: $("#ex-enc").value,
    quote: $("#ex-quote").value,
    line_ending: $("#ex-eol").value,
    headers: $("#ex-headers").checked ? "true" : "false",
  }).toString();
}
const downloadUrl = (id) => `/api/files/${id}/download?${exportParams()}`;
const zipUrl = (ids, name) => `/api/zip?ids=${ids.join(",")}&name=${encodeURIComponent(name)}&${exportParams()}`;

async function columnValues(fileId, column) {
  const key = `${fileId}\u0000${column}`;
  if (!state.valuesCache.has(key)) {
    const p = api("GET", `/api/files/${fileId}/values?column=${encodeURIComponent(column)}&limit=1000`);
    state.valuesCache.set(key, p);
    p.catch(() => state.valuesCache.delete(key));
  }
  return state.valuesCache.get(key);
}

// ===========================================================================
// Composants
// ===========================================================================

function colOptions(columns, selected) {
  return (columns || []).map((c) => h("option", { value: c, selected: c === selected }, c));
}

/** Liste à cocher dont l'ordre de sélection est mémorisé (pastilles numérotées). */
function orderedChecklist(box, keys, getOrder, setOrder, label, onChange) {
  const update = () =>
    $$("label[data-key]", box).forEach((l) => {
      const pos = getOrder().indexOf(l.dataset.key);
      const badge = $(".order", l);
      badge.hidden = pos < 0;
      badge.textContent = pos + 1;
    });
  box.replaceChildren(
    ...keys.map((key) =>
      h("label", { class: "check", "data-key": key },
        h("input", {
          type: "checkbox",
          checked: getOrder().includes(key),
          onchange: (e) => {
            setOrder(e.target.checked ? [...getOrder(), key] : getOrder().filter((x) => x !== key));
            update();
            onChange?.();
          },
        }),
        h("span", {}, h("span", { class: "order" }), " ", label(key))))
  );
  update();
}

/** Sélecteur de colonnes multiples, avec option « toutes les colonnes ». */
function colPicker(columns, { all = false, selected = [] } = {}) {
  let order = [...selected];
  const box = h("div", { class: "checklist cols" });
  orderedChecklist(box, columns, () => order, (o) => (order = o), (c) => c);
  const wrap = h("div", { class: "picker" });
  let allCb = null;
  if (all) {
    allCb = h("input", { type: "checkbox", checked: !selected.length, onchange: () => (box.hidden = allCb.checked) });
    wrap.append(h("label", { class: "check" }, allCb, h("b", {}, "Toutes les colonnes")));
    box.hidden = allCb.checked;
  }
  wrap.append(box);
  return { el: wrap, get: () => (allCb && allCb.checked ? [] : order) };
}

/** Aperçu compact d'un fichier résultat, avec actions. */
async function miniPreview(container, fileId, title) {
  const f = fileById(fileId);
  container.replaceChildren(h("p", { class: "hint" }, "Chargement de l'aperçu…"));
  try {
    const data = await api("GET", `/api/files/${fileId}/rows?offset=0&limit=50`);
    container.replaceChildren(
      h("div", { class: "card" },
        h("div", { class: "card-head" },
          h("h3", {}, title || f?.name || "Résultat"),
          h("div", { class: "actions tight" },
            h("button", { class: "btn small", onclick: () => setActive(fileId, "preview") }, "Ouvrir dans l'aperçu"),
            h("a", { class: "btn small primary", href: downloadUrl(fileId) }, "Télécharger"))),
        h("p", { class: "hint" }, `${fmt(data.total)} lignes · ${data.headers.length} colonnes${data.total > 50 ? " — 50 premières lignes affichées" : ""}`),
        h("div", { class: "table-wrap small" }, dataTable(data))));
  } catch (e) {
    container.replaceChildren(h("p", { class: "message error" }, e.message));
  }
}

function dataTable(data) {
  return h("table", { class: "data" },
    h("thead", {}, h("tr", {}, h("th", { class: "num" }, "#"), data.headers.map((c) => h("th", { title: c }, c)))),
    h("tbody", {}, data.rows.map((r, i) =>
      h("tr", {}, h("td", { class: "num" }, fmt(data.offset + i + 1)), r.map((v) => h("td", { title: v }, v))))));
}

// ===========================================================================
// Onglets et fichier de travail
// ===========================================================================

const panels = {}; // nom → { single, render(), onFiles() }

function openTab(name) {
  state.tab = name;
  $$(".tab").forEach((t) => t.classList.toggle("active", t.dataset.tab === name));
  $$(".panel").forEach((p) => p.classList.toggle("active", p.id === `panel-${name}`));
  const single = $(`.tab[data-tab="${name}"]`).hasAttribute("data-single");
  $("#active-bar").hidden = !single;
  syncPanel(name);
}
$$(".tab").forEach((t) => t.addEventListener("click", () => openTab(t.dataset.tab)));

/** (Re)construit un onglet si le fichier de travail a changé depuis son dernier rendu. */
function syncPanel(name, force = false) {
  const p = panels[name];
  if (!p) return;
  const key = p.single ? state.active : "";
  if (force || p.renderedFor !== key) {
    p.renderedFor = key;
    p.render();
  }
}

function setActive(id, tab) {
  state.active = id || null;
  $("#active-file").value = state.active || "";
  const f = activeFile();
  $("#active-meta").textContent = f
    ? `${fmt(f.row_count)} lignes · ${f.columns.length} colonnes · ${fmtBytes(f.size)} · séparateur source « ${delimLabel(f.delimiter)} » · ${f.encoding}`
    : "";
  renderFileList();
  if (tab) openTab(tab);
  else syncPanel(state.tab);
}
$("#active-file").addEventListener("change", (e) => setActive(e.target.value));

function requireActive() {
  const f = activeFile();
  if (!f) showMessage("Choisissez d'abord un fichier de travail.", "error");
  return f;
}

/** Après une opération : message, rafraîchissement et ouverture du résultat. */
async function resultDone(info, done, { stay = false } = {}) {
  showMessage(`${done} : « ${info.name} » — ${fmt(info.row_count)} lignes, ${info.columns.length} colonnes.\nLe résultat est ajouté à vos fichiers : vous pouvez le télécharger, poursuivre les opérations ou l'enregistrer comme recette.`);
  await refreshFiles();
  if (!stay) setActive(info.id, "preview");
}

/** Applique une étape au fichier de travail. */
async function runStep(step, done, label) {
  const f = requireActive();
  if (!f) return;
  const info = await busy(label, () => api("POST", "/api/step", { id: f.id, step }));
  if (info) await resultDone(info, done);
}

// ===========================================================================
// Import
// ===========================================================================

$("#imp-delim").addEventListener("change", (e) => ($("#imp-delim-custom-wrap").hidden = e.target.value !== "custom"));

function postWithProgress(url, fd, onProgress) {
  return new Promise((resolve, reject) => {
    const x = new XMLHttpRequest();
    x.open("POST", url);
    x.upload.onprogress = (e) => e.lengthComputable && onProgress(e.loaded, e.total);
    x.upload.onload = () => onProgress(null);
    x.onload = () => {
      let d = {};
      try { d = JSON.parse(x.responseText); } catch { /* réponse vide */ }
      if (x.status < 300) resolve(d);
      else reject(new Error(d.error || `Erreur ${x.status}`));
    };
    x.onerror = () => reject(new Error("Connexion au serveur perdue pendant l'envoi"));
    x.send(fd);
  });
}

async function uploadFiles(fileList) {
  if (!fileList.length) return;
  const fd = new FormData();
  let delim = $("#imp-delim").value;
  if (delim === "custom") delim = $("#imp-delim-custom").value || "auto";
  fd.append("delimiter", delim);
  fd.append("encoding", $("#imp-enc").value);
  fd.append("has_headers", $("#imp-headers").checked);
  fd.append("trim", $("#imp-trim").checked);
  for (const f of fileList) fd.append("file", f, f.name);
  const added = await busy("Envoi…", (setLabel) =>
    postWithProgress("/api/files", fd, (loaded, total) => {
      if (loaded === null) setLabel("Analyse et conversion du fichier…");
      else setLabel(`Envoi : ${Math.round((loaded / total) * 100)} % (${fmtBytes(loaded)} / ${fmtBytes(total)})`);
    }));
  if (!added) return;
  await refreshFiles();
  showMessage(added
    .map((f) => `« ${f.name} » importé : ${fmt(f.row_count)} lignes, ${f.columns.length} colonnes, séparateur ${delimLabel(f.delimiter)}, ${f.encoding}`)
    .join("\n"));
  setActive(added[added.length - 1].id, "preview");
}

const fileInput = $("#file-input");
fileInput.addEventListener("change", () => { uploadFiles([...fileInput.files]); fileInput.value = ""; });
const dz = $("#dropzone");
["dragenter", "dragover"].forEach((ev) => dz.addEventListener(ev, (e) => { e.preventDefault(); dz.classList.add("over"); }));
["dragleave", "drop"].forEach((ev) => dz.addEventListener(ev, () => dz.classList.remove("over")));
dz.addEventListener("drop", (e) => { e.preventDefault(); uploadFiles([...e.dataTransfer.files]); });

// ===========================================================================
// Liste des fichiers
// ===========================================================================

async function refreshFiles() {
  state.files = await api("GET", "/api/files");
  if (state.active && !fileById(state.active)) state.active = null;
  renderFileSelects();
  setActive(state.active);
  for (const p of Object.values(panels)) p.onFiles?.();
}

function renderFileList() {
  const ul = $("#file-list");
  $("#empty-files").hidden = state.files.length > 0;
  $("#clear-all").hidden = state.files.length === 0;
  ul.replaceChildren(...state.files.map((f) =>
    h("li", { class: `file-item${f.id === state.active ? " selected" : ""}`, onclick: () => setActive(f.id) },
      h("span", { class: "file-name" }, f.name),
      h("span", { class: "file-meta" },
        h("span", { class: "badge" }, f.origin),
        f.steps.length ? h("span", { class: "badge accent", title: "Opérations enregistrables comme recette" }, `${f.steps.length} étape${f.steps.length > 1 ? "s" : ""}`) : null,
        `${fmt(f.row_count)} lignes · ${f.columns.length} col. · ${fmtBytes(f.size)}`),
      h("span", { class: "file-actions", onclick: (e) => e.stopPropagation() },
        h("a", { class: "link", href: downloadUrl(f.id), title: "Télécharger avec les options d'export" }, "Télécharger"),
        h("button", { class: "link", onclick: () => renameFile(f) }, "Renommer"),
        f.steps.length ? h("button", { class: "link", onclick: () => { $("#rc-from").value = $("#rc-from").dataset.chosen = f.id; openTab("recipes"); renderRecipePreview(); $("#rc-name").focus(); } }, "Recette…") : null,
        h("button", { class: "link danger", onclick: () => deleteFile(f) }, "Supprimer")))));
}

async function renameFile(f) {
  const name = prompt("Nouveau nom du fichier :", f.name);
  if (!name || name === f.name) return;
  await busy("Renommage…", () => api("PATCH", `/api/files/${f.id}`, { name }));
  await refreshFiles();
}

async function deleteFile(f) {
  if (!confirm(`Retirer « ${f.name} » de l'espace de travail ?`)) return;
  await busy("Suppression…", () => api("DELETE", `/api/files/${f.id}`));
  await refreshFiles();
}

$("#clear-all").addEventListener("click", async () => {
  if (!confirm("Retirer tous les fichiers de l'espace de travail ?")) return;
  await busy("Suppression…", () => api("DELETE", "/api/files"));
  state.valuesCache.clear();
  state.profiles.clear();
  await refreshFiles();
});

/** Remplit toutes les listes déroulantes de fichiers en conservant la sélection. */
function renderFileSelects() {
  for (const sel of $$(".file-select")) {
    const current = sel.value;
    sel.replaceChildren(
      h("option", { value: "" }, state.files.length ? "— choisir un fichier —" : "— aucun fichier —"),
      ...state.files.map((f) => h("option", { value: f.id }, `${f.name} (${fmt(f.row_count)} lignes)`)));
    sel.value = fileById(current) ? current : "";
    if (sel.value !== current && sel.id !== "active-file") sel.dispatchEvent(new Event("change"));
  }
}

// Les liens de téléchargement suivent les options d'export.
$$("#ex-delim, #ex-enc, #ex-quote, #ex-eol, #ex-headers").forEach((el) =>
  el.addEventListener("change", () => {
    renderFileList();
    updateDownloadLink();
    $$("a[data-zip]").forEach((a) => (a.href = zipUrl(a.dataset.zip.split(","), a.dataset.name)));
    $$("a[data-dl]").forEach((a) => (a.href = downloadUrl(a.dataset.dl)));
  }));

// ===========================================================================
// Aperçu
// ===========================================================================

function updateDownloadLink() {
  const a = $("#pv-download");
  a.classList.toggle("disabled", !state.active);
  a.href = state.active ? downloadUrl(state.active) : "#";
}

async function showPreview(offset = 0) {
  updateDownloadLink();
  const table = $("#pv-table");
  if (!state.active) {
    table.replaceChildren();
    $("#pv-range").textContent = "Importez ou choisissez un fichier.";
    $$("#pv-first, #pv-prev, #pv-next").forEach((b) => (b.disabled = true));
    return;
  }
  try {
    const data = await api("GET", `/api/files/${state.active}/rows?offset=${offset}&limit=${state.pageSize}`);
    state.previewOffset = data.offset;
    table.replaceWith(Object.assign(dataTable(data), { id: "pv-table" }));
    const end = data.offset + data.rows.length;
    $("#pv-range").textContent = data.total ? `Lignes ${fmt(data.offset + 1)} à ${fmt(end)} sur ${fmt(data.total)}` : "Aucune ligne";
    $("#pv-first").disabled = $("#pv-prev").disabled = data.offset === 0;
    $("#pv-next").disabled = end >= data.total;
  } catch (e) {
    showMessage(e.message, "error");
  }
}

panels.preview = { single: true, render: () => showPreview(0) };
$("#pv-first").addEventListener("click", () => showPreview(0));
$("#pv-prev").addEventListener("click", () => showPreview(Math.max(0, state.previewOffset - state.pageSize)));
$("#pv-next").addEventListener("click", () => showPreview(state.previewOffset + state.pageSize));
$("#pv-goto").addEventListener("change", (e) => {
  const n = parseInt(e.target.value, 10);
  if (n > 0) showPreview(n - 1);
});

// ===========================================================================
// Profil
// ===========================================================================

const KIND_CLASS = { entier: "k-num", "décimal": "k-num", date: "k-date", "booléen": "k-bool", texte: "k-text", vide: "k-empty" };

function renderProfile(p) {
  const f = activeFile();
  const pct = (n) => (p.rows ? `${Math.round((n / p.rows) * 1000) / 10} %` : "—");
  const emptyCols = p.columns.filter((c) => c.empty > 0).length;
  $("#pf-result").replaceChildren(
    h("div", { class: "stats" },
      h("div", { class: "stat" }, h("b", {}, fmt(p.rows)), "lignes"),
      h("div", { class: "stat" }, h("b", {}, fmt(p.columns.length)), "colonnes"),
      h("div", { class: "stat" }, h("b", {}, fmt(emptyCols)), "colonnes avec des vides"),
      h("div", { class: "stat" }, h("b", {}, fmtBytes(f?.size || 0)), "taille")),
    h("div", { class: "table-wrap" },
      h("table", { class: "data profile" },
        h("thead", {}, h("tr", {}, ["Colonne", "Type", "Remplissage", "Distinctes", "Min", "Max", "Moyenne", "Longueur", "Valeurs les plus fréquentes"].map((t) => h("th", {}, t)))),
        h("tbody", {}, p.columns.map((c) => {
          const fill = p.rows ? c.filled / p.rows : 0;
          return h("tr", {},
            h("td", { title: c.name }, h("b", {}, c.name)),
            h("td", {}, h("span", { class: `kind ${KIND_CLASS[c.kind] || ""}`, title: c.kind !== "texte" && c.type_ratio < 1 ? `${Math.round(c.type_ratio * 100)} % des valeurs` : "" }, c.kind)),
            h("td", { title: `${fmt(c.filled)} remplies, ${fmt(c.empty)} vides` },
              h("div", { class: "fill" }, h("div", { class: "fill-bar", style: `width:${fill * 100}%` })),
              h("small", {}, `${pct(c.filled)}${c.empty ? ` · ${fmt(c.empty)} vides` : ""}`)),
            h("td", { class: "num-right" }, `${c.distinct_capped ? "> " : ""}${fmt(c.distinct)}`, c.distinct === c.filled && c.filled > 1 && !c.distinct_capped ? h("small", { class: "tag" }, "unique") : null),
            h("td", { title: c.min ?? "" }, c.min ?? "—"),
            h("td", { title: c.max ?? "" }, c.max ?? "—"),
            h("td", { class: "num-right" }, c.mean == null ? "—" : c.mean.toLocaleString("fr-FR", { maximumFractionDigits: 2 })),
            h("td", { class: "num-right" }, c.filled ? (c.min_length === c.max_length ? `${c.min_length}` : `${c.min_length}–${c.max_length}`) : "—"),
            h("td", { class: "top" }, c.top.slice(0, 5).map(([v, n]) => h("span", { class: "chip small", title: v }, `${v} (${fmt(n)})`))));
        })))));
  $("#pf-export").disabled = false;
}

async function runProfile() {
  const f = requireActive();
  if (!f) return;
  const p = await busy("Analyse du fichier…", () => api("GET", `/api/files/${f.id}/profile`));
  if (p) {
    state.profiles.set(f.id, p);
    if (state.active === f.id) renderProfile(p);
  }
}

panels.profile = {
  single: true,
  render() {
    $("#pf-export").disabled = true;
    const p = state.profiles.get(state.active);
    if (p) return renderProfile(p);
    $("#pf-result").replaceChildren();
    if (state.active) runProfile();
  },
};
$("#pf-run").addEventListener("click", runProfile);
$("#pf-export").addEventListener("click", () => {
  const p = state.profiles.get(state.active);
  if (!p) return;
  const esc = (v) => (/[";\n]/.test(String(v)) ? `"${String(v).replace(/"/g, '""')}"` : String(v ?? ""));
  const lines = [["colonne", "type", "remplies", "vides", "distinctes", "min", "max", "moyenne", "longueur_min", "longueur_max", "valeurs_frequentes"]];
  for (const c of p.columns) {
    lines.push([c.name, c.kind, c.filled, c.empty, c.distinct, c.min ?? "", c.max ?? "", c.mean ?? "", c.min_length, c.max_length,
      c.top.map(([v, n]) => `${v} (${n})`).join(" | ")]);
  }
  const name = (activeFile()?.name || "fichier").replace(/\.[^.]+$/, "");
  download(`profil_${name}.csv`, "﻿" + lines.map((l) => l.map(esc).join(";")).join("\r\n"), "text/csv");
});

// ===========================================================================
// Colonnes
// ===========================================================================

let columnRows = []; // [{ source, keep, name }]

function renderColumnEditor() {
  const list = $("#cl-list");
  list.replaceChildren(...columnRows.map((c, i) =>
    h("div", { class: `col-row${c.keep ? "" : " off"}` },
      h("input", { type: "checkbox", checked: c.keep, title: "Conserver", onchange: (e) => { c.keep = e.target.checked; renderColumnEditor(); } }),
      h("span", { class: "col-src", title: c.source }, c.source),
      h("input", { value: c.name, placeholder: "nouveau nom (facultatif)", oninput: (e) => (c.name = e.target.value) }),
      h("button", { class: "btn small", title: "Monter", disabled: i === 0, onclick: () => moveColumn(i, -1) }, "↑"),
      h("button", { class: "btn small", title: "Descendre", disabled: i === columnRows.length - 1, onclick: () => moveColumn(i, 1) }, "↓"))));
}

function moveColumn(i, d) {
  [columnRows[i], columnRows[i + d]] = [columnRows[i + d], columnRows[i]];
  renderColumnEditor();
}

panels.columns = {
  single: true,
  render() {
    columnRows = (activeFile()?.columns || []).map((c) => ({ source: c, keep: true, name: "" }));
    renderColumnEditor();
  },
};
$("#cl-all").addEventListener("click", () => { columnRows.forEach((c) => (c.keep = true)); renderColumnEditor(); });
$("#cl-none").addEventListener("click", () => { columnRows.forEach((c) => (c.keep = false)); renderColumnEditor(); });
$("#cl-az").addEventListener("click", () => { columnRows.sort((a, b) => a.source.localeCompare(b.source, "fr")); renderColumnEditor(); });
$("#cl-reset").addEventListener("click", () => {
  const order = activeFile()?.columns || [];
  columnRows.sort((a, b) => order.indexOf(a.source) - order.indexOf(b.source));
  renderColumnEditor();
});
$("#cl-run").addEventListener("click", () => {
  const columns = columnRows.filter((c) => c.keep).map((c) => ({ source: c.source, name: c.name.trim() }));
  if (!columns.length) return showMessage("Conservez au moins une colonne.", "error");
  runStep({ type: "columns", columns }, "Colonnes modifiées", "Réorganisation des colonnes…");
});

// ===========================================================================
// Tri
// ===========================================================================

function addSortKey(column) {
  const f = activeFile();
  if (!f) return;
  const row = h("div", { class: "key-row sort" },
    h("select", { class: "s-col" }, colOptions(f.columns, column)),
    h("select", { class: "s-mode" },
      h("option", { value: "text" }, "Alphabétique"),
      h("option", { value: "number" }, "Numérique"),
      h("option", { value: "natural" }, "Naturel (a2 < a10)"),
      h("option", { value: "date" }, "Chronologique")),
    h("select", { class: "s-dir" }, h("option", { value: "asc" }, "Croissant"), h("option", { value: "desc" }, "Décroissant")),
    h("button", { class: "link danger", title: "Retirer", onclick: () => row.remove() }, "✕"));
  $("#so-keys").append(row);
}

panels.sort = {
  single: true,
  render() {
    $("#so-keys").replaceChildren();
    if (activeFile()) addSortKey();
  },
};
$("#so-add").addEventListener("click", () => requireActive() && addSortKey());
$("#so-run").addEventListener("click", () => {
  const keys = $$(".key-row.sort").map((r) => ({
    column: $(".s-col", r).value,
    mode: $(".s-mode", r).value,
    descending: $(".s-dir", r).value === "desc",
  }));
  if (!keys.length) return showMessage("Ajoutez au moins un critère de tri.", "error");
  runStep({ type: "sort", keys }, "Tri terminé", "Tri en cours…");
});

// ===========================================================================
// Doublons
// ===========================================================================

let dedupeCols = [];
panels.dedupe = {
  single: true,
  render() {
    dedupeCols = [];
    orderedChecklist($("#dd-cols"), activeFile()?.columns || [], () => dedupeCols, (o) => (dedupeCols = o), (c) => c);
  },
};
$$("input[name=dd-mode]").forEach((r) =>
  r.addEventListener("change", () => {
    const mode = $("input[name=dd-mode]:checked").value;
    $("#dd-keep-wrap").hidden = !["remove", "removed"].includes(mode);
  }));
$("#dd-run").addEventListener("click", () => {
  const mode = $("input[name=dd-mode]:checked").value;
  runStep({
    type: "dedupe",
    columns: dedupeCols,
    mode,
    keep: $("input[name=dd-keep]:checked").value,
    case_insensitive: $("#dd-ci").checked,
    trim: $("#dd-trim").checked,
  }, mode === "remove" ? "Doublons supprimés" : "Doublons isolés", "Recherche des doublons…");
});

// ===========================================================================
// Filtre
// ===========================================================================

const MULTI_OPS = ["in", "not_in", "contains", "not_contains", "starts_with", "ends_with"];
const SINGLE_OPS = ["gt", "ge", "lt", "le", "regex"];
const conditions = [];

function addCondition() {
  const f = requireActive();
  if (!f) return;
  const el = $("#tpl-condition").content.firstElementChild.cloneNode(true);
  const cond = { el, values: new Set() };
  conditions.push(cond);
  $(".c-col", el).replaceChildren(...colOptions(f.columns));
  $(".c-col", el).addEventListener("change", () => { cond.values.clear(); renderConditionValues(cond); });
  $(".c-op", el).addEventListener("change", () => renderConditionValues(cond));
  $(".c-remove", el).addEventListener("click", () => { conditions.splice(conditions.indexOf(cond), 1); el.remove(); });
  $("#ft-conditions").append(el);
  renderConditionValues(cond);
}

async function renderConditionValues(cond) {
  const box = $(".c-values", cond.el);
  const op = $(".c-op", cond.el).value;
  const column = $(".c-col", cond.el).value;
  const fileId = state.active;
  box.replaceChildren();

  if (SINGLE_OPS.includes(op)) {
    box.append(h("input", { class: "c-v1", placeholder: op === "regex" ? "ex. ^(Paris|Lyon)$" : "valeur (nombre, date JJ/MM/AAAA ou texte)" }));
    return;
  }
  if (op === "between") {
    box.append(h("div", { class: "grid2" },
      h("label", {}, "Valeur minimale", h("input", { class: "c-v1" })),
      h("label", {}, "Valeur maximale", h("input", { class: "c-v2" }))));
    return;
  }
  if (!MULTI_OPS.includes(op)) return; // vide / non vide

  const chips = h("div", { class: "chips" });
  const listId = `dl-${Math.random().toString(36).slice(2)}`;
  const datalist = h("datalist", { id: listId });
  const input = h("input", { class: "c-pending", placeholder: "Saisissez une valeur puis Entrée (ou collez une liste)", list: listId });
  let pickerBoxes = [];

  const renderChips = () => {
    chips.replaceChildren(...[...cond.values].map((v) =>
      h("span", { class: "chip" }, v === "" ? "(vide)" : v,
        h("button", { title: "Retirer", onclick: () => { cond.values.delete(v); renderChips(); } }, "✕"))));
    pickerBoxes.forEach(([cb, v]) => (cb.checked = cond.values.has(v)));
  };
  const addValue = () => {
    input.value.split(/\r?\n/).map((s) => s.trim()).filter((s) => s !== "").forEach((p) => cond.values.add(p));
    input.value = "";
    renderChips();
  };
  input.addEventListener("keydown", (e) => { if (e.key === "Enter") { e.preventDefault(); addValue(); } });
  input.addEventListener("paste", (e) => {
    const text = e.clipboardData.getData("text");
    if (text.includes("\n")) { e.preventDefault(); input.value = text; addValue(); }
  });
  box.append(chips, h("div", { class: "value-input" }, input, h("button", { class: "btn", onclick: addValue }, "Ajouter")), datalist);

  if (op === "in" || op === "not_in") {
    const picker = h("details", { class: "value-picker" }, h("summary", {}, "Choisir parmi les valeurs existantes…"));
    box.append(picker);
    try {
      const data = await columnValues(fileId, column);
      datalist.replaceChildren(...data.values.slice(0, 500).map(([v]) => h("option", { value: v })));
      const search = h("input", { placeholder: "Rechercher une valeur…" });
      const list = h("div", { class: "checklist" });
      pickerBoxes = data.values.map(([v, n]) => {
        const cb = h("input", { type: "checkbox", checked: cond.values.has(v),
          onchange: (e) => { e.target.checked ? cond.values.add(v) : cond.values.delete(v); renderChips(); } });
        const label = h("label", { class: "check" }, cb, h("span", { class: "v" }, v === "" ? "(vide)" : v), h("span", { class: "count" }, fmt(n)));
        label.dataset.v = v.toLowerCase();
        list.append(label);
        return [cb, v];
      });
      search.addEventListener("input", () => {
        const q = search.value.toLowerCase();
        $$("label", list).forEach((l) => (l.hidden = !l.dataset.v.includes(q)));
      });
      const setVisible = (checked) => $$("label", list).filter((l) => !l.hidden).forEach((l) => {
        const cb = $("input", l);
        cb.checked = checked;
        cb.dispatchEvent(new Event("change"));
      });
      picker.append(
        h("div", { class: "value-input" }, search,
          h("button", { class: "btn small", onclick: () => setVisible(true) }, "Tout cocher"),
          h("button", { class: "btn small", onclick: () => setVisible(false) }, "Tout décocher")),
        list,
        data.distinct > data.values.length
          ? h("p", { class: "hint" }, `Seules les ${fmt(data.values.length)} valeurs les plus fréquentes sur ${data.capped ? "plus de " : ""}${fmt(data.distinct)} sont listées ; saisissez les autres manuellement.`)
          : null);
    } catch (e) {
      picker.append(h("p", { class: "hint" }, e.message));
    }
  }
  renderChips();
}

panels.filter = {
  single: true,
  render() {
    conditions.length = 0;
    $("#ft-conditions").replaceChildren();
    if (activeFile()) addCondition();
  },
};
$("#ft-add").addEventListener("click", addCondition);
$("#ft-run").addEventListener("click", () => {
  if (!conditions.length) return showMessage("Ajoutez au moins une condition.", "error");
  const conds = conditions.map(({ el, values }) => {
    const op = $(".c-op", el).value;
    let vals = [];
    if (MULTI_OPS.includes(op)) {
      const pending = $(".c-pending", el)?.value.trim();
      if (pending) values.add(pending);
      vals = [...values];
    } else if (SINGLE_OPS.includes(op)) vals = [$(".c-v1", el).value];
    else if (op === "between") vals = [$(".c-v1", el).value, $(".c-v2", el).value];
    return { column: $(".c-col", el).value, operator: op, values: vals, case_sensitive: $(".c-case", el).checked };
  });
  runStep({ type: "filter", conditions: conds, combine: $("input[name=ft-combine]:checked").value }, "Filtrage terminé", "Filtrage en cours…");
});

// ===========================================================================
// Nettoyer & calculer
// ===========================================================================

const DATE_FORMATS = ["AAAA-MM-JJ", "JJ/MM/AAAA", "JJ-MM-AAAA", "JJ.MM.AAAA", "AAAA/MM/JJ", "MM/JJ/AAAA", "JJ/MM/AA", "AAAAMMJJ", "AAAA-MM-JJ HH:mm:ss", "JJ/MM/AAAA HH:mm"];

function formatSelect(withAuto) {
  const sel = h("select", {},
    withAuto ? h("option", { value: "" }, "Détection automatique") : null,
    DATE_FORMATS.map((f) => h("option", { value: f }, f)),
    h("option", { value: "custom" }, "Autre…"));
  const custom = h("input", { placeholder: "ex. JJ MM AAAA", hidden: true });
  sel.addEventListener("change", () => (custom.hidden = sel.value !== "custom"));
  return { el: h("div", { class: "fmt" }, sel, custom), get: () => (sel.value === "custom" ? custom.value : sel.value) };
}

const TRANSFORMS = {
  trim: {
    title: "Supprimer les espaces superflus",
    build(cols) {
      const p = colPicker(cols, { all: true });
      const collapse = h("input", { type: "checkbox", checked: true });
      return { el: [p.el, h("label", { class: "check" }, collapse, "Réduire aussi les espaces multiples à un seul")], get: () => ({ op: "trim", columns: p.get(), collapse: collapse.checked }) };
    },
  },
  case: {
    title: "Changer la casse",
    build(cols) {
      const p = colPicker(cols, { all: true });
      const mode = h("select", {},
        h("option", { value: "upper" }, "MAJUSCULES"), h("option", { value: "lower" }, "minuscules"),
        h("option", { value: "title" }, "Première Lettre De Chaque Mot"), h("option", { value: "sentence" }, "Première lettre de la cellule"));
      return { el: [p.el, h("label", {}, "Transformation", mode)], get: () => ({ op: "case", columns: p.get(), mode: mode.value }) };
    },
  },
  replace: {
    title: "Rechercher / remplacer",
    build(cols) {
      const p = colPicker(cols, { all: true });
      const search = h("input", { placeholder: "texte à rechercher" });
      const rep = h("input", { placeholder: "remplacer par (vide = supprimer)" });
      const regex = h("input", { type: "checkbox" });
      const cs = h("input", { type: "checkbox" });
      return {
        el: [p.el, h("div", { class: "grid2" }, h("label", {}, "Rechercher", search), h("label", {}, "Remplacer par", rep)),
          h("div", { class: "radios inline" }, h("label", { class: "check" }, regex, "Expression régulière ($1, $2… pour les groupes)"), h("label", { class: "check" }, cs, "Respecter la casse"))],
        get: () => ({ op: "replace", columns: p.get(), search: search.value, replacement: rep.value, regex: regex.checked, case_sensitive: cs.checked }),
      };
    },
  },
  date: {
    title: "Convertir des dates",
    build(cols) {
      const p = colPicker(cols);
      const input = formatSelect(true);
      const output = formatSelect(false);
      const clear = h("input", { type: "checkbox" });
      return {
        el: [p.el, h("div", { class: "grid2" }, h("label", {}, "Format d'origine", input.el), h("label", {}, "Nouveau format", output.el)),
          h("p", { class: "hint" }, "Jetons : AAAA (année), AA, MM (mois), M, JJ (jour), J, HH (heure), mm (minutes), ss (secondes)."),
          h("label", { class: "check" }, clear, "Vider les cellules dont la date n'est pas reconnue (sinon elles restent inchangées)")],
        get: () => ({ op: "date", columns: p.get(), input: input.get(), output: output.get(), clear_invalid: clear.checked }),
      };
    },
  },
  number: {
    title: "Normaliser des nombres",
    build(cols) {
      const p = colPicker(cols);
      const dec = h("select", {}, h("option", { value: "." }, "Point : 1234.5 (format international)"), h("option", { value: "," }, "Virgule : 1234,5 (format français)"));
      return { el: [p.el, h("label", {}, "Séparateur décimal", dec), h("p", { class: "hint" }, "Les espaces de milliers sont retirés ; les valeurs non numériques restent inchangées.")], get: () => ({ op: "number", columns: p.get(), decimal: dec.value }) };
    },
  },
  concat: {
    title: "Concaténer des colonnes",
    build(cols) {
      const p = colPicker(cols);
      const sep = h("input", { value: " " });
      const target = h("input", { value: "concatenation" });
      const skip = h("input", { type: "checkbox", checked: true });
      return {
        el: [h("p", { class: "hint" }, "Cochez les colonnes dans l'ordre souhaité."), p.el,
          h("div", { class: "grid2" }, h("label", {}, "Séparateur", sep), h("label", {}, "Colonne résultat (nouvelle ou existante)", target)),
          h("label", { class: "check" }, skip, "Ignorer les cellules vides")],
        get: () => ({ op: "concat", columns: p.get(), separator: sep.value, target: target.value, skip_empty: skip.checked }),
      };
    },
  },
  extract: {
    title: "Extraire une partie",
    build(cols) {
      const col = h("select", {}, colOptions(cols));
      const pattern = h("input", { placeholder: String.raw`ex. (\d{5}) pour un code postal` });
      const target = h("input", { value: "extrait" });
      const cs = h("input", { type: "checkbox" });
      return {
        el: [h("div", { class: "grid2" }, h("label", {}, "Colonne source", col), h("label", {}, "Colonne résultat", target)),
          h("label", {}, "Expression régulière (le premier groupe entre parenthèses est extrait)", pattern),
          h("label", { class: "check" }, cs, "Respecter la casse")],
        get: () => ({ op: "extract", column: col.value, pattern: pattern.value, target: target.value, case_sensitive: cs.checked }),
      };
    },
  },
  constant: {
    title: "Colonne à valeur fixe",
    build() {
      const target = h("input", { value: "nouvelle_colonne" });
      const value = h("input", {});
      return { el: [h("div", { class: "grid2" }, h("label", {}, "Nom de la colonne", target), h("label", {}, "Valeur", value))], get: () => ({ op: "constant", target: target.value, value: value.value }) };
    },
  },
};

let transformOps = [];

/** Colonnes disponibles pour une opération : celles du fichier + celles créées avant elle. */
function columnsBefore(index) {
  const cols = [...(activeFile()?.columns || [])];
  for (const op of transformOps.slice(0, index)) {
    const t = op.peekTarget?.();
    if (t && !cols.includes(t)) cols.push(t);
  }
  return cols;
}

function addTransform(type) {
  const def = TRANSFORMS[type];
  const built = def.build(columnsBefore(transformOps.length));
  const op = { type, get: built.get, peekTarget: () => built.get().target };
  op.el = h("div", { class: "card op" },
    h("div", { class: "card-head" },
      h("h3", {}, h("span", { class: "order" }), " ", def.title),
      h("div", { class: "actions tight" },
        h("button", { class: "btn small", title: "Monter", onclick: () => moveTransform(op, -1) }, "↑"),
        h("button", { class: "btn small", title: "Descendre", onclick: () => moveTransform(op, 1) }, "↓"),
        h("button", { class: "link danger", title: "Supprimer", onclick: () => { transformOps = transformOps.filter((o) => o !== op); renderTransforms(); } }, "✕"))),
    built.el);
  transformOps.push(op);
  renderTransforms();
}

function moveTransform(op, d) {
  const i = transformOps.indexOf(op);
  const j = i + d;
  if (j < 0 || j >= transformOps.length) return;
  [transformOps[i], transformOps[j]] = [transformOps[j], transformOps[i]];
  renderTransforms();
}

function renderTransforms() {
  const box = $("#tf-ops");
  box.replaceChildren(...transformOps.map((o) => o.el));
  transformOps.forEach((o, i) => ($(".order", o.el).textContent = i + 1));
  if (!transformOps.length) box.append(h("p", { class: "hint" }, "Aucune opération : choisissez-en une ci-dessous."));
}

panels.transform = {
  single: true,
  render() {
    transformOps = [];
    renderTransforms();
  },
};
$("#tf-add").addEventListener("click", () => requireActive() && addTransform($("#tf-type").value));
$("#tf-run").addEventListener("click", () => {
  if (!transformOps.length) return showMessage("Ajoutez au moins une opération.", "error");
  runStep({ type: "transform", operations: transformOps.map((o) => o.get()) }, "Nettoyage terminé", "Application des opérations…");
});

// ===========================================================================
// Fusion
// ===========================================================================

let mergeOrder = [];
panels.merge = {
  render() { this.onFiles(); },
  onFiles() {
    mergeOrder = mergeOrder.filter(fileById);
    const box = $("#mg-files");
    if (!state.files.length) return box.replaceChildren(h("p", { class: "hint" }, "Importez au moins deux fichiers."));
    orderedChecklist(box, state.files.map((f) => f.id), () => mergeOrder, (o) => (mergeOrder = o), (id) => {
      const f = fileById(id);
      return [f.name, " ", h("span", { class: "file-meta" },
        `— ${f.columns.length} colonnes : ${f.columns.slice(0, 6).join(", ")}${f.columns.length > 6 ? "…" : ""}`)];
    });
  },
};
$("#mg-source").addEventListener("change", (e) => ($("#mg-source-name-wrap").hidden = !e.target.checked));
$("#mg-run").addEventListener("click", async () => {
  if (mergeOrder.length < 2) return showMessage("Cochez au moins deux fichiers à fusionner.", "error");
  const body = {
    ids: mergeOrder,
    mode: $("input[name=mg-mode]:checked").value,
    source_column: $("#mg-source").checked ? $("#mg-source-name").value : null,
    dedupe: $("#mg-dedupe").checked,
    name: $("#mg-name").value || null,
  };
  const info = await busy("Fusion en cours…", () => api("POST", "/api/merge", body));
  if (info) await resultDone(info, "Fusion terminée");
});

// ===========================================================================
// Découpage
// ===========================================================================

let splitOrder = [];
panels.split = {
  single: true,
  render() {
    const f = activeFile();
    splitOrder = [];
    $("#sp-result").hidden = true;
    orderedChecklist($("#sp-cols"), f?.columns || [], () => splitOrder, (o) => (splitOrder = o), (c) => c, () => updateSplitEstimate(f));
    updateSplitEstimate(f);
  },
};

async function updateSplitEstimate(f) {
  const hint = $("#sp-estimate");
  if (!f || !splitOrder.length) return (hint.textContent = "Cochez une ou plusieurs colonnes (l'ordre de sélection détermine le nom des fichiers).");
  try {
    const counts = await Promise.all(splitOrder.map((c) => columnValues(f.id, c)));
    hint.textContent = splitOrder.map((c, i) => `« ${c} » : ${fmt(counts[i].distinct)} valeurs distinctes`).join(" · ") +
      (splitOrder.length > 1 ? " — le nombre de fichiers sera au plus le produit de ces valeurs." : ` — ${fmt(counts[0].distinct)} fichiers seront créés.`);
  } catch {
    hint.textContent = "";
  }
}

$("#sp-run").addEventListener("click", async () => {
  const f = requireActive();
  if (!f) return;
  if (!splitOrder.length) return showMessage("Cochez au moins une colonne de découpage.", "error");
  const res = await busy("Découpage en cours…", () => api("POST", "/api/split", { id: f.id, columns: splitOrder, remove_columns: $("#sp-remove").checked }));
  if (!res) return;
  await refreshFiles();
  const ids = res.files.map((x) => x.id);
  const zipName = `${f.name.replace(/\.[^.]+$/, "")}_decoupage.zip`;
  const box = $("#sp-result");
  box.hidden = false;
  box.replaceChildren(
    h("div", { class: "card-head" },
      h("h3", {}, `${fmt(res.files.length)} fichiers créés`),
      h("div", { class: "actions tight" },
        h("a", { class: "btn primary", href: zipUrl(ids, zipName), "data-zip": ids.join(","), "data-name": zipName }, "Tout télécharger (ZIP)"),
        h("button", {
          class: "btn",
          onclick: async () => {
            if (!confirm("Retirer ces fichiers de l'espace de travail ?")) return;
            await busy("Suppression…", () => Promise.all(ids.map((x) => api("DELETE", `/api/files/${x}`))));
            box.hidden = true;
            await refreshFiles();
          },
        }, "Retirer de l'espace de travail"))),
    h("div", { class: "parts" }, res.files.map((x) =>
      h("div", { class: "part" },
        h("button", { class: "link", onclick: () => setActive(x.id, "preview") }, x.name),
        h("span", { class: "file-meta" }, `${fmt(x.row_count)} lignes `, h("a", { class: "link", href: downloadUrl(x.id), "data-dl": x.id }, "Télécharger"))))));
  showMessage(`Découpage terminé : ${fmt(res.files.length)} fichiers créés.`);
});

// ===========================================================================
// Jointure
// ===========================================================================

const JOIN_KINDS = [
  { v: "inner", t: "Interne", d: "Lignes présentes des deux côtés", fill: ["inter"] },
  { v: "left", t: "Gauche", d: "Toutes les lignes de gauche", fill: ["A"] },
  { v: "right", t: "Droite", d: "Toutes les lignes de droite", fill: ["B"] },
  { v: "full", t: "Complète", d: "Toutes les lignes des deux fichiers", fill: ["A", "B"] },
  { v: "left_anti", t: "Gauche sans correspondance", d: "Lignes de gauche absentes à droite", fill: ["Aonly"] },
  { v: "right_anti", t: "Droite sans correspondance", d: "Lignes de droite absentes à gauche", fill: ["Bonly"] },
  { v: "left_semi", t: "Semi-jointure", d: "Lignes de gauche ayant une correspondance (colonnes de gauche seules)", fill: ["inter"] },
];

function venn(fill) {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 60 36");
  svg.setAttribute("width", "60");
  svg.setAttribute("height", "36");
  const shapes = {
    A: '<circle class="fill" cx="22" cy="18" r="14"/>',
    B: '<circle class="fill" cx="38" cy="18" r="14"/>',
    inter: '<circle class="fill" cx="22" cy="18" r="14" clip-path="url(#venn-clip-b)"/>',
    Aonly: '<circle class="fill" cx="22" cy="18" r="14" mask="url(#venn-not-b)"/>',
    Bonly: '<circle class="fill" cx="38" cy="18" r="14" mask="url(#venn-not-a)"/>',
  };
  svg.innerHTML = fill.map((f) => shapes[f]).join("") + '<circle class="outline" cx="22" cy="18" r="14"/><circle class="outline" cx="38" cy="18" r="14"/>';
  return svg;
}

function installVennDefs() {
  const defs = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  defs.setAttribute("width", "0");
  defs.setAttribute("height", "0");
  defs.style.position = "absolute";
  defs.innerHTML = `<defs>
    <clipPath id="venn-clip-b"><circle cx="38" cy="18" r="14"/></clipPath>
    <mask id="venn-not-b"><rect width="60" height="36" fill="white"/><circle cx="38" cy="18" r="14" fill="black"/></mask>
    <mask id="venn-not-a"><rect width="60" height="36" fill="white"/><circle cx="22" cy="18" r="14" fill="black"/></mask>
  </defs>`;
  document.body.prepend(defs);
}

function renderJoinKinds() {
  $("#jn-kinds").replaceChildren(...JOIN_KINDS.map((k) =>
    h("label", { class: `join-kind${k.v === "inner" ? " selected" : ""}`, title: k.d },
      h("input", {
        type: "radio", name: "jn-kind", value: k.v, checked: k.v === "inner",
        onchange: () => $$(".join-kind").forEach((el) => el.classList.toggle("selected", $("input", el).checked)),
      }),
      venn(k.fill), h("b", {}, k.t), h("small", {}, k.d))));
}

function addKeyRow(leftCol, rightCol) {
  const lf = fileById($("#jn-left").value);
  const rf = fileById($("#jn-right").value);
  const row = h("div", { class: "key-row join" },
    h("select", { class: "k-left" }, colOptions(lf?.columns, leftCol)),
    h("span", { class: "eq" }, "="),
    h("select", { class: "k-right" }, colOptions(rf?.columns, rightCol)),
    h("button", { class: "link danger", title: "Retirer", onclick: () => row.remove() }, "✕"));
  $("#jn-keys").append(row);
}

function resetJoinKeys() {
  $("#jn-keys").replaceChildren();
  const lf = fileById($("#jn-left").value);
  const rf = fileById($("#jn-right").value);
  if (!lf || !rf) return $("#jn-keys").append(h("p", { class: "hint" }, "Choisissez les deux fichiers."));
  const common = lf.columns.filter((c) => rf.columns.includes(c));
  addKeyRow(common[0] ?? lf.columns[0], common[0] ?? rf.columns[0]);
}

$("#jn-left").addEventListener("change", resetJoinKeys);
$("#jn-right").addEventListener("change", resetJoinKeys);
$("#jn-add-key").addEventListener("click", () => {
  if (!$("#jn-left").value || !$("#jn-right").value) return showMessage("Choisissez d'abord les deux fichiers.", "error");
  $(".hint", $("#jn-keys"))?.remove();
  addKeyRow();
});
$("#jn-run").addEventListener("click", async () => {
  const left_id = $("#jn-left").value, right_id = $("#jn-right").value;
  if (!left_id || !right_id) return showMessage("Choisissez les fichiers de gauche et de droite.", "error");
  const rows = $$(".key-row.join");
  if (!rows.length) return showMessage("Ajoutez au moins une colonne de correspondance.", "error");
  const body = {
    left_id, right_id,
    left_columns: rows.map((r) => $(".k-left", r).value),
    right_columns: rows.map((r) => $(".k-right", r).value),
    kind: $("input[name=jn-kind]:checked").value,
    case_insensitive: $("#jn-ci").checked,
    trim: $("#jn-trim").checked,
    right_suffix: $("#jn-suffix").value,
    name: $("#jn-name").value || null,
  };
  const info = await busy("Jointure en cours…", () => api("POST", "/api/join", body));
  if (info) await resultDone(info, "Jointure terminée");
});

// ===========================================================================
// Comparaison
// ===========================================================================

let compareKeys = [];
let compareCols = [];

function renderCompareColumns() {
  const o = fileById($("#cp-old").value);
  const n = fileById($("#cp-new").value);
  const common = o && n ? n.columns.filter((c) => o.columns.includes(c)) : [];
  compareKeys = compareKeys.filter((c) => common.includes(c));
  compareCols = compareCols.filter((c) => common.includes(c));
  if (!common.length) {
    const msg = o && n ? "Ces deux fichiers n'ont aucune colonne en commun." : "Choisissez les deux versions.";
    $("#cp-keys").replaceChildren(h("p", { class: "hint" }, msg));
    $("#cp-cols").replaceChildren();
    return;
  }
  if (!compareKeys.length) compareKeys = [common[0]];
  orderedChecklist($("#cp-keys"), common, () => compareKeys, (x) => (compareKeys = x), (c) => c);
  orderedChecklist($("#cp-cols"), common, () => compareCols, (x) => (compareCols = x), (c) => c);
}
$("#cp-old").addEventListener("change", renderCompareColumns);
$("#cp-new").addEventListener("change", renderCompareColumns);
panels.compare = { render: renderCompareColumns };

$("#cp-run").addEventListener("click", async () => {
  const old_id = $("#cp-old").value, new_id = $("#cp-new").value;
  if (!old_id || !new_id) return showMessage("Choisissez l'ancienne et la nouvelle version.", "error");
  if (!compareKeys.length) return showMessage("Cochez au moins une colonne identifiante.", "error");
  const res = await busy("Comparaison en cours…", () => api("POST", "/api/compare", {
    old_id, new_id, keys: compareKeys, columns: compareCols.filter((c) => !compareKeys.includes(c)),
    include_unchanged: $("#cp-unchanged").checked, case_insensitive: $("#cp-ci").checked, trim: $("#cp-trim").checked,
  }));
  if (!res) return;
  await refreshFiles();
  const s = res.summary;
  const preview = h("div", {});
  $("#cp-result").replaceChildren(
    h("div", { class: "stats" },
      h("div", { class: "stat add" }, h("b", {}, fmt(s.added)), "ajoutées"),
      h("div", { class: "stat del" }, h("b", {}, fmt(s.removed)), "supprimées"),
      h("div", { class: "stat mod" }, h("b", {}, fmt(s.modified)), "modifiées"),
      h("div", { class: "stat" }, h("b", {}, fmt(s.unchanged)), "identiques")),
    h("div", { class: "card" },
      s.changes_by_column.length ? h("p", {}, "Modifications par colonne : ", s.changes_by_column.map(([c, n]) => h("span", { class: "chip small" }, `${c} (${fmt(n)})`))) : null,
      s.columns_added.length ? h("p", {}, "Colonnes apparues : ", s.columns_added.join(", ")) : null,
      s.columns_removed.length ? h("p", {}, "Colonnes disparues : ", s.columns_removed.join(", ")) : null,
      s.duplicate_keys ? h("p", { class: "warn" }, `Attention : ${fmt(s.duplicate_keys)} identifiants en double ; ces lignes sont appariées dans l'ordre. Ajoutez des colonnes identifiantes pour lever l'ambiguïté.`) : null,
      h("p", { class: "hint" }, "Le fichier résultat contient une colonne « statut », la liste des colonnes modifiées et, pour chaque colonne comparée, l'ancienne valeur (« (avant) »). Filtrez-le sur « statut » pour isoler une catégorie.")),
    preview);
  miniPreview(preview, res.file.id);
  showMessage(`Comparaison terminée : ${fmt(s.added)} ajoutées, ${fmt(s.removed)} supprimées, ${fmt(s.modified)} modifiées.`);
});

// ===========================================================================
// SQL
// ===========================================================================

function insertAtCursor(ta, text) {
  const { selectionStart: a, selectionEnd: b, value } = ta;
  ta.value = value.slice(0, a) + text + value.slice(b);
  ta.selectionStart = ta.selectionEnd = a + text.length;
  ta.focus();
}

const quoteCol = (c) => (/^[A-Za-z_][A-Za-z0-9_]*$/.test(c) ? c : `"${c.replace(/"/g, '""')}"`);

panels.sql = {
  render() { this.onFiles(); },
  onFiles() {
    const ta = $("#sq-query");
    $("#sq-tables").replaceChildren(...(state.files.length ? state.files.map((f) =>
      h("details", { class: "sql-table" },
        h("summary", {},
          h("button", { class: "link mono", title: "Insérer le nom de la table", onclick: (e) => { e.preventDefault(); insertAtCursor(ta, f.sql_name); } }, f.sql_name),
          h("span", { class: "file-meta" }, ` — ${f.name}, ${fmt(f.row_count)} lignes`)),
        h("div", { class: "chips" }, f.columns.map((c) => h("button", { class: "chip small mono", onclick: () => insertAtCursor(ta, quoteCol(c)) }, c)))))
      : [h("p", { class: "hint" }, "Importez des fichiers pour les interroger.")]));
    const t = state.files[0];
    const ex = $("#sq-examples");
    ex.replaceChildren();
    if (!t) return;
    const c0 = quoteCol(t.columns[0] || "colonne");
    const examples = [
      ["Aperçu", `SELECT *\nFROM ${t.sql_name}\nLIMIT 100`],
      ["Compter par valeur", `SELECT ${c0}, COUNT(*) AS nombre\nFROM ${t.sql_name}\nGROUP BY ${c0}\nORDER BY nombre DESC`],
      ["Somme numérique", `SELECT ${c0}, SUM(num(${quoteCol(t.columns[1] || "montant")})) AS total\nFROM ${t.sql_name}\nGROUP BY ${c0}`],
    ];
    if (state.files[1]) {
      const u = state.files[1];
      const common = t.columns.find((c) => u.columns.includes(c));
      const on = common ? `a.${quoteCol(common)} = b.${quoteCol(common)}` : `a.${c0} = b.${quoteCol(u.columns[0])}`;
      examples.push(["Jointure", `SELECT a.*, b.*\nFROM ${t.sql_name} a\nLEFT JOIN ${u.sql_name} b ON ${on}`]);
    }
    ex.append(h("span", { class: "hint" }, "Exemples :"), ...examples.map(([label, q]) => h("button", { class: "btn small", onclick: () => { ta.value = q; ta.focus(); } }, label)));
  },
};

async function runSql() {
  const query = $("#sq-query").value;
  if (!query.trim()) return showMessage("Saisissez une requête.", "error");
  const info = await busy("Exécution de la requête…", () => api("POST", "/api/sql", { query, name: $("#sq-name").value || null }));
  if (!info) return;
  await resultDone(info, "Requête exécutée", { stay: true });
  miniPreview($("#sq-result"), info.id);
}
$("#sq-run").addEventListener("click", runSql);
$("#sq-query").addEventListener("keydown", (e) => {
  if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) { e.preventDefault(); runSql(); }
  if (e.key === "Tab") { e.preventDefault(); insertAtCursor(e.target, "  "); }
});

// ===========================================================================
// Recettes
// ===========================================================================

const OP_LABELS = {
  in: "est l'une des valeurs", not_in: "n'est aucune des valeurs", contains: "contient", not_contains: "ne contient pas",
  starts_with: "commence par", ends_with: "se termine par", empty: "est vide", not_empty: "n'est pas vide",
  gt: ">", ge: "≥", lt: "<", le: "≤", between: "est entre", regex: "correspond à",
};
const MODE_LABELS = { text: "alphabétique", number: "numérique", natural: "naturel", date: "chronologique" };
const DEDUPE_LABELS = { remove: "Supprimer les doublons", duplicates: "Isoler les doublons", removed: "Garder les lignes écartées par le dédoublonnage", singles: "Garder les lignes uniques" };
const CASE_LABELS = { upper: "MAJUSCULES", lower: "minuscules", title: "Première Lettre Des Mots", sentence: "Première lettre" };
const list = (a) => (a && a.length ? a.map((x) => `« ${x} »`).join(", ") : "toutes les colonnes");

function describeTransform(t) {
  switch (t.op) {
    case "trim": return `Espaces superflus supprimés (${list(t.columns)})`;
    case "case": return `Casse ${CASE_LABELS[t.mode]} (${list(t.columns)})`;
    case "replace": return `Remplacer ${t.regex ? "l'expression" : ""} « ${t.search} » par « ${t.replacement} » (${list(t.columns)})`;
    case "date": return `Dates ${list(t.columns)} : ${t.input || "auto"} → ${t.output}`;
    case "number": return `Nombres ${list(t.columns)} au format ${t.decimal === "," ? "1234,5" : "1234.5"}`;
    case "concat": return `« ${t.target} » = ${list(t.columns)} séparées par « ${t.separator} »`;
    case "extract": return `« ${t.target} » = extrait de « ${t.column} » via ${t.pattern}`;
    case "constant": return `« ${t.target} » = « ${t.value} »`;
    default: return t.op;
  }
}

function describeStep(s) {
  switch (s.type) {
    case "columns": return `Colonnes : ${s.columns.map((c) => (c.name && c.name !== c.source ? `${c.source} → ${c.name}` : c.source)).join(", ")}`;
    case "sort": return `Trier par ${s.keys.map((k) => `${k.column} (${MODE_LABELS[k.mode] || k.mode}${k.descending ? ", décroissant" : ""})`).join(", puis ")}`;
    case "dedupe": return `${DEDUPE_LABELS[s.mode] || "Doublons"} sur ${s.columns.length ? list(s.columns) : "la ligne entière"}`;
    case "filter": return `Filtrer : ${s.conditions.map((c) => `${c.column} ${OP_LABELS[c.operator]}${c.values.length ? " " + c.values.map((v) => `« ${v} »`).join(", ") : ""}`).join(s.combine === "any" ? " OU " : " ET ")}`;
    case "transform": return `Nettoyage : ${s.operations.map(describeTransform).join(" ; ")}`;
    default: return s.type;
  }
}

async function refreshRecipes() {
  state.recipes = await api("GET", "/api/recipes");
  renderRecipes();
}

function renderRecipePreview() {
  const f = fileById($("#rc-from").value);
  $("#rc-preview").replaceChildren(...(f
    ? f.steps.map((st) => h("li", {}, describeStep(st)))
    : [h("li", { class: "hint" }, "Aucun fichier transformé : appliquez d'abord des colonnes, un tri, un filtre… à un fichier.")]));
}

function renderRecipeSources() {
  const sel = $("#rc-from");
  const current = sel.value;
  const withSteps = state.files.filter((f) => f.steps.length > 0);
  sel.replaceChildren(...(withSteps.length
    ? withSteps.map((f) => h("option", { value: f.id }, `${f.name} (${f.steps.length} étape${f.steps.length > 1 ? "s" : ""})`))
    : [h("option", { value: "" }, "— aucun fichier transformé —")]));
  // Par défaut : le fichier de travail s'il est transformé, sinon le plus récent.
  const preferred = [sel.dataset.chosen === current ? current : null, state.active, withSteps.at(-1)?.id].find((id) => withSteps.some((f) => f.id === id));
  if (preferred) sel.value = preferred;
  renderRecipePreview();
}
$("#rc-from").addEventListener("change", (e) => { e.target.dataset.chosen = e.target.value; renderRecipePreview(); });

function renderRecipes() {
  const box = $("#rc-list");
  if (!state.recipes.length) return box.replaceChildren(h("p", { class: "hint" }, "Aucune recette enregistrée."));
  box.replaceChildren(...state.recipes.map((r) => {
    let targets = [];
    const targetBox = h("div", { class: "checklist" });
    orderedChecklist(targetBox, state.files.map((f) => f.id), () => targets, (t) => (targets = t), (id) => fileById(id).name);
    return h("div", { class: "card recipe" },
      h("div", { class: "card-head" },
        h("h3", {}, r.name),
        h("div", { class: "actions tight" },
          h("button", { class: "btn small", onclick: () => download(`${r.name.replace(/[^\p{L}\p{N}_-]+/gu, "_")}.recette.json`, JSON.stringify({ name: r.name, description: r.description, steps: r.steps }, null, 2), "application/json") }, "Exporter"),
          h("button", { class: "link danger", onclick: async () => {
            if (!confirm(`Supprimer la recette « ${r.name} » ?`)) return;
            await busy("Suppression…", () => api("DELETE", `/api/recipes/${r.id}`));
            refreshRecipes();
          } }, "Supprimer"))),
      r.description ? h("p", { class: "hint" }, r.description) : null,
      h("ol", { class: "steps" }, r.steps.map((s) => h("li", {}, describeStep(s)))),
      h("details", { class: "value-picker" },
        h("summary", {}, "Appliquer à des fichiers…"),
        state.files.length ? targetBox : h("p", { class: "hint" }, "Importez d'abord des fichiers."),
        h("button", { class: "btn primary", onclick: async () => {
          if (!targets.length) return showMessage("Cochez au moins un fichier.", "error");
          const res = await busy(`Application de « ${r.name} »…`, () => api("POST", `/api/recipes/${r.id}/apply`, { ids: targets }));
          if (!res) return;
          await refreshFiles();
          showMessage(`Recette appliquée : ${res.map((f) => `« ${f.name} » (${fmt(f.row_count)} lignes)`).join(", ")}.`);
          setActive(res[res.length - 1].id, "preview");
        } }, "Appliquer")));
  }));
}

panels.recipes = {
  render() { renderRecipeSources(); refreshRecipes(); },
  onFiles() { renderRecipeSources(); renderRecipes(); },
};

$("#rc-save").addEventListener("click", async () => {
  const from = $("#rc-from").value;
  if (!from) return showMessage("Choisissez un fichier transformé.", "error");
  const name = $("#rc-name").value.trim();
  if (!name) return showMessage("Donnez un nom à la recette.", "error");
  const r = await busy("Enregistrement…", () => api("POST", "/api/recipes", { name, description: $("#rc-desc").value, from_file: from }));
  if (!r) return;
  $("#rc-name").value = $("#rc-desc").value = "";
  showMessage(`Recette « ${r.name} » enregistrée (${r.steps.length} étape${r.steps.length > 1 ? "s" : ""}).`);
  refreshRecipes();
});

$("#rc-import").addEventListener("change", async (e) => {
  const file = e.target.files[0];
  e.target.value = "";
  if (!file) return;
  let data;
  try {
    data = JSON.parse(await file.text());
  } catch {
    return showMessage("Ce fichier n'est pas un JSON valide.", "error");
  }
  const items = Array.isArray(data) ? data : [data];
  const ok = await busy("Import des recettes…", async () => {
    for (const r of items) await api("POST", "/api/recipes", { name: r.name || file.name.replace(/\..*$/, ""), description: r.description || "", steps: r.steps || [] });
    return true;
  });
  if (ok) { showMessage(`${items.length} recette${items.length > 1 ? "s importées" : " importée"}.`); refreshRecipes(); }
});

// ===========================================================================
// Démarrage
// ===========================================================================

installVennDefs();
renderJoinKinds();
refreshFiles().then(() => {
  resetJoinKeys();
  setActive(state.files[0]?.id || null, "preview");
});
