<!--
	One flying site: what its source says about it, and what that is worth.
	The words are those of the iPhone app's `SiteScreens.swift`. The iPhone
	can also keep a site as a waypoint for its cockpit; a browser has neither.

	Under what the page says of the site itself stands what is said beyond
	that, where anything is, under the core's heading for it
	(`descriptionTitle`: "What its source says", "What the partner wrote",
	"What pilots wrote"): a description in its writer's words, a sentence each about a fee, rules for guests and a time without
	flying (the core's sentences), and the site's web page. All of it is
	set as text, and the web page is a link that opens outside the app.

	The first row of its actions marks the site as one of the pilot's own,
	and unmarks it: "Keep as my take-off", and "Kept" with a tick once it
	is, in the manner of a plan's "I am in". The core keeps the site in this
	browser and says whether it is kept; Today lists the kept take-offs first.

	A site of Venti's own two tiers (`byPilots`) says its tier in the core's
	label ("From pilots", "From a partner"), who added it and who changed it
	last, in the core's words; a crowdsourced one has the core's notice above
	its description. What may be done about it is the core's one answer
	(`Core.siteOffers`), asked for this site and whoever is signed in: to
	correct it, remove it, read its history, report it, and to say "I took
	off here and it was good". Only what is offered is shown, in the core's
	words, and its one note says why something is missing. Under the facts
	stands how many pilots confirmed it, as the core's line, which opens
	their list (while it is read "Loading…", and where it cannot be read
	the reason with "Try again"; under a list that is not whole the core's
	words say so), or the core's sentence that nobody has yet, where it has
	one for this kind of site; and once this pilot has, the core's sentence
	in the button's place, with changing the day and taking it back. What the server
	answered to the pilot's last change stands at the top, in the core's line.

	Under the confirmations, for a take-off of any layer, what the public
	flights that began there say about it (`SiteFlights.svelte`).
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import AlertMe from '#lib/alerts/AlertMe.svelte';
	import { dayDraft } from '#lib/alerts/model.svelte.ts';
	import { getApp } from '#lib/app/app.svelte.ts';
	import type {
		Credit,
		SiteConfirmations,
		SiteOffers,
		SiteOutlook,
		SiteView,
		SiteWritten
	} from '#lib/core/index.ts';
	import ConfirmDialog from '#lib/design/ConfirmDialog.svelte';
	import FoldRow from '#lib/design/FoldRow.svelte';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import Sheet from '#lib/design/Sheet.svelte';
	import StatusChip from '#lib/today/StatusChip.svelte';
	import AirChart from '#lib/today/AirChart.svelte';
	import NearbyLine from '#lib/today/NearbyLine.svelte';
	import ThermalLine from '#lib/today/ThermalLine.svelte';
	import { said as whyNot, type SiteEditsModel } from './edit.svelte';
	import SiteConfirmSheet from './SiteConfirmSheet.svelte';
	import SiteFlights from './SiteFlights.svelte';
	import { flownFrom } from './mark';
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
		 * "Plan route": the map's route planner, with the site as the first
		 * place. Only where the page lies over the map.
		 */
		onplan?: () => void;
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
		/** What became of the pilot's last change to the site, in the core's line. */
		written?: string;
		/**
		 * Where a site can be written and confirmed: the core is asked what
		 * may be offered for this one, and the page does what is chosen.
		 */
		own?: {
			edits: SiteEditsModel;
			oncorrect: () => void;
			onremove: () => void;
			onhistory: () => void;
			onreport: () => void;
			/** The site as the server has it after a confirmation or its taking back, with the core's line. */
			onchanged: (written: SiteWritten) => void;
		};
	}

	let {
		site,
		credit,
		asOfUnixS,
		onforecast,
		onplan,
		flightsHref,
		outlook,
		written,
		own: can
	}: Props = $props();

	const edits = $derived(can?.edits);
	const words = $derived(edits?.words);

	/** Who says they took off here: asked for a site of Venti's own tiers. */
	let confirmations = $state.raw<SiteConfirmations>();
	/** Why the list could not be read, in the words it came with. */
	let confirmationsProblem = $state.raw<string>();
	/**
	 * Counts the changes of the pilot's own confirmation and the pilot's
	 * "Try again", after which the list is read again.
	 */
	let confirmed = $state(0);
	$effect(() => {
		const id = site.id;
		void confirmed;
		// Another site's list is let go of at once.
		if (untrack(() => confirmations)?.siteId !== id) confirmations = undefined;
		confirmationsProblem = undefined;
		if (!edits || !site.byPilots) return;
		// Read for the site as it is used: the core says by it what stands while nobody has.
		const here = untrack(() => $state.snapshot(site) as SiteView);
		let current = true;
		edits.confirmations(here).then(
			(list) => {
				if (current) confirmations = list;
			},
			(error) => {
				if (current) confirmationsProblem = whyNot(error);
			}
		);
		return () => {
			current = false;
		};
	});

	/** What the core says may be offered for this site, to whoever is signed in. */
	let offers = $state.raw<SiteOffers>();
	$effect(() => {
		// Asked again when the account changes, and with the list of confirmations.
		void edits?.asked;
		const list = confirmations;
		const here = $state.snapshot(site) as SiteView;
		if (!edits) {
			offers = undefined;
			return;
		}
		let current = true;
		edits.offers(here, list).then(
			(answer) => {
				if (current) offers = answer;
			},
			() => {}
		);
		return () => {
			current = false;
		};
	});

	/** Whether the form of "I took off here" is open, and the list of who did. */
	let confirming = $state(false);
	let listing = $state(false);
	/** Whether the pilot is asked before the confirmation is taken back. */
	let withdrawing = $state(false);
	let withdrawProblem = $state.raw<string>();

	function wasConfirmed(now: SiteWritten) {
		confirming = false;
		confirmed += 1;
		can?.onchanged(now);
	}

	async function withdraw() {
		withdrawing = false;
		withdrawProblem = undefined;
		if (!edits) return;
		try {
			const line = await edits.withdraw(site.id);
			confirmed += 1;
			// The site's own count is the server's: read as it stands now.
			const page = await edits.page(site.id);
			can?.onchanged({ site: page.site, words: line });
		} catch (error) {
			withdrawProblem = whyNot(error);
		}
	}

	/**
	 * How many pilots took off here, as the core's line; empty while nobody
	 * has said so. Until the list is read, the site's own count.
	 */
	const confirmedLine = $derived(
		confirmations?.siteId === site.id
			? confirmations.words
			: (site.byPilots?.confirmedInWords ?? '')
	);
	/** The core's sentence while nobody has; empty where it has none for this kind of site. */
	const nobodyLine = $derived(confirmations?.siteId === site.id ? confirmations.nobodyWords : '');

	/** The first day that still has flying hours: the one the forecast's line is about. */
	const day = $derived(outlook?.days.find((one) => one.hours.length > 0) ?? outlook?.days[0]);
	/** Whether the page draws the air through the day. */
	const showsAir = $derived(Boolean(day && day.hours.length > 0 && day.groundM !== undefined));

	const text = format();
	const marked = $derived(marks(site));
	const sentences = $derived(said(site));
	const web = $derived(webPage(site.website));

	const app = getApp();
	const own = $derived(app.kept);
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

	/**
	 * Who flies at the site now, in the core's line: "5 pilots in the air at
	 * Wallberg, 2 took off in the last hour". Empty when nobody does, and
	 * then nothing is shown.
	 */
	let inTheAir = $state('');
	$effect(() => {
		const asked = {
			id: site.id,
			// A site its source does not name is "Take-off" on the page: the
			// line then names no place.
			name: site.isNamed ? site.name : '',
			lat: site.position.lat,
			lon: site.position.lon
		};
		inTheAir = '';
		let current = true;
		app.siteAir(asked).then(
			(line) => {
				if (current) inTheAir = line;
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
	{#if written}
		<p class="card row said" role="status">{written}</p>
	{/if}
	<!-- The pack's site sheet: its status, the site in a line, the forecast's line. -->
	<div class="head">
		{#if day}
			<StatusChip verdict={day.verdict} status={day.status} />
		{/if}
		<p>{siteLine(site, text)}</p>
		{#if inTheAir}
			<p class="in-the-air numeric" data-site-air>{inTheAir}</p>
		{/if}
		{#if site.byPilots?.label}
			<!-- The tier, by the name: "From pilots", "From a partner". -->
			<p class="lesser tier">{site.byPilots.label}</p>
		{/if}
		{#if day}
			<p class="lesser">{forecastLine(day)}</p>
			<!-- Much stronger wind near by: a note under the day's reason, and no verdict. -->
			<NearbyLine words={day.nearby} />
			{#if day.thermals}
				<ThermalLine {day} />
			{/if}
		{/if}
	</div>
	{#if showsAir && day}
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

	{#if site.byPilots && words && (confirmedLine || nobodyLine || offers?.confirm.isOffered || offers?.hasConfirmed)}
		<section class="card" aria-label={words.confirmationsTitle}>
			{#if confirmedLine}
				<button class="row link" type="button" onclick={() => (listing = true)}>
					{confirmedLine}
				</button>
			{:else if nobodyLine}
				<p class="row said nobody">{nobodyLine}</p>
			{/if}
			{#if offers?.hasConfirmed}
				<p class="row said" role="status">{offers.confirmedWords}</p>
			{/if}
			{#if offers?.confirm.isOffered}
				<button class="row link" type="button" onclick={() => (confirming = true)}>
					{offers.confirm.words}
				</button>
			{/if}
			{#if offers?.withdraw.isOffered}
				<button class="row link" type="button" onclick={() => (withdrawing = true)}>
					{offers.withdraw.words}
				</button>
			{/if}
			{#if withdrawProblem}
				<p class="row said" role="alert">{withdrawProblem}</p>
			{/if}
		</section>
	{/if}

	{#if flownFrom(site)}
		<!-- What the public flights that began here say about it: the core's, for a take-off. -->
		<SiteFlights id={site.id} />
	{/if}

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
		<!-- The phones' order: the forecast, the alarm, keeping; then the web's own. -->
		<button class="row link" type="button" onclick={onforecast}>Forecast for this place</button>
		{#if site.kind === 'takeoff' || site.uses.includes('takeoff')}
			<AlertMe look="row" start={() => dayDraft(site)} />
		{/if}
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
		{#if onplan}
			<!-- The phones have it beside "Start flight", which a browser has not. -->
			<button class="row link" type="button" onclick={onplan}>Plan route</button>
		{/if}
		{#if flightsHref}
			<a class="row link" href={flightsHref}>Flights from here</a>
		{/if}
	</div>

	{#if site.byPilots?.fromPilots}
		<!-- A place from pilots: the core's notice, above what is said about it. -->
		<p class="card row mark notice">
			<Icon name="warning" />
			{site.byPilots.note}
		</p>
	{/if}

	{#if site.description || sentences.length > 0 || web}
		<section class="card" aria-label={site.descriptionTitle || 'What its source says'}>
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

	{#if site.byPilots}
		{@const by = site.byPilots}
		<section aria-label={by.label || 'Added on Venti'}>
			<div class="card">
				<p class="row">
					<span class="who">{by.addedInWords}</span>
					<span class="host when numeric">{text.day(by.addedUnixS)}</span>
				</p>
				{#if by.changedInWords}
					<p class="row">
						<span class="who">{by.changedInWords}</span>
						<span class="host when numeric">{text.day(by.changedUnixS)}</span>
					</p>
				{/if}
				{#if can && offers}
					<!-- What the core offers, in its words; what it does not is not shown. -->
					{#if offers.correct.isOffered}
						<button class="row link" type="button" onclick={can.oncorrect}>
							{offers.correct.words}
						</button>
					{/if}
					{#if offers.remove.isOffered}
						<button class="row link" type="button" onclick={can.onremove}>
							{offers.remove.words}
						</button>
					{/if}
					{#if offers.history.isOffered}
						<button class="row link" type="button" onclick={can.onhistory}>
							{offers.history.words}
						</button>
					{/if}
					{#if offers.report.isOffered}
						<button class="row link" type="button" onclick={can.onreport}>
							{offers.report.words}
						</button>
					{/if}
				{/if}
			</div>
			{#if offers?.note}
				<p class="footnote">{offers.note}</p>
			{/if}
			{#if !by.fromPilots}
				<p class="footnote">{by.note}</p>
			{/if}
		</section>
	{/if}

	{#if site.url || (!site.byPilots && offers?.note)}
		<section>
			{#if site.url}
				<div class="card">
					<a class="row link" href={site.url} target="_blank" rel="noopener noreferrer">
						See or correct it at the source
					</a>
				</div>
			{/if}
			{#if !site.byPilots && offers?.note}
				<p class="footnote">{offers.note}</p>
			{/if}
		</section>
	{/if}

	<!-- How the page is read, what a site on a map is worth, whose data it
	     is and of when: behind a row at the end, closed from the start, as
	     on the phones. -->
	<FoldRow title={app.said?.siteSheet.howReadTitle ?? ''}>
		<div class="how-read">
			{#if showsAir && outlook?.layersNote}
				<p>{outlook.layersNote}</p>
			{/if}
			{#if (showsAir || day?.nearby) && outlook?.nearbyNote}
				<p>{outlook.nearbyNote}</p>
			{/if}
			<p>{CAUTION}</p>
			{#if credit?.notice}
				<p>
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
				<p>
					<a href={credit.url} target="_blank" rel="noopener noreferrer">{credit.text}</a>
				</p>
			{/if}
			{#if asOfUnixS !== undefined}
				<p>{asOf(asOfUnixS, text)}</p>
			{/if}
		</div>
	</FoldRow>
</div>

{#if site.byPilots && edits && words}
	<!-- Who says they took off here, the newest first, each in the core's line. -->
	<Sheet open={listing} title={words.confirmationsTitle} onclose={() => (listing = false)}>
		{#if confirmations}
			{#if confirmations.words || confirmations.nobodyWords}
				<p class="footnote count">{confirmations.words || confirmations.nobodyWords}</p>
			{/if}
			{#if confirmations.lines.length > 0}
				<ul class="card lines" aria-label={words.confirmationsTitle}>
					{#each confirmations.lines as line (line.by.handle + line.day)}
						<li class="row said">{line.words}</li>
					{/each}
				</ul>
			{/if}
			{#if confirmations.notWholeWords}
				<p class="footnote">{confirmations.notWholeWords}</p>
			{/if}
		{:else if confirmationsProblem}
			<!-- The list could not be read: why, and the way to ask again. -->
			<div class="card">
				<p class="row said" role="alert">{confirmationsProblem}</p>
				<button class="row link" type="button" onclick={() => (confirmed += 1)}
					>{app.said?.common.tryAgain}</button
				>
			</div>
		{:else}
			<p class="waiting" role="status">Loading…</p>
		{/if}
	</Sheet>

	<SiteConfirmSheet
		open={confirming}
		{site}
		{edits}
		onconfirmed={wasConfirmed}
		onclose={() => (confirming = false)}
	/>

	<ConfirmDialog
		open={withdrawing}
		title={words.withdrawQuestion}
		message={offers?.confirmedWords ?? ''}
		confirm={words.withdraw}
		cancel={words.cancel}
		onconfirm={() => void withdraw()}
		oncancel={() => (withdrawing = false)}
	/>
{/if}

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

	/* Who flies here now: a line of its own under the site's. */
	.in-the-air {
		font-weight: 500;
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
		gap: var(--space-5);
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
		gap: var(--space-4);
		min-height: var(--target);
		padding: var(--space-2) var(--space-4);
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
		gap: var(--space-3);
	}

	.mark :global(svg) {
		color: var(--text-secondary);
	}

	/* What speaks against flying there is set apart, as a warning is. */
	.warns,
	.warns :global(svg) {
		color: var(--danger);
	}

	/* The notice of a place from pilots: a row of its own, read before the rest. */
	.notice {
		align-items: flex-start;
		padding-block: var(--space-3);
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.notice :global(svg) {
		flex: none;
	}

	.count {
		padding-top: var(--space-1);
		padding-bottom: var(--space-3);
	}

	/* While the list of who took off here is read. */
	.waiting {
		padding: var(--space-4);
		color: var(--text-secondary);
		text-align: center;
	}

	/* Nobody has yet: said, and nothing to open. */
	.nobody {
		color: var(--text-secondary);
	}

	.lines {
		list-style: none;
	}

	/* The source's own words: text that may run over lines, never markup. */
	.said {
		justify-content: flex-start;
		overflow-wrap: anywhere;
	}

	.description {
		white-space: pre-line;
	}

	.who {
		min-width: 0;
		overflow-wrap: anywhere;
	}

	.when {
		flex: none;
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
	}

	.link:focus-visible {
		outline-offset: -0.1875rem;
		border-radius: var(--radius-card);
	}

	.footnote {
		padding: var(--space-2) var(--space-4) 0;
		font-size: var(--footnote);
		color: var(--text-secondary);
		overflow-wrap: anywhere;
	}

	.how-read {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		font-size: var(--footnote);
		color: var(--text-secondary);
		overflow-wrap: anywhere;
	}

	.how-read a {
		color: var(--link);
	}

	@media (hover: hover) {
		.how-read a:hover {
			text-decoration: underline;
		}
	}
</style>
