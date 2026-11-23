<!--
	One of the pilot's flights on the account. Which one is in the address:
	`?id=`, its address on the server.

	The flight is fetched from its file with the session when this page
	opens, and is kept nowhere, as another pilot's is: the website keeps no
	logbook of its own (owner decision 20). What can be done with it: who may
	see it, another flight beside it, its file, sending it to a league, and
	deleting it. It is checked against the airspace the app has, from the
	same file.
-->
<script lang="ts">
	import { MediaQuery } from 'svelte/reactivity';
	import { browser } from '$app/env';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import ShareFlightSheet from '#lib/account/ShareFlightSheet.svelte';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { accountHref, myFlightsHref } from '#lib/app/routes.ts';
	import { words } from '#lib/community/pages.ts';
	import type {
		FlightRef,
		GliderClassKind,
		LeagueHandover,
		OpenedOwnFlight
	} from '#lib/core/index.ts';
	import ConfirmDialog from '#lib/design/ConfirmDialog.svelte';
	import { flightFormat } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import type { MenuItem } from '#lib/design/Menu.svelte';
	import { type Beside, besideIn, besideValue } from '#lib/flight/beside.ts';
	import CompareChooser from '#lib/flight/CompareChooser.svelte';
	import FlightScreen from '#lib/flight/FlightScreen.svelte';
	import { saveFile } from '#lib/logbook/flight-file.ts';
	import { deletionWarning } from '#lib/logbook/import-report.ts';
	import GliderClassSheet from '#lib/logbook/GliderClassSheet.svelte';
	import LeagueSheet from '#lib/logbook/LeagueSheet.svelte';
	import { reasonFor } from '#lib/logbook/model.svelte.ts';

	const app = getApp();

	/** Wide enough for the panel to stand beside the map. */
	const wide = new MediaQuery('min-width: 56rem', false);

	/** The flight asked for. The address is not known while the pages are written out at build time. */
	const id = $derived(browser ? page.url.searchParams.get('id') : null);

	type Phase =
		| { is: 'loading' }
		| { is: 'loaded'; opened: OpenedOwnFlight }
		| { is: 'signedOut' }
		| { is: 'failed'; message: string };

	let phase = $state.raw<Phase>({ is: 'loading' });
	let confirmingDelete = $state(false);
	/** Whether the sheet that says who can see the flight is open. */
	let sharing = $state(false);
	/** Whether the sheet that sets the class of the flight's glider is open. */
	let classing = $state(false);
	/** The sheet that hands the flight to a league, while it is open. */
	let league = $state.raw<{ handover: LeagueHandover | undefined; problem: string | undefined }>();

	$effect(() => {
		if (app.failure) {
			phase = { is: 'failed', message: app.failure };
			return;
		}
		const flights = app.myFlights;
		if (!flights) return;
		if (!id) {
			phase = { is: 'failed', message: 'This address names no flight.' };
			return;
		}
		// Asked again when the pilot signs in or out.
		const signedIn = app.account?.signedIn;
		if (!signedIn) {
			phase = { is: 'signedOut' };
			return;
		}

		// Only the answer to the latest question counts.
		let current = true;
		phase = { is: 'loading' };
		flights.open(id).then(
			(opened) => {
				if (current) phase = { is: 'loaded', opened };
			},
			(error) => {
				if (current) phase = { is: 'failed', message: reasonFor(error) };
			}
		);
		return () => {
			current = false;
		};
	});

	/** What can be done with the flight besides looking at it. */
	const actions: MenuItem[] = $derived([
		{ label: 'Share with a link', icon: 'link', onselect: () => (sharing = true) },
		{ label: 'Lay another flight beside it', icon: 'track', onselect: () => (choosing = true) },
		{
			label: app.said?.glider.classTitle ?? 'Glider class',
			icon: 'glider',
			onselect: () => (classing = true)
		},
		{ label: 'Download flight file', icon: 'download', onselect: downloadFlight },
		{
			label: 'Delete flight',
			icon: 'trash',
			destructive: true,
			onselect: () => (confirmingDelete = true)
		}
	]);

	/** The flight laid beside this one, as the address names it, once compared. */
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
					name:
						compared.pilot ??
						`Your flight of ${flightFormat(compared.other).day(compared.other.startUnixS)}`,
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

	/**
	 * Saves the flight's file, as it was uploaded, where the browser keeps
	 * downloads, under the name the core gives it: the file fetched when
	 * the page opened.
	 */
	function downloadFlight() {
		if (phase.is !== 'loaded') return;
		saveFile(phase.opened.name, phase.opened.text);
	}

	/** Opens the sheet that hands the flight to a league, with what the core says about it now. */
	async function sendToALeague() {
		if (phase.is !== 'loaded' || !app.myFlights) return;
		league = { handover: undefined, problem: undefined };
		try {
			const handover = await app.myFlights.leagueHandover(phase.opened.text);
			if (league) league = { handover, problem: undefined };
		} catch (error) {
			if (league) league = { handover: undefined, problem: reasonFor(error) };
		}
	}

	/** Sets the class of the flight's glider; why not, in words. */
	async function setGliderClass(kind: GliderClassKind | undefined): Promise<string | undefined> {
		if (phase.is !== 'loaded' || !app.myFlights) return 'Not signed in.';
		const opened = phase.opened;
		try {
			const changed = await app.myFlights.setGliderClass(opened.row.id, kind);
			if (phase.is === 'loaded' && phase.opened === opened) {
				phase = { is: 'loaded', opened: { ...opened, row: changed.row } };
			}
			return undefined;
		} catch (error) {
			return reasonFor(error);
		}
	}

	async function deleteFlight() {
		confirmingDelete = false;
		if (!id) return;
		try {
			await app.deleteFlight(id);
			await goto(myFlightsHref());
		} catch (error) {
			app.notice = { title: 'The flight could not be deleted', lines: [reasonFor(error)] };
		}
	}

	const title = $derived(
		phase.is === 'loaded'
			? `${flightFormat(phase.opened.flight).day(phase.opened.flight.startUnixS)} · Venti`
			: 'Flight · Venti'
	);
</script>

<svelte:head>
	<title>{title}</title>
</svelte:head>

{#if phase.is === 'loaded'}
	{@const opened = phase.opened}
	{#key opened.row.id}
		<FlightScreen
			flight={opened.flight}
			analysis={opened.analysis}
			wide={wide.current}
			back={{ href: myFlightsHref(), label: 'My flights' }}
			{actions}
			takeoffSite={opened.row.takeoffName
				? { name: opened.row.takeoffName, id: opened.row.takeoffSiteId }
				: undefined}
			{beside}
			onendbeside={() => layBeside(undefined)}
			checkAirspace={() =>
				app.myFlights?.airspace(opened.text) ?? Promise.reject(new Error('Not signed in.'))}
			onleague={() => void sendToALeague()}
		/>
		<LeagueSheet
			open={Boolean(league)}
			handover={league?.handover}
			problem={league?.problem}
			ondownload={() =>
				league?.handover && saveFile(league.handover.file.name, league.handover.file.igcText)}
			onclose={() => (league = undefined)}
		/>
		<CompareChooser
			open={choosing}
			flight={opened.flight}
			publicId={opened.row.id}
			onchoose={layBeside}
			onclose={() => (choosing = false)}
		/>
		<GliderClassSheet
			open={classing}
			chosen={opened.row.gliderClass}
			onchoose={setGliderClass}
			onclose={() => (classing = false)}
		/>
		<ShareFlightSheet
			open={sharing}
			flight={opened.row.id}
			shared={app.myFlights?.flights.find((one) => one.row.id === opened.row.id)?.sharing ??
				opened.sharing}
			takenDown={opened.takenDown}
			onclose={() => (sharing = false)}
		/>
	{/key}
{:else if phase.is === 'signedOut'}
	<div class="unavailable">
		<span class="symbol"><Icon name="person" /></span>
		<h1>Sign in to see this flight</h1>
		<a class="button-prominent" href={accountHref()}>Sign in</a>
	</div>
{:else if phase.is === 'failed'}
	<div class="unavailable">
		<span class="symbol"><Icon name="warning" /></span>
		<h1>This flight could not be opened</h1>
		<p>{phase.message}</p>
		<a class="button-plain" href={myFlightsHref()}>Back to my flights</a>
	</div>
{:else}
	<p class="visually-hidden" role="status">Fetching the flight…</p>
{/if}

<ConfirmDialog
	open={confirmingDelete}
	title="Delete this flight?"
	message={deletionWarning()}
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

	.button-plain,
	.button-prominent {
		margin-top: 0.75rem;
	}
</style>
