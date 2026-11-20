<!--
	One flight of the logbook. Which one is in the address: `?id=`.

	The page asks the core for the flight, and shows it, or says why it cannot.
-->
<script lang="ts">
	import { MediaQuery } from 'svelte/reactivity';
	import { browser } from '$app/env';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import ShareFlightSheet from '#lib/account/ShareFlightSheet.svelte';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { logbookHref } from '#lib/app/routes.ts';
	import type { FlightAnalysis, FlightView } from '#lib/core/index.ts';
	import ConfirmDialog from '#lib/design/ConfirmDialog.svelte';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import type { MenuItem } from '#lib/design/Menu.svelte';
	import FlightScreen from '#lib/flight/FlightScreen.svelte';
	import { saveFile } from '#lib/logbook/flight-file.ts';
	import { deletionWarning } from '#lib/logbook/import-report.ts';
	import { reasonFor } from '#lib/logbook/model.svelte.ts';

	const app = getApp();

	/** Wide enough for the panel to stand beside the map: the width at which the navigation moves to the side. */
	const wide = new MediaQuery('min-width: 56rem', false);

	/** The flight asked for. The address is not known while the pages are written out at build time. */
	const id = $derived(browser ? page.url.searchParams.get('id') : null);

	type Phase =
		{ is: 'loading' } | { is: 'loaded'; flight: FlightView } | { is: 'failed'; message: string };

	let phase = $state.raw<Phase>({ is: 'loading' });
	/** What happened in the flight. It is worked out after the flight is on screen. */
	let analysis = $state.raw<FlightAnalysis>();
	let confirmingDelete = $state(false);
	/** Whether the sheet that says who can see the flight is open. */
	let sharing = $state(false);

	/** What can be done with the flight besides looking at it. */
	const actions: MenuItem[] = [
		{ label: 'Share with a link', icon: 'link', onselect: () => (sharing = true) },
		{ label: 'Download flight file', icon: 'download', onselect: () => void downloadFlight() },
		{
			label: 'Delete flight',
			icon: 'trash',
			destructive: true,
			onselect: () => (confirmingDelete = true)
		}
	];

	$effect(() => {
		if (app.failure) {
			phase = { is: 'failed', message: app.failure };
			return;
		}
		const logbook = app.logbook;
		if (!logbook) return;
		if (!id) {
			phase = { is: 'failed', message: 'This address names no flight.' };
			return;
		}

		// Only the answer to the latest question counts.
		let current = true;
		phase = { is: 'loading' };
		analysis = undefined;
		logbook.flight(id).then(
			(flight) => {
				if (!current) return;
				phase = { is: 'loaded', flight };
				// Without the analysis the details are shorter, not wrong.
				logbook.analysis(flight.id).then(
					(found) => {
						if (current) analysis = found;
					},
					() => {}
				);
			},
			(error) => {
				if (current) phase = { is: 'failed', message: reasonFor(error) };
			}
		);
		return () => {
			current = false;
		};
	});

	/** Saves the flight's file, as it was imported, where the browser keeps downloads. */
	async function downloadFlight() {
		if (!id || !app.logbook) return;
		try {
			const file = await app.logbook.flightFile(id);
			saveFile(file.name, file.igcText);
		} catch (error) {
			app.notice = { title: 'The flight file could not be saved', lines: [reasonFor(error)] };
		}
	}

	async function deleteFlight() {
		confirmingDelete = false;
		if (!id || !app.logbook) return;
		try {
			await app.deleteFlight(id);
			await goto(logbookHref());
		} catch (error) {
			phase = { is: 'failed', message: reasonFor(error) };
		}
	}

	const title = $derived(
		phase.is === 'loaded' ? `${format().day(phase.flight.startUnixS)} · Venti` : 'Flight · Venti'
	);
</script>

<svelte:head>
	<title>{title}</title>
</svelte:head>

{#if phase.is === 'loaded'}
	{#key phase.flight.id}
		<FlightScreen
			flight={phase.flight}
			{analysis}
			wide={wide.current}
			back={{ href: logbookHref(), label: 'Logbook' }}
			{actions}
			inLogbook
		/>
		<ShareFlightSheet open={sharing} flight={phase.flight.id} onclose={() => (sharing = false)} />
	{/key}
{:else if phase.is === 'failed'}
	<div class="unavailable">
		<span class="symbol"><Icon name="warning" /></span>
		<h1>This flight could not be opened</h1>
		<p>{phase.message}</p>
		<a class="button-plain" href={logbookHref()}>Back to the logbook</a>
	</div>
{:else}
	<p class="visually-hidden" role="status">Opening the flight…</p>
{/if}

<ConfirmDialog
	open={confirmingDelete}
	title="Delete this flight?"
	message={deletionWarning(app.account?.signedIn ?? false)}
	confirm="Delete flight"
	onconfirm={deleteFlight}
	oncancel={() => (confirmingDelete = false)}
/>

<style>
	.unavailable {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 0.375rem;
		min-height: 100dvh;
		max-width: 24rem;
		margin: 0 auto;
		padding: 2rem var(--gutter);
		text-align: center;
	}

	.symbol {
		color: var(--text-secondary);
		font-size: 2.25rem;
		margin-bottom: 0.5rem;
	}

	h1 {
		font-size: var(--title);
		font-weight: 700;
	}

	p {
		color: var(--text-secondary);
	}

	.button-plain {
		margin-top: 0.75rem;
	}
</style>
