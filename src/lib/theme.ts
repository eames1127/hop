import type { Theme } from "./types";

// "system" leaves the attribute off so the prefers-color-scheme rules apply.
export function applyTheme(theme: Theme): void {
	if (theme === "system") {
		delete document.documentElement.dataset.theme;
	}
	else {
		document.documentElement.dataset.theme = theme;
	}
}
