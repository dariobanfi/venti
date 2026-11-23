<!--
	The pilots near a place, for Community: the pilots whose public flights
	took off near it in the last months, each a card with their face, their
	name, the core's line of their flights and take-offs from there, and
	what their season adds up to, each to the pilot's page. Every word of a
	card is the core's (`pilotsNear`), and the empty list says the core's
	sentence.

	Which place: the browser's position where the pilot has allowed the
	site to have it already (nothing is asked: the browser is not made to
	put its question for this list); else the place the home page's weather
	is for (`place.ts`: the position the browser gave before, or a take-off
	chosen there); else the region's middle, which is where the map opens.
	The server takes the place to about a kilometre and keeps it nowhere.
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import { pilotHref } from '#lib/app/routes.ts';
	import type { Coordinate, PilotsNear } from '#lib/core/index.ts';
	import { rememberedPlace } from '#lib/home/place.ts';
	import { watchPosition } from '#lib/map/position.ts';
	import { FIRST_REGION } from '#lib/today/model.svelte.ts';
	import PilotFace from './PilotFace.svelte';

	interface Props {
		/** The id of the section's heading, for its name. */
		labelledby: string;
	}

	let { labelledby }: Props = $props();

	const app = getApp();
	const everyone = $derived(app.everyone);

	/** Where the list is for: what this browser remembers, until the position comes. */
	let place = $state.raw<Coordinate>(rememberedPlace()?.at ?? FIRST_REGION);
	let near = $state.raw<PilotsNear>();
	let problem = $state.raw<string>();
	/** Counts the pilot's "Try again". */
	let again = $state(0);

	// The browser's position, where it was allowed already: once.
	$effect(() => {
		let current = true;
		let stop = () => {};
		navigator.permissions
			?.query({ name: 'geolocation' })
			.then((status) => {
				if (!current || status.state !== 'granted') return;
				stop = watchPosition(
					navigator.geolocation,
					(fix) => {
						stop();
						if (current) place = fix;
					},
					() => {}
				);
			})
			.catch(() => {});
		return () => {
			current = false;
			stop();
		};
	});

	$effect(() => {
		const model = everyone;
		const at = place;
		void again;
		if (!model) return;
		let current = true;
		problem = undefined;
		model.pilotsNear(at).then(
			(answer) => {
				if (current) near = answer;
			},
			(error) => {
				if (current) problem = error instanceof Error ? error.message : String(error);
			}
		);
		return () => {
			current = false;
		};
	});
</script>

{#if near}
	<p class="said">{near.words}</p>
	{#if near.pilots.length === 0}
		<p class="quiet">{near.empty}</p>
	{:else}
		<ul class="cards" aria-labelledby={labelledby}>
			{#each near.pilots as pilot (pilot.handle)}
				<li class="group-card pilot">
					<PilotFace
						picture={pilot.picturePath}
						name={pilot.name}
						initials={pilot.initials}
						size="m"
					/>
					<span class="lines">
						<a class="name" href={pilotHref(pilot.handle)}>{pilot.name}</a>
						<span class="line">{pilot.words}</span>
						{#if pilot.takeoffs.length > 1}
							<span class="line">
								{pilot.takeoffs
									.slice(1)
									.map((site) => site.name)
									.join(' · ')}
							</span>
						{/if}
						{#if pilot.season}
							<span class="line season">{pilot.season.name}: {pilot.season.words}</span>
						{/if}
					</span>
				</li>
			{/each}
		</ul>
	{/if}
{:else if problem}
	<p class="quiet" role="alert">{problem}</p>
	<button class="button-plain again" type="button" onclick={() => (again += 1)}>
		{app.said?.common.tryAgain ?? 'Try again'}
	</button>
{:else if everyone}
	<p class="quiet" role="status">Loading…</p>
{/if}

<style>
	.said,
	.quiet {
		color: var(--text-secondary);
	}

	.said {
		font-size: var(--footnote);
	}

	.again {
		align-self: flex-start;
		margin-left: calc(-1 * var(--space-3));
	}

	.cards {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(min(100%, 15rem), 1fr));
		gap: var(--space-3);
		list-style: none;
	}

	/* The card is the pilot's link: the name's link is stretched over it. */
	.pilot {
		position: relative;
		display: flex;
		align-items: flex-start;
		gap: var(--space-3);
		padding: var(--space-3) var(--space-4);
		min-width: 0;
	}

	/* Not a row of the card: none of the card's hairline over it. */
	.lines {
		box-shadow: none;
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		min-width: 0;
	}

	.name {
		font-weight: 600;
		color: var(--link);
		overflow-wrap: anywhere;
	}

	.name::after {
		content: '';
		position: absolute;
		inset: 0;
		border-radius: inherit;
	}

	.line {
		font-size: var(--footnote);
		color: var(--text-secondary);
		overflow-wrap: anywhere;
	}

	.season {
		font-variant-numeric: tabular-nums;
	}

	@media (hover: hover) {
		.pilot:hover {
			background: var(--fill);
		}
	}
</style>
