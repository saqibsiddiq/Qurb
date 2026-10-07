// Which theme to draw in (decision 0056), set before anything draws: the
// person's choice from Settings, or the system's. Loaded first, from <head>,
// since a window that drew light and then turned dark would flash.
//
// The choice is this window's own, kept in its storage: it is how this screen
// looks, not something the other devices need to know.
(function () {
  const dark = matchMedia("(prefers-color-scheme: dark)");
  function chosen() {
    try { return localStorage.getItem("qurb.theme") || "system"; } catch (e) { return "system"; }
  }
  function apply() {
    const choice = chosen();
    const isDark = choice === "dark" || (choice === "system" && dark.matches);
    document.documentElement.dataset.theme = isDark ? "dark" : "light";
  }
  apply();
  dark.addEventListener("change", apply);
  window.qurbTheme = {
    chosen,
    choose(choice) {
      try { localStorage.setItem("qurb.theme", choice); } catch (e) { /* kept for this run */ }
      apply();
    },
  };
})();
