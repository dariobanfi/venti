import { error } from '@sveltejs/kit';
import { postOf, posts } from '#lib/news/posts.ts';
import type { EntryGenerator, PageLoad } from './$types';

/** Every post is written out at build time, at its own address. */
export const entries: EntryGenerator = () => posts.map((post) => ({ slug: post.slug }));

export const load: PageLoad = ({ params }) => {
	const post = postOf(params.slug);
	if (!post) error(404, 'No such post');
	return { post };
};
