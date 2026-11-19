/**
 * What the end-to-end tests share: a page with the bundled map behind it,
 * the files a pilot would import, and ways to wait for a map and to keep a
 * screenshot.
 */

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { test as base, expect, type Locator, type Page, type Request } from '@playwright/test';
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
	/** Whether the window is wide enough for the navigation to be at the side. */
	wide: boolean;
	/** What the page wrote to the console as errors. A test checks it is empty. */
	problems: string[];
}

export const test = base.extend<Fixtures>({
	// Every page draws its map from the region in the repository.
	context: async ({ context }, use) => {
		await serveBundledMap(context);
		await use(context);
	},

	wide: async ({ viewport }, use) => {
		// 56rem, where the stylesheet moves the navigation to the side.
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

	mapDrawn: async ({ page }, use) => {
		// The tiles on their way, with the moment each was asked for.
		const fetching = new Map<Request, number>();
		let lastActivity = Date.now();
		const isTile = (request: Request) => request.url().startsWith('https://tiles.');
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
	await expect(page.getByRole('button', { name: 'Wind stations' })).toBeVisible();
	await mapDrawn();
}

/** Adds the sample flight to an empty logbook and waits for its screen. */
export async function addSampleFlight(page: Page): Promise<void> {
	await page.goto('/logbook/');
	await page.getByRole('button', { name: 'Add a sample flight' }).click();
	await expect(page.getByRole('heading', { name: 'Sat, 4 Jul 2026' })).toBeVisible();
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
