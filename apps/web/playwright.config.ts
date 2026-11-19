import { defineConfig, type PlaywrightTestConfig } from '@playwright/test';

/**
 * The end-to-end tests use the built site the way a pilot would, in a real
 * browser, and keep a screenshot of every screen: at a phone's width and at
 * a desktop's, each in light and in dark appearance.
 *
 * They run against `vite preview`, which serves the static files of
 * `npm run build` as any web server would. Build first.
 */

type Project = NonNullable<PlaywrightTestConfig['projects']>[number];

// A phone has two pixels to the point. The desktop is drawn one to one:
// its map is five times the size, and drawing that in software at double
// density takes the tests minutes.
const PHONE = {
	viewport: { width: 390, height: 844 },
	deviceScaleFactor: 2,
	hasTouch: true,
	isMobile: true
};
const DESKTOP = { viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 };

function project(name: string, size: typeof PHONE | typeof DESKTOP, dark: boolean): Project {
	return { name, use: { ...size, colorScheme: dark ? 'dark' : 'light' } };
}

export default defineConfig({
	testDir: 'e2e',
	outputDir: 'test-results',
	// A journey through the whole app, with a map drawn in software on every screen.
	timeout: 180_000,
	expect: { timeout: 15_000 },
	forbidOnly: Boolean(process.env.CI),
	retries: process.env.CI ? 1 : 0,
	reporter: process.env.CI ? [['list'], ['github']] : [['list']],
	use: {
		baseURL: 'http://localhost:4173',
		// Fixed, so that dates and times read the same on every machine.
		locale: 'en-GB',
		timezoneId: 'Europe/Berlin',
		trace: 'off',
		launchOptions: {
			// Test machines have no graphics card. The map is drawn in software.
			args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist']
		}
	},
	projects: [
		project('phone-light', PHONE, false),
		project('phone-dark', PHONE, true),
		project('desktop-light', DESKTOP, false),
		project('desktop-dark', DESKTOP, true)
	],
	webServer: {
		command: 'npm run preview -- --port 4173 --strictPort',
		url: 'http://localhost:4173',
		// A server left running from before would serve the build it started with.
		reuseExistingServer: false
	}
});
