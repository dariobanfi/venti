/**
 * The guide to adding a data source, `docs/adding-a-source.md`, read at
 * build time into blocks: the same file the repository has, without its
 * title, which is the page's. A link to another file of the repository
 * leads to that file on GitHub; a link to a heading of the guide stays on
 * the page.
 */

import source from '../../../../../docs/adding-a-source.md?raw';
import { SOURCE_CODE } from '#lib/app/routes.ts';
import { type Block, type Inline, parse } from '#lib/news/markdown.ts';

/** The template a new source starts from, and the one check, on GitHub. */
export const TEMPLATE = `${SOURCE_CODE}/tree/main/tools/new-source/template`;
export const CHECK = `${SOURCE_CODE}/blob/main/tools/new-source/check.sh`;

/** Where a link of a file in `docs/` leads from this site. */
export function linkFrom(href: string): string {
	if (/^[a-z]+:/i.test(href) || href.startsWith('#') || href.startsWith('/')) return href;
	const [path, anchor] = href.split('#');
	const file = new URL(path, 'https://repository/docs/').pathname.slice(1);
	const kind = /\.\w+$/.test(file) ? 'blob' : 'tree';
	return `${SOURCE_CODE}/${kind}/main/${file}${anchor ? `#${anchor}` : ''}`;
}

function relinked(line: Inline[]): Inline[] {
	return line.map((piece) =>
		piece.kind === 'link' ? { ...piece, href: linkFrom(piece.href) } : piece
	);
}

/** The blocks of a document, its title left out and its links led to where they go from here. */
export function guideOf(markdown: string): Block[] {
	return parse(markdown)
		.blocks.filter((block) => !(block.kind === 'heading' && block.level === 1))
		.map((block) => {
			switch (block.kind) {
				case 'heading':
				case 'paragraph':
					return { ...block, line: relinked(block.line) };
				case 'list':
					return { ...block, items: block.items.map(relinked) };
				case 'table':
					return {
						...block,
						head: block.head.map(relinked),
						rows: block.rows.map((row) => row.map(relinked))
					};
				default:
					return block;
			}
		});
}

export const guide = guideOf(source);
