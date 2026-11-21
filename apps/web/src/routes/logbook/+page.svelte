<!--
	The logbook: the pilot's flights by year, a way to bring more in, and, with
	none yet, what to do next.
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import { queryOf } from '#lib/community/pages.ts';
	import FrontHead from '#lib/community/FrontHead.svelte';
	import WhoseFlights from '#lib/community/WhoseFlights.svelte';
	import { dayOf } from '#lib/groups/days.ts';
	import type { LogbookEntry } from '#lib/core/index.ts';
	import ConfirmDialog from '#lib/design/ConfirmDialog.svelte';
	import Icon from '#lib/design/Icon.svelte';
	import { deletionWarning } from '#lib/logbook/import-report.ts';
	import LogbookList from '#lib/logbook/LogbookList.svelte';
	import { reasonFor } from '#lib/logbook/model.svelte.ts';
	import LogbookTable from '#lib/logbook/LogbookTable.svelte';
	import { saveFile } from '#lib/logbook/flight-file.ts';
	import { format } from '#lib/design/format.ts';
	import { statisticsHref } from '#lib/app/routes.ts';
	import { MediaQuery } from 'svelte/reactivity';

	const text = format();

	const app = getApp();
	const logbook = $derived(app.logbook);

	let picker: HTMLInputElement;
	/** The flights the pilot asked to delete, while the question is open. */
	let toDelete = $state.raw<LogbookEntry[]>([]);

	/** On a wide screen the logbook is a table, with its own way to choose several flights. */
	const wide = new MediaQuery('min-width: 56rem', false);

	/** The files the pilot chose. The core says which of them are flights. */
	function picked() {
		const files = [...(picker.files ?? [])];
		// Cleared so that choosing the same file again is noticed.
		picker.value = '';
		void app.importFiles(files);
	}

	async function deleteFlights(entries: LogbookEntry[]) {
		toDelete = [];
		for (const entry of entries) {
			try {
				await app.deleteFlight(entry.id);
			} catch (error) {
				app.notice = { title: 'The flight could not be deleted', lines: [reasonFor(error)] };
				return;
			}
		}
	}

	/** Saves the chosen flights' files, as they were imported, where the browser keeps downloads. */
	async function downloadFlights(entries: LogbookEntry[]) {
		if (!logbook) return;
		for (const entry of entries) {
			try {
				const file = await logbook.flightFile(entry.id);
				saveFile(file.name, file.igcText);
			} catch (error) {
				app.notice = { title: 'The flight file could not be saved', lines: [reasonFor(error)] };
				return;
			}
		}
	}

	// With an empty logbook, what others fly is shown under it.
	$effect(() => {
		if (app.account?.hasServer && logbook && logbook.view.years.length === 0) {
			app.everyone?.showFront(dayOf(new Date()));
		}
	});
</script>

<svelte:head>
	<title>Logbook · Venti</title>
</svelte:head>

<div class="page">
	<header>
		<h1>Logbook</h1>
		{#if logbook}
			<button class="import" type="button" onclick={() => picker.click()}>
				<Icon name="plus" />
				<span>Import flights</span>
			</button>
		{/if}
	</header>

	<!-- With a Venti server behind the site, the logbook has a second side:
	     the flights everyone on that server shares. -->
	{#if app.account?.hasServer}
		<div class="whose">
			<WhoseFlights current="mine" query={app.everyone ? queryOf(app.everyone.asked) : ''} />
		</div>
	{/if}

	{#if logbook?.progress}
		<p class="status" role="status">
			Importing {Math.min(logbook.progress.done + 1, logbook.progress.of)} of {logbook.progress.of}…
		</p>
	{/if}

	{#if app.failure}
		<div class="unavailable">
			<span class="symbol"><Icon name="warning" /></span>
			<h2>Venti could not open its data</h2>
			<p>{app.failure}</p>
		</div>
	{:else if logbook && logbook.view.years.length === 0}
		<div class="unavailable">
			<span class="symbol"><Icon name="book" /></span>
			<h2>No flights yet</h2>
			<p>
				Import the IGC files your vario or flight app recorded.
				<span class="with-mouse">Or drag them onto this page.</span>
				They stay in this browser.
			</p>
			<div class="actions">
				<button class="button-prominent" type="button" onclick={() => picker.click()}>
					Import flights
				</button>
				<button class="button-plain" type="button" onclick={() => app.addSample()}>
					Add a sample flight
				</button>
			</div>
		</div>
		<!-- Without flights of one's own: what others fly on Venti, and the way to it. -->
		{#if app.account?.hasServer && app.everyone?.front}
			<div class="others">
				<FrontHead front={app.everyone.front} />
			</div>
		{/if}
	{:else if logbook}
		{#if !logbook.durable}
			<p class="warning" role="note">
				This browser is not letting Venti keep anything, as some do in private windows. The flights
				you import are gone when you close this page.
			</p>
		{/if}
		{#if wide.current}
			<!-- The totals in a line over the table, and the way to the statistics. -->
			<div class="totals">
				<dl class="numeric" aria-label="All flights">
					<div>
						<dt>{logbook.view.totals.flights === 1 ? 'Flight' : 'Flights'}</dt>
						<dd>{text.count(logbook.view.totals.flights)}</dd>
					</div>
					<div>
						<dt>Airtime</dt>
						<dd>{text.hoursMinutes(logbook.view.totals.airtimeS)} h</dd>
					</div>
					<div>
						<dt>Distance</dt>
						<dd>{text.wholeKilometres(logbook.view.totals.distanceM)} km</dd>
					</div>
				</dl>
				<a href={statisticsHref()}>Statistics</a>
			</div>
			<LogbookTable
				view={logbook.view}
				ondelete={(entries) => (toDelete = entries)}
				ondownload={(entries) => void downloadFlights(entries)}
			/>
		{:else}
			<LogbookList view={logbook.view} ondelete={(entry) => (toDelete = [entry])} />
		{/if}
	{/if}
</div>

<!-- Flight files have no type a browser knows, so any file can be picked. -->
<input
	bind:this={picker}
	class="visually-hidden"
	type="file"
	multiple
	tabindex="-1"
	aria-hidden="true"
	onchange={picked}
/>

<ConfirmDialog
	open={toDelete.length > 0}
	title={toDelete.length === 1 ? 'Delete this flight?' : `Delete ${toDelete.length} flights?`}
	message={deletionWarning(app.account?.signedIn ?? false, toDelete.length)}
	confirm={toDelete.length === 1 ? 'Delete flight' : `Delete ${toDelete.length} flights`}
	onconfirm={() => deleteFlights(toDelete)}
	oncancel={() => (toDelete = [])}
/>

<style>
	.others {
		margin-top: 2rem;
	}

	.page {
		display: flex;
		flex-direction: column;
		min-height: 100dvh;
		max-width: 60rem;
		margin: 0 auto;
		padding: calc(1rem + var(--safe-top)) calc(var(--gutter) + var(--safe-right))
			calc(1.5rem + var(--nav-bottom)) calc(var(--gutter) + var(--safe-left));
	}

	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		min-height: 3.25rem;
		margin-bottom: 0.75rem;
	}

	h1 {
		font-size: var(--large-title);
		font-weight: 600;
	}

	.whose {
		margin-bottom: 1rem;
	}

	/* Narrow: a round button with a plus, as in the iPhone's toolbar. */
	.import {
		display: grid;
		place-items: center;
		width: var(--target);
		height: var(--target);
		border-radius: 50%;
		background: var(--surface);
		color: var(--link);
		box-shadow: var(--shadow-float);
	}

	.import span {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip-path: inset(50%);
		white-space: nowrap;
	}

	.status {
		margin-bottom: 0.75rem;
		font-size: var(--subheadline);
		color: var(--text-secondary);
	}

	.warning {
		margin-bottom: 1rem;
		padding: 0.75rem 1rem;
		border-radius: var(--radius-control);
		background: var(--fill);
		font-size: var(--footnote);
	}

	/* A screen with nothing on it says so, and what to do next. */
	.unavailable {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 0.375rem;
		max-width: 24rem;
		margin: 0 auto;
		padding-bottom: 3rem;
		text-align: center;
	}

	.symbol {
		color: var(--text-secondary);
		font-size: 2.25rem;
		margin-bottom: 0.5rem;
	}

	.unavailable h2 {
		font-size: var(--title);
		font-weight: 600;
	}

	.unavailable p {
		color: var(--text-secondary);
	}

	.with-mouse {
		display: none;
	}

	.actions {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.5rem;
		margin-top: 1rem;
	}

	@media (hover: hover) and (pointer: fine) {
		.with-mouse {
			display: inline;
		}
	}

	/* Wide: the button has room to say what it does. */
	.totals {
		display: flex;
		align-items: baseline;
		gap: var(--space-5);
		margin-bottom: var(--space-3);
	}

	.totals dl {
		display: flex;
		gap: var(--space-5);
	}

	/* A figure with its caption before it, in a line. */
	.totals dl div {
		display: flex;
		align-items: baseline;
		gap: var(--space-2);
	}

	.totals dt {
		color: var(--text-secondary);
	}

	.totals dd {
		font-weight: 600;
	}

	.totals a {
		color: var(--link);
	}

	@media (min-width: 56rem) {
		.page {
			max-width: 80rem;
			padding-top: var(--space-4);
		}

		header {
			min-height: 0;
			margin-bottom: var(--space-2);
		}

		.whose {
			margin-bottom: var(--space-3);
		}

		.import {
			display: inline-flex;
			gap: 0.375rem;
			width: auto;
			height: var(--target);
			padding: 0 var(--space-3) 0 var(--space-2);
			border-radius: var(--radius-s);
			background: var(--accent);
			color: var(--on-accent);
			font-weight: 600;
			box-shadow: none;
		}

		.import span {
			position: static;
			width: auto;
			height: auto;
			clip-path: none;
		}
	}
</style>
