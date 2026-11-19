/**
 * The map's label fonts, served by the app itself so that place names never
 * depend on someone else's server.
 *
 * The font files are the ones the iPhone app bundles (one copy in the
 * repository). The build gives each a name with a hash of its contents, so
 * the map cannot ask for them by the plain `{fontstack}/{range}.pbf` its
 * style names. MapLibre is told instead how to answer such a request.
 */

import { FONT_SCHEME } from './style';

/** The address of each font file after the build, by its path in the repository. */
const files = import.meta.glob<string>('../../../../ios/Venti/Resources/MapDemo/fonts/*/*.pbf', {
	query: '?url',
	import: 'default',
	eager: true
});

/** The same addresses by what the map asks for: `noto-sans-regular/0-255.pbf`. */
export function fontTable(byPath: Record<string, string> = files): Map<string, string> {
	return new Map(
		Object.entries(byPath).map(([path, url]) => [path.split('/').slice(-2).join('/'), url])
	);
}

/**
 * Where to fetch what the map asked for, or nothing if the app has no such
 * file.
 *
 * The folder holds a file for every range of 256 characters up to 65,535:
 * real glyphs for Latin, Greek, Cyrillic, punctuation and common symbols,
 * and a file without glyphs for each other range. A label in another
 * script is drawn without those characters. For anything else the map may
 * ask for, another font or a range beyond those, there is no file, and
 * {@link fontLoader} answers that with no glyphs too.
 */
export function fontUrl(requested: string, table: Map<string, string>): string | undefined {
	const prefix = `${FONT_SCHEME}://`;
	if (!requested.startsWith(prefix)) return undefined;
	return table.get(decodeURIComponent(requested.slice(prefix.length)));
}

/** The answer MapLibre expects from a handler of a URL scheme. */
type Answer = { data: ArrayBuffer };

/** Answers the map's requests for fonts. Give it to MapLibre's `addProtocol`. */
export function fontLoader(
	table = fontTable()
): (request: { url: string }, abort: AbortController) => Promise<Answer> {
	return async (request, abort) => {
		const url = fontUrl(request.url, table);
		// Not an error: a failed font would cost the map tile all its labels.
		// A font file with nothing in it holds no characters, which is the truth.
		if (!url) return { data: new ArrayBuffer(0) };
		const response = await fetch(url, { signal: abort.signal });
		if (!response.ok) throw new Error(`The label font could not be loaded (${response.status}).`);
		return { data: await response.arrayBuffer() };
	};
}
