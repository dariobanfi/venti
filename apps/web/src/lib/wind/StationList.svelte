<!--
	The wind stations around the part of the map in view, as a list: for
	reading the numbers side by side, and for pilots who use the keyboard or a
	screen reader, which cannot reach symbols drawn on a map. The words are
	those of the iPhone app's `StationList.swift`.
-->
<script lang="ts">
	import type { WindStation } from '#lib/core/index.ts';
	import { age, compassPoint, format } from '#lib/design/format.ts';
	import Sheet from '#lib/design/Sheet.svelte';
	import { spoken, windMark } from './mark';
	import type { WindState } from './watch';
	import WindGlyph from './WindGlyph.svelte';

	interface Props {
		open: boolean;
		wind: WindState;
		/** Called with the station the pilot chose. */
		onchoose: (station: WindStation) => void;
		onclose: () => void;
	}

	let { open, wind, onchoose, onclose }: Props = $props();

	const text = format();

	let search = $state('');

	// A list opened again starts with every station.
	$effect(() => {
		if (!open) search = '';
	});

	/** By name, the way a pilot looks one up. */
	const shown = $derived.by(() => {
		const wanted = search.trim().toLocaleLowerCase();
		return wind.stations
			.filter((station) => station.name.toLocaleLowerCase().includes(wanted))
			.toSorted((first, second) =>
				first.name.localeCompare(second.name, undefined, { sensitivity: 'base' })
			);
	});

	/** The station's height, and how old its reading is if that matters. */
	function detail(station: WindStation): string {
		const parts: string[] = [];
		if (station.elevationM !== undefined) parts.push(`${text.metres(station.elevationM)} m`);
		if (!station.latest) parts.push('no reading');
		else if (station.isOld && station.ageMin !== undefined) parts.push(age(station.ageMin));
		return parts.join(' · ');
	}

	/** A row in one sentence, for a screen reader. */
	function sentence(station: WindStation): string {
		return [station.name, spoken(station, compassPoint), detail(station)]
			.filter(Boolean)
			.join('. ');
	}
</script>

<Sheet {open} title="Wind stations" {onclose}>
	{#if wind.stations.length === 0}
		<div class="nothing">
			<h3>No stations here</h3>
			<p>{wind.problem ?? 'Move the map to an area with wind stations.'}</p>
		</div>
	{:else}
		<input
			type="search"
			placeholder="Station"
			aria-label="Station"
			autocomplete="off"
			bind:value={search}
		/>
		{#if shown.length === 0}
			<div class="nothing">
				<h3>No results for “{search.trim()}”</h3>
				<p>Check the spelling or try a new search.</p>
			</div>
		{:else}
			<ul class="card" aria-label="Wind stations">
				{#each shown as station (station.id)}
					{@const mark = windMark(station)}
					<li>
						<button type="button" aria-label={sentence(station)} onclick={() => onchoose(station)}>
							<WindGlyph {mark} />
							<span class="pair">
								<span class="name">{station.name}</span>
								<span class="lesser">{detail(station)}</span>
							</span>
							{#if mark.label}
								<span class="wind numeric">
									<strong>{mark.label}</strong>&nbsp;km/h
								</span>
							{/if}
						</button>
					</li>
				{/each}
			</ul>
		{/if}
		<p class="footnote">
			{#each wind.credits as credit (credit.source)}
				<span>{credit.text}</span>
			{/each}
		</p>
	{/if}
</Sheet>

<style>
	input {
		width: 100%;
		min-height: 2.5rem;
		margin-bottom: 0.75rem;
		padding: 0 0.875rem;
		border: 0;
		border-radius: 0.75rem;
		background: var(--fill);
		color: var(--text);
		font: inherit;
	}

	input::placeholder {
		color: var(--text-secondary);
	}

	.card {
		background: var(--surface);
		border-radius: var(--radius-card);
		overflow: hidden;
	}

	li + li {
		box-shadow: inset 0 0.5px 0 var(--separator);
	}

	button {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		width: 100%;
		min-height: 3.25rem;
		padding: 0.375rem 1rem 0.375rem 0.75rem;
		text-align: left;
	}

	@media (hover: hover) {
		button:hover {
			background: var(--fill);
		}
	}

	button:active {
		background: var(--fill-strong);
	}

	button:focus-visible {
		outline-offset: -0.1875rem;
		border-radius: 0.5rem;
	}

	.pair {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
		min-width: 0;
	}

	.name {
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.lesser {
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.wind {
		flex: none;
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.wind strong {
		font-family: var(--font-rounded);
		font-size: var(--body);
		font-weight: 600;
		color: var(--text);
	}

	.nothing {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.375rem;
		padding: 3rem 1.5rem;
		text-align: center;
	}

	.nothing h3 {
		font-size: var(--title);
		font-weight: 700;
	}

	.nothing p {
		color: var(--text-secondary);
	}

	.footnote {
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
		padding: 0.5rem 1rem 0;
		font-size: var(--footnote);
		color: var(--text-secondary);
		overflow-wrap: anywhere;
	}
</style>
