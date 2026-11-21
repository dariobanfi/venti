/**
 * A flight in the air: the flight map's 3D view, with the track at its
 * height over the terrain of the bundled valley, its curtain, its thermals
 * and the marked moment. The test browser draws WebGL in software.
 */

import type { Page } from '@playwright/test';
import { addSampleFlight, expect, test } from './helpers';

interface Stats {
	frames: number;
	lastFrameMs: number;
	vertices: number;
}

/** What the air layer reports of itself: a hook for tests, read from the page. */
const stats = (page: Page) =>
	page.evaluate(
		() => (globalThis as unknown as { ventiAir?: { stats: Stats } }).ventiAir?.stats ?? null
	);

test('the flight flies over the terrain in 3D, with its thermals, and the camera can follow it', async ({
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	await addSampleFlight(page);
	const terrain = page.getByRole('button', { name: 'Terrain in 3D' });
	await terrain.click();
	await expect(terrain).toHaveAttribute('aria-pressed', 'true');
	await mapDrawn();

	// The layer drew the flight: thousands of vertices, frame after frame.
	await expect.poll(async () => (await stats(page))?.frames ?? 0).toBeGreaterThan(0);
	const drawn = (await stats(page))!;
	expect(drawn.vertices).toBeGreaterThan(10_000);
	// The thermals' words stand in the air beside them, the core's numbers.
	await expect(page.locator('.air .thermal').first()).toContainText(/^\+\d\.\d m\/s · \+\d+ m$/);
	await expect(page.locator('.air .sink')).toContainText(/^−\d+ m on one glide$/);

	await page.getByRole('button', { name: 'Whole flight' }).click();
	await page.waitForTimeout(800);
	await keep('103-flight-air');

	// Playing the flight moves the marked moment, and the layer draws it.
	const before = (await stats(page))!.frames;
	await page
		.getByRole('button', { name: /Play the flight/ })
		.first()
		.click();
	await expect.poll(async () => (await stats(page))!.frames).toBeGreaterThan(before + 3);
	await expect(page.locator('.air .marker')).toBeVisible();
	await page.getByRole('button', { name: 'Pause' }).first().click();

	// Follow: the camera goes behind the glider, and lets go when switched off.
	const follow = page.getByRole('button', { name: 'Follow the glider' });
	await follow.click();
	await expect(follow).toHaveAttribute('aria-pressed', 'true');
	const slider = page.getByRole('slider', { name: 'Altitude over time' });
	await slider.focus();
	await page.keyboard.press('PageUp');
	await page.keyboard.press('PageUp');
	await page.waitForTimeout(600);
	await keep('104-flight-air-follow');
	await follow.click();
	await expect(follow).toHaveAttribute('aria-pressed', 'false');

	// What a frame costs in this browser, in software.
	console.log(`air layer: ${(await stats(page))!.lastFrameMs.toFixed(2)} ms a frame`);

	// Flat again: the layer is gone and the controls of 3D with it.
	await terrain.click();
	await expect(page.getByRole('button', { name: 'Whole flight' })).toHaveCount(0);
	await expect(page.locator('.air')).toHaveCount(0);
	expect(problems).toEqual([]);
});
