<!--
	One glider of another pilot: who flies it, how old its position is, how
	high it is and how it moves, and the way to the server's page of the
	track. While this is on screen the flight is followed, and the map draws
	it as a line. The words are those of the iPhone app's `LiveScreens.swift`.
-->
<script lang="ts">
	import { compassPoint, format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import Metric from '#lib/design/Metric.svelte';
	import type { LiveModel } from './model.svelte';
	import { CAUTION, FOLLOW_ON_THE_MAP, NO_LONGER_HERE } from './words';

	interface Props {
		/** The identifier of the glider's track. */
		id: string;
		live: LiveModel;
		/** Opens the form that reports the track to whoever runs the server. */
		onreport: () => void;
		/** Whether the map is kept on this glider. */
		onMap?: boolean;
		/** Keeps the map on this glider, or lets it go. Offered while it is in the air. */
		onfollowonmap?: () => void;
	}

	let { id, live, onreport, onMap = false, onfollowonmap }: Props = $props();

	const text = format();

	// The flight's track is asked for while its page is open, and drawn on
	// the map behind it.
	$effect(() => live.follow(id));
	// How old the position is, second by second.
	$effect(() => live.tellAges());

	/** The glider as the newest answer about it has it. */
	const glider = $derived(live.glider(id));
	/** The flight so far, once the server has handed its track out. */
	const flight = $derived(live.followed.id === id ? live.followed.view : undefined);
	/** Why the flight cannot be followed, if it cannot. */
	const problem = $derived(live.followed.id === id ? live.followed.problem : undefined);
</script>

<!-- A track the server says is gone is gone, also while the map still has
     its glider from a moment before. -->
{#if glider && !problem}
	<div class="glider">
		<section>
			<dl class="card">
				<div class="row">
					<dt>Pilot</dt>
					<dd>{glider.name}</dd>
				</div>
				<div class="row">
					<dt>Position</dt>
					<dd class="numeric">{live.ageInWords(glider)}</dd>
				</div>
			</dl>
		</section>

		<section>
			<dl class="card numbers">
				<Metric caption="Altitude" value={text.metres(glider.altitudeM)} unit="m" />
				{#if !glider.landed}
					{#if glider.climbMs !== undefined}
						<Metric caption="Climb" value={text.climb(glider.climbMs)} unit="m/s" />
					{/if}
					{#if glider.groundSpeedMs !== undefined}
						<Metric
							caption="Speed"
							value={text.kilometresPerHour(glider.groundSpeedMs)}
							unit="km/h"
						/>
					{/if}
					{#if glider.courseDeg !== undefined}
						<Metric
							caption="Flying"
							value={compassPoint(glider.courseDeg)}
							unit="{text.metres(glider.courseDeg)}°"
						/>
					{/if}
				{/if}
				<!-- The flight so far, whose line is on the map. -->
				{#if flight}
					<Metric caption="Shown for" value={text.hoursMinutes(flight.durationS)} unit="h" />
					<Metric caption="Highest" value={text.metres(flight.highestM)} unit="m" />
				{/if}
			</dl>
			<p class="footnote">{CAUTION}</p>
		</section>

		{#if onfollowonmap && !glider.landed && !glider.quiet}
			<div class="card">
				<button class="link" type="button" aria-pressed={onMap} onclick={onfollowonmap}>
					{FOLLOW_ON_THE_MAP}
					{#if onMap}<Icon name="check" />{/if}
				</button>
			</div>
		{/if}

		<section>
			<div class="card">
				<a class="link" href={glider.pagePath} target="_blank" rel="noopener">
					This flight’s page on Venti
				</a>
			</div>
			<p class="footnote">That page needs no app and keeps itself up to date.</p>
		</section>

		<div class="card">
			<button class="link" type="button" onclick={onreport}>Report this track</button>
		</div>
	</div>
{:else}
	<p class="gone">{problem ?? NO_LONGER_HERE}</p>
{/if}

<style>
	.glider {
		display: flex;
		flex-direction: column;
		gap: 1.25rem;
	}

	.card {
		background: var(--surface);
		border-radius: var(--radius-card);
		overflow: hidden;
	}

	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		min-height: var(--target);
		padding: 0.5rem 1rem;
	}

	/* A hairline between rows. */
	.row + .row {
		box-shadow: inset 0 0.5px 0 var(--separator);
	}

	dd {
		color: var(--text-secondary);
		text-align: right;
		overflow-wrap: anywhere;
	}

	/* Up to six numbers, three to a row. */
	.numbers {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 1rem 0.75rem;
		padding: 1rem;
	}

	.link {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		width: 100%;
		min-height: var(--target);
		padding: 0.5rem 1rem;
		color: var(--link);
		text-align: left;
	}

	@media (hover: hover) {
		.link:hover {
			background: var(--fill);
		}
	}

	.link:focus-visible {
		outline-offset: -0.1875rem;
		border-radius: var(--radius-card);
	}

	.footnote {
		padding: 0.5rem 1rem 0;
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.gone {
		padding: 1rem;
		border-radius: var(--radius-card);
		background: var(--surface);
		color: var(--text-secondary);
	}
</style>
