import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vitest/config';

// Where the app is served from when it is not at the root of its host, for
// example "/venti". Empty at the root.
const base = (process.env.VENTI_BASE_PATH ?? '') as '' | `/${string}`;

// The app asks the origin it is served from for the Venti server's API, as
// it will when one web server hands out both. While developing, that is this
// machine's `cargo run -p venti-server -- dev`.
const api = { '/v1': process.env.VENTI_DEV_SERVER ?? 'http://127.0.0.1:8080' };

export default defineConfig({
	plugins: [
		sveltekit({
			// Static files only: every page is written out at build time and any
			// web server can serve the result. No Node process runs in production.
			adapter: adapter({ precompress: true }),
			paths: { base }
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
