(() => {
  const root = document.documentElement;
  const storedTheme = localStorage.getItem("inferqos-docs-theme");
  root.dataset.theme = storedTheme || (matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");
  document.querySelector(".theme-toggle")?.addEventListener("click", () => {
    root.dataset.theme = root.dataset.theme === "dark" ? "light" : "dark";
    localStorage.setItem("inferqos-docs-theme", root.dataset.theme);
  });
  const sidebar = document.querySelector(".docs-sidebar");
  const sidebarToggle = document.querySelector(".sidebar-toggle");
  sidebarToggle?.addEventListener("click", () => {
    const open = sidebar?.classList.toggle("open") || false;
    sidebarToggle.setAttribute("aria-expanded", String(open));
  });
  const currentPath = location.pathname.replace(/index\.html$/, "");
  document.querySelectorAll(".docs-sidebar a").forEach((link) => {
    const path = new URL(link.href).pathname.replace(/index\.html$/, "");
    if (path === currentPath) link.classList.add("active");
    link.addEventListener("click", () => sidebar?.classList.remove("open"));
  });
  const search = document.querySelector(".docs-search input");
  document.addEventListener("keydown", (event) => {
    if (event.key === "/" && document.activeElement !== search) { event.preventDefault(); search?.focus(); }
  });
  search?.addEventListener("input", () => {
    const query = search.value.trim().toLowerCase();
    document.querySelectorAll(".nav-group a").forEach((link) => { link.hidden = query.length > 0 && !link.textContent.toLowerCase().includes(query); });
  });
  const headings = [...document.querySelectorAll(".docs-content article h2, .docs-content article h3")]
    .filter((heading) => !heading.closest(".doc-card"));
  const toc = document.querySelector(".docs-toc nav");
  headings.forEach((heading, index) => {
    if (!heading.id) heading.id = `${heading.textContent.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/(^-|-$)/g, "")}-${index}`;
    const link = document.createElement("a");
    link.href = `#${heading.id}`; link.textContent = heading.textContent; link.dataset.level = heading.tagName === "H3" ? "3" : "2"; toc?.appendChild(link);
  });
  if (headings.length) {
    const observer = new IntersectionObserver((entries) => entries.forEach((entry) => {
      if (!entry.isIntersecting) return;
      document.querySelectorAll(".docs-toc nav a").forEach((link) => link.classList.toggle("active", link.getAttribute("href") === `#${entry.target.id}`));
    }), { rootMargin: "-20% 0px -70%" });
    headings.forEach((heading) => observer.observe(heading));
  }
  document.querySelectorAll("pre").forEach((pre) => {
    const button = document.createElement("button"); button.className = "copy-code"; button.type = "button"; button.textContent = "COPY";
    button.addEventListener("click", async () => { await navigator.clipboard.writeText(pre.textContent || ""); button.textContent = "COPIED"; setTimeout(() => { button.textContent = "COPY"; }, 1200); });
    pre.appendChild(button);
  });
  const progress = document.querySelector(".reading-progress i");
  const updateProgress = () => { const available = document.documentElement.scrollHeight - innerHeight; if (progress) progress.style.width = `${available > 0 ? Math.min(100, (scrollY / available) * 100) : 0}%`; };
  addEventListener("scroll", updateProgress, { passive: true }); updateProgress();
})();
