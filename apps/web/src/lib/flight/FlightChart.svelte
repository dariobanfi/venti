<!--
	A flight's altitude profile on a card of its own, with the second
	flight's line where one is laid beside it, and the climb colours' legend.
	Dragging along it moves the marker along the track. It stands in the
	flight's panel, or under the map on a wide screen.
-->
<script lang="ts">
	import type { FlightView, GroundUnderFlight } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import AltitudeChart from './AltitudeChart.svelte';
	import { type Beside, otherInWords } from './beside';
	import ClimbLegend from './ClimbLegend.svelte';

	interface Props {
		flight: FlightView;
		ground: readonly GroundUnderFlight[];
		/** The position in the track the pilot is inspecting, if any. */
		cursor: number | undefined;
		/** Whether the marker is moving through the flight by itself. */
		playing: boolean;
		beside?: Beside;
		onbeside?: () => void;
	}

	let { flight, ground, cursor = $bindable(), playing, beside, onbeside }: Props = $props();

	const text = format();
</script>

<div class="chart-card">
	<div class="profile">
		<AltitudeChart {flight} {ground} bind:cursor other={beside} />
	</div>
	{#if beside}
		<!-- Whose the second line is, and where its glider was at the marked moment. -->
		<button class="beside" type="button" onclick={onbeside}>
			<span class="swatch" aria-hidden="true"></span>
			<span class="who">
				<span class="name">{beside.name}</span>
				<span class="said" aria-live={playing ? 'off' : 'polite'}>
					{cursor === undefined
						? beside.comparison.words
						: otherInWords(beside, flight.track[cursor], text)}
				</span>
			</span>
			<span class="open">Side by side</span>
		</button>
	{/if}
	<ClimbLegend />
</div>

<style>
	/* The altitude profile and its legend, on a card of their own. */
	.chart-card {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		padding: 0.75rem;
		border: 1px solid var(--separator);
		border-radius: var(--radius-card);
	}

	.beside {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		min-height: var(--target);
		padding: 0.375rem 0.625rem;
		border: 1px solid var(--separator);
		border-radius: var(--radius-control);
		text-align: left;
		font-size: var(--subheadline);
	}

	.swatch {
		flex: none;
		width: 1rem;
		height: 0.25rem;
		border-radius: var(--radius-pill);
		background: var(--other);
	}

	.who {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
	}

	.name {
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.said {
		color: var(--metric-secondary, var(--text-secondary));
		font-variant-numeric: tabular-nums;
	}

	.open {
		flex: none;
		color: var(--link);
		font-weight: 500;
	}

	.profile {
		height: var(--profile-height, 7.75rem);
	}
</style>
