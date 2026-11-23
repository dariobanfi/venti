import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { anchorOf } from '#lib/news/markdown.ts';
import { description, groupsOf, schemasOf, scopesOf } from './reference';
import { ADDRESS_BUDGET, TOKEN_BUDGET } from './words';

/** The description as the repository has it, read by hand, not through the build. */
const document = JSON.parse(
	readFileSync(new URL('../../../../../docs/openapi.json', import.meta.url), 'utf8')
);
const apiDocs = readFileSync(new URL('../../../../../docs/api.md', import.meta.url), 'utf8');

describe('the reference', () => {
	it('is of the description the repository has', () => {
		expect(description).toEqual(document);
	});

	it('has every operation of the description, once', () => {
		const described = Object.entries(document.paths as Record<string, object>).flatMap(
			([path, item]) =>
				Object.keys(item)
					.filter((method) => ['get', 'post', 'put', 'patch', 'delete'].includes(method))
					.map((method) => `${method.toUpperCase()} ${path}`)
		);
		const shown = groupsOf(description).flatMap((group) =>
			group.operations.map((operation) => `${operation.method} ${operation.path}`)
		);
		expect(described.length).toBeGreaterThan(100);
		expect([...shown].sort()).toEqual([...described].sort());
	});

	it('gives every operation an anchor of its own and a group with a name', () => {
		const groups = groupsOf(description);
		const ids = groups.flatMap((group) => group.operations.map((operation) => operation.id));
		expect(new Set(ids).size).toBe(ids.length);
		for (const group of groups) expect(group.name).not.toBe('');
	});

	it('links every operation to the heading of api.md that says it in words', () => {
		const anchors = new Set(
			[...apiDocs.matchAll(/^#{2,4} (.+)$/gm)].map((found) =>
				anchorOf(found[1].replaceAll('`', ''))
			)
		);
		for (const group of groupsOf(description)) {
			for (const operation of group.operations) {
				expect(operation.docs, operation.id).toBeDefined();
				const anchor = operation.docs?.slice(operation.docs.indexOf('#') + 1) ?? '';
				expect(anchors.has(anchor), `${operation.id}: ${anchor}`).toBe(true);
			}
		}
	});

	it('reads a shared answer where it is written', () => {
		const sites = groupsOf(description)
			.flatMap((group) => group.operations)
			.find((operation) => operation.id === 'post_sites');
		const unauthorized = sites?.responses.find((response) => response.status === '401');
		expect(unauthorized?.description).toMatch(/unauthorized/);
		expect(unauthorized?.bodies[0]?.schema?.$ref).toBe('#/components/schemas/ErrorResponse');
	});

	it('lists every schema and every scope', () => {
		expect(schemasOf(description)).toHaveLength(Object.keys(document.components.schemas).length);
		expect(scopesOf(description)).toContain('flights:read');
	});
});

describe('the start page’s numbers', () => {
	// The start page says the bounds in a few words of its own; api.md is
	// where they are kept, and these hold the two together.
	it('are the numbers api.md gives', () => {
		expect(apiDocs).toContain(TOKEN_BUDGET);
		expect(apiDocs).toContain(ADDRESS_BUDGET);
	});
});
