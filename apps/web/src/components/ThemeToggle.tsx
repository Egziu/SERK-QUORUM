import { useEffect, useState } from "react";

type Theme = "system" | "light" | "dark";
const KEY = "quorum.theme";

function readTheme(): Theme {
  try {
    const v = localStorage.getItem(KEY);
    return v === "light" || v === "dark" ? v : "system";
  } catch {
    return "system";
  }
}

/** Theme preference is a non-sensitive convenience, stored locally. */
export function ThemeToggle() {
  const [theme, setTheme] = useState<Theme>(readTheme);
  useEffect(() => {
    const root = document.documentElement;
    if (theme === "system") root.removeAttribute("data-theme");
    else root.setAttribute("data-theme", theme);
    try {
      if (theme === "system") localStorage.removeItem(KEY);
      else localStorage.setItem(KEY, theme);
    } catch {
      /* storage unavailable: preference lasts for this page only */
    }
  }, [theme]);
  return (
    <label className="row small">
      <span className="visually-hidden">Colour theme</span>
      <select value={theme} onChange={(e) => setTheme(e.target.value as Theme)} aria-label="Colour theme">
        <option value="system">System theme</option>
        <option value="light">Light</option>
        <option value="dark">Dark</option>
      </select>
    </label>
  );
}
