/**
 * The Markdown of the news posts and of the repository's documents the
 * developer pages show, read into blocks the page draws itself: no markup
 * of a file's is ever set as HTML. Enough of Markdown for those files and
 * no more: a head of `key: value` lines between `---` lines, paragraphs,
 * headings (`#` to `####`), `- ` lists, fenced code, tables, and in a line
 * `[words](link)`, `` `code` ``, `**strong**` and `*emphasis*`.
 */

/** A piece of a line. */
export type Inline =
	| { kind: 'text'; text: string }
	| { kind: 'strong'; text: string }
	| { kind: 'em'; text: string }
	| { kind: 'code'; text: string }
	| { kind: 'link'; text: string; href: string; code?: boolean };

export type Block =
	| { kind: 'heading'; level: number; id: string; line: Inline[] }
	| { kind: 'paragraph'; line: Inline[] }
	| { kind: 'list'; items: Inline[][] }
	| { kind: 'code'; language: string; text: string }
	| { kind: 'table'; head: Inline[][]; rows: Inline[][][] };

export interface Parsed {
	/** The head's lines, by key. */
	head: Record<string, string>;
	blocks: Block[];
}

const HEAD = /^---\n([\s\S]*?)\n---\n?/;
const PIECE = /`([^`]+)`|\[([^\]]+)\]\(([^)\s]+)\)|\*\*([^*]+)\*\*|\*([^*]+)\*/g;
const HEADING = /^(#{1,4}) +(.*)$/;
const FENCE = /^```\s*([\w-]*)\s*$/;

/** A line's words, with its code, links, strong and emphasised words. */
export function inline(line: string): Inline[] {
	const pieces: Inline[] = [];
	let at = 0;
	for (const found of line.matchAll(PIECE)) {
		const index = found.index ?? 0;
		if (index > at) pieces.push({ kind: 'text', text: line.slice(at, index) });
		if (found[1] !== undefined) pieces.push({ kind: 'code', text: found[1] });
		else if (found[2] !== undefined) {
			const code = /^`[^`]+`$/.test(found[2]);
			pieces.push(
				code
					? { kind: 'link', text: found[2].slice(1, -1), href: found[3], code }
					: { kind: 'link', text: found[2], href: found[3] }
			);
		} else if (found[4] !== undefined) pieces.push({ kind: 'strong', text: found[4] });
		else pieces.push({ kind: 'em', text: found[5] });
		at = index + found[0].length;
	}
	if (at < line.length) pieces.push({ kind: 'text', text: line.slice(at) });
	return pieces;
}

/**
 * The anchor GitHub gives a heading, so that a link into a document reads
 * the same here as there: lower case, every sign but letters, digits,
 * spaces, hyphens and underscores dropped, spaces as hyphens.
 */
export function anchorOf(heading: string): string {
	return heading
		.toLowerCase()
		.replace(/[^\p{L}\p{N} _-]/gu, '')
		.replace(/ /g, '-');
}

/** The words of a line, without its marks: what a heading's anchor is made from. */
function plain(pieces: Inline[]): string {
	return pieces.map((piece) => piece.text).join('');
}

/** The cells of a table's line. */
function cells(line: string): Inline[][] {
	return line
		.replace(/^\|/, '')
		.replace(/\|$/, '')
		.split('|')
		.map((cell) => inline(cell.trim()));
}

/** A file's head and blocks. */
export function parse(source: string): Parsed {
	const text = source.replace(/\r\n?/g, '\n');
	const head: Record<string, string> = {};
	const found = HEAD.exec(text);
	if (found) {
		for (const line of found[1].split('\n')) {
			const colon = line.indexOf(':');
			if (colon > 0) head[line.slice(0, colon).trim()] = line.slice(colon + 1).trim();
		}
	}
	const lines = (found ? text.slice(found[0].length) : text).split('\n');
	const blocks: Block[] = [];
	let paragraph: string[] = [];
	let list: string[] = [];
	const close = () => {
		if (paragraph.length > 0) blocks.push({ kind: 'paragraph', line: inline(paragraph.join(' ')) });
		if (list.length > 0) blocks.push({ kind: 'list', items: list.map(inline) });
		paragraph = [];
		list = [];
	};
	for (let at = 0; at < lines.length; at++) {
		const line = lines[at].trim();
		const fence = FENCE.exec(line);
		if (fence) {
			close();
			// The code as written, its indentation kept, up to the closing fence.
			const indent = lines[at].length - lines[at].trimStart().length;
			const code: string[] = [];
			for (at++; at < lines.length && !/^```\s*$/.test(lines[at].trim()); at++) {
				code.push(lines[at].slice(Math.min(indent, lines[at].search(/\S|$/))));
			}
			blocks.push({ kind: 'code', language: fence[1], text: code.join('\n') });
			continue;
		}
		const heading = HEADING.exec(line);
		if (heading) {
			close();
			const words = inline(heading[2].trim());
			blocks.push({
				kind: 'heading',
				level: heading[1].length,
				id: anchorOf(plain(words)),
				line: words
			});
		} else if (line.startsWith('|')) {
			close();
			const rows: string[] = [];
			for (; at < lines.length && lines[at].trim().startsWith('|'); at++)
				rows.push(lines[at].trim());
			at--;
			// The second line is the one of dashes under the heads.
			const [first, , ...rest] = rows;
			blocks.push({ kind: 'table', head: cells(first), rows: rest.map(cells) });
		} else if (line === '') {
			close();
		} else if (line.startsWith('- ')) {
			if (paragraph.length > 0) close();
			list.push(line.slice(2).trim());
		} else if (list.length > 0) {
			// A line that goes on with the item above it.
			list[list.length - 1] += ` ${line}`;
		} else {
			paragraph.push(line);
		}
	}
	close();
	return { head, blocks };
}
