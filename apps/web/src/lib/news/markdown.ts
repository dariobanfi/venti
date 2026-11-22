/**
 * The news posts' Markdown, read into blocks the page draws itself: no
 * markup of a post's is ever set as HTML. Enough of Markdown for a short
 * post and no more: a head of `key: value` lines between `---` lines,
 * paragraphs, `## ` headings, `- ` lists, and in a line `[words](link)`,
 * `**strong**` and `*emphasis*`.
 */

/** A piece of a line. */
export type Inline =
	| { kind: 'text'; text: string }
	| { kind: 'strong'; text: string }
	| { kind: 'em'; text: string }
	| { kind: 'link'; text: string; href: string };

export type Block =
	| { kind: 'heading'; line: Inline[] }
	| { kind: 'paragraph'; line: Inline[] }
	| { kind: 'list'; items: Inline[][] };

export interface Parsed {
	/** The head's lines, by key. */
	head: Record<string, string>;
	blocks: Block[];
}

const HEAD = /^---\n([\s\S]*?)\n---\n?/;
const PIECE = /\[([^\]]+)\]\(([^)\s]+)\)|\*\*([^*]+)\*\*|\*([^*]+)\*/g;

/** A line's words, with its links, strong and emphasised words. */
export function inline(line: string): Inline[] {
	const pieces: Inline[] = [];
	let at = 0;
	for (const found of line.matchAll(PIECE)) {
		const index = found.index ?? 0;
		if (index > at) pieces.push({ kind: 'text', text: line.slice(at, index) });
		if (found[1] !== undefined) pieces.push({ kind: 'link', text: found[1], href: found[2] });
		else if (found[3] !== undefined) pieces.push({ kind: 'strong', text: found[3] });
		else pieces.push({ kind: 'em', text: found[4] });
		at = index + found[0].length;
	}
	if (at < line.length) pieces.push({ kind: 'text', text: line.slice(at) });
	return pieces;
}

/** A post's head and blocks. */
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
	const body = found ? text.slice(found[0].length) : text;
	const blocks: Block[] = [];
	for (const chunk of body.split(/\n\s*\n/)) {
		const lines = chunk
			.split('\n')
			.map((line) => line.trim())
			.filter(Boolean);
		if (lines.length === 0) continue;
		if (lines[0].startsWith('## ')) {
			blocks.push({ kind: 'heading', line: inline(lines[0].slice(3).trim()) });
			if (lines.length > 1)
				blocks.push({ kind: 'paragraph', line: inline(lines.slice(1).join(' ')) });
		} else if (lines.every((line) => line.startsWith('- '))) {
			blocks.push({ kind: 'list', items: lines.map((line) => inline(line.slice(2).trim())) });
		} else {
			blocks.push({ kind: 'paragraph', line: inline(lines.join(' ')) });
		}
	}
	return { head, blocks };
}
