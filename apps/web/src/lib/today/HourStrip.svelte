<!--
	A day's hours side by side, as the pack's hour strip: the hour, an arrow
	where the wind goes, its speed, and a bar for what the forecast says of
	flying in it, by its height as well as its colour. With `climbs`, a
	second bar for the climb in the thermal scale's colour.

	Today's card has it, and the home page's weather panel the next hours of
	it. The words of what it shows are the core's, and stand under it where
	it is used.
-->
<script lang="ts">
	import type { OutlookHour } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import { thermalStep } from '#lib/design/palette.ts';

	interface Props {
		hours: OutlookHour[];
		/** Whether the climb of each hour is shown under it. */
		climbs: boolean;
	}

	let { hours, climbs }: Props = $props();

	const text = format();

	/** How tall an hour's bar stands: tall for flyable, low for not. */
	const BAR = { flyable: 1, marginal: 0.6, notFlyable: 0.25 } as const;

	/** An hour for a screen reader: its status, its wind and its thermals. */
	function spoken(hour: OutlookHour): string {
		const verdict =
			hour.verdict === 'flyable'
				? 'Flyable'
				: hour.verdict === 'marginal'
					? `Marginal: ${hour.why}`
					: `Not flyable: ${hour.why}`;
		if (hour.climbMs === undefined) return verdict;
		if (hour.climbMs <= 0) return `${verdict}, no thermals to climb in`;
		const top = hour.thermalTopM === undefined ? '' : ` to ${text.metres(hour.thermalTopM)} m`;
		return `${verdict}, thermals ${text.forecastClimb(hour.climbMs)} m/s${top}`;
	}
</script>

<ol class="hours numeric" aria-label="Hour by hour">
	{#each hours as hour (hour.timeUnixS)}
		<li>
			<span class="hour">{hour.hour}</span>
			{#if hour.windFromDeg !== undefined}
				<span
					class="arrow"
					style:transform="rotate({hour.windFromDeg + 180}deg)"
					aria-hidden="true"
				>
					<Icon name="arrowWind" />
				</span>
			{:else}
				<span class="arrow" aria-hidden="true"></span>
			{/if}
			<span class="speed">{hour.windKmh || '–'}</span>
			<span
				class="bar {hour.verdict}"
				style:--tall={BAR[hour.verdict]}
				title={hour.why || 'Flyable'}
			></span>
			{#if climbs}
				<span class="climb">
					{hour.climbMs === undefined ? '–' : text.forecastClimb(hour.climbMs)}
				</span>
				<span class="bar thermal t{thermalStep(hour.climbMs ?? 0)}"></span>
			{/if}
			<span class="visually-hidden">{spoken(hour)}</span>
		</li>
	{/each}
</ol>

<style>
	.hours {
		display: grid;
		grid-auto-flow: column;
		grid-auto-columns: minmax(2rem, 1fr);
		gap: 0.125rem;
		overflow-x: auto;
		text-align: center;
		font-size: var(--subheadline);
	}

	.hours li {
		display: grid;
		justify-items: center;
		gap: 0.3125rem;
	}

	.hour {
		color: var(--text-secondary);
	}

	.arrow {
		display: grid;
		place-items: center;
		width: 1.125rem;
		height: 1.125rem;
	}

	.arrow :global(svg) {
		width: 1.125rem;
		height: 1.125rem;
	}

	.speed {
		font-weight: 600;
	}

	/* The hour's status by the bar's height as well as its colour. */
	.bar {
		align-self: end;
		width: 100%;
		height: calc(1.5rem * var(--tall));
		margin-top: calc(1.5rem * (1 - var(--tall)));
		border-radius: 0.25rem;
	}

	.bar.flyable {
		background: var(--color-good);
	}

	.bar.marginal {
		background: var(--color-caution);
	}

	.bar.notFlyable {
		background: var(--color-danger);
	}

	.climb {
		margin-top: var(--space-1);
		font-weight: 600;
	}

	/* The climb's bar: taller for a stronger step of the thermal scale, in its colour. */
	.bar.thermal {
		border-radius: 0.125rem;
	}

	.t1 {
		--tall: 0.17;
		background: var(--thermal-1);
	}

	.t2 {
		--tall: 0.33;
		background: var(--thermal-2);
	}

	.t3 {
		--tall: 0.5;
		background: var(--thermal-3);
	}

	.t4 {
		--tall: 0.67;
		background: var(--thermal-4);
	}

	.t5 {
		--tall: 0.83;
		background: var(--thermal-5);
	}

	.t6 {
		--tall: 1;
		background: var(--thermal-6);
	}
</style>
