/**
 * The route planner of the map tab: places clicked on the map, in order,
 * and what the core says the route comes to, kept in the browser until it
 * is cleared.
 */

import { expect, mapTabDrawn, test } from './helpers';

test('a route is drawn on the map, scored by the core and kept', async ({
	page,
	mapDrawn,
	keep,
	problems
}) => {
	await page.goto('/');
	await mapTabDrawn(page, mapDrawn);

	const planning = page.getByRole('button', { name: 'Plan a route' });
	await expect(planning).toHaveAttribute('aria-pressed', 'false');
	await planning.click();
	await expect(planning).toHaveAttribute('aria-pressed', 'true');

	const card = page.getByRole('region', { name: 'Plan a route' });
	const words = card.locator('[data-route-words]');
	const legs = card.getByRole('list', { name: 'Legs' }).getByRole('listitem');
	await expect(words).toHaveText('Tap the map where the route begins.');

	// Three places in the upper part of the map, clear of the card at its
	// side or its foot and of the round buttons: a triangle.
	const map = (await page.locator('canvas.maplibregl-canvas').boundingBox())!;
	const at = (x: number, y: number) =>
		page.mouse.click(map.x + map.width * x, map.y + map.height * y);
	await at(0.4, 0.2);
	await expect(words).toHaveText('Tap the next place to fly to.');
	await at(0.7, 0.22);
	await expect(legs).toHaveCount(1);
	await expect(words).toContainText('A third place inside a dashed area makes an FAI triangle');
	await at(0.55, 0.38);
	await expect(legs).toHaveCount(2);
	await expect(words).toContainText('km along the route. Flown like this it scores as');
	await expect(card.getByRole('heading', { level: 3 })).toBeVisible();
	// The weather along it: the core's line for the whole route, at the
	// planner's 20 km/h, and a line for each place and leg.
	await expect(card.locator('[data-route-weather]')).toContainText('20 km/h');
	await expect(
		card.getByRole('list', { name: 'Along the route' }).getByRole('listitem')
	).toHaveCount(5);
	await expect
		.poll(() => page.evaluate(() => (globalThis as { ventiRoute?: { places: number } }).ventiRoute))
		.toMatchObject({ places: 3 });
	await mapDrawn();
	await keep('213-route-three-places');

	// Back to the start: the core finds the triangle, closed.
	await card.getByRole('button', { name: 'Back to the start' }).click();
	await expect(legs).toHaveCount(3);
	await expect(card.getByRole('heading', { level: 3 })).toHaveText(/^Closed (FAI )?triangle$/);
	await expect(card.getByText('Points', { exact: true })).toBeVisible();
	await keep('214-route-triangle');

	// Undo takes the last place away again.
	await card.getByRole('button', { name: 'Undo' }).click();
	await expect(legs).toHaveCount(2);

	// A reload shows the route as it was left.
	await page.reload();
	await mapTabDrawn(page, mapDrawn);
	await expect(page.getByRole('button', { name: 'Plan a route' })).toHaveAttribute(
		'aria-pressed',
		'true'
	);
	await expect(legs).toHaveCount(2);
	await expect(words).toContainText('Flown like this it scores as');
	await expect
		.poll(() => page.evaluate(() => (globalThis as { ventiRoute?: { places: number } }).ventiRoute))
		.toMatchObject({ places: 3 });

	// Cleared, it is gone.
	await card.getByRole('button', { name: 'Clear' }).click();
	await expect(words).toHaveText('Tap the map where the route begins.');
	await expect(legs).toHaveCount(0);

	expect(problems).toEqual([]);
});
