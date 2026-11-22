/**
 * Venti's news: posts the owner writes as files in the repository,
 * `src/content/news/<date>-<slug>.md`, each with a picture of its own
 * beside it if it names one. They are built into the site: no server
 * table, nothing to sign in to. The newest first.
 *
 * A post's head:
 *
 *     ---
 *     title: Welcome to Venti
 *     date: 2026-11-22
 *     summary: One or two lines for the list.
 *     image: 2026-11-22-welcome.svg
 *     image_alt: What the picture shows, for whoever cannot see it.
 *     draft: true
 *     ---
 *
 * `draft: true` shows "Draft" on the post and in the lists until it is
 * taken out. The picture is an SVG drawn with the classes of
 * `NewsPicture.svelte`, which give it the pack's brand colours.
 */

import { type Block, parse } from './markdown.ts';

export interface Post {
	/** Its file's name without `.md`: "2026-11-22-welcome-to-venti". */
	slug: string;
	title: string;
	/** "2026-11-22". */
	date: string;
	summary: string;
	draft: boolean;
	/** The picture's SVG, as written. */
	image: string | undefined;
	imageAlt: string;
	blocks: Block[];
}

const sources = import.meta.glob('/src/content/news/*.md', {
	query: '?raw',
	import: 'default',
	eager: true
}) as Record<string, string>;

const pictures = import.meta.glob('/src/content/news/*.svg', {
	query: '?raw',
	import: 'default',
	eager: true
}) as Record<string, string>;

/** The posts of these files, the newest first. */
export function read(files: Record<string, string>, images: Record<string, string>): Post[] {
	const byName = new Map(
		Object.entries(images).map(([path, svg]) => [path.slice(path.lastIndexOf('/') + 1), svg])
	);
	return Object.entries(files)
		.map(([path, source]) => {
			const slug = path.slice(path.lastIndexOf('/') + 1).replace(/\.md$/, '');
			const { head, blocks } = parse(source);
			return {
				slug,
				title: head.title || slug,
				date: /^\d{4}-\d{2}-\d{2}$/.test(head.date ?? '') ? head.date : slug.slice(0, 10),
				summary: head.summary ?? '',
				draft: head.draft === 'true',
				image: head.image ? byName.get(head.image) : undefined,
				imageAlt: head.image_alt ?? '',
				blocks
			};
		})
		.toSorted((one, other) =>
			one.date === other.date
				? other.slug.localeCompare(one.slug)
				: other.date.localeCompare(one.date)
		);
}

export const posts: Post[] = read(sources, pictures);

export function postOf(slug: string): Post | undefined {
	return posts.find((post) => post.slug === slug);
}

/** A post's date, "2026-11-22", as a moment in it for the app's date format: noon, by UTC. */
export function postMoment(date: string): number {
	return Date.parse(`${date}T12:00:00Z`) / 1000;
}
