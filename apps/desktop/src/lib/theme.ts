/** Theme handling for the `ui.theme` setting (`system` | `light` |
 *  `dark`). Tailwind v4 is configured with a class-based dark
 *  variant (`@custom-variant dark` in `styles/index.css`), so the
 *  theme is a `.dark` class on `<html>`. `system` follows the OS
 *  preference as it stands when the app starts; changing the OS
 *  theme while the app runs takes effect on the next start. */

export type ThemeSetting = "system" | "light" | "dark";

export function parseTheme(value: string | null | undefined): ThemeSetting {
  if (value === "light" || value === "dark") return value;
  return "system";
}

export function applyTheme(theme: ThemeSetting): void {
  const dark =
    theme === "dark" ||
    (theme === "system" &&
      window.matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.classList.toggle("dark", dark);
}
