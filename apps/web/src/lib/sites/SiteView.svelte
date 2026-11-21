<!--
	One flying site: what its source says about it, and what that is worth.
	The words are those of the iPhone app's `SiteScreens.swift`. The iPhone
	can also keep a site as a waypoint for its cockpit; a browser has neither.

	Under what the page says of the site itself stands what its source says
	beyond that, where it says anything: a description in its mapper's
	words, a sentence each about a fee, rules for guests and a time without
	flying (the core's sentences), and the site's web page. All of it is
	set as text, and the web page is a link that opens outside the app.

	The first row of its actions marks the site as one of the pilot's own,
	and unmarks it: "Keep as my take-off", and "Kept" with a tick once it
	is, in the manner of a plan's "I am in". The core keeps the site in this
	browser and says whether it is kept; Today lists the kept take-offs first.
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import type { Credit, SiteOutlook, SiteView } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import StatusChip from '#lib/today/StatusChip.svelte';
	import AirChart from '#lib/today/AirChart.svelte';
	import ThermalLine from '#lib/today/ThermalLine.svelte';
	import { asOf, CAUTION, forecastLine, marks, said, siteLine, webPage, withLinks } from './words';
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
	const sentences = $derived(said(site));
	const web = $derived(webPage(site.website));

	const own = $derived(getApp().kept);
	/** Whether the site is one of the pilot's own, by the core. Unknown until it has answered. */
	let kept = $state<boolean>();
	let keeping = $state(false);
	let keepProblem = $state<string>();

	$effect(() => {
		const id = site.id;
		// Asked again after every change, in this tab or another.
		void own?.revision;
		let current = true;
		own?.isKept(id).then(
			(is) => {
				if (current) kept = is;
			},
			() => {}
		);
		return () => {
			current = false;
		};
	});

	/** Keeps the site, with the credit and the date of the answer it came from, or forgets it. */
	async function keepOrForget() {
		if (!own || kept === undefined) return;
		keeping = true;
		keepProblem = undefined;
		try {
			if (kept) await own.forget(site.id);
			else
				await own.keep($state.snapshot(site), credit ? [$state.snapshot(credit)] : [], asOfUnixS);
		} catch (error) {
			keepProblem = error instanceof Error ? error.message : String(error);
		} finally {
			keeping = false;
		}
	}
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
			{#if day.thermals}
				<ThermalLine {day} />
			{/if}
		{/if}
	</div>
	{#if day && day.hours.length > 0 && day.groundM !== undefined}
		<section class="card chart" aria-label="The air through the day">
			<h3>The air through the day</h3>
			<AirChart {day} takeoffM={site.elevationM} />
		</section>
	{/if}

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
		<button
			class="row link"
			type="button"
			aria-pressed={kept ?? false}
			disabled={keeping || kept === undefined}
			onclick={keepOrForget}
		>
			{kept ? 'Kept' : 'Keep as my take-off'}
			{#if kept}<Icon name="check" />{/if}
		</button>
		{#if keepProblem}
			<p class="row said" role="alert">{keepProblem}</p>
		{/if}
		<button class="row link" type="button" onclick={onforecast}>Forecast for this place</button>
		{#if flightsHref}
			<a class="row link" href={flightsHref}>Flights from here</a>
		{/if}
	</div>

	{#if site.description || sentences.length > 0 || web}
		<section class="card" aria-label="What its source says about it">
			{#if site.description}
				<p class="row said description">{site.description}</p>
			{/if}
			{#each sentences as sentence (sentence)}
				<p class="row said">{sentence}</p>
			{/each}
			{#if web}
				<a class="row link" href={web.href} target="_blank" rel="noopener noreferrer">
					Web page
					<span class="host">{web.host}</span>
				</a>
			{/if}
		</section>
	{/if}

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

	.chart {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-3) var(--space-4) var(--space-4);
	}

	.chart h3 {
		font-size: var(--subheadline);
		font-weight: 600;
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

	/* The source's own words: text that may run over lines, never markup. */
	.said {
		justify-content: flex-start;
		overflow-wrap: anywhere;
	}

	.description {
		white-space: pre-line;
	}

	.host {
		color: var(--text-secondary);
		text-align: right;
		overflow-wrap: anywhere;
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
