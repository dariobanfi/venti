/**
 * A flight in the air: the flight map's 3D view, with the track at its
 * height over the terrain of the bundled valley, its curtain, its thermals
 * and the marked moment. The test browser draws WebGL in software.
 */

import type { Page } from '@playwright/test';
import { addSampleFlight, expect, metric, test } from './helpers';

interface Stats {
	frames: number;
	lastFrameMs: number;
	vertices: number;
	glider?: { headingDeg: number; bankDeg: number };
}

/** What the air layer reports of itself: a hook for tests, read from the page. */
const stats = (page: Page) =>
	page.evaluate(
		() => (globalThis as unknown as { ventiAir?: { stats: Stats } }).ventiAir?.stats ?? null
	);

test('the flight flies over the terrain in 3D, with its thermals, and the camera chases the glider', async ({
	page,
	mapDrawn,
	keep,
	problems
}) => {
	await addSampleFlight(page);
	const terrain = page.getByRole('button', { name: 'Terrain in 3D' });
	// The button says what it does in two letters.
	await expect(terrain).toHaveText('3D');
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

	// Playing the flight brings the camera behind the glider and follows it;
	// the glider is drawn at the marked moment, pointed along the track.
	const before = (await stats(page))!.frames;
	await page
		.getByRole('button', { name: /Play the flight/ })
		.first()
		.click();
	await expect(follow).toHaveAttribute('aria-pressed', 'true');
	await expect.poll(async () => (await stats(page))!.frames).toBeGreaterThan(before + 3);
	await expect(page.locator('.air .marker')).toBeVisible();
	await expect.poll(async () => (await stats(page))?.glider?.headingDeg).toBeDefined();
	const glider = (await stats(page))!.glider!;
	expect(Math.abs(glider.bankDeg)).toBeLessThanOrEqual(55);
	// The camera comes close, leans back behind the glider and looks along its course.
	const chase = () =>
		page.evaluate(
			() =>
				(
					globalThis as unknown as {
						ventiChase?: { bearing: number; course: number; pitch: number; zoom: number };
					}
				).ventiChase
		);
	await expect
		.poll(async () => (await chase())?.zoom ?? 0, { timeout: 10_000 })
		.toBeGreaterThan(14);
	const camera = (await chase())!;
	expect(camera.pitch).toBeGreaterThan(60);

	// The replay's speed is the pilot's to see and change: 30× first, 60× chosen.
	const speeds = page.getByRole('radiogroup', { name: 'Replay speed' });
	await expect(speeds.getByRole('radio')).toHaveCount(4);
	await expect(speeds.getByRole('radio', { name: '30×' })).toBeChecked();
	await speeds.getByRole('radio', { name: '60×' }).check();
	await expect(speeds.getByRole('radio', { name: '60×' })).toBeChecked();
	// At 60× a minute of the flight goes by in a second.
	const time = metric(page.getByLabel('At the marked moment'), 'Time');
	const then = await time.textContent();
	await expect.poll(() => time.textContent(), { timeout: 5000 }).not.toBe(then);
	// Back to the flight's own time, so that it still plays when paused: at
	// 60× in a browser drawing in software it is over within the test's waits.
	await speeds.getByRole('radio', { name: '1×' }).check();
	await page.waitForTimeout(800);
	await keep('104-flight-air-follow');
	await page.getByRole('button', { name: 'Pause' }).first().click();

	// "Follow the glider" turns the chase off and on.
	await follow.click();
	await expect(follow).toHaveAttribute('aria-pressed', 'false');
	await follow.click();
	await expect(follow).toHaveAttribute('aria-pressed', 'true');

	// Dragging the map takes the camera: it lets go of the glider.
	const map = page.locator('canvas.maplibregl-canvas');
	const box = (await map.boundingBox())!;
	// High on the map: on a phone the panel, and the credits over it, cover its lower half.
	await page.mouse.move(box.x + box.width / 2, box.y + box.height / 4);
	await page.mouse.down();
	await page.mouse.move(box.x + box.width / 2 + 80, box.y + box.height / 4 + 30, { steps: 8 });
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
