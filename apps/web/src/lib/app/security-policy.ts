/**
 * The site's content security policy: everything a page of the app may
 * load, run and talk to. Whatever is not listed, the browser refuses.
 *
 * The session token of a signed-in pilot is kept where the app's own code
 * can read it (`apps/web/AGENTS.md` says why), so a stranger's script that
 * got to run on a page could reach it too. The policy is what stands in the
 * way: no script but the app's own files runs, and a page can send nothing
 * to a host the app does not use.
 *
 * The build writes it into every page as a `meta` element
 * (`vite.config.ts`), because the app is static files and whoever serves
 * them may set no headers. Two things a policy in a page cannot do: it
 * cannot say who may frame the site (`frame-ancestors`), and it does not
 * reach the app's workers. `apps/web/AGENTS.md` says what the web server
 * has to send for those.
 *
 * Every source in it is there because the browser refused something
 * without it, seen in Chromium on 2026-11-19. The browser tests fail on any
 * refusal (`e2e/helpers.ts`), which keeps the list from being too short;
 * only taking an entry out and running them shows that it is too long.
 */

import { ONLINE } from '../map/online.ts';

/** The scheme and host of an address. */
function origin(address: string): string {
	return new URL(address.replaceAll(/[{}]/g, '')).origin;
}

/** The hosts the map's tiles come from, as the map style names them. */
export const TILE_HOSTS: readonly string[] = [
	origin(ONLINE.basemap),
	origin(ONLINE.terrainTiles),
	...ONLINE.aerialHosts
];

/**
 * The policy, as SvelteKit's `csp` setting takes it. SvelteKit adds to
 * `script-src` the hash of the one inline script it writes into each page,
 * the one that starts the app.
 *
 * `inlineStyles` are the hashes of the `style` attributes that are let
 * through, each as `sha256-…`: see `vite.config.ts` for the one there is.
 */
export function contentSecurityPolicy(inlineStyles: readonly `sha256-${string}`[]) {
	return {
		// Nothing, unless a line below allows it: no frames, no plug-ins, no
		// media.
		'default-src': ['none'],
		// The app's own script files, and nothing written into a page or
		// made from text.
		'script-src': ['self'],
		// The app's own style sheets. No `style` element.
		'style-src': ['self'],
		// No `style` attribute in markup either, but for the one SvelteKit
		// writes. Styles the app sets from its scripts are not attributes
		// and need no leave.
		'style-src-attr': ['unsafe-hashes', ...inlineStyles],
		// The app icon; and the map engine's style sheet draws the icons of
		// its own controls from `data:` addresses.
		'img-src': ['self', 'data:'],
		// The app's own files that a page fetches: the font files for the
		// map's labels and the sample flight. The small font files the build
		// keeps inside a script, as `data:` addresses. And the tile
		// services, which a page asks for the base map's description, for
		// the elevation tiles and, once the pilot switches to the aerial
		// view, for its photographs. (What the app asks its own server under
		// `/v1`, the core's worker asks, which this policy does not reach;
		// held to it, it would find `/v1` under the app's own origin.)
		'connect-src': ['self', 'data:', ...TILE_HOSTS],
		// The typeface of the design pack, Fira Sans, served by the app
		// from its own files. Nothing is fetched from a font host.
		'font-src': ['self'],
		// The manifest that names the app's icons, from the app's own files.
		'manifest-src': ['self'],
		// The core and the map engine each run in a worker, both from files
		// of the app. Without this line `script-src` would say the same;
		// it is here so that a change there cannot change it unseen.
		'worker-src': ['self'],
		// No `base` element may send the app's relative addresses elsewhere.
		'base-uri': ['none'],
		// No form posts anywhere: the app's forms are handled by its scripts.
		'form-action': ['none']
	} satisfies Record<string, string[]>;
}
