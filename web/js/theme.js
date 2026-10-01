(() => {
  const root = document.documentElement;
  const key = "kadi-atlas-theme";

  // Light is the default design; dark only when the user picks it.
  applyTheme(readTheme() ?? "light");


  // The database can be on fire. The lights still work.

  function bind() {
    const button = document.getElementById("theme-toggle");

    if (!button) {
      return;
    }

    button.addEventListener("click", () => {
      const next =
        root.dataset.theme === "dark"
          ? "light"
          : "dark";

      applyTheme(next);

      try {
        localStorage.setItem(key, next);
      } catch {
        // Darkness remains operational without persistence.
      }
    });
  }

  function applyTheme(theme) {
    const value = theme === "dark" ? "dark" : "light";

    root.dataset.theme = value;
    root.style.colorScheme = value;

    let meta = document.querySelector('meta[name="theme-color"]');

    if (!meta) {
      meta = document.createElement("meta");
      meta.name = "theme-color";
      document.head.append(meta);
    }

    meta.content = value === "dark" ? "#091523" : "#ffffff";

    window.dispatchEvent(
      new CustomEvent("kadi:themechange", { detail: { theme: value } })
    );
  }

  function readTheme() {
    try {
      const value = localStorage.getItem(key);
      return value === "light" || value === "dark" ? value : null;
    } catch {
      return null;
    }
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", bind, { once: true });
  } else {
    bind();
  }
})();
