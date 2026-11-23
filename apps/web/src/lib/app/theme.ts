/**
 * Light, dark, or the system's: the pilot's choice in Settings, kept in
 * this browser only. The design pack's tokens do the rest: `data-theme` on
 * the root wins over `prefers-color-scheme`, and without it the system
 * decides (design/tokens/tokens.css). The map is drawn the same in both.
 *
 * `static/theme.js` applies the kept choice before the first paint, as a
 * file of the app, since the page's policy runs no script written into it;
 * it does what `applyTheme` does and reads the same key.
 */

export type ThemeChoice = 'Automatic' | 'Light' | 'Dark';

export const THEME_CHOICES: readonly ThemeChoice[] = ['Automatic', 'Light', 'Dark'];

/** Where the choice is kept. `static/theme.js` reads the same. */
export const THEME_KEY = 'venti.theme';

/** The kept choice; the system's where none was made or storage cannot be read. */
export function keptTheme(storage: Pick<Storage, 'getItem'> | undefined): ThemeChoice {
	try {
		const kept = storage?.getItem(THEME_KEY);
		if (kept === 'light') return 'Light';
		if (kept === 'dark') return 'Dark';
	} catch {
		// A browser that keeps nothing follows the system.
	}
	return 'Automatic';
}

/** Keeps the choice; the system's is kept as no choice at all. */
export function keepTheme(
	choice: ThemeChoice,
	storage: Pick<Storage, 'setItem' | 'removeItem'> | undefined
): void {
	try {
		if (choice === 'Automatic') storage?.removeItem(THEME_KEY);
		else storage?.setItem(THEME_KEY, choice.toLowerCase());
	} catch {
		// Applied for this page all the same.
	}
}

/**
 * Sets the theme on the page: the root's `data-theme`, and the colour of
 * the browser's own bar, whose two `theme-color` elements in `app.html`
 * follow the system until a theme is chosen.
 */
export function applyTheme(choice: ThemeChoice, document: Document): void {
	const root = document.documentElement;
	if (choice === 'Automatic') delete root.dataset.theme;
	else root.dataset.theme = choice.toLowerCase();
	for (const meta of document.querySelectorAll<HTMLMetaElement>('meta[name="theme-color"]')) {
		const scheme = meta.dataset.scheme;
		meta.media =
			choice === 'Automatic'
				? `(prefers-color-scheme: ${scheme})`
				: choice.toLowerCase() === scheme
					? 'all'
					: 'not all';
	}
}
