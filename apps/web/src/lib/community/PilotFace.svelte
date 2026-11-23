<!--
	A pilot's face: their picture, round, where they have one, else their
	initials on a quiet disc. The picture is the server's own copy, 256 by
	256, at the path the core names (`/v1/pictures/…`), fetched from this
	site's origin as every other answer of the server is. It is beside the
	name, which says who it is, so it carries no words of its own.
-->
<script lang="ts">
	import { initials as initialsOf } from '#lib/groups/faces.ts';

	interface Props {
		/** The path of the pilot's picture, if they have one. */
		picture?: string;
		/** The name the pilot is shown by, for the initials. */
		name: string;
		/** The core's initials, where it gives them. */
		initials?: string;
		/** How large: a row's, a card's, or a page's head. */
		size?: 's' | 'm' | 'l';
	}

	let { picture, name, initials, size = 's' }: Props = $props();

	/** A picture that could not be loaded shows the initials. */
	let failed = $state<string>();
	const shown = $derived(picture && failed !== picture ? picture : undefined);
	const letters = $derived(initials || initialsOf(name));
</script>

<span class="face {size}" aria-hidden="true">
	{#if shown}
		<img src={shown} alt="" loading="lazy" decoding="async" onerror={() => (failed = shown)} />
	{:else}
		<span class="letters">{letters}</span>
	{/if}
</span>

<style>
	.face {
		flex: none;
		display: inline-grid;
		place-items: center;
		width: 2rem;
		height: 2rem;
		border-radius: 50%;
		overflow: hidden;
		background: var(--fill-strong);
		color: var(--text);
		font-size: var(--caption);
		font-weight: 600;
		letter-spacing: 0.02em;
	}

	.face.m {
		width: var(--size-touch);
		height: var(--size-touch);
		font-size: var(--subheadline);
	}

	.face.l {
		width: calc(var(--size-touch) * 1.5);
		height: calc(var(--size-touch) * 1.5);
		font-size: var(--title);
	}

	img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.letters {
		line-height: 1;
	}
</style>
