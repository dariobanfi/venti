<!--
	The season's flights that stand out, as large cards across the home
	page: the track's shape, what the flight stands out for, its distance
	large, the pilot and where they took off. The whole card opens the
	flight; the pilot's name is a link of its own.

	What another pilot wrote, a name or a place, is set as text.
-->
<script lang="ts">
	import PilotFace from '#lib/community/PilotFace.svelte';
	import { getApp } from '#lib/app/app.svelte.ts';
	import type { PublicFlight } from '#lib/core/index.ts';
	import { pilotHref, publicFlightHref } from '#lib/app/routes.ts';
	import { distance, shape } from '#lib/community/words.ts';
	import { flightFormat, format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import TrackThumbnail from '#lib/logbook/TrackThumbnail.svelte';
	import type { Achievement } from './achievements.ts';
	import type { Panel } from './model.svelte.ts';

	interface Props {
		shown: Panel<Achievement[]> | undefined;
		again: () => void;
		class?: string;
	}

	let { shown, again, class: className = '' }: Props = $props();

	const app = getApp();
	const text = format();

	/** The day, and the points where the flight scores: "Sat, 9 May 2026 · 12.01 points". */
	function when(flight: PublicFlight): string {
		const day = flightFormat(flight).day(flight.startUnixS);
		return flight.score ? `${day} · ${text.points(flight.score.points)} points` : day;
	}
</script>

<section
	class="achievements {className}"
	aria-label="Achievements"
	aria-busy={!shown || shown.phase === 'loading'}
>
	{#if !shown || shown.phase === 'loading'}
		<p class="quiet card">Loading…</p>
	{:else if shown.phase === 'failed'}
		<div class="card">
			<p class="quiet">{shown.problem}</p>
			<button class="button-plain" type="button" onclick={again}>{app.said?.common.tryAgain}</button
			>
		</div>
	{:else if shown.phase === 'loaded'}
		{#if shown.value.length === 0}
			<p class="quiet card">No public flights this season yet.</p>
		{:else}
			<ul>
				{#each shown.value as { title, flight } (flight.id)}
					{@const triangle = shape(flight)}
					<li class="card">
						<span class="drawing"><TrackThumbnail outline={flight.outline} fill /></span>
						<span class="words">
							<span class="title">{title}</span>
							<a class="number numeric" href={publicFlightHref(flight.id)}>
								{#if triangle}
									<span class="shape" role="img" aria-label={triangle}
										><Icon name="triangle" /></span
									>
								{/if}
								{distance(flight, text)}<small>km</small>
							</a>
							<span class="named">
								<PilotFace picture={flight.pilotPicture} name={flight.pilot} />
								<a class="pilot" href={pilotHref(flight.handle)}>{flight.pilot}</a>
							</span>
							<span class="where">
								{flight.takeoffName ??
									(flight.takeoff ? text.place(flight.takeoff.lat, flight.takeoff.lon) : '')}
							</span>
							<span class="when numeric">{when(flight)}</span>
						</span>
					</li>
				{/each}
			</ul>
		{/if}
	{/if}
</section>

<style>
	.achievements ul {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(min(100%, 20rem), 1fr));
		gap: var(--space-4);
	}

	.card {
		position: relative;
		display: flex;
		align-items: center;
		gap: var(--space-4);
		min-width: 0;
		padding: var(--space-4);
		background: var(--surface);
		border: 1px solid var(--separator);
		border-radius: var(--radius-card);
	}

	div.card {
		flex-direction: column;
		align-items: flex-start;
		gap: var(--space-2);
	}

	.quiet {
		color: var(--text-secondary);
	}

	.drawing {
		flex: none;
		width: 7.5rem;
		height: 7.5rem;
	}

	.words {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: var(--space-1);
	}

	.title {
		color: var(--text-secondary);
		font-size: var(--subheadline);
		font-weight: 500;
	}

	.number {
		font-size: var(--number-large);
		font-weight: 700;
		line-height: 1.05;
		color: var(--text);
		white-space: nowrap;
	}

	.number small {
		margin-left: 0.15em;
		font-size: 0.45em;
		font-weight: 500;
		color: var(--text-secondary);
	}

	.shape {
		display: inline-block;
		margin-right: 0.15em;
		font-size: 0.4em;
		vertical-align: 0.4em;
		color: var(--text-secondary);
	}

	/* The whole card opens the flight. */
	.number::after {
		content: '';
		position: absolute;
		inset: 0;
		border-radius: var(--radius-card);
	}

	.number:focus-visible {
		outline: none;
	}

	.number:focus-visible::after {
		outline: 0.1875rem solid var(--color-focus);
		outline-offset: -0.1875rem;
	}

	/* Above the flight's link, which covers the card: a target of its own. */
	.named {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		min-width: 0;
	}

	.pilot {
		position: relative;
		z-index: 1;
		max-width: 100%;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-weight: 600;
		color: var(--link);
	}

	.where,
	.when {
		max-width: 100%;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--text-secondary);
		font-size: var(--subheadline);
	}

	@media (hover: hover) {
		li.card:hover {
			background: var(--fill);
		}

		.pilot:hover {
			text-decoration: underline;
		}
	}

	/* A phone: a smaller drawing beside the words. */
	@media (max-width: 55.99rem) {
		.drawing {
			width: 5.5rem;
			height: 5.5rem;
		}
	}
</style>
