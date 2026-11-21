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
	import type { FlightAnalysis, FlightRef, FlightView, LeagueHandover } from '#lib/core/index.ts';
	import ConfirmDialog from '#lib/design/ConfirmDialog.svelte';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import type { MenuItem } from '#lib/design/Menu.svelte';
	import { type Beside, besideIn, besideValue } from '#lib/flight/beside.ts';
	import CompareChooser from '#lib/flight/CompareChooser.svelte';
	import FlightScreen from '#lib/flight/FlightScreen.svelte';
	import { words } from '#lib/community/pages.ts';
	import { saveFile } from '#lib/logbook/flight-file.ts';
	import { deletionWarning } from '#lib/logbook/import-report.ts';
	import LeagueSheet from '#lib/logbook/LeagueSheet.svelte';
	import LogbookTable from '#lib/logbook/LogbookTable.svelte';
	import { reasonFor } from '#lib/logbook/model.svelte.ts';

	const app = getApp();

	/** Wide enough for the panel to stand beside the map: the width at which the navigation moves to the side. */
	const wide = new MediaQuery('min-width: 56rem', false);
	/**
	 * Wide enough for the logbook to stay beside the flight as well: the
	 * list, the map and the flight's column side by side. Narrower, the
	 * list is the logbook's own page.
	 */
	const listed = new MediaQuery('min-width: 80rem', false);

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

	/** The sheet that hands the flight to a league, while it is open. */
	let league = $state.raw<{ handover: LeagueHandover | undefined; problem: string | undefined }>();

	/** What can be done with the flight besides looking at it. */
	const actions: MenuItem[] = [
		{ label: 'Share with a link', icon: 'link', onselect: () => (sharing = true) },
		{ label: 'Lay another flight beside it', icon: 'track', onselect: () => (choosing = true) },
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

	/** The flight laid beside this one, as the address names it, once compared. */
	/** The logbook's name for where the flight took off, if it has one. */
	const takeoffName = $derived(
		app.logbook?.view.years.flatMap((year) => year.flights).find((entry) => entry.id === id)
			?.takeoffName
	);

	let beside = $state.raw<Beside>();
	let choosing = $state(false);
	const besideRef = $derived(browser ? besideIn(page.url.searchParams) : undefined);

	$effect(() => {
		beside = undefined;
		const first = id ? { own: id } : undefined;
		if (!besideRef || !first || !app.everyone) return;
		let current = true;
		const ref = besideRef;
		app.everyone.compare(first, ref).then(
			(compared) => {
				if (!current) return;
				beside = {
					flight: compared.other,
					name: compared.pilot ?? `Your flight of ${format().day(compared.other.startUnixS)}`,
					comparison: compared.comparison
				};
			},
			(error) => {
				if (!current) return;
				app.notice = { title: 'This flight could not be laid beside it', lines: [words(error)] };
			}
		);
		return () => {
			current = false;
		};
	});

	/** Lays another flight beside this one, or takes it away: the address says which. */
	function layBeside(ref: FlightRef | undefined) {
		choosing = false;
		const url = new URL(page.url.href);
		if (ref) url.searchParams.set('beside', besideValue(ref));
		else url.searchParams.delete('beside');
		void goto(`${url.pathname}${url.search}`, {
			replaceState: true,
			keepFocus: true,
			noScroll: true
		});
	}

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

	/** Opens the sheet that hands the flight to a league, with what the core says about it now. */
	async function sendToALeague() {
		if (!id || !app.logbook) return;
		league = { handover: undefined, problem: undefined };
		try {
			const handover = await app.logbook.leagueHandover(id);
			if (league) league = { handover, problem: undefined };
		} catch (error) {
			if (league) league = { handover: undefined, problem: reasonFor(error) };
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

<div class="together" class:listed={listed.current && app.logbook !== undefined}>
	{#if listed.current && app.logbook}
		<!-- On a large screen the logbook stays beside the flight, as a list
	     to go from one flight to the next. -->
		<aside class="list" aria-label="Logbook">
			<LogbookTable view={app.logbook.view} current={id ?? undefined} compact />
		</aside>
	{/if}
	<div class="flight">
		{#if phase.is === 'loaded'}
			{#key phase.flight.id}
				<FlightScreen
					flight={phase.flight}
					{analysis}
					wide={wide.current}
					back={{ href: logbookHref(), label: 'Logbook' }}
					{actions}
					inLogbook
					takeoffSite={takeoffName ? { name: takeoffName } : undefined}
					{beside}
					onendbeside={() => layBeside(undefined)}
					onleague={() => void sendToALeague()}
				/>
				<CompareChooser
					open={choosing}
					flight={phase.flight}
					onchoose={layBeside}
					onclose={() => (choosing = false)}
				/>
				<ShareFlightSheet
					open={sharing}
					flight={phase.flight.id}
					onclose={() => (sharing = false)}
				/>
				<LeagueSheet
					open={Boolean(league)}
					handover={league?.handover}
					problem={league?.problem}
					ondownload={() => void downloadFlight()}
					onclose={() => (league = undefined)}
				/>
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
	</div>
</div>

<ConfirmDialog
	open={confirmingDelete}
	title="Delete this flight?"
	message={deletionWarning(app.account?.signedIn ?? false)}
	confirm="Delete flight"
	onconfirm={deleteFlight}
	oncancel={() => (confirmingDelete = false)}
/>

<style>
	.listed {
		display: grid;
		grid-template-columns: 22rem minmax(0, 1fr);
	}

	.list {
		display: flex;
		flex-direction: column;
		height: calc(100dvh - var(--nav-top));
		padding: var(--space-3);
		border-right: 1px solid var(--separator);
		background: var(--background);
	}

	.list :global(.table) {
		flex: 1;
	}

	.flight {
		min-width: 0;
	}

	.unavailable {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 0.375rem;
		min-height: calc(100dvh - var(--nav-top));
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
		font-weight: 600;
	}

	p {
		color: var(--text-secondary);
	}

	.button-plain {
		margin-top: 0.75rem;
	}
</style>
