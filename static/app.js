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
  for (const c of children.flat()) {
    if (c === null || c === undefined || c === false) continue;
    el.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
  return el;
}

const fmt = (n) => n.toLocaleString("fr-FR");
const DELIM_LABELS = { ",": "virgule", ";": "point-virgule", "\t": "tabulation", "|": "barre", ":": "deux-points", " ": "espace" };
const ORIGIN_LABELS = { import: "importé", fusion: "fusion", "découpage": "découpage", jointure: "jointure", filtre: "filtre" };

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

async function busy(label, fn) {
  const overlay = h("div", { class: "busy" }, h("span", {}, label));
  document.body.append(overlay);
  try {
    return await fn();
  } catch (e) {
    showMessage(e.message, "error");
    return undefined;
  } finally {
    overlay.remove();
  }
}

function showMessage(text, kind = "ok") {
  const m = $("#message");
  m.textContent = text;
  m.className = `message ${kind}`;
  m.hidden = false;
  clearTimeout(showMessage.timer);
  if (kind === "ok") showMessage.timer = setTimeout(() => (m.hidden = true), 8000);
}

// ===========================================================================
// État
// ===========================================================================

const state = {
  files: [],
  previewId: null,
  previewOffset: 0,
  pageSize: 100,
  valuesCache: new Map(),
};

const fileById = (id) => state.files.find((f) => f.id === id);

function exportParams() {
  const p = new URLSearchParams({
    delimiter: $("#ex-delim").value,
    encoding: $("#ex-enc").value,
    quote: $("#ex-quote").value,
    line_ending: $("#ex-eol").value,
    headers: $("#ex-headers").checked ? "true" : "false",
  });
  return p.toString();
}
const downloadUrl = (id) => `/api/files/${id}/download?${exportParams()}`;
const zipUrl = (ids, name) =>
  `/api/zip?ids=${ids.join(",")}&name=${encodeURIComponent(name)}&${exportParams()}`;

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
// Onglets
// ===========================================================================

function openTab(name) {
  $$(".tab").forEach((t) => t.classList.toggle("active", t.dataset.tab === name));
  $$(".panel").forEach((p) => p.classList.toggle("active", p.id === `panel-${name}`));
}
$$(".tab").forEach((t) => t.addEventListener("click", () => openTab(t.dataset.tab)));

// ===========================================================================
// Import
// ===========================================================================

$("#imp-delim").addEventListener("change", (e) => {
  $("#imp-delim-custom-wrap").hidden = e.target.value !== "custom";
});

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
  const added = await busy("Import en cours…", () => api("POST", "/api/files", fd));
  if (!added) return;
  await refreshFiles();
  const last = added[added.length - 1];
  showMessage(
    added
      .map((f) => `« ${f.name} » importé : ${fmt(f.row_count)} lignes, ${f.columns.length} colonnes, séparateur ${DELIM_LABELS[f.delimiter] || f.delimiter}, ${f.encoding}`)
      .join("\n")
  );
  showPreview(last.id);
}

const fileInput = $("#file-input");
fileInput.addEventListener("change", () => {
  uploadFiles([...fileInput.files]);
  fileInput.value = "";
});
const dz = $("#dropzone");
["dragenter", "dragover"].forEach((ev) =>
  dz.addEventListener(ev, (e) => { e.preventDefault(); dz.classList.add("over"); })
);
["dragleave", "drop"].forEach((ev) => dz.addEventListener(ev, () => dz.classList.remove("over")));
dz.addEventListener("drop", (e) => {
  e.preventDefault();
  uploadFiles([...e.dataTransfer.files]);
});

// ===========================================================================
// Liste des fichiers
// ===========================================================================

async function refreshFiles() {
  state.files = await api("GET", "/api/files");
  renderFileList();
  renderFileSelects();
  renderMergeList();
}

function renderFileList() {
  const ul = $("#file-list");
  ul.replaceChildren();
  $("#empty-files").hidden = state.files.length > 0;
  $("#clear-all").hidden = state.files.length === 0;
  for (const f of state.files) {
    const li = h(
      "li",
      { class: `file-item${f.id === state.previewId ? " selected" : ""}`, onclick: () => showPreview(f.id) },
      h("span", { class: "file-name" }, f.name),
      h(
        "span",
        { class: "file-meta" },
        h("span", { class: "badge" }, ORIGIN_LABELS[f.origin] || f.origin),
        `${fmt(f.row_count)} lignes · ${f.columns.length} col. · « ${DELIM_LABELS[f.delimiter] || f.delimiter} »`
      ),
      h(
        "span",
        { class: "file-actions", onclick: (e) => e.stopPropagation() },
        h("a", { class: "link", href: downloadUrl(f.id), title: "Télécharger avec les options d'export" }, "Télécharger"),
        h("button", { class: "link", onclick: () => renameFile(f) }, "Renommer"),
        h("button", { class: "link danger", onclick: () => deleteFile(f) }, "Supprimer")
      )
    );
    ul.append(li);
  }
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
  if (state.previewId === f.id) state.previewId = null;
  await refreshFiles();
  if (!state.previewId) renderPreviewEmpty();
}

$("#clear-all").addEventListener("click", async () => {
  if (!confirm("Retirer tous les fichiers de l'espace de travail ?")) return;
  await busy("Suppression…", () => api("DELETE", "/api/files"));
  state.previewId = null;
  state.valuesCache.clear();
  await refreshFiles();
  renderPreviewEmpty();
});

/** Remplit toutes les listes déroulantes de fichiers en conservant la sélection. */
function renderFileSelects() {
  for (const sel of $$(".file-select")) {
    const current = sel.value;
    sel.replaceChildren(
      h("option", { value: "" }, state.files.length ? "— choisir un fichier —" : "— aucun fichier —"),
      ...state.files.map((f) => h("option", { value: f.id }, `${f.name} (${fmt(f.row_count)} lignes)`))
    );
    sel.value = fileById(current) ? current : "";
    if (sel.value !== current) sel.dispatchEvent(new Event("change"));
  }
}

// Export : régénère les liens quand les options changent.
$$("#ex-delim, #ex-enc, #ex-quote, #ex-eol, #ex-headers").forEach((el) =>
  el.addEventListener("change", () => {
    renderFileList();
    updateDownloadLink();
    $$("#sp-result a[data-zip]").forEach((a) => (a.href = zipUrl(a.dataset.zip.split(","), a.dataset.name)));
    $$("#sp-result a[data-id]").forEach((a) => (a.href = downloadUrl(a.dataset.id)));
  })
);

// ===========================================================================
// Aperçu
// ===========================================================================

function renderPreviewEmpty() {
  $("#pv-table").replaceChildren();
  $("#pv-meta").textContent = "";
  $("#pv-range").textContent = "";
  $("#pv-prev").disabled = $("#pv-next").disabled = true;
  updateDownloadLink();
}

function updateDownloadLink() {
  const a = $("#pv-download");
  if (state.previewId) {
    a.href = downloadUrl(state.previewId);
    a.removeAttribute("aria-disabled");
    a.style.pointerEvents = "";
    a.style.opacity = "";
  } else {
    a.href = "#";
    a.setAttribute("aria-disabled", "true");
    a.style.pointerEvents = "none";
    a.style.opacity = ".5";
  }
}

async function showPreview(id, offset = 0) {
  state.previewId = id;
  state.previewOffset = offset;
  $("#pv-file").value = id;
  openTab("preview");
  renderFileList();
  updateDownloadLink();
  try {
    const data = await api("GET", `/api/files/${id}/rows?offset=${offset}&limit=${state.pageSize}`);
    const f = fileById(id);
    $("#pv-meta").textContent = f
      ? `${fmt(data.total)} lignes · ${data.headers.length} colonnes · séparateur source « ${DELIM_LABELS[f.delimiter] || f.delimiter} » · ${f.encoding}`
      : "";
    const table = $("#pv-table");
    table.replaceChildren(
      h("thead", {}, h("tr", {}, h("th", { class: "num" }, "#"), ...data.headers.map((c) => h("th", { title: c }, c)))),
      h(
        "tbody",
        {},
        ...data.rows.map((r, i) =>
          h("tr", {}, h("td", { class: "num" }, fmt(data.offset + i + 1)), ...r.map((v) => h("td", { title: v }, v)))
        )
      )
    );
    const end = data.offset + data.rows.length;
    $("#pv-range").textContent = data.total ? `Lignes ${fmt(data.offset + 1)} à ${fmt(end)} sur ${fmt(data.total)}` : "Aucune ligne";
    $("#pv-prev").disabled = data.offset === 0;
    $("#pv-next").disabled = end >= data.total;
  } catch (e) {
    showMessage(e.message, "error");
  }
}

$("#pv-file").addEventListener("change", (e) => {
  if (e.target.value) showPreview(e.target.value);
  else { state.previewId = null; renderPreviewEmpty(); renderFileList(); }
});
$("#pv-prev").addEventListener("click", () => showPreview(state.previewId, Math.max(0, state.previewOffset - state.pageSize)));
$("#pv-next").addEventListener("click", () => showPreview(state.previewId, state.previewOffset + state.pageSize));

function resultDone(info, done) {
  showMessage(`${done} : « ${info.name} » — ${fmt(info.row_count)} lignes, ${info.columns.length} colonnes.\nLe résultat est ajouté à vos fichiers : vous pouvez le télécharger ou l'utiliser dans une autre opération.`);
  return refreshFiles().then(() => showPreview(info.id));
}

// ===========================================================================
// Fusion
// ===========================================================================

let mergeOrder = [];

/** Met à jour les pastilles indiquant l'ordre de sélection dans une liste à cocher. */
function updateOrderBadges(box, order) {
  $$("label[data-key]", box).forEach((l) => {
    const pos = order.indexOf(l.dataset.key);
    const badge = $(".order", l);
    badge.hidden = pos < 0;
    badge.textContent = pos + 1;
  });
}

/** Liste à cocher dont l'ordre de sélection est mémorisé via getOrder()/setOrder(). */
function orderedChecklist(box, keys, getOrder, setOrder, label, onChange) {
  box.replaceChildren(
    ...keys.map((key) =>
      h("label", { class: "check", "data-key": key },
        h("input", {
          type: "checkbox",
          checked: getOrder().includes(key),
          onchange: (e) => {
            setOrder(e.target.checked ? [...getOrder(), key] : getOrder().filter((x) => x !== key));
            updateOrderBadges(box, getOrder());
            onChange?.();
          },
        }),
        h("span", {}, h("span", { class: "order" }), " ", label(key))))
  );
  updateOrderBadges(box, getOrder());
}

function renderMergeList() {
  mergeOrder = mergeOrder.filter(fileById);
  const box = $("#mg-files");
  if (!state.files.length) return box.replaceChildren(h("p", { class: "hint" }, "Importez au moins deux fichiers."));
  orderedChecklist(box, state.files.map((f) => f.id), () => mergeOrder, (o) => (mergeOrder = o), (id) => {
    const f = fileById(id);
    return [f.name, " ", h("span", { class: "file-meta" },
      `— ${f.columns.length} colonnes : ${f.columns.slice(0, 6).join(", ")}${f.columns.length > 6 ? "…" : ""}`)];
  });
}

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

function renderSplitCols() {
  const f = fileById($("#sp-file").value);
  const box = $("#sp-cols");
  splitOrder = f ? splitOrder.filter((c) => f.columns.includes(c)) : [];
  if (!f) {
    $("#sp-estimate").textContent = "";
    return box.replaceChildren(h("p", { class: "hint" }, "Choisissez un fichier."));
  }
  orderedChecklist(box, f.columns, () => splitOrder, (o) => (splitOrder = o), (c) => c, () => updateSplitEstimate(f));
  updateSplitEstimate(f);
}

async function updateSplitEstimate(f) {
  const hint = $("#sp-estimate");
  if (!splitOrder.length) return (hint.textContent = "Cochez une ou plusieurs colonnes (l'ordre de sélection détermine le nom des fichiers).");
  try {
    const counts = await Promise.all(splitOrder.map((c) => columnValues(f.id, c)));
    hint.textContent = splitOrder.map((c, i) => `« ${c} » : ${fmt(counts[i].distinct)} valeurs distinctes`).join(" · ") +
      (splitOrder.length > 1 ? " — le nombre de fichiers sera au plus le produit de ces valeurs." : ` — ${fmt(counts[0].distinct)} fichiers seront créés.`);
  } catch (e) {
    hint.textContent = "";
  }
}

$("#sp-file").addEventListener("change", () => { splitOrder = []; renderSplitCols(); $("#sp-result").hidden = true; });

$("#sp-run").addEventListener("click", async () => {
  const id = $("#sp-file").value;
  if (!id) return showMessage("Choisissez un fichier à découper.", "error");
  if (!splitOrder.length) return showMessage("Cochez au moins une colonne de découpage.", "error");
  const res = await busy("Découpage en cours…", () =>
    api("POST", "/api/split", { id, columns: splitOrder, remove_columns: $("#sp-remove").checked })
  );
  if (!res) return;
  await refreshFiles();
  const ids = res.files.map((f) => f.id);
  const src = fileById(id);
  const zipName = `${(src?.name || "decoupage").replace(/\.[^.]+$/, "")}_decoupage.zip`;
  const box = $("#sp-result");
  box.hidden = false;
  box.replaceChildren(
    h("div", { class: "card-head" },
      h("h3", {}, `${fmt(res.files.length)} fichiers créés`),
      h("div", { class: "actions" },
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
    h("div", { class: "parts" },
      ...res.files.map((f) =>
        h("div", { class: "part" },
          h("button", { class: "link", onclick: () => showPreview(f.id) }, f.name),
          h("span", { class: "file-meta" }, `${fmt(f.row_count)} lignes `,
            h("a", { class: "link", href: downloadUrl(f.id), "data-id": f.id }, "Télécharger")))))
  );
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
  const NS = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(NS, "svg");
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
  svg.innerHTML = fill.map((f) => shapes[f]).join("") +
    '<circle class="outline" cx="22" cy="18" r="14"/><circle class="outline" cx="38" cy="18" r="14"/>';
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
  const box = $("#jn-kinds");
  for (const k of JOIN_KINDS) {
    box.append(
      h("label", { class: `join-kind${k.v === "inner" ? " selected" : ""}`, title: k.d },
        h("input", {
          type: "radio", name: "jn-kind", value: k.v, checked: k.v === "inner",
          onchange: () => $$(".join-kind").forEach((el) => el.classList.toggle("selected", $("input", el).checked)),
        }),
        venn(k.fill), h("b", {}, k.t), h("small", {}, k.d))
    );
  }
}

function colOptions(f, selected) {
  return (f ? f.columns : []).map((c) => h("option", { value: c, selected: c === selected }, c));
}

function addKeyRow(leftCol, rightCol) {
  const lf = fileById($("#jn-left").value);
  const rf = fileById($("#jn-right").value);
  const row = h("div", { class: "key-row" },
    h("select", { class: "k-left" }, ...colOptions(lf, leftCol)),
    h("span", { class: "eq" }, "="),
    h("select", { class: "k-right" }, ...colOptions(rf, rightCol)),
    h("button", { class: "link danger", title: "Retirer", onclick: () => { row.remove(); } }, "✕"));
  $("#jn-keys").append(row);
}

function resetJoinKeys() {
  $("#jn-keys").replaceChildren();
  const lf = fileById($("#jn-left").value);
  const rf = fileById($("#jn-right").value);
  if (!lf || !rf) return $("#jn-keys").append(h("p", { class: "hint" }, "Choisissez les deux fichiers."));
  // Propose automatiquement les colonnes de même nom.
  const common = lf.columns.filter((c) => rf.columns.includes(c));
  if (common.length) addKeyRow(common[0], common[0]);
  else addKeyRow(lf.columns[0], rf.columns[0]);
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
  const rows = $$(".key-row", $("#jn-keys"));
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
// Filtre
// ===========================================================================

const MULTI_OPS = ["in", "not_in", "contains", "not_contains", "starts_with", "ends_with"];
const SINGLE_OPS = ["gt", "ge", "lt", "le", "regex"];
const conditions = [];

function addCondition() {
  const f = fileById($("#ft-file").value);
  if (!f) return showMessage("Choisissez d'abord le fichier à filtrer.", "error");
  const el = $("#tpl-condition").content.firstElementChild.cloneNode(true);
  const cond = { el, values: new Set() };
  conditions.push(cond);
  $(".c-col", el).replaceChildren(...colOptions(f));
  $(".c-col", el).addEventListener("change", () => { cond.values.clear(); renderConditionValues(cond); });
  $(".c-op", el).addEventListener("change", () => renderConditionValues(cond));
  $(".c-remove", el).addEventListener("click", () => {
    conditions.splice(conditions.indexOf(cond), 1);
    el.remove();
  });
  $("#ft-conditions").append(el);
  renderConditionValues(cond);
}

async function renderConditionValues(cond) {
  const box = $(".c-values", cond.el);
  const op = $(".c-op", cond.el).value;
  const column = $(".c-col", cond.el).value;
  const fileId = $("#ft-file").value;
  box.replaceChildren();

  if (SINGLE_OPS.includes(op)) {
    box.append(h("input", { class: "c-v1", placeholder: op === "regex" ? "ex. ^(Paris|Lyon)$" : "valeur (nombre, date AAAA-MM-JJ ou texte)" }));
    return;
  }
  if (op === "between") {
    box.append(h("div", { class: "grid2" },
      h("label", {}, "Valeur minimale", h("input", { class: "c-v1" })),
      h("label", {}, "Valeur maximale", h("input", { class: "c-v2" }))));
    return;
  }
  if (!MULTI_OPS.includes(op)) return; // vide / non vide : pas de valeur

  const chips = h("div", { class: "chips" });
  const listId = `dl-${Math.random().toString(36).slice(2)}`;
  const datalist = h("datalist", { id: listId });
  const input = h("input", { placeholder: "Saisissez une valeur puis Entrée", list: listId });
  const picker = h("details", { class: "value-picker" });
  let pickerBoxes = [];

  const renderChips = () => {
    chips.replaceChildren(
      ...[...cond.values].map((v) =>
        h("span", { class: "chip" }, v === "" ? "(vide)" : v,
          h("button", { title: "Retirer", onclick: () => { cond.values.delete(v); renderChips(); } }, "✕")))
    );
    pickerBoxes.forEach(([cb, v]) => (cb.checked = cond.values.has(v)));
  };
  const addValue = () => {
    const parts = input.value.split("\n").map((s) => s.trim()).filter((s) => s !== "");
    parts.forEach((p) => cond.values.add(p));
    input.value = "";
    renderChips();
  };
  input.addEventListener("keydown", (e) => { if (e.key === "Enter") { e.preventDefault(); addValue(); } });
  input.addEventListener("paste", () => setTimeout(() => { if (input.value.includes("\n")) addValue(); }));
  input.addEventListener("change", () => { if (input.value.trim()) addValue(); });

  box.append(chips, h("div", { class: "value-input" }, input, h("button", { class: "btn", onclick: addValue }, "Ajouter")), datalist);

  if (op === "in" || op === "not_in") {
    box.append(picker);
    picker.append(h("summary", {}, "Choisir parmi les valeurs existantes…"));
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
      const visible = () => $$("label", list).filter((l) => !l.hidden).map((l) => $("input", l));
      picker.append(
        h("div", { class: "value-input" }, search,
          h("button", { class: "btn small", onclick: () => { visible().forEach((cb) => { cb.checked = true; cb.dispatchEvent(new Event("change")); }); } }, "Tout cocher"),
          h("button", { class: "btn small", onclick: () => { visible().forEach((cb) => { cb.checked = false; cb.dispatchEvent(new Event("change")); }); } }, "Tout décocher")),
        list,
        ...(data.distinct > data.values.length
          ? [h("p", { class: "hint" }, `Seules les ${fmt(data.values.length)} valeurs les plus fréquentes sur ${fmt(data.distinct)} sont listées ; saisissez les autres manuellement.`)]
          : [])
      );
    } catch (e) {
      picker.append(h("p", { class: "hint" }, e.message));
    }
  }
  renderChips();
}

$("#ft-file").addEventListener("change", () => {
  conditions.length = 0;
  $("#ft-conditions").replaceChildren();
  if ($("#ft-file").value) addCondition();
});
$("#ft-add").addEventListener("click", addCondition);

$("#ft-run").addEventListener("click", async () => {
  const id = $("#ft-file").value;
  if (!id) return showMessage("Choisissez un fichier à filtrer.", "error");
  if (!conditions.length) return showMessage("Ajoutez au moins une condition.", "error");
  const conds = conditions.map(({ el, values }) => {
    const op = $(".c-op", el).value;
    let vals = [];
    if (MULTI_OPS.includes(op)) {
      // Une saisie en cours non validée est prise en compte.
      const pending = $(".value-input input", el)?.value.trim();
      if (pending) values.add(pending);
      vals = [...values];
    } else if (SINGLE_OPS.includes(op)) vals = [$(".c-v1", el).value];
    else if (op === "between") vals = [$(".c-v1", el).value, $(".c-v2", el).value];
    return { column: $(".c-col", el).value, operator: op, values: vals, case_sensitive: $(".c-case", el).checked };
  });
  const body = { id, conditions: conds, combine: $("input[name=ft-combine]:checked").value, name: $("#ft-name").value || null };
  const info = await busy("Filtrage en cours…", () => api("POST", "/api/filter", body));
  if (info) await resultDone(info, "Filtrage terminé");
});

// ===========================================================================
// Démarrage
// ===========================================================================

installVennDefs();
renderJoinKinds();
renderPreviewEmpty();
refreshFiles().then(() => {
  renderSplitCols();
  resetJoinKeys();
  if (state.files.length) showPreview(state.files[0].id);
});
