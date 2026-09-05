"use strict";

const $ = (selector) => document.querySelector(selector);
const history = [];
let previous = null;
let refreshTimer = null;
let adminToken = "";

const fmt = new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 });
const compact = new Intl.NumberFormat(undefined, { notation: "compact", maximumFractionDigits: 1 });

function setText(selector, value) {
  const node = $(selector);
  if (node) node.textContent = String(value);
}

function formatBytes(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${fmt.format(bytes / 1024)} KiB`;
  return `${fmt.format(bytes / (1024 * 1024))} MiB`;
}

function formatDuration(seconds) {
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`;
  return `${Math.floor(seconds / 86400)}d ${Math.floor((seconds % 86400) / 3600)}h`;
}

async function api(path) {
  const headers = adminToken ? { authorization: `Bearer ${adminToken}` } : {};
  const response = await fetch(path, { headers, cache: "no-store" });
  if (response.status === 401) {
    const error = new Error("Admin authentication required");
    error.auth = true;
    throw error;
  }
  if (!response.ok) throw new Error(`${path} returned HTTP ${response.status}`);
  return response.json();
}

function outcomeKind(outcome) {
  if (outcome.startsWith("rejected")) return "rejected";
  if (outcome.includes("queue")) return "queued";
  return "admitted";
}

function percentile(values, value) {
  if (!values.length) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.ceil(value * sorted.length) - 1)];
}

function cell(row, value, className = "") {
  const node = document.createElement("td");
  node.textContent = String(value);
  if (className) node.className = className;
  row.append(node);
  return node;
}

function renderPools(capacity) {
  const root = $("#capacity-pools");
  root.replaceChildren();
  const entries = Object.entries(capacity);
  if (!entries.length) {
    const empty = document.createElement("p");
    empty.className = "empty-cell";
    empty.textContent = "No capacity pools are configured.";
    root.append(empty);
    return;
  }
  for (const [name, pool] of entries) {
    const utilization = pool.configured_units > 0 ? Math.min(100, pool.reserved_units / pool.configured_units * 100) : 0;
    const article = document.createElement("article");
    article.className = "pool";
    const head = document.createElement("div"); head.className = "pool-head";
    const poolName = document.createElement("span"); poolName.className = "pool-name";
    const status = document.createElement("i"); status.setAttribute("aria-hidden", "true");
    const nameText = document.createElement("span"); nameText.textContent = name;
    poolName.append(status, nameText);
    const pressure = document.createElement("span"); pressure.className = "pool-pressure"; pressure.textContent = `${fmt.format(utilization)}% reserved`;
    head.append(poolName, pressure);
    const numbers = document.createElement("div"); numbers.className = "pool-numbers";
    const reserved = document.createElement("strong"); reserved.textContent = fmt.format(pool.reserved_units);
    const total = document.createElement("span"); total.textContent = `of ${fmt.format(pool.configured_units)} units`;
    numbers.append(reserved, total);
    const progress = document.createElement("progress"); progress.max = 100; progress.value = utilization; progress.setAttribute("aria-label", `${name} reserved capacity`);
    const calibration = document.createElement("div"); calibration.className = "pool-calibration";
    const values = [
      ["Safety factor", `${fmt.format(pool.safety_factor)}x`],
      ["Confidence", `${fmt.format(pool.confidence * 100)}%`],
      ["Estimate error", `${fmt.format(pool.estimate_error_ewma * 100)}%`],
      ["Observations", compact.format(pool.observations)]
    ];
    for (const [label, value] of values) {
      const wrap = document.createElement("div");
      const caption = document.createElement("span"); caption.textContent = label;
      const amount = document.createElement("strong"); amount.textContent = value;
      wrap.append(caption, amount); calibration.append(wrap);
    }
    article.append(head, numbers, progress, calibration); root.append(article);
  }
}

function renderClasses(decisions) {
  const groups = new Map();
  for (const item of decisions) {
    const group = groups.get(item.effective_class) || { count: 0, queued: 0, rejected: 0, queues: [], work: 0 };
    group.count += 1;
    group.work += item.estimated_work_units;
    if (item.queue_age_ms > 0 || item.outcome.includes("queue")) group.queued += 1;
    if (item.outcome.startsWith("rejected")) group.rejected += 1;
    if (item.queue_age_ms > 0) group.queues.push(item.queue_age_ms);
    groups.set(item.effective_class, group);
  }
  const body = $("#class-rows"); body.replaceChildren();
  if (!groups.size) {
    const row = document.createElement("tr"); const value = cell(row, "Waiting for decisions", "empty-cell"); value.colSpan = 6; body.append(row); return;
  }
  const order = ["realtime", "interactive", "standard", "workflow", "batch"];
  for (const name of [...groups.keys()].sort((a, b) => order.indexOf(a) - order.indexOf(b))) {
    const group = groups.get(name); const row = document.createElement("tr");
    cell(row, name, "class-name"); cell(row, group.count, "mono"); cell(row, group.queued, "mono"); cell(row, group.rejected, "mono");
    cell(row, group.queues.length ? `${fmt.format(percentile(group.queues, .95))} ms` : "0 ms", "mono");
    cell(row, fmt.format(group.work), "mono"); body.append(row);
  }
}

function renderDecisions(decisions) {
  const body = $("#decision-rows"); body.replaceChildren();
  const recent = [...decisions].reverse().slice(0, 50);
  setText("#decision-count", `${decisions.length} record${decisions.length === 1 ? "" : "s"}`);
  if (!recent.length) {
    const row = document.createElement("tr"); const value = cell(row, "No decisions recorded yet. Send traffic through the proxy to populate this view.", "empty-cell"); value.colSpan = 7; body.append(row); return;
  }
  for (const item of recent) {
    const row = document.createElement("tr");
    cell(row, item.request_id.slice(0, 8), "mono"); cell(row, item.effective_class, "class-name");
    cell(row, `${item.tenant} / ${item.application}`); cell(row, item.pool, "mono");
    cell(row, fmt.format(item.estimated_work_units), "mono"); cell(row, `${item.queue_age_ms} ms`, "mono");
    const outcome = cell(row, ""); const badge = document.createElement("span"); badge.className = "outcome"; badge.dataset.kind = outcomeKind(item.outcome); badge.textContent = item.outcome; outcome.append(badge);
    body.append(row);
  }
}

function drawChart() {
  const canvas = $("#activity-chart");
  const rect = canvas.getBoundingClientRect();
  const ratio = window.devicePixelRatio || 1;
  canvas.width = Math.max(1, Math.floor(rect.width * ratio)); canvas.height = Math.max(1, Math.floor(rect.height * ratio));
  const ctx = canvas.getContext("2d"); ctx.scale(ratio, ratio); ctx.clearRect(0, 0, rect.width, rect.height);
  const style = getComputedStyle(document.documentElement);
  const line = style.getPropertyValue("--line").trim(); const accent = style.getPropertyValue("--accent").trim(); const danger = style.getPropertyValue("--danger").trim();
  ctx.strokeStyle = line; ctx.lineWidth = 1;
  for (let i = 1; i < 4; i += 1) { const y = rect.height * i / 4; ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(rect.width, y); ctx.stroke(); }
  const maximum = Math.max(1, ...history.flatMap(point => [point.admitted, point.rejected]));
  const draw = (key, color) => {
    ctx.strokeStyle = color; ctx.lineWidth = 2; ctx.lineJoin = "round"; ctx.lineCap = "round"; ctx.beginPath();
    history.forEach((point, index) => { const x = history.length === 1 ? rect.width : index / (history.length - 1) * rect.width; const y = rect.height - (point[key] / maximum * (rect.height - 8)) - 4; if (index === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y); }); ctx.stroke();
  };
  draw("admitted", accent); draw("rejected", danger);
}

function render(status, capacity, decisions, ready) {
  const counters = status.counters;
  const totalCapacity = Object.values(capacity).reduce((sum, pool) => sum + pool.configured_units, 0);
  const totalReserved = Object.values(capacity).reduce((sum, pool) => sum + pool.reserved_units, 0);
  setText("#active", status.active); setText("#active-detail", `${Object.keys(capacity).length} pool${Object.keys(capacity).length === 1 ? "" : "s"}, ${fmt.format(totalReserved)} of ${fmt.format(totalCapacity)} units reserved`);
  setText("#queue-depth", status.queue.depth); setText("#queue-detail", `${formatBytes(status.queue.bytes)} buffered across ${status.queue.active_queues} queues`);
  setText("#admission-rate", counters.requests ? `${fmt.format(counters.admitted / counters.requests * 100)}%` : "100%"); setText("#admission-detail", `${compact.format(counters.admitted)} admitted of ${compact.format(counters.requests)} requests`);
  setText("#throttles", compact.format(counters.provider_throttles)); setText("#throttle-detail", counters.provider_throttles ? "Provider feedback is tightening capacity safety" : "No upstream throttles observed");
  setText("#version", status.version); setText("#uptime", formatDuration(status.uptime_seconds)); setText("#active-queues", status.queue.active_queues); setText("#drain-state", status.draining ? "Draining" : "Accepting traffic"); setText("#readiness", ready.status === "ready" ? `${ready.usable_pools}/${ready.pools} pools usable` : ready.status);
  const badge = $("#mode-badge"); badge.textContent = `${status.mode} mode`; badge.dataset.mode = status.mode;
  const now = Date.now(); const delta = previous ? { admitted: Math.max(0, counters.admitted - previous.admitted), rejected: Math.max(0, counters.rejected - previous.rejected), time: now } : { admitted: 0, rejected: 0, time: now };
  previous = counters; history.push(delta); if (history.length > 48) history.shift();
  renderPools(capacity); renderClasses(decisions); renderDecisions(decisions); drawChart();
  setText("#updated-at", new Date().toLocaleTimeString());
  const chip = $("#health-chip"); chip.dataset.state = "ready"; chip.querySelector("span").textContent = status.draining ? "Draining" : "Ready";
}

function showError(error) {
  if (error.auth) {
    const dialog = $("#auth-dialog");
    $("#auth-error").hidden = true;
    if (!dialog.open) dialog.showModal();
    return;
  }
  $("#error-banner").hidden = false; setText("#error-message", error.message);
  const chip = $("#health-chip"); chip.dataset.state = "error"; chip.querySelector("span").textContent = "Unavailable";
}

async function refresh() {
  try {
    const [status, capacity, decisions, ready] = await Promise.all([api("/api/v1/status"), api("/api/v1/capacity"), api("/api/v1/decisions"), api("/health/ready")]);
    $("#error-banner").hidden = true; render(status, capacity, decisions, ready);
  } catch (error) { showError(error); }
}

function schedule() {
  clearInterval(refreshTimer);
  refreshTimer = setInterval(() => { if (!document.hidden) refresh(); }, 2000);
}

function applyTheme(theme) {
  document.documentElement.dataset.theme = theme;
  $("#theme-toggle").textContent = theme === "dark" ? "Light" : "Dark";
  localStorage.setItem("inferqos-theme", theme); drawChart();
}

const storedTheme = localStorage.getItem("inferqos-theme");
applyTheme(storedTheme || (matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light"));
$("#theme-toggle").addEventListener("click", () => applyTheme(document.documentElement.dataset.theme === "dark" ? "light" : "dark"));
$("#refresh").addEventListener("click", refresh); $("#retry").addEventListener("click", refresh);
$("#auth-form").addEventListener("submit", async (event) => {
  event.preventDefault(); adminToken = $("#admin-token").value; $("#admin-token").value = "";
  try { await api("/api/v1/status"); $("#auth-dialog").close(); refresh(); } catch (error) { $("#auth-error").hidden = false; }
});
window.addEventListener("resize", drawChart, { passive: true });
refresh(); schedule();
