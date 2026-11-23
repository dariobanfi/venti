<!--
	The pilots of a season in the order of their points: each with the place,
	the name, what counts, the points, and under the name the flights that
	count, each a link to the flight.

	What another pilot wrote, a name, is set as text. The core has cut it to
	one line; nothing of it is ever read as markup.
-->
<script lang="ts">
	import { pilotHref, publicFlightHref } from '#lib/app/routes.ts';
	import type { RankedPilot } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import PilotFace from './PilotFace.svelte';
	import { distance, shape } from './words';

	interface Props {
		/** What the list is called, for a screen reader. */
		label: string;
		pilots: RankedPilot[];
	}

	let { label, pilots }: Props = $props();

	const text = format();
</script>

<ol class="card" aria-label={label}>
	{#each pilots as pilot (pilot.handle)}
		<li class="pilot">
			<span class="place numeric">
				<span class="visually-hidden">Place</span>
				{pilot.place}
			</span>
			<PilotFace picture={pilot.picturePath} name={pilot.name} />
			<div class="who">
				<div class="head">
					<span class="name">
						<a href={pilotHref(pilot.handle)}>{pilot.name}</a>
						<span class="counted">{pilot.counted}</span>
					</span>
					<span class="points numeric">
						{text.points(pilot.points)}
						<span class="unit">points</span>
					</span>
				</div>
				<ul class="flights" aria-label="The flights of {pilot.name} that count">
					{#each pilot.flights as flight (flight.id)}
						{@const triangle = shape(flight)}
						<li>
							<a href={publicFlightHref(flight.id)}>
								<span class="day">{text.day(flight.startUnixS)}</span>
								<span class="distance numeric">
									{#if triangle}
										<span class="shape" role="img" aria-label={triangle}>
											<Icon name="triangle" />
										</span>
									{/if}
									{distance(flight, text)} km
								</span>
								<span class="flight-points numeric">
									<span class="visually-hidden">Points</span>
									{flight.score ? text.points(flight.score.points) : '–'}
								</span>
							</a>
						</li>
					{/each}
				</ul>
			</div>
		</li>
	{/each}
</ol>

<style>
	.card {
		background: var(--surface);
		border-radius: var(--radius-card);
		overflow: hidden;
	}

	.pilot {
		position: relative;
		display: flex;
		align-items: flex-start;
		gap: 0.75rem;
		padding: 0.875rem 1rem 0.625rem;
	}

	/* A hairline between pilots, starting where the names do. */
	.pilot:not(:first-child)::before {
		content: '';
		position: absolute;
		top: 0;
		left: 6.25rem;
		right: 0;
		border-top: 0.5px solid var(--separator);
	}

	.place {
		flex: none;
		width: 1.75rem;
		font-size: var(--title);
		font-weight: 600;
		line-height: 1.5rem;
		text-align: center;
		color: var(--text-secondary);
	}

	/* The first three are the ones looked for. */
	.pilot:nth-child(-n + 3) .place {
		color: var(--link);
	}

	.who {
		flex: 1;
		min-width: 0;
	}

	.head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 0.75rem;
	}

	.name {
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
	}

	.name a {
		font-weight: 600;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.counted {
		font-size: var(--subheadline);
		color: var(--text-secondary);
	}

	.points {
		flex: none;
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		font-size: var(--title);
		font-weight: 600;
		line-height: 1.5rem;
	}

	.unit {
		font-size: var(--footnote);
		font-weight: 400;
		color: var(--text-secondary);
	}

	.flights {
		margin-top: 0.375rem;
	}

	.flights a {
		display: flex;
		align-items: baseline;
		gap: 0.75rem;
		min-height: 2.25rem;
		margin: 0 -0.5rem;
		padding: 0.375rem 0.5rem;
		border-radius: 0.5rem;
		font-size: var(--subheadline);
	}

	.day {
		flex: 1;
		min-width: 0;
		color: var(--text-secondary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.distance {
		white-space: nowrap;
	}

	.flight-points {
		width: 3.75rem;
		text-align: right;
		white-space: nowrap;
	}

	.shape {
		display: inline-block;
		font-size: 0.5rem;
		vertical-align: 0.05em;
	}

	@media (hover: hover) {
		.name a:hover {
			text-decoration: underline;
		}

		.flights a:hover {
			background: var(--fill);
		}
	}

	.flights a:active {
		background: var(--fill-strong);
	}

	.flights a:focus-visible {
		outline: 0.1875rem solid var(--color-focus);
		outline-offset: -0.1875rem;
	}
</style>
