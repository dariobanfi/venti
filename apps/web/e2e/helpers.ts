/**
 * What the end-to-end tests share: a page with the bundled map behind it,
 * the files a pilot would import, and ways to wait for a map and to keep a
 * screenshot.
 */

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import {
	test as base,
	expect,
	type BrowserContext,
	type Locator,
	type Page,
	type Request
} from '@playwright/test';
import { serveBundledMap } from './bundled-map';

/** The made-up sample flight, the one copy every app shares. */
export const SAMPLE_FLIGHT = fileURLToPath(
	new URL('../../ios/Venti/Resources/sample.igc', import.meta.url)
);

/** The sample flight on another day of July 2026, which makes it another flight. */
export function sampleFlightOn(day: number): string {
	return readFileSync(SAMPLE_FLIGHT, 'utf8').replace(
		'040726',
		`${String(day).padStart(2, '0')}0726`
	);
}

/** A place in the valley of the sample flight, for the pilot to stand at. */
export const IN_THE_VALLEY = { latitude: 47.682, longitude: 11.765 };

interface Fixtures {
	/** Waits until the map on screen has fetched its tiles and drawn them. */
	mapDrawn: () => Promise<void>;
	/** Keeps a screenshot of the screen under `name`, in this project's folder. */
	keep: (name: string) => Promise<void>;
	/** Whether the window is wide enough for the navigation to be a bar across the top. */
	wide: boolean;
	/** What the page wrote to the console as errors. A test checks it is empty. */
	problems: string[];
	/**
	 * What the browser refused because the site's content security policy
	 * does not allow it. Every test fails if this is not empty.
	 */
	refused: string[];
}

/**
 * Opens the account screen with its handle and password form in this
 * context, as for a pilot who opened it last time. A second browser a test
 * makes (`browser.newContext()`) needs it too, to sign in with a password.
 */
export const withPasswordForm = (context: BrowserContext) =>
	context.addInitScript(() => localStorage.setItem('venti.account-with-password', '1'));

export const test = base.extend<Fixtures>({
	// Every page draws its map from the region in the repository. The
	// account screen opens with its handle and password form, as for a
	// pilot who opened it last time: the tests sign in with a password.
	// `google.spec.ts` closes it again.
	context: async ({ context }, use) => {
		await serveBundledMap(context);
		await withPasswordForm(context);
		await use(context);
		// A routed request may still be on its way when the test ends: let go
		// of every route without waiting for it, so that its handler's
		// "Test ended" is not the test's failure.
		await context.unrouteAll({ behavior: 'ignoreErrors' });
	},

	// The same for the routes a test sets on its page.
	page: async ({ page }, use) => {
		await use(page);
		await page.unrouteAll({ behavior: 'ignoreErrors' });
	},

	wide: async ({ viewport }, use) => {
		// 56rem, where the stylesheet moves the navigation to the top.
		await use((viewport?.width ?? 0) >= 896);
	},

	problems: async ({ page }, use) => {
		const problems: string[] = [];
		page.on('pageerror', (error) => problems.push(error.message));
		page.on('console', (message) => {
			// A request that failed is named with its address, to be found again.
			const address = message.location().url;
			if (message.type() === 'error') {
				problems.push(address ? `${message.text()} (${address})` : message.text());
			}
		});
		await use(problems);
	},

	// The policy lists what the app may load, run and talk to. Every screen
	// the tests open keeps that list true: whatever the browser refuses, in
	// a page or in a worker, fails the test that came across it.
	refused: [
		async ({ context, page }, use) => {
			const refused: string[] = [];
			const note = (message: { text(): string }) => {
				if (/content security policy/i.test(message.text())) refused.push(message.text());
			};
			page.on('console', note);
			page.on('worker', (worker) => worker.on('console', note));
			// The browser's own message does not always say what was asked for.
			await context.addInitScript(() => {
				document.addEventListener('securitypolicyviolation', (event) => {
					console.error(
						`Content security policy: ${event.effectiveDirective} refused ${event.blockedURI}` +
							(event.sourceFile ? ` (${event.sourceFile}:${event.lineNumber})` : '')
					);
				});
			});
			await use(refused);
			expect([...new Set(refused)]).toEqual([]);
		},
		{ auto: true }
	],

	mapDrawn: async ({ page }, use) => {
		// The tiles on their way, with the moment each was asked for.
		const fetching = new Map<Request, number>();
		let lastActivity = Date.now();
		const isTile = (request: Request) => /^https:\/\/(tiles|wmtsod\d)\./.test(request.url());
		page.on('request', (request) => {
			if (!isTile(request)) return;
			fetching.set(request, Date.now());
			lastActivity = Date.now();
		});
		const finished = (request: Request) => {
			if (fetching.delete(request)) lastActivity = Date.now();
		};
		page.on('requestfinished', finished);
		page.on('requestfailed', finished);

		/** A map that was closed half-way never hears back about its tiles. Stop waiting for those. */
		const stillFetching = () => {
			for (const [request, asked] of fetching) {
				if (Date.now() - asked > 10_000) fetching.delete(request);
			}
			return fetching.size > 0;
		};

		await use(async () => {
			await expect(page.locator('canvas.maplibregl-canvas')).toBeVisible();
			// No tile on its way and none asked for in a while: the map has
			// what it wants. Then give it a moment to fade the last ones in,
			// which is slow when the map is drawn in software.
			await expect
				.poll(() => !stillFetching() && Date.now() - lastActivity > 800, { timeout: 45_000 })
				.toBe(true);
			await page.waitForTimeout(900);
		});
	},

	keep: async ({ page }, use, info) => {
		await use(async (name) => {
			await page.screenshot({
				path: `screenshots/${info.project.name}/${name}.png`,
				// A menu or a sheet is kept as it looks once it has opened, not
				// half-way through unfolding.
				animations: 'disabled'
			});
		});
	}
});

export { expect };

/**
 * The value shown for a number, found by what it measures, within one part
 * of the screen: the same caption can stand in the panel and in the details.
 */
export function metric(within: Page | Locator, caption: string): Locator {
	return within
		.locator('dt', { hasText: new RegExp(`^${caption}$`) })
		.locator('xpath=following-sibling::dd');
}

/** The flight panel's four numbers, for the whole flight or for the marked moment. */
export function panel(page: Page) {
	return {
		totals: page.getByLabel('The whole flight'),
		moment: page.getByLabel('At the marked moment')
	};
}

/**
 * Waits until a flight's altitude profile is drawn over the ground it was
 * flown over. The elevation tiles under a flight are fetched when it
 * opens, so the ground comes a moment after the profile; the chart says
 * in words when it has it.
 */
export async function groundDrawn(page: Page): Promise<void> {
	await expect(page.getByRole('slider', { name: 'Altitude over time' })).toHaveAttribute(
		'aria-valuetext',
		/over ground from [\d,]+ to [\d,]+ metres/
	);
}

/** Everything else known about the flight: in a sheet on a phone, on screen on a desktop. */
export function flightDetails(page: Page): Locator {
	return page.getByRole('group', { name: 'Flight details' });
}

/**
 * Waits for the map tab to be complete: its map drawn, with the app's own
 * layers on it and the wind for what it shows. Map tiles alone can be there
 * a moment before the flights and the arrows; the wind button appears with
 * the first answer of the wind server, which comes after both.
 */
export async function mapTabDrawn(page: Page, mapDrawn: () => Promise<void>): Promise<void> {
	await expect(page.locator('.screen[data-wind]')).toBeVisible();
	await mapDrawn();
}

/**
 * The one sheet of the map's layers and tools: a sheet at a phone's width,
 * a card at the map's side on a wide screen.
 */
export function layersSheet(page: Page): Locator {
	return page
		.getByRole('dialog', { name: 'Layers', exact: true })
		.or(page.getByRole('complementary', { name: 'Layers', exact: true }));
}

/** Opens the layers' sheet with the map's "Layers" button, where it is not open. */
export async function openLayers(page: Page): Promise<Locator> {
	const sheet = layersSheet(page);
	if (!(await sheet.isVisible())) {
		await page.getByRole('button', { name: 'Layers', exact: true }).click();
	}
	await expect(sheet).toBeVisible();
	return sheet;
}

/** Closes the layers' sheet, where it is open. */
export async function closeLayers(page: Page): Promise<void> {
	const sheet = layersSheet(page);
	if (await sheet.isVisible()) await sheet.getByRole('button', { name: /^(Done|Close)$/ }).click();
	await expect(sheet).toBeHidden();
}

/** The switch of a layer in the layers' sheet, which has to be open. */
export function layerSwitch(page: Page, name: string): Locator {
	return layersSheet(page).getByRole('switch', { name, exact: true });
}

/** Switches a layer on or off in the layers' sheet, and closes the sheet again. */
export async function switchLayer(page: Page, name: string, on: boolean): Promise<void> {
	await openLayers(page);
	await layerSwitch(page, name).setChecked(on);
	await closeLayers(page);
}

/** Switches several layers on or off in the layers' sheet, and closes the sheet again. */
export async function switchLayers(page: Page, names: string[], on: boolean): Promise<void> {
	await openLayers(page);
	for (const name of names) await layerSwitch(page, name).setChecked(on);
	await closeLayers(page);
}

/** A tool of the map, a row of the layers' sheet, which has to be open. */
export function toolRow(page: Page, name: string | RegExp): Locator {
	const tools = layersSheet(page).getByRole('group', { name: 'Lists and plans' });
	return tools.getByRole('button', { name, exact: true }).or(tools.getByRole('link', { name }));
}

/** Opens a tool of the map from the layers' sheet: "Wind stations", "Flying sites", … */
export async function mapTool(page: Page, name: string): Promise<void> {
	await openLayers(page);
	await toolRow(page, name).click();
}

/**
 * The sites' filter, which is inside the list of the sites: opens the list
 * from the layers' sheet and unfolds the filter's choices in it. The list
 * is what the choices are looked for in; "Done" closes it.
 */
export async function openSitesFilter(page: Page): Promise<Locator> {
	await mapTool(page, 'Flying sites');
	const list = page.getByRole('dialog', { name: 'Flying sites' });
	const fold = list.locator('button.fold');
	if ((await fold.getAttribute('aria-expanded')) !== 'true') await fold.click();
	await expect(fold).toHaveAttribute('aria-expanded', 'true');
	return list;
}

/**
 * Unfolds the credits with the map's "i": the map's own and Venti's, which
 * are closed from the start.
 */
export async function openCredits(page: Page): Promise<void> {
	const control = page.locator('.maplibregl-ctrl-attrib').first();
	const state = () =>
		control.evaluate((element) => ({
			folds: element.classList.contains('maplibregl-compact'),
			open: element.classList.contains('maplibregl-compact-show')
		}));
	// The control folds once the map's sources have said who made them.
	await expect(control).not.toHaveClass(/maplibregl-attrib-empty/);
	// A map that writes its credits out, as a flight's does on a wide
	// screen, has nothing to unfold.
	const now = await state();
	if (!now.folds || now.open) return;
	await page.getByLabel('Credits', { exact: true }).first().click();
	await expect.poll(async () => (await state()).open).toBe(true);
}

/**
 * Between the Wallberg and the Spitzingsee: two ramps on the one, a
 * take-off and two landing fields at the other, all within an hour's walk.
 */
export const BETWEEN_THE_MOUNTAINS = { latitude: 47.679, longitude: 11.85 };

/**
 * Goes to where the pilot stands, from the map of the northern Alps the
 * app starts with: presses the position button and waits until the map has
 * arrived there and drawn the place.
 *
 * Waiting for the map's tiles alone is not enough. On a busy machine the
 * position arrives a moment after the click, and until then no tile is on
 * its way: a test that went on would press its keys or click the map's
 * middle while the map was still where it started. The button zooms in on
 * a hillside, which shows in what the map asks for: tiles of zoom 12.
 */
export async function showMyPosition(page: Page, mapDrawn: () => Promise<void>): Promise<void> {
	const arriving = page.waitForRequest(
		(request) =>
			/^https:\/\/tiles\.openfreemap\.org\/.*\/1[2-9]\/\d+\/\d+\.pbf$/.test(request.url()),
		{ timeout: 30_000 }
	);
	await page.getByRole('button', { name: 'Show my position' }).click();
	await arriving;
	await mapDrawn();
}

/**
 * Goes to where the pilot stands and then further out, with the keyboard:
 * the position button zooms in to a hillside, and sites are spread over a
 * valley.
 */
export async function showTheValley(
	page: Page,
	mapDrawn: () => Promise<void>,
	steps: number
): Promise<void> {
	await showMyPosition(page, mapDrawn);
	const map = page.locator('canvas.maplibregl-canvas');
	await map.focus();
	for (let step = 0; step < steps; step += 1) {
		await page.keyboard.press('Minus');
		await mapDrawn();
	}
	// The screenshots show the map, not the ring of the keyboard's focus.
	await map.blur();
}

/**
 * Adds the sample flight to an empty logbook and waits for its screen,
 * with the ground under its profile: the test map has the valley it was
 * flown in.
 */
export async function addSampleFlight(page: Page): Promise<void> {
	await page.goto('/logbook/');
	await page.getByRole('button', { name: 'Add a sample flight' }).click();
	await expect(page.getByRole('heading', { name: 'Sat, 4 Jul 2026' })).toBeVisible();
	await groundDrawn(page);
}

/**
 * Drags files onto the page and drops them, as from a file manager.
 * `says` is what the page says a drop will do while the files hover.
 */
export async function dropFiles(
	page: Page,
	files: { name: string; text: string }[],
	says = 'Drop flight files to import them'
): Promise<void> {
	const transfer = await page.evaluateHandle((files) => {
		const transfer = new DataTransfer();
		for (const file of files) transfer.items.add(new File([file.text], file.name));
		return transfer;
	}, files);
	await page.dispatchEvent('body', 'dragenter', { dataTransfer: transfer });
	await page.dispatchEvent('body', 'dragover', { dataTransfer: transfer });
	await expect(page.getByText(says)).toBeVisible();
	await page.dispatchEvent('body', 'drop', { dataTransfer: transfer });
	await expect(page.getByText(says)).toBeHidden();
}

/**
 * The rows of a list of flights or pilots: a table's on a wide screen,
 * a list's on a phone.
 */
export function rowsOf(within: Page | Locator, name: string | RegExp): Locator {
	return within
		.getByRole('list', { name })
		.locator('> li')
		.or(within.getByRole('table', { name }).locator('tbody > tr'));
}
