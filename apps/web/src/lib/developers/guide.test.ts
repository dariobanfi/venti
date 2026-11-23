import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { guide, linkFrom } from './guide';

const markdown = readFileSync(
	new URL('../../../../../docs/adding-a-source.md', import.meta.url),
	'utf8'
);

describe('the guide', () => {
	it('has every heading of docs/adding-a-source.md but its title', () => {
		const headings = [...markdown.matchAll(/^(#{2,4}) (.+)$/gm)].length;
		expect(guide.filter((block) => block.kind === 'heading')).toHaveLength(headings);
		expect(guide.some((block) => block.kind === 'heading' && block.level === 1)).toBe(false);
	});

	it('keeps every command of it as code', () => {
		const fences = [...markdown.matchAll(/^```/gm)].length / 2;
		expect(guide.filter((block) => block.kind === 'code')).toHaveLength(fences);
		expect(
			guide.some((block) => block.kind === 'code' && block.text.includes('tools/new-source/new.sh'))
		).toBe(true);
	});

	it('leads a link to another file of the repository to GitHub', () => {
		expect(linkFrom('data-sources.md#manifest')).toBe(
			'https://github.com/dariobanfi/venti/blob/main/docs/data-sources.md#manifest'
		);
		expect(linkFrom('../AGENTS.md#commands')).toBe(
			'https://github.com/dariobanfi/venti/blob/main/AGENTS.md#commands'
		);
		expect(linkFrom('#before-any-code')).toBe('#before-any-code');
		expect(linkFrom('https://example.org/')).toBe('https://example.org/');
	});

	it('leads a link to a heading of the guide to one the page has', () => {
		const ids = new Set(guide.flatMap((block) => (block.kind === 'heading' ? [block.id] : [])));
		const inPage = guide
			.flatMap((block) =>
				block.kind === 'paragraph' || block.kind === 'heading'
					? block.line
					: block.kind === 'list'
						? block.items.flat()
						: []
			)
			.flatMap((piece) =>
				piece.kind === 'link' && piece.href.startsWith('#') ? [piece.href] : []
			);
		for (const href of inPage) expect(ids.has(href.slice(1)), href).toBe(true);
	});
});
