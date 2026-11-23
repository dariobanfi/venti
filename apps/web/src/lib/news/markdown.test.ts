import { describe, expect, it } from 'vitest';
import { anchorOf, inline, parse } from './markdown';
import { posts, read } from './posts';

describe('inline', () => {
	it('reads links, strong and emphasised words, and leaves the rest as text', () => {
		expect(inline('See [the code](https://example.org), **free** and *open*.')).toEqual([
			{ kind: 'text', text: 'See ' },
			{ kind: 'link', text: 'the code', href: 'https://example.org' },
			{ kind: 'text', text: ', ' },
			{ kind: 'strong', text: 'free' },
			{ kind: 'text', text: ' and ' },
			{ kind: 'em', text: 'open' },
			{ kind: 'text', text: '.' }
		]);
	});

	it('takes markup as words: nothing is read as HTML', () => {
		expect(inline('<img src=x onerror=alert(1)>')).toEqual([
			{ kind: 'text', text: '<img src=x onerror=alert(1)>' }
		]);
	});
});

describe('parse', () => {
	it('reads the head, paragraphs, headings and lists', () => {
		const { head, blocks } = parse(
			'---\ntitle: Hello: world\ndraft: true\n---\n\nOne line\nand its second.\n\n## A heading\n\n- first\n- second\n'
		);
		expect(head).toEqual({ title: 'Hello: world', draft: 'true' });
		expect(blocks).toEqual([
			{ kind: 'paragraph', line: [{ kind: 'text', text: 'One line and its second.' }] },
			{
				kind: 'heading',
				level: 2,
				id: 'a-heading',
				line: [{ kind: 'text', text: 'A heading' }]
			},
			{
				kind: 'list',
				items: [[{ kind: 'text', text: 'first' }], [{ kind: 'text', text: 'second' }]]
			}
		]);
	});
});

describe('documents', () => {
	it('reads code, fenced code with its blank lines, tables and deeper headings', () => {
		const { blocks } = parse(
			'### 1. The `skeleton`\n\nRun `new.sh`:\n\n```toml\n# a comment\n\nkey = 1\n```\n\n| Kind | Copy |\n|---|---|\n| Airspace | `openaip` |\n'
		);
		expect(blocks).toEqual([
			{
				kind: 'heading',
				level: 3,
				id: '1-the-skeleton',
				line: [
					{ kind: 'text', text: '1. The ' },
					{ kind: 'code', text: 'skeleton' }
				]
			},
			{
				kind: 'paragraph',
				line: [
					{ kind: 'text', text: 'Run ' },
					{ kind: 'code', text: 'new.sh' },
					{ kind: 'text', text: ':' }
				]
			},
			{ kind: 'code', language: 'toml', text: '# a comment\n\nkey = 1' },
			{
				kind: 'table',
				head: [[{ kind: 'text', text: 'Kind' }], [{ kind: 'text', text: 'Copy' }]],
				rows: [[[{ kind: 'text', text: 'Airspace' }], [{ kind: 'code', text: 'openaip' }]]]
			}
		]);
	});

	it('gives a heading the anchor GitHub gives it', () => {
		expect(anchorOf('GET /v1/sites?bbox=west,south,east,north')).toBe(
			'get-v1sitesbboxwestsoutheastnorth'
		);
		expect(anchorOf('Pioupiou’s condition')).toBe('pioupious-condition');
	});
});

describe('posts', () => {
	it('lists the newest first, with its picture and whether it is a draft', () => {
		const read_ = read(
			{
				'/src/content/news/2026-01-02-b.md': '---\ntitle: B\nimage: b.svg\n---\nText.',
				'/src/content/news/2026-03-04-a.md':
					'---\ntitle: A\ndate: 2026-03-04\ndraft: true\n---\nText.'
			},
			{ '/src/content/news/b.svg': '<svg></svg>' }
		);
		expect(read_.map((post) => [post.slug, post.date, post.draft, post.image])).toEqual([
			['2026-03-04-a', '2026-03-04', true, undefined],
			['2026-01-02-b', '2026-01-02', false, '<svg></svg>']
		]);
	});

	it('has the first post, with its picture, as a draft', () => {
		const first = posts.at(-1);
		expect(first?.title).toBeTruthy();
		expect(first?.image).toMatch(/^<svg/);
		expect(first?.image?.length).toBeLessThan(30_000);
		// The picture is drawn with the pack's classes only: no colour, no style of its own.
		expect(first?.image).not.toMatch(/#[0-9a-f]{3,6}\b|style=|<style/i);
	});
});
