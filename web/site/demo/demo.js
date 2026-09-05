"use strict";

const classes = {
  interactive: { weight: 50, work: [7, 18], duration: [2, 6], deadline: 15, queueable: true },
  workflow: { weight: 10, work: [18, 48], duration: [5, 13], deadline: 55, queueable: true },
  batch: { weight: 1, work: [42, 94], duration: [10, 24], deadline: 180, queueable: true }
};
const seedValue = 42731;
let results = null;
let selected = "inferqos";

function rng(seed) { let value = seed >>> 0; return () => { value = (value * 1664525 + 1013904223) >>> 0; return value / 4294967296; }; }
function range(random, pair) { return Math.round(pair[0] + random() * (pair[1] - pair[0])); }
function percentile(values, p) { if (!values.length) return 0; const sorted = [...values].sort((a,b) => a-b); return sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * p) - 1)]; }
function jain(values) { if (!values.length) return 1; const sum = values.reduce((a,b)=>a+b,0); if (!sum) return 0; return sum * sum / (values.length * values.reduce((a,b)=>a+b*b,0)); }
function value(id) { return Number(document.querySelector(id).value); }

function updateControls() {
  document.querySelector("#capacity-value").textContent = `${value("#capacity")} units`;
  document.querySelector("#burst-value").textContent = `${(value("#burst") / 10).toFixed(1)}x`;
  document.querySelector("#interactive-value").textContent = `${value("#interactive-mix")}%`;
  document.querySelector("#batch-value").textContent = `${value("#batch-mix")}%`;
}
document.querySelectorAll("input[type=range]").forEach(input => input.addEventListener("input", updateControls)); updateControls();

function generateWorkload() {
  const random = rng(seedValue); const pattern = document.querySelector("input[name=pattern]:checked").value;
  let interactiveShare = value("#interactive-mix") / 100; let batchShare = value("#batch-mix") / 100;
  if (interactiveShare + batchShare > .95) { const scale = .95 / (interactiveShare + batchShare); interactiveShare *= scale; batchShare *= scale; }
  const workflowShare = 1 - interactiveShare - batchShare;
  const burst = value("#burst") / 10; const jobs = []; let id = 0; let peak = 0; const offered = new Array(600).fill(0);
  for (let tick = 0; tick < 600; tick += 1) {
    let multiplier = 1;
    if (pattern === "bursty") multiplier = (tick > 105 && tick < 175) || (tick > 355 && tick < 425) ? burst : .56;
    if (pattern === "diurnal") multiplier = .45 + .75 * (1 + Math.sin((tick - 120) / 600 * Math.PI * 2)) / 2;
    const arrivals = Math.floor(.72 * multiplier + random() * 1.55 * multiplier);
    for (let i = 0; i < arrivals; i += 1) {
      const roll = random(); const kind = roll < interactiveShare ? "interactive" : roll < interactiveShare + workflowShare ? "workflow" : "batch"; const config = classes[kind];
      const job = { id: id += 1, kind, arrival: tick, work: range(random, config.work), duration: range(random, config.duration), deadline: tick + config.deadline };
      jobs.push(job); offered[tick] += job.work;
    }
    peak = Math.max(peak, offered[tick]);
  }
  return { jobs, peak, offered };
}

function simulate(workload, policy, capacity) {
  const queue = []; const active = []; const completed = []; const throttled = []; const served = { interactive: 0, workflow: 0, batch: 0 }; const pressure = { interactive: [], workflow: [], batch: [] };
  let utilized = 0; let index = 0; const quantum = 120;
  const select = tick => {
    if (policy === "fifo") return 0;
    if (policy === "strict") {
      const order = { interactive: 3, workflow: 2, batch: 1 }; let best = 0;
      for (let i = 1; i < queue.length; i += 1) if (order[queue[i].kind] > order[queue[best].kind]) best = i;
      return best;
    }
    let best = 0; let score = -Infinity;
    for (let i = 0; i < queue.length; i += 1) {
      const job = queue[i]; const age = (tick - job.arrival) / 14; const remaining = Math.max(1, job.deadline - tick); const deadline = 20 / remaining; const fair = classes[job.kind].weight * quantum / (served[job.kind] + quantum); const size = 12 / Math.max(6, job.work);
      const current = fair + age + deadline + size;
      if (current > score || (current === score && job.id < queue[best].id)) { score = current; best = i; }
    }
    return best;
  };
  for (let tick = 0; tick < 900; tick += 1) {
    for (let i = active.length - 1; i >= 0; i -= 1) {
      active[i].left -= 1;
      if (active[i].left <= 0) { const [done] = active.splice(i, 1); completed.push({ ...done, finished: tick }); }
    }
    while (index < workload.jobs.length && workload.jobs[index].arrival === tick) {
      const job = { ...workload.jobs[index], left: workload.jobs[index].duration, admitted: null };
      if (policy === "baseline") {
        const used = active.reduce((sum, item) => sum + item.work, 0);
        if (used + job.work <= capacity) { job.admitted = tick; active.push(job); } else throttled.push(job);
      } else queue.push(job);
      index += 1;
    }
    if (policy !== "baseline") {
      let guard = queue.length + 1;
      while (queue.length && guard > 0) {
        guard -= 1; const position = select(tick); const job = queue[position]; const used = active.reduce((sum, item) => sum + item.work, 0);
        if (used + job.work <= capacity) { queue.splice(position, 1); job.admitted = tick; served[job.kind] += job.work / classes[job.kind].weight; active.push(job); }
        else break;
      }
    }
    utilized += Math.min(capacity, active.reduce((sum, item) => sum + item.work, 0));
    for (const kind of Object.keys(classes)) pressure[kind].push(queue.filter(job => job.kind === kind).length);
    if (tick >= 600 && !queue.length && !active.length) break;
  }
  const deadlines = completed.filter(job => job.finished <= job.deadline).length; const latencies = { interactive: [], workflow: [], batch: [] }; const completions = { interactive: 0, workflow: 0, batch: 0 };
  completed.forEach(job => { latencies[job.kind].push((job.admitted - job.arrival) * 200); completions[job.kind] += 1; });
  const normalized = Object.keys(classes).map(kind => completions[kind] / Math.max(1, workload.jobs.filter(job => job.kind === kind).length));
  const starvation = completed.filter(job => job.admitted - job.arrival > classes[job.kind].deadline).length + queue.length;
  return { policy, throttles: throttled.length, interactiveP95: percentile(latencies.interactive, .95), deadline: workload.jobs.length ? deadlines / workload.jobs.length : 0, utilization: utilized / (capacity * pressure.interactive.length), fairness: jain(normalized), starvation, pressure, completed: completed.length, total: workload.jobs.length };
}

function renderTable() {
  const labels = { baseline: "No admission control", fifo: "FIFO", strict: "Strict priority", inferqos: "InferQoS" }; const body = document.querySelector("#comparison-body"); body.replaceChildren();
  Object.entries(results).forEach(([key, result]) => {
    const row = document.createElement("tr"); row.tabIndex = 0; row.dataset.policy = key; if (key === selected) row.dataset.selected = "true";
    const values = [labels[key], result.throttles, `${result.interactiveP95} ms`, `${(result.deadline * 100).toFixed(1)}%`, `${(result.utilization * 100).toFixed(1)}%`, result.fairness.toFixed(3), result.starvation];
    values.forEach((entry, index) => { const cell = document.createElement("td"); cell.textContent = entry; if (index > 0) cell.className = "mono"; row.append(cell); });
    const choose = () => { selected = key; renderTable(); drawChart(); };
    row.addEventListener("click", choose); row.addEventListener("keydown", event => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); choose(); } }); body.append(row);
  });
}

function drawChart() {
  if (!results) return; const canvas = document.querySelector("#simulation-chart"); const rect = canvas.getBoundingClientRect(); const ratio = window.devicePixelRatio || 1; canvas.width = Math.floor(rect.width * ratio); canvas.height = Math.floor(rect.height * ratio);
  const ctx = canvas.getContext("2d"); ctx.scale(ratio, ratio); ctx.clearRect(0, 0, rect.width, rect.height); const css = getComputedStyle(document.documentElement); const grid = css.getPropertyValue("--line").trim(); const colors = { interactive: css.getPropertyValue("--accent").trim(), workflow: css.getPropertyValue("--blue").trim(), batch: css.getPropertyValue("--amber").trim() };
  ctx.strokeStyle = grid; ctx.lineWidth = 1; for (let i = 1; i < 5; i += 1) { const y = rect.height * i / 5; ctx.beginPath(); ctx.moveTo(0,y); ctx.lineTo(rect.width,y); ctx.stroke(); }
  const pressure = results[selected].pressure; const max = Math.max(1, ...Object.values(pressure).flat());
  Object.entries(pressure).forEach(([kind, values]) => { ctx.strokeStyle = colors[kind]; ctx.lineWidth = 2; ctx.beginPath(); values.forEach((amount, index) => { const x = index / Math.max(1, values.length - 1) * rect.width; const y = rect.height - amount / max * (rect.height - 8) - 4; if (!index) ctx.moveTo(x,y); else ctx.lineTo(x,y); }); ctx.stroke(); });
  document.querySelector("#selected-policy").textContent = { baseline: "No admission control", fifo: "FIFO", strict: "Strict priority", inferqos: "InferQoS" }[selected];
}

function renderRecommendation(workload, capacity) {
  const qos = results.inferqos; const sustained = workload.offered.filter(amount => amount > capacity).length / workload.offered.length; const box = document.querySelector("#recommendation"); const title = box.querySelector("h2"); const text = box.querySelector("p:last-child");
  if (sustained > .34 || qos.deadline < .86) { title.textContent = "Scheduling cannot fully solve this workload."; text.textContent = `Demand exceeds the ${capacity}-unit envelope for ${(sustained * 100).toFixed(1)}% of the arrival window. More capacity or less offered work is recommended after validating with production metadata.`; box.dataset.tone = "warning"; }
  else { title.textContent = "Queueable work can absorb these peaks in this model."; text.textContent = `InferQoS preserves ${(qos.deadline * 100).toFixed(1)}% deadline attainment with ${(qos.utilization * 100).toFixed(1)}% utilization and ${qos.throttles} modeled provider throttles. Validate the projection in shadow mode before enforcement.`; box.dataset.tone = "positive"; }
}

function run() {
  const button = document.querySelector("#run-simulation"); button.disabled = true; document.querySelector("#simulation-state").textContent = "Computing";
  requestAnimationFrame(() => {
    const workload = generateWorkload(); const capacity = value("#capacity"); results = { baseline: simulate(workload, "baseline", capacity), fifo: simulate(workload, "fifo", capacity), strict: simulate(workload, "strict", capacity), inferqos: simulate(workload, "inferqos", capacity) };
    document.querySelector("#request-count").textContent = workload.jobs.length.toLocaleString(); document.querySelector("#peak-demand").textContent = `${workload.peak} units`; const queueable = workload.jobs.filter(job => job.kind !== "interactive").length / Math.max(1, workload.jobs.length); document.querySelector("#queueable-work").textContent = `${(queueable * 100).toFixed(1)}%`;
    renderTable(); drawChart(); renderRecommendation(workload, capacity); document.querySelector("#simulation-state").textContent = "Complete"; button.disabled = false;
  });
}

document.querySelector("#run-simulation").addEventListener("click", run);
document.querySelector("#reset-simulation").addEventListener("click", () => { document.querySelector("#capacity").value = 100; document.querySelector("#burst").value = 14; document.querySelector("#interactive-mix").value = 35; document.querySelector("#batch-mix").value = 35; document.querySelector("input[value=bursty]").checked = true; updateControls(); run(); });
window.addEventListener("resize", drawChart, { passive: true }); window.addEventListener("inferqos-theme-change", drawChart); run();
