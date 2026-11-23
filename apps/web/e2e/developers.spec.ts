/**
 * The pages for developers: reached from the site's foot and the about
 * page, the REST API's reference written out from the server's own
 * description, the connector's tools as the development server lists them,
 * the sources' licences from their manifests with the server's downloads,
 * and the guide to adding a source. None is wider than the screen.
 */

import { readFileSync } from 'node:fs';
import type { Page } from '@playwright/test';
import { expect, test } from './helpers';

/** The operations of the description the site was built from. */
function describedOperations(): number {
	const document = JSON.parse(
		readFileSync(new URL('../../../docs/openapi.json', import.meta.url), 'utf8')
	) as { paths: Record<string, Record<string, unknown>> };
	return Object.values(document.paths)
		.flatMap((item) => Object.keys(item))
		.filter((method) => ['get', 'post', 'put', 'patch', 'delete'].includes(method)).length;
}

/** Whether anything on the page is drawn wider than the window, which would scroll the page sideways. */
async function sideways(page: Page): Promise<boolean> {
	return page.evaluate(() => document.documentElement.scrollWidth > innerWidth + 1);
}

test('the start page is reached from the foot of the home page and from the about page', async ({
	page,
	keep,
	problems
}) => {
	await page.goto('/');
	await page
		.getByRole('navigation', { name: 'About Venti' })
		.getByRole('link', { name: 'Developers' })
		.click();
	await expect(page).toHaveTitle('Developers · Venti');
	await expect(page.getByRole('heading', { level: 1, name: 'Venti for developers' })).toBeVisible();
	// The address the site is served from, with the version.
	await expect(page.getByTestId('base-address')).toHaveText('http://localhost:4173/v1');
	await expect(page.getByText('3,600 requests an hour')).toBeVisible();
	// Not in the navigation: the app's sections are for pilots.
	await expect(
		page.getByRole('navigation', { name: 'Developers' }).getByRole('link', { name: 'REST API' })
	).toBeVisible();
	expect(await sideways(page)).toBe(false);
	await keep('207-developers');

	await page.goto('/about/');
	await page.getByRole('link', { name: 'For developers' }).click();
	await expect(page).toHaveTitle('Developers · Venti');
	expect(problems).toEqual([]);
});

test('the REST API has every operation of the description, each with its parameters and answers', async ({
	page,
	keep,
	problems
}) => {
	await page.goto('/developers/');
	await page
		.getByRole('navigation', { name: 'Developers' })
		.getByRole('link', { name: 'REST API' })
		.click();
	await expect(page).toHaveTitle('REST API · Developers · Venti');
	await expect(page.locator('[data-operation]')).toHaveCount(describedOperations());
	await expect(page.getByRole('heading', { level: 2, name: 'Sites' })).toBeVisible();
	await expect(page.getByText('flights:read', { exact: true }).first()).toBeVisible();
	expect(await sideways(page)).toBe(false);
	await keep('208-developers-api');

	// One unfolded: its parameters, its answer's schema, the words in api.md.
	const sites = page.locator('[data-operation="GET /v1/sites"]');
	await sites.locator('summary').click();
	await expect(sites.getByText('bbox', { exact: true })).toBeVisible();
	await expect(sites.getByRole('link', { name: 'In words, in api.md' })).toHaveAttribute(
		'href',
		'https://github.com/dariobanfi/venti/blob/main/docs/api.md#get-v1sitesbboxwestsoutheastnorth'
	);
	await sites.scrollIntoViewIfNeeded();
	expect(await sideways(page)).toBe(false);
	await keep('209-developers-api-operation');

	await sites.getByRole('link', { name: 'SitesResponse' }).click();
	await expect(page).toHaveTitle('Schemas · REST API · Developers · Venti');
	const schema = page.locator('#schema-SitesResponse');
	await expect(schema).toBeInViewport();
	await expect(schema.getByText('attributions', { exact: true })).toBeVisible();
	expect(await sideways(page)).toBe(false);
	await keep('210-developers-schemas');

	// An address that names an operation opens it unfolded.
	await page.goto('/developers/api/#post_sites');
	await expect(page.locator('#post_sites')).toHaveAttribute('open', '');
	expect(problems).toEqual([]);
});

test('the connector lists its tools as the server offers them, with their arguments', async ({
	page,
	keep,
	problems
}) => {
	await page.goto('/developers/connector/');
	await expect(page).toHaveTitle('Connector · Developers · Venti');
	await expect(page.getByTestId('connector-address')).toHaveText('http://localhost:4173/mcp');
	await expect(page.getByText('flights:read', { exact: true })).toBeVisible();
	const tool = page.locator('[data-tool="sites_in_box"]');
	await expect(tool).toBeVisible();
	await expect(page.locator('[data-tool="wind_stations"]')).toBeVisible();
	await expect(tool.getByText('west', { exact: true })).toBeVisible();
	await expect(tool.getByText('Only reads')).toBeVisible();
	expect(await sideways(page)).toBe(false);
	await keep('211-developers-connector');
	expect(problems).toEqual([]);
});

test('the connector says so when it cannot be asked, and asks again', async ({
	page,
	problems
}) => {
	// Made up: the connector refuses once, then answers as it does.
	let refusals = 1;
	await page.route('**/mcp', (route) =>
		refusals-- > 0
			? route.fulfill({ status: 503, contentType: 'application/json', body: '{}' })
			: route.fallback()
	);
	await page.goto('/developers/connector/');
	await expect(page.getByRole('alert')).toContainText('could not be asked for its tools');
	await page.getByRole('button', { name: 'Try again' }).click();
	await expect(page.locator('[data-tool="sites_in_box"]')).toBeVisible();
	expect(problems.filter((problem) => !/503/.test(problem))).toEqual([]);
});

test('data and licences: each source with its credit, and Venti’s own sites to download', async ({
	page,
	keep,
	problems
}) => {
	await page.goto('/developers/data/');
	await expect(page).toHaveTitle('Data and licences · Developers · Venti');
	const geosphere = page.locator('[data-source="geosphere-tawes"]');
	await expect(geosphere.getByRole('link', { name: 'CC BY 4.0' })).toBeVisible();
	await expect(geosphere.getByText(/Data source: GeoSphere Austria/)).toBeVisible();
	const venti = page.locator('[data-layer="venti"]');
	await expect(venti.getByRole('link', { name: 'CC0 1.0' })).toBeVisible();
	await expect(venti.getByRole('link', { name: 'geojson' })).toHaveAttribute(
		'href',
		'/v1/dumps/sites/venti.geojson'
	);
	expect(await sideways(page)).toBe(false);
	await keep('212-developers-data');
	expect(problems).toEqual([]);
});

test('adding a data source is the guide of the repository, with the template', async ({
	page,
	keep,
	problems
}) => {
	await page.goto('/developers/add-a-source/');
	await expect(page).toHaveTitle('Add a data source · Developers · Venti');
	await expect(page.getByRole('link', { name: 'The template' })).toHaveAttribute(
		'href',
		'https://github.com/dariobanfi/venti/tree/main/tools/new-source/template'
	);
	await expect(
		page.getByRole('heading', { level: 2, name: 'A network of stations, step by step' })
	).toBeVisible();
	await expect(page.getByRole('heading', { level: 3, name: '2. The manifest' })).toBeVisible();
	await expect(
		page.locator('pre').filter({ hasText: 'tools/new-source/new.sh weirdland' })
	).toHaveCount(1);
	await expect(page.getByRole('link', { name: 'data-sources.md' }).first()).toHaveAttribute(
		'href',
		/^https:\/\/github\.com\/dariobanfi\/venti\/blob\/main\/docs\/data-sources\.md/
	);
	expect(await sideways(page)).toBe(false);
	await keep('213-developers-add-a-source');
	expect(problems).toEqual([]);
});
