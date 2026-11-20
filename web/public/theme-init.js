// Applies the theme before the first paint (src/features/theme/theme.ts has
// the same rules and takes over once the app runs). A file rather than an
// inline script, because the Content-Security-Policy allows no inline scripts.
(function () {
  var choice = "system";
  try {
    choice = localStorage.getItem("saldo.theme") || "system";
  } catch (e) {}
  var dark =
    choice === "dark" ||
    (choice !== "light" && window.matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.classList.toggle("dark", dark);
  document.documentElement.style.colorScheme = dark ? "dark" : "light";
  if (dark) document.querySelector('meta[name="theme-color"]').content = "#0a0a0a";
})();
