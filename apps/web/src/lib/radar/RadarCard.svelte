<!--
	The card of the map tab's "Radar" layer, in the manner of the forecast
	layer's: the frames as the pack's strip of hours, a cell a frame with the
	minutes of its time, the newest at the right; under it the core's line
	for the picture that is on the map ("Rain radar, 11:40, 4 minutes ago")
	and a button that plays the frames once; then the key with a mark in each
	step's colour and the veil's line last, the core's note on what a radar
	does not see, and the source's credit.

	When the newest picture is too old to be the rain of now, the core's
	sentence stands over the line in the colour for caution. While the list
	is asked for, or when there is none, the card says so, with "Try again".

	On a narrow screen the card lies over the map, so it is short
	(`compact`): the key, the note and the credit stand behind "Key and
	notes", folded until the pilot asks for them.
-->
<script lang="ts">
	import { tick } from 'svelte';
	import { MediaQuery } from 'svelte/reactivity';
	import Icon from '#lib/design/Icon.svelte';
	import { keyColor } from '#lib/design/palette.ts';
	import HourStrip from '#lib/forecast/HourStrip.svelte';
	import type { RadarModel } from './radar.svelte';
	import { stripCells, UNSEEN_ON_MAP, WORDS } from './radar';

	interface Props {
		radar: RadarModel;
		/** Whether the card lies over the map, on a narrow screen. */
		compact?: boolean;
	}

	let { radar, compact = false }: Props = $props();

	const view = $derived(radar.view);
	const layer = $derived(radar.layer);
	const frames = $derived(layer?.frames ?? []);
	const cells = $derived(stripCells(frames));
	/** The place of the frame the strip marks. */
	const marked = $derived(Math.max(frames.indexOf(radar.frame!), 0));
	/** Nothing moves by itself for a pilot who asked the system for less motion. */
	const still = new MediaQuery('prefers-reduced-motion: reduce', false);

	/** "Key and notes" with what it unfolds, on a narrow screen. */
	let folded = $state<HTMLDivElement>();

	/** Unfolds the key and the notes and brings them into view, or folds them away. */
	async function fold() {
		radar.keyOpen = !radar.keyOpen;
		if (!radar.keyOpen) return;
		await tick();
		folded?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
	}
</script>

<!-- The key of the steps of rain, what a radar does not see, and whose pictures they are. -->
{#snippet key()}
	{#if view && layer}
		<h3 class="label" id="radar-key">Key</h3>
		<ul class="key" aria-labelledby="radar-key">
			{#each view.key as entry (entry.step)}
				<li>
					<span class="swatch" style:background={keyColor(entry)} aria-hidden="true"></span>
					{entry.words}
				</li>
			{/each}
			<li>
				<span class="swatch" style:background={UNSEEN_ON_MAP} aria-hidden="true"></span>
				{view.unseenWords}
			</li>
		</ul>
		<p class="caption">{view.note}</p>
		{#if layer.credit}
			{@const credit = layer.credit}
			<p class="caption credit">
				{#if credit.url}
					<a href={credit.url} target="_blank" rel="noopener noreferrer">
						{credit.text || credit.provider}
					</a>{:else}{credit.text || credit.provider}{/if}, {credit.licence}
			</p>
		{/if}
	{/if}
{/snippet}

<!-- What the pilot has to be told about the radar as it stands: never folded away. -->
{#snippet states()}
	{#if radar.pictureProblem && !radar.pictureAsking}
		<p class="caption" role="status">
			{radar.pictureProblem}
			<button class="again" type="button" onclick={() => void radar.showFrame()}>Try again</button>
		</p>
	{/if}
	{#if radar.problem && !radar.asking}
		<p class="caption" role="status">
			{radar.problem}
			<button class="again" type="button" onclick={() => void radar.load()}>Try again</button>
		</p>
	{/if}
{/snippet}

<section class="radar" aria-labelledby="radar-title">
	<h2 id="radar-title" class="visually-hidden">Rain radar</h2>
	{#if view && layer}
		{#if frames.length > 0}
			<HourStrip
				label="Radar picture"
				{cells}
				value={marked}
				onchange={(index) => radar.choose(index)}
			/>
		{/if}
		{#if layer.stale}
			<p class="stale" role="status">
				<Icon name="warning" />
				{layer.staleWords}
			</p>
		{/if}
		<div class="said">
			{#if radar.shownFrame}
				<p class="title numeric">{radar.shownFrame.label}</p>
			{:else if radar.pictureAsking}
				<p class="waiting" role="status">{WORDS.askingPicture}</p>
			{/if}
			{#if frames.length > 1 && !still.current}
				<button
					class="play"
					type="button"
					aria-label={radar.playing ? 'Stop' : 'Play'}
					title={radar.playing ? 'Stop' : 'Play'}
					onclick={() => (radar.playing ? radar.stop() : void radar.play())}
				>
					<Icon name={radar.playing ? 'pause' : 'play'} />
				</button>
			{/if}
		</div>
		{#if compact}
			{@render states()}
			<div class="folded" bind:this={folded}>
				<button class="fold" type="button" aria-expanded={radar.keyOpen} onclick={fold}>
					Key and notes
					<span class="turn" class:up={radar.keyOpen} aria-hidden="true">
						<Icon name="forward" />
					</span>
				</button>
				{#if radar.keyOpen}
					{@render key()}
				{/if}
			</div>
		{:else}
			{@render key()}
			{@render states()}
		{/if}
	{:else if radar.asking || !radar.problem}
		<p class="waiting" role="status">{WORDS.asking}</p>
	{:else}
		<div role="status">
			<p>{radar.problem}</p>
			<button class="again" type="button" onclick={() => void radar.load()}>Try again</button>
		</div>
	{/if}
</section>

<style>
	.radar {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	/* The line of the picture on the left, the play button on the right. */
	.said {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		min-width: 0;
		min-height: 2.25rem;
	}

	.title {
		flex: 1;
		min-width: 0;
		margin: 0;
		font-size: var(--subheadline);
		font-weight: 600;
	}

	/* A round control of the card's own fill, as high as the height's menu
	   of the forecast card. */
	.play {
		flex: none;
		display: grid;
		place-items: center;
		width: 2.25rem;
		height: 2.25rem;
		margin-left: auto;
		border-radius: var(--radius-pill);
		background: var(--fill);
		color: var(--text);
	}

	.play :global(svg) {
		width: 1.125rem;
		height: 1.125rem;
	}

	/* Compact on a wide screen: controls about 32 px. */
	@media (min-width: 56rem) {
		.said {
			min-height: 2rem;
		}

		.play {
			width: 2rem;
			height: 2rem;
		}

		.play :global(svg) {
			width: 1rem;
			height: 1rem;
		}
	}

	/* Old rain is not the rain of now: the status colour for caution, with its icon. */
	.stale {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
		margin: 0;
		color: var(--color-caution);
		font-size: var(--subheadline);
		font-weight: 600;
	}

	.stale :global(svg) {
		flex: none;
		width: 1.125rem;
		height: 1.125rem;
	}

	.folded {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		scroll-margin-top: var(--space-1);
	}

	/* A row to press, as high as a finger needs; it reaches into the gaps
	   above and below it, so that the folded card ends close under its words. */
	.fold {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
		min-height: var(--target);
		margin: calc(var(--space-2) * -1) 0;
		color: var(--link);
		font-size: var(--subheadline);
		font-weight: 600;
		text-align: left;
	}

	/* The pack's forward chevron, turned to point down, and up once unfolded. */
	.turn {
		display: grid;
		transform: rotate(90deg);
	}

	.turn.up {
		transform: rotate(-90deg);
	}

	.label {
		margin: 0;
		color: var(--text-secondary);
		font-size: var(--caption);
		font-weight: 600;
	}

	/* One line a step: the words are long. */
	.key {
		display: grid;
		gap: var(--space-1);
		margin: 0;
		padding: 0;
		list-style: none;
		font-size: var(--subheadline);
	}

	.key li {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	/* The step as the map draws it, with an edge, since the palest lie close
	   to the card's surface. */
	.swatch {
		flex: none;
		width: 0.875rem;
		height: 0.875rem;
		border: 1px solid var(--color-border-strong);
	}

	.caption {
		margin: 0;
		color: var(--text-secondary);
		font-size: var(--caption);
	}

	.credit a {
		color: var(--link);
	}

	.waiting {
		margin: 0;
		color: var(--text-secondary);
		font-size: var(--subheadline);
	}

	p {
		margin: 0;
	}

	.again {
		margin-top: var(--space-1);
		color: var(--link);
		font-weight: 600;
	}

	.caption .again {
		margin: 0 0 0 var(--space-1);
		font-size: inherit;
	}
</style>
