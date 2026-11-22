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

test('the flight flies over the terrain in 3D, with its thermals, and the camera follows it', async ({
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
	// In 3D the camera follows the glider from the start.
	const follow = page.getByRole('button', { name: 'Follow the glider' });
	await expect(follow).toHaveAttribute('aria-pressed', 'true');
	await mapDrawn();

	// The layer drew the flight: thousands of vertices, frame after frame.
	await expect.poll(async () => (await stats(page))?.frames ?? 0).toBeGreaterThan(0);
	const drawn = (await stats(page))!;
	expect(drawn.vertices).toBeGreaterThan(10_000);
	// The thermals' words stand in the air beside them, the core's numbers.
	await expect(page.locator('.air .thermal').first()).toContainText(/^\+\d\.\d m\/s · \+\d+ m$/);
	await expect(page.locator('.air .sink')).toContainText(/^−\d+ m on one glide$/);

	// The whole flight is a view of its own: the camera lets go of the glider.
	await page.getByRole('button', { name: 'Whole flight' }).click();
	await expect(follow).toHaveAttribute('aria-pressed', 'false');
	await page.waitForTimeout(800);
	await keep('103-flight-air');

	// "Follow the glider" brings it back; playing the flight moves the marked
	// moment, the layer draws it, and the camera stays behind the glider.
	await follow.click();
	await expect(follow).toHaveAttribute('aria-pressed', 'true');
	const before = (await stats(page))!.frames;
	await page
		.getByRole('button', { name: /Play the flight/ })
		.first()
		.click();
	await expect.poll(async () => (await stats(page))!.frames).toBeGreaterThan(before + 3);
	await expect(page.locator('.air .marker')).toBeVisible();
	await page.waitForTimeout(600);
	await keep('104-flight-air-follow');
	await page.getByRole('button', { name: 'Pause' }).first().click();

	// Dragging the map takes the camera: it lets go of the glider.
	const map = page.locator('canvas.maplibregl-canvas');
	const box = (await map.boundingBox())!;
	await page.mouse.move(box.x + box.width / 2, box.y + box.height / 3);
	await page.mouse.down();
	await page.mouse.move(box.x + box.width / 2 + 80, box.y + box.height / 3 + 30, { steps: 8 });
	await page.mouse.up();
	await expect(follow).toHaveAttribute('aria-pressed', 'false');

	// What a frame costs in this browser, in software.
	console.log(`air layer: ${(await stats(page))!.lastFrameMs.toFixed(2)} ms a frame`);

	// Flat again: the layer is gone and the controls of 3D with it.
	await terrain.click();
	await expect(page.getByRole('button', { name: 'Whole flight' })).toHaveCount(0);
	await expect(page.locator('.air')).toHaveCount(0);
	expect(problems).toEqual([]);
});
