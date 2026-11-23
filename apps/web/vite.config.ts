import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import type { Plugin } from 'vite';
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
// The connector (`/mcp`) too, which the developers' page asks for its tools.
const api = { '/v1/': server, '/f/': server, '/live/': server, '/mcp': server };

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
	const root = readFileSync(new URL('./src/core/sync/write_root.js', `file://${kit}`), 'utf8');
	const style = /id="svelte-announcer"[^>]*?\sstyle="([^"]+)"/.exec(root)?.[1];
	if (!style) {
		throw new Error(
			'The inline style of SvelteKit’s route announcer was not found where it used to be. ' +
				'The content security policy has to allow it by its hash: see vite.config.ts.'
		);
	}
	return `sha256-${createHash('sha256').update(style).digest('base64')}`;
}

/**
 * The app icon, the favicons and the manifest, served at the root of the
 * site from the owner's design pack, where the repository keeps its one
 * copy (`design/assets/app-icon/web`). They are not copied into the app:
 * the development server answers them from there, and the build writes
 * them into the site. The manifest names its icons relative to itself, so
 * that a site served from a folder (`VENTI_BASE_PATH`) finds them.
 */
function appIcons(): Plugin {
	const folder = new URL('../../design/assets/app-icon/web/', import.meta.url);
	const types: Record<string, string> = {
		'favicon.ico': 'image/x-icon',
		'favicon.svg': 'image/svg+xml',
		'apple-touch-icon.png': 'image/png',
		'icon-192.png': 'image/png',
		'icon-512.png': 'image/png',
		'icon-maskable-512.png': 'image/png',
		'site.webmanifest': 'application/manifest+json'
	};
	const contents = (name: string): Buffer | string => {
		const bytes = readFileSync(new URL(name, folder));
		return name === 'site.webmanifest'
			? bytes.toString('utf8').replaceAll('"/icon-', '"icon-')
			: bytes;
	};
	return {
		name: 'venti-app-icons',
		configureServer(server) {
			server.middlewares.use((request, response, next) => {
				const name = (request.url ?? '').split('?')[0].slice(`${base}/`.length);
				if (!(name in types)) return next();
				response.setHeader('Content-Type', types[name]);
				response.end(contents(name));
			});
		},
		generateBundle() {
			// The site's files are those of the client's build; the server's
			// build of the pages is thrown away.
			if (this.environment.name !== 'client') return;
			for (const fileName of Object.keys(types)) {
				this.emitFile({ type: 'asset', fileName, source: contents(fileName) });
			}
		}
	};
}

export default defineConfig({
	plugins: [
		appIcons(),
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
