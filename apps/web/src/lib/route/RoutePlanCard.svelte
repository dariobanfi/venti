<!--
	What a route drawn on the map comes to, in the core's words: its
	sentence, what the route would score flown as drawn (the core's name for
	it, "Closed FAI triangle", its distance and points, and for a triangle
	where it closes: "Closes within 1.6 km of the start: …"), and each leg's
	length and direction. Under them "Undo", "Back to the start" and
	"Clear". A card at the map's side on a wide screen, a card over the
	map's foot on a narrow one, so that the map stays in reach of the finger.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import { getApp } from '#lib/app/app.svelte.ts';
	import ConfirmDialog from '#lib/design/ConfirmDialog.svelte';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import Menu, { type MenuItem } from '#lib/design/Menu.svelte';
	import { saveFile } from '#lib/logbook/flight-file.ts';
	import Metric from '#lib/design/Metric.svelte';
	import type { RoutePlanner } from './planner.svelte';

	interface Props {
		planner: RoutePlanner;
		onclose: () => void;
		/**
		 * The weather along the route: a place kept for it under the score,
		 * for when the core says what the forecast is along a route. Nothing
		 * is shown there until then.
		 */
		weather?: Snippet;
	}

	let { planner, onclose, weather }: Props = $props();

	const text = format();
	const plan = $derived(planner.plan);
	const score = $derived(plan?.score);

	/*
	 * The route as a GPX file, out and in: two entries of the card's menu,
	 * in the core's words. The core writes the file and reads one
	 * (`routeToGpx`, `routeFromGpx`); what it read is said once, under the
	 * title, and a file it cannot read is refused in its words.
	 */
	const app = getApp();
	const said = $derived(app.said);
	let picker = $state<HTMLInputElement>();
	/** What the core read from a file, or why it could not, until the route changes again. */
	let fromFile = $state.raw<{ words: string; refused: boolean; places?: unknown }>();
	// Said once: it goes when the route is changed by hand.
	$effect(() => {
		const places = planner.places;
		if (fromFile?.places && fromFile.places !== places) fromFile = undefined;
	});

	async function exportGpx() {
		const core = app.social?.core;
		if (!core || planner.places.length === 0) return;
		const text = await core.routeToGpx(planner.places.flatMap(({ lat, lon }) => [lat, lon]));
		saveFile('venti-route.gpx', text);
	}

	/** A file's route read while one is drawn: it waits for the pilot's answer to the core's question. */
	let waiting = $state.raw<{ words: string; places: { lat: number; lon: number }[] }>();

	async function importGpx(file: File | undefined) {
		const core = app.social?.core;
		if (!core || !file) return;
		try {
			const route = await core.routeFromGpx(await file.text());
			const read = {
				words: route.words,
				places: route.points.map(({ lat, lon }) => ({ lat, lon }))
			};
			// A route already drawn is not replaced without asking.
			if (planner.places.length > 0) waiting = read;
			else takeFromFile(read);
		} catch (error) {
			fromFile = { words: error instanceof Error ? error.message : String(error), refused: true };
		}
	}

	function takeFromFile(read: { words: string; places: { lat: number; lon: number }[] }) {
		planner.replace(read.places);
		fromFile = { words: read.words, refused: false, places: planner.places };
	}

	const menu = $derived<MenuItem[]>(
		said
			? [
					...(planner.places.length >= 2
						? [{ label: said.planner.exportGpx, icon: 'download' as const, onselect: exportGpx }]
						: []),
					{ label: said.planner.importGpx, icon: 'route', onselect: () => picker?.click() }
				]
			: []
	);

	/** What a place is called in the list of legs: the site's name, or its number. */
	function placeName(index: number): string {
		return planner.places[index]?.name ?? String(index + 1);
	}
</script>

<section class="route" aria-labelledby="route-title">
	<header>
		<h2 id="route-title">Plan a route</h2>
		<span class="head-actions">
			{#if said && menu.length > 0}
				<Menu label={said.common.more} items={menu} />
			{/if}
			<button
				class="close"
				type="button"
				aria-label={said?.common.close ?? 'Close'}
				title={said?.common.close ?? 'Close'}
				onclick={onclose}
			>
				<Icon name="close" />
			</button>
		</span>
	</header>
	<input
		bind:this={picker}
		class="visually-hidden"
		type="file"
		accept=".gpx,application/gpx+xml,application/xml,text/xml"
		aria-label={said?.planner.importGpx}
		tabindex="-1"
		onchange={(event) => {
			const input = event.currentTarget;
			void importGpx(input.files?.[0]);
			// The same file can be chosen again.
			input.value = '';
		}}
	/>
	{#if fromFile}
		<p class={fromFile.refused ? 'problem' : 'words'} role="status" data-route-file>
			{fromFile.words}
		</p>
	{/if}

	{#if planner.problem}
		<p class="problem" role="alert">{planner.problem}</p>
	{:else if plan}
		<p class="words" role="status" data-route-words>{plan.words}</p>
	{/if}

	{#if score}
		<div class="score">
			<h3>{score.name}</h3>
			<dl class="metrics">
				<Metric caption="Scoring distance" value={text.kilometres(score.distanceM)} unit="km" />
				<Metric caption="Points" value={text.points(score.points)} />
				<Metric caption="Along the route" value={text.kilometres(plan?.distanceM ?? 0)} unit="km" />
			</dl>
			{#if plan?.closingCircle}
				<!-- Where the triangle closes, the dashed ring on the map. -->
				<p class="closing" data-closing-words>{plan.closingCircle.words}</p>
			{/if}
		</div>
	{/if}

	{#if weather}
		<!-- The weather along the route: the core's, once it has it. -->
		<section class="weather" aria-label="The weather along the route">
			{@render weather()}
		</section>
	{/if}

	{#if plan && plan.legs.length > 0}
		<ol class="legs" aria-label="Legs">
			{#each plan.legs as leg, index (index)}
				<li>
					<span class="from-to">{placeName(index)} → {placeName(index + 1)}</span>
					<span class="numeric">{leg.words}</span>
				</li>
			{/each}
		</ol>
	{/if}

	<div class="acts">
		<button
			class="button-plain"
			type="button"
			disabled={planner.places.length === 0}
			onclick={() => planner.undo()}
		>
			Undo
		</button>
		<button
			class="button-plain"
			type="button"
			disabled={!planner.canClose}
			onclick={() => planner.backToStart()}
		>
			Back to the start
		</button>
		<button
			class="button-plain clear"
			type="button"
			disabled={planner.places.length === 0}
			onclick={() => planner.clear()}
		>
			<Icon name="trash" />
			Clear
		</button>
	</div>
</section>

{#if said}
	<!-- Before a file's route takes the place of the one drawn: the core's question. -->
	<ConfirmDialog
		open={waiting !== undefined}
		title={said.planner.replaceQuestion}
		message={waiting?.words ?? ''}
		confirm={said.planner.replace}
		cancel={said.common.cancel}
		onconfirm={() => {
			if (waiting) takeFromFile(waiting);
			waiting = undefined;
		}}
		oncancel={() => (waiting = undefined)}
	/>
{/if}

<style>
	.route {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}

	h2 {
		margin: 0;
		font-size: var(--subheadline);
		font-weight: 600;
	}

	.head-actions {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	header button.close {
		display: grid;
		place-items: center;
		width: var(--target);
		height: var(--target);
		margin: calc(var(--space-2) * -1);
		color: var(--text-secondary);
	}

	.words {
		font-size: var(--subheadline);
	}

	.problem {
		font-size: var(--subheadline);
		color: var(--danger);
	}

	.score {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding-top: var(--space-2);
		border-top: 1px solid var(--separator);
	}

	h3 {
		margin: 0;
		font-size: var(--subheadline);
		font-weight: 600;
	}

	.closing {
		margin: 0;
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.metrics {
		display: flex;
		gap: var(--space-3);
	}

	.legs {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		margin: 0;
		padding: var(--space-2) 0 0;
		border-top: 1px solid var(--separator);
		list-style: none;
		font-size: var(--footnote);
	}

	.legs li {
		display: flex;
		justify-content: space-between;
		gap: var(--space-3);
	}

	.from-to {
		min-width: 0;
		overflow-wrap: anywhere;
		color: var(--text-secondary);
	}

	.legs .numeric {
		flex: none;
	}

	.acts {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-1);
		margin-inline: calc(var(--space-3) * -1);
	}

	.clear {
		gap: 0.375rem;
		margin-left: auto;
		color: var(--danger);
	}

	.clear :global(svg) {
		width: 1.125rem;
		height: 1.125rem;
	}
</style>
