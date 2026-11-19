<!--
	One number with its unit and what it measures: the building block of the
	logbook totals and the flight panel.

	The value is large, its unit small and grey beside it, what it measures a
	caption below. A screen reader hears them as one sentence: "Airtime, 0:29 h".
-->
<script lang="ts">
	interface Props {
		caption: string;
		value: string;
		unit?: string;
	}

	let { caption, value, unit = '' }: Props = $props();
</script>

<div class="metric">
	<dt>{caption}</dt>
	<dd class="numeric">
		<span class="value">{value}</span>{#if unit}<span class="unit">&nbsp;{unit}</span>{/if}
	</dd>
</div>

<style>
	.metric {
		/* The caption comes first in the markup, as the name of the value, and
		   below it on screen. */
		display: flex;
		flex-direction: column-reverse;
		gap: 0.0625rem;
		min-width: 0;
		flex: 1 1 0;
	}

	dt {
		font-size: var(--caption);
		color: var(--metric-secondary, var(--text-secondary));
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	dd {
		white-space: nowrap;
		line-height: 1.2;
	}

	.value {
		font-size: var(--number);
		font-weight: 600;
		letter-spacing: -0.01em;
	}

	.unit {
		font-size: var(--footnote);
		font-weight: 500;
		color: var(--metric-secondary, var(--text-secondary));
	}
</style>
