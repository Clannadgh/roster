export type ThemePreference = "system" | "light" | "dark";

const THEME_STORAGE_KEY = "roster.theme";

function isThemePreference(value: string | null): value is ThemePreference {
  return value === "system" || value === "light" || value === "dark";
}

export function getThemePreference(): ThemePreference {
  try {
    const stored = localStorage.getItem(THEME_STORAGE_KEY);
    if (isThemePreference(stored)) return stored;
  } catch {
    // 存储不可用时仍跟随系统。
  }
  return "system";
}

export function applyTheme(preference: ThemePreference = getThemePreference()) {
  const dark =
    preference === "dark" ||
    (preference === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.classList.toggle("dark", dark);
}

export function setThemePreference(preference: ThemePreference) {
  try {
    localStorage.setItem(THEME_STORAGE_KEY, preference);
  } catch {
    // 存储不可用时仍立即应用当前选择。
  }
  applyTheme(preference);
}

export function watchSystemTheme(): () => void {
  const query = window.matchMedia("(prefers-color-scheme: dark)");
  const onChange = () => {
    if (getThemePreference() === "system") applyTheme("system");
  };
  query.addEventListener("change", onChange);
  return () => query.removeEventListener("change", onChange);
}
