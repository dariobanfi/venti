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

	const app = getApp();
	const logbook = $derived(app.logbook);

	let picker: HTMLInputElement;
	let flightToDelete = $state.raw<LogbookEntry>();

	/** The files the pilot chose. The core says which of them are flights. */
	function picked() {
		const files = [...(picker.files ?? [])];
		// Cleared so that choosing the same file again is noticed.
		picker.value = '';
		void app.importFiles(files);
	}

	async function deleteFlight(entry: LogbookEntry) {
		flightToDelete = undefined;
		try {
			await app.deleteFlight(entry.id);
		} catch (error) {
			app.notice = { title: 'The flight could not be deleted', lines: [reasonFor(error)] };
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
		<LogbookList view={logbook.view} ondelete={(entry) => (flightToDelete = entry)} />
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
	open={flightToDelete !== undefined}
	title="Delete this flight?"
	message={deletionWarning(app.account?.signedIn ?? false)}
	confirm="Delete flight"
	onconfirm={() => flightToDelete && deleteFlight(flightToDelete)}
	oncancel={() => (flightToDelete = undefined)}
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
	@media (min-width: 56rem) {
		.page {
			padding-top: 2rem;
		}

		.import {
			display: inline-flex;
			gap: 0.375rem;
			width: auto;
			padding: 0 1.125rem 0 0.875rem;
			border-radius: 999px;
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
