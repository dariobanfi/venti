<!--
	One flying site: what its source says about it, and what that is worth.
	The words are those of the iPhone app's `SiteScreens.swift`. The iPhone
	can also keep a site as a waypoint for its cockpit; a browser has neither.
-->
<script lang="ts">
	import type { Credit, SiteOutlook, SiteView } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import StatusChip from '#lib/today/StatusChip.svelte';
	import { asOf, CAUTION, forecastLine, marks, siteLine, withLinks } from './words';
	import WindRose from './WindRose.svelte';

	interface Props {
		site: SiteView;
		/** Whose data the site is, with what the licence asks to be shown. */
		credit: Credit | undefined;
		/** The date of the source's data, seconds since 1970-01-01 UTC. */
		asOfUnixS: number | undefined;
		/** Opens the forecast for the site's place. */
		onforecast: () => void;
		/**
		 * The page of the flights that took off from the site, where the page
		 * shows the site alone. Nothing for a site that is only a landing.
		 */
		flightsHref?: string;
		/**
		 * What the forecast says about flying from the site, for a place to
		 * take off from, once Venti has answered.
		 */
		outlook?: SiteOutlook;
	}

	let { site, credit, asOfUnixS, onforecast, flightsHref, outlook }: Props = $props();

	/** The first day that still has flying hours: the one the forecast's line is about. */
	const day = $derived(outlook?.days.find((one) => one.hours.length > 0) ?? outlook?.days[0]);

	const text = format();
	const marked = $derived(marks(site));
</script>

<div class="site">
	<!-- The pack's site sheet: its status, the site in a line, the forecast's line. -->
	<div class="head">
		{#if day}
			<StatusChip verdict={day.verdict} status={day.status} />
		{/if}
		<p>{siteLine(site, text)}</p>
		{#if day}
			<p class="lesser">{forecastLine(day)}</p>
		{/if}
	</div>
	<dl class="card">
		<div class="row">
			<dt>What it is</dt>
			<dd>{site.kindInWords}</dd>
		</div>
		{#if site.elevationM !== undefined}
			<div class="row">
				<dt>Height</dt>
				<dd class="numeric">{text.metres(site.elevationM)} m</dd>
			</div>
		{/if}
		<div class="row">
			<dt>Wind it works in</dt>
			{#if site.wind.length === 0}
				<dd>Not given</dd>
			{:else}
				<dd class="wind">
					{site.windInWords}
					<WindRose spans={site.wind} />
				</dd>
			{/if}
		</div>
	</dl>

	{#if marked.length > 0}
		<ul class="card" aria-label="What its source marks it as">
			{#each marked as mark (mark.says)}
				<li class="row mark" class:warns={mark.warns}>
					<Icon name={mark.icon} />
					{mark.says}
				</li>
			{/each}
		</ul>
	{/if}

	<div class="card">
		<button class="row link" type="button" onclick={onforecast}>Forecast for this place</button>
		{#if flightsHref}
			<a class="row link" href={flightsHref}>Flights from here</a>
		{/if}
	</div>

	<section>
		{#if site.url}
			<div class="card">
				<a class="row link" href={site.url} target="_blank" rel="noopener noreferrer">
					See or correct it at the source
				</a>
			</div>
		{/if}
		<p class="footnote">{CAUTION}</p>
		{#if credit?.notice}
			<p class="footnote">
				{#each withLinks(credit.notice) as piece, index (index)}
					{#if piece.href}
						<a href={piece.href} target="_blank" rel="noopener noreferrer">{piece.text}</a>
					{:else}
						{piece.text}
					{/if}
				{/each}
			</p>
		{/if}
		{#if credit}
			<p class="footnote">
				<a href={credit.url} target="_blank" rel="noopener noreferrer">{credit.text}</a>
			</p>
		{/if}
		{#if asOfUnixS !== undefined}
			<p class="footnote">{asOf(asOfUnixS, text)}</p>
		{/if}
	</section>
</div>

<style>
	.head {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: var(--space-1);
	}

	.lesser {
		color: var(--text-secondary);
	}

	.site {
		display: flex;
		flex-direction: column;
		gap: 1.5rem;
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

	.wind {
		display: flex;
		align-items: center;
		gap: 0.625rem;
	}

	.mark {
		justify-content: flex-start;
		gap: 0.75rem;
	}

	.mark :global(svg) {
		color: var(--text-secondary);
	}

	/* What speaks against flying there is set apart, as a warning is. */
	.warns,
	.warns :global(svg) {
		color: var(--danger);
	}

	.link {
		width: 100%;
		color: var(--link);
		text-align: left;
	}

	@media (hover: hover) {
		.link:hover {
			background: var(--fill);
		}

		.footnote a:hover {
			text-decoration: underline;
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
		overflow-wrap: anywhere;
	}

	.footnote a {
		color: var(--link);
	}
</style>
