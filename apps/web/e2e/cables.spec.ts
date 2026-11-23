/**
 * Cables on the map and the closing circle in the route planner.
 *
 * The development server serves cables recorded from OpenStreetMap for a
 * small box near the Tegernsee: power lines, a gondola, a material
 * ropeway, none with a mapped height, so all are drawn dashed.
 */

import {
	expect,
	mapTabDrawn,
	test,
	mapTool,
	openCredits,
	openLayers,
	closeLayers,
	layerSwitch
} from './helpers';

interface Drawn {
	lines: string[];
	dashed: number;
	onScreen: (id: string) => { x: number; y: number }[] | undefined;
}

test('the map draws the cables near the Tegernsee, and a click on one says what it is', async ({
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	await page.goto('/?lat=47.70&lon=11.76');
	await mapTabDrawn(page, mapDrawn);

	// The cables are drawn, every one dashed, with their credit at the foot.
	await expect
		.poll(() =>
			page.evaluate(() => (globalThis as { ventiCables?: Drawn }).ventiCables?.lines.length ?? 0)
		)
		.toBeGreaterThan(0);
	const drawn = await page.evaluate(() => {
		const cables = (globalThis as { ventiCables?: Drawn }).ventiCables!;
		return { lines: cables.lines.length, dashed: cables.dashed };
	});
	expect(drawn.dashed).toBe(drawn.lines);
	await openCredits(page);
	await expect(page.locator('.credits')).toContainText('Cables: © OpenStreetMap contributors');
	await expect(
		page.getByText(/^Cables on this part of the map: \d+\. Not every cable is mapped/)
	).toBeAttached();
	await mapDrawn();
	await keep('246-map-cables');

	// With the wind's arrows, the sites' signs and the pilots off, nothing
	// else lies under a click on a line: the middle of a stretch of it
	// that is in the open part of the map.
	await page
		.getByRole('group', { name: 'What the map shows' })
		.getByRole('button', { name: 'Wind' })
		.click();
	await openLayers(page);
	for (const layer of ['Sites', 'Pilots']) {
		await layerSwitch(page, layer).uncheck();
	}
	await closeLayers(page);
	await mapDrawn();
	const canvas = page.locator('canvas.maplibregl-canvas');
	const box = (await canvas.boundingBox())!;
	const covered = await page.locator('.navigation-height').evaluate((bar) => bar.clientHeight);
	const target = await page.evaluate(
		({ width, height }) => {
			const cables = (globalThis as { ventiCables?: Drawn }).ventiCables!;
			for (const id of cables.lines) {
				const points = cables.onScreen(id) ?? [];
				for (let index = 1; index < points.length; index += 1) {
					const x = (points[index - 1].x + points[index].x) / 2;
					const y = (points[index - 1].y + points[index].y) / 2;
					const open = x > width * 0.35 && x < width * 0.7 && y > height * 0.2 && y < height * 0.6;
					if (open) return { id, x, y };
				}
			}
			return undefined;
		},
		{ width: box.width, height: box.height - covered }
	);
	expect(target).toBeDefined();
	await canvas.click({ position: { x: target!.x, y: target!.y } });

	// That cable or one that runs beside it: power lines share pylons.
	const cable = page.locator('[data-cable]');
	await expect(cable).toBeVisible();
	const sheet = page.getByRole(wide ? 'complementary' : 'dialog').filter({ has: cable });
	await expect(sheet).toContainText(/height not mapped/);
	await expect(sheet).toContainText('Not every cable is mapped, and few have a height.');
	await expect(sheet.getByRole('link', { name: '© OpenStreetMap contributors' })).toBeVisible();
	// Not the card of the bare place under it.
	await expect(page.getByRole(wide ? 'complementary' : 'dialog', { name: /° N, / })).toHaveCount(0);
	await mapDrawn();
	await keep('247-map-cable');

	expect(problems).toEqual([]);
});

test('three places that come back near the first make a triangle, and the planner says where it closes', async ({
	page,
	mapDrawn,
	keep,
	wide,
	problems
}) => {
	await page.goto('/?lat=47.70&lon=11.76');
	await mapTabDrawn(page, mapDrawn);
	await mapTool(page, 'Plan a route');
	const card = page.getByRole('region', { name: 'Plan a route' });
	const legs = card.getByRole('list', { name: 'Legs' }).getByRole('listitem');

	// Out east, and back to close by where it began: a triangle the core
	// scores, open by a few kilometres.
	const map = (await page.locator('canvas.maplibregl-canvas').boundingBox())!;
	const at = (x: number, y: number) =>
		page.mouse.click(map.x + map.width * x, map.y + map.height * y);
	// A phone's card is one line of the route until it is unfolded.
	const summary = card.locator('[data-route-summary]');
	await at(0.4, 0.2);
	await at(0.7, 0.22);
	await expect(wide ? legs : summary).toHaveText(wide ? [/./] : /^1 leg · /);
	await at(0.42, 0.25);
	if (!wide) {
		await expect(summary).toHaveText(/^2 legs · /);
		await summary.click();
	}
	await expect(legs).toHaveCount(2);

	await expect(card.locator('[data-closing-words]')).toHaveText(
		/^Closes within [\d.]+ km of the start: closed (FAI )?triangle of [\d.]+ km, [\d.]+ points$/
	);
	await expect
		.poll(() =>
			page.evaluate(() => (globalThis as { ventiRoute?: { closing: number } }).ventiRoute?.closing)
		)
		.toBe(73);
	await mapDrawn();
	await keep('248-route-closing-circle');

	expect(problems).toEqual([]);
});
