/**
 * What the app's frame says on its own, around every screen. The core
 * cannot give these words: they are said where it cannot run.
 */

/** Said over the whole window in a browser that has WebAssembly switched off. */
export const NO_WEBASSEMBLY = {
	title: 'Venti needs WebAssembly',
	why: 'The map, the forecast, your logbook and nearly everything else in Venti rely on it, and this browser has switched it off. Organisations often switch it off with JavaScript optimisation.',
	/** What the pilot can do, a line each. A page cannot open Chrome's own settings, so none is a link. */
	ways: [
		'Open Venti in another browser: Safari, Firefox, or Chrome where JavaScript optimisation is allowed.',
		'Or allow JavaScript optimisation for this site in Chrome’s site settings, if your organisation lets you.'
	]
} as const;
