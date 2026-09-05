"use strict";

const root = document.documentElement;
const storedTheme = localStorage.getItem("inferqos-site-theme");
const initialTheme = storedTheme || (matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");

function applyTheme(theme) {
  root.dataset.theme = theme;
  document.querySelectorAll("[data-theme-toggle]").forEach(button => { button.textContent = theme === "dark" ? "Light" : "Dark"; });
  localStorage.setItem("inferqos-site-theme", theme);
  window.dispatchEvent(new CustomEvent("inferqos-theme-change"));
}

applyTheme(initialTheme);
document.querySelectorAll("[data-theme-toggle]").forEach(button => button.addEventListener("click", () => applyTheme(root.dataset.theme === "dark" ? "light" : "dark")));

const menu = document.querySelector(".menu-toggle");
const mobileNav = document.querySelector("#mobile-nav");
if (menu && mobileNav) {
  menu.addEventListener("click", () => {
    const open = menu.getAttribute("aria-expanded") === "true";
    menu.setAttribute("aria-expanded", String(!open));
    mobileNav.hidden = open;
  });
  mobileNav.querySelectorAll("a").forEach(link => link.addEventListener("click", () => { menu.setAttribute("aria-expanded", "false"); mobileNav.hidden = true; }));
}

const revealItems = document.querySelectorAll(".reveal");
if (matchMedia("(prefers-reduced-motion: reduce)").matches) {
  revealItems.forEach(item => item.classList.add("revealed"));
} else {
  const observer = new IntersectionObserver(entries => {
    entries.forEach(entry => { if (entry.isIntersecting) { entry.target.classList.add("revealed"); observer.unobserve(entry.target); } });
  }, { threshold: .12 });
  revealItems.forEach(item => observer.observe(item));
}

const copy = document.querySelector("[data-copy-command]");
if (copy) copy.addEventListener("click", async () => {
  const text = document.querySelector("#install-command").textContent;
  try { await navigator.clipboard.writeText(text); copy.textContent = "Copied"; setTimeout(() => { copy.textContent = "Copy"; }, 1600); }
  catch { copy.textContent = "Select text"; }
});

const preview = document.querySelector("#preview-chart");
if (preview) {
  const drawPreview = () => {
    const rect = preview.getBoundingClientRect(); const ratio = window.devicePixelRatio || 1;
    preview.width = Math.floor(rect.width * ratio); preview.height = Math.floor(rect.height * ratio);
    const ctx = preview.getContext("2d"); ctx.scale(ratio, ratio); ctx.clearRect(0, 0, rect.width, rect.height);
    const css = getComputedStyle(root); const line = css.getPropertyValue("--line").trim(); const teal = css.getPropertyValue("--accent").trim(); const amber = css.getPropertyValue("--amber").trim(); const blue = css.getPropertyValue("--blue").trim();
    ctx.strokeStyle = line; ctx.lineWidth = 1;
    for (let i = 1; i < 5; i += 1) { const y = rect.height * i / 5; ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(rect.width, y); ctx.stroke(); }
    const series = [
      { color: teal, data: [8,9,11,14,29,41,22,15,12,9,8,16,34,47,31,18,12,10,9,8,7] },
      { color: blue, data: [13,14,14,15,16,18,21,24,26,25,23,22,20,20,19,18,17,16,15,14,13] },
      { color: amber, data: [31,32,33,34,32,29,24,19,21,24,28,30,25,18,14,17,22,27,30,31,32] }
    ];
    series.forEach(item => { ctx.strokeStyle = item.color; ctx.lineWidth = 2; ctx.beginPath(); item.data.forEach((value, index) => { const x = index / (item.data.length - 1) * rect.width; const y = rect.height - value / 55 * rect.height; if (!index) ctx.moveTo(x, y); else ctx.lineTo(x, y); }); ctx.stroke(); });
  };
  drawPreview(); window.addEventListener("resize", drawPreview, { passive: true }); window.addEventListener("inferqos-theme-change", drawPreview);
}
