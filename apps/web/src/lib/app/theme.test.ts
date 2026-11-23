import { describe, expect, it } from 'vitest';
import { applyTheme, keepTheme, keptTheme, THEME_KEY } from './theme';

/** A browser's storage, in memory. */
function memory(): Storage {
	const kept = new Map<string, string>();
	return {
		getItem: (key: string) => kept.get(key) ?? null,
		setItem: (key: string, value: string) => void kept.set(key, value),
		removeItem: (key: string) => void kept.delete(key)
	} as Storage;
}

/** What `applyTheme` touches of a page: the root and the two theme colours of `app.html`. */
function page() {
	const metas = ['light', 'dark'].map((scheme) => ({
		dataset: { scheme },
		media: `(prefers-color-scheme: ${scheme})`
	}));
	const root = { dataset: {} as Record<string, string> };
	const document = {
		documentElement: root,
		querySelectorAll: () => metas
	} as unknown as Document;
	return { document, root, metas };
}

describe('the theme', () => {
	it('follows the system until one is chosen, and keeps the choice', () => {
		const storage = memory();
		expect(keptTheme(storage)).toBe('Automatic');
		keepTheme('Dark', storage);
		expect(storage.getItem(THEME_KEY)).toBe('dark');
		expect(keptTheme(storage)).toBe('Dark');
		keepTheme('Automatic', storage);
		expect(storage.getItem(THEME_KEY)).toBeNull();
		expect(keptTheme(storage)).toBe('Automatic');
	});

	it('follows the system where the browser keeps nothing', () => {
		const refusing = {
			getItem: () => {
				throw new Error('denied');
			},
			setItem: () => {
				throw new Error('denied');
			},
			removeItem: () => {
				throw new Error('denied');
			}
		};
		expect(keptTheme(refusing)).toBe('Automatic');
		expect(() => keepTheme('Light', refusing)).not.toThrow();
		expect(keptTheme(undefined)).toBe('Automatic');
	});

	it('sets the root and the browser bar’s colour, and gives both back to the system', () => {
		const { document, root, metas } = page();
		applyTheme('Dark', document);
		expect(root.dataset.theme).toBe('dark');
		expect(metas.map((meta) => meta.media)).toEqual(['not all', 'all']);
		applyTheme('Light', document);
		expect(root.dataset.theme).toBe('light');
		expect(metas.map((meta) => meta.media)).toEqual(['all', 'not all']);
		applyTheme('Automatic', document);
		expect(root.dataset.theme).toBeUndefined();
		expect(metas.map((meta) => meta.media)).toEqual([
			'(prefers-color-scheme: light)',
			'(prefers-color-scheme: dark)'
		]);
	});
});
