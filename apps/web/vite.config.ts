import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vitest/config';
import { contentSecurityPolicy } from './src/lib/app/security-policy.ts';

// Where the app is served from when it is not at the root of its host, for
// example "/venti". Empty at the root.
const base = (process.env.VENTI_BASE_PATH ?? '') as '' | `/${string}`;

// The app asks the origin it is served from for the Venti server's API, as
// it will when one web server hands out both, and the links to shared
// flights (`/f/…`) and to live tracks (`/live/…`) lead to pages that server
// makes. While developing, that is this machine's
// `cargo run -p venti-server -- dev`.
const server = process.env.VENTI_DEV_SERVER ?? 'http://127.0.0.1:8080';
// With the slash: "/f" alone would also take "/favicon.svg" away from the app.
const api = { '/v1/': server, '/f/': server, '/live/': server };

/** What SvelteKit's `csp` setting takes as a policy. */
type Directives = NonNullable<
	NonNullable<NonNullable<Parameters<typeof sveltekit>[0]>['csp']>['directives']
>;

/**
 * The hash of the one inline style the app's pages carry, which is not the
 * app's: SvelteKit tells a screen reader the title of a page the pilot has
 * moved to in an element it hides with a `style` attribute. The policy
 * allows no inline styles, so it has to name this one, and it is read from
 * the SvelteKit that is installed, so that an update cannot leave the two
 * apart. If SvelteKit moves it, the build stops here and says so.
 */
function announcerStyleHash(): `sha256-${string}` {
	const kit = createRequire(import.meta.url).resolve('@sveltejs/kit/package.json');
	const root = readFileSync(
		new URL('./src/runtime/components/root.svelte', `file://${kit}`),
		'utf8'
	);
	const style = /id="svelte-announcer"[^>]*?\sstyle="([^"]+)"/.exec(root)?.[1];
	if (!style) {
		throw new Error(
			'The inline style of SvelteKit’s route announcer was not found where it used to be. ' +
				'The content security policy has to allow it by its hash: see vite.config.ts.'
		);
	}
	return `sha256-${createHash('sha256').update(style).digest('base64')}`;
}

export default defineConfig({
	plugins: [
		sveltekit({
			// Static files only: every page is written out at build time and any
			// web server can serve the result. No Node process runs in production.
			adapter: adapter({ precompress: true }),
			paths: { base },
			// What a page may load, run and talk to. Every page is written out
			// at build time, so the policy travels in the page itself, as a
			// `meta` element, and the one inline script, which starts the app,
			// is allowed by the hash of its text.
			csp: {
				mode: 'hash',
				// SvelteKit types every source by its spelling, and the tile
				// hosts are spelled only once the map style has been read.
				directives: contentSecurityPolicy([announcerStyleHash()]) as Directives
			}
		})
	],
	server: {
		fs: {
			// The map style, the label fonts and the sample flight are shared with
			// the other apps and live elsewhere in the repository.
			allow: ['../..']
		},
		proxy: api
	},
	preview: { proxy: api },
	test: {
		include: ['src/**/*.test.ts']
	}
});
