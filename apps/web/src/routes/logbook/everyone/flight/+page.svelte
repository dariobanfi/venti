<!--
	Another pilot's flight, shown like one of the logbook: the track on the
	map, coloured by climb, with its numbers, its altitude profile and its
	thermals. Which one is in the address: `?id=`, its address on the server.

	The flight is fetched from its file when this page opens and is kept
	nowhere: not in the logbook, not in the browser's store.
-->
<script lang="ts">
	import { MediaQuery } from 'svelte/reactivity';
	import { browser } from '$app/env';
	import { page } from '$app/state';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { everyoneHref, pilotHref } from '#lib/app/routes.ts';
	import { queryOf, words } from '#lib/community/pages.ts';
	import { reportedFlight } from '#lib/community/report.ts';
	import ReportSheet from '#lib/community/ReportSheet.svelte';
	import type { FlightRef, OpenedFlight } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import type { MenuItem } from '#lib/design/Menu.svelte';
	import { type Beside, besideIn, besideValue } from '#lib/flight/beside.ts';
	import CompareChooser from '#lib/flight/CompareChooser.svelte';
	import FlightScreen from '#lib/flight/FlightScreen.svelte';
	import FlightSocial from '#lib/social/FlightSocial.svelte';
	import { goto } from '$app/navigation';

	const app = getApp();
	const everyone = $derived(app.everyone);

	/** Wide enough for the panel to stand beside the map. */
	const wide = new MediaQuery('min-width: 56rem', false);

	/** The flight asked for. The address is not known while the pages are written out at build time. */
	const id = $derived(browser ? page.url.searchParams.get('id') : null);

	type Phase =
		{ is: 'loading' } | { is: 'loaded'; opened: OpenedFlight } | { is: 'failed'; message: string };

	let phase = $state.raw<Phase>({ is: 'loading' });
	/** Whether the form that reports the flight is open. */
	let reporting = $state(false);

	$effect(() => {
		if (app.failure) {
			phase = { is: 'failed', message: app.failure };
			return;
		}
		if (!everyone) return;
		if (!id) {
			phase = { is: 'failed', message: 'This address names no flight.' };
			return;
		}

		// Only the answer to the latest question counts.
		let current = true;
		phase = { is: 'loading' };
		everyone.open(id).then(
			(opened) => {
				if (!current) return;
				phase = opened
					? { is: 'loaded', opened }
					: {
							is: 'failed',
							message:
								'Other pilots’ flights are kept by Venti, and this copy of the app is served without Venti behind it.'
						};
			},
			(error) => {
				if (current) phase = { is: 'failed', message: words(error) };
			}
		);
		return () => {
			current = false;
		};
	});

	/** Back to everyone's flights as they were asked for. */
	const back = $derived(everyoneHref(everyone ? queryOf(everyone.asked) : ''));

	/** The flight laid beside this one, as the address names it, once compared. */
	let beside = $state.raw<Beside>();
	let choosing = $state(false);
	const besideRef = $derived(browser ? besideIn(page.url.searchParams) : undefined);

	$effect(() => {
		beside = undefined;
		const first = id ? { public: id } : undefined;
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

	/** The flight's page on the server, which is what to hand on. */
	function pageOf(opened: OpenedFlight): string {
		return new URL(opened.row.pagePath, location.origin).href;
	}

	async function copyLink(opened: OpenedFlight) {
		const link = pageOf(opened);
		try {
			await navigator.clipboard.writeText(link);
			app.notice = { title: 'Link copied', lines: [link] };
		} catch {
			// Browsers hand the clipboard only to pages served securely.
			app.notice = {
				title: 'The link could not be copied',
				lines: ['This browser did not let the page copy. Here it is to select and copy:', link]
			};
		}
	}

	/**
	 * What can be done with another pilot's flight: hand its page on, and
	 * tell whoever runs the server that it should not be there.
	 */
	function actions(opened: OpenedFlight): MenuItem[] {
		return [
			{
				label: 'Open its page on Venti',
				icon: 'link',
				onselect: () => window.open(pageOf(opened), '_blank', 'noopener')
			},
			{ label: 'Copy the link to its page', icon: 'copy', onselect: () => void copyLink(opened) },
			{ label: 'Lay another flight beside it', icon: 'track', onselect: () => (choosing = true) },
			{ label: 'Report this flight', icon: 'warning', onselect: () => (reporting = true) }
		];
	}

	/** The core's lines of the flight's cheers and comments, for the row that opens them on a phone. */
	let said = $state.raw<string[]>([]);

	const title = $derived(
		phase.is === 'loaded'
			? `${phase.opened.row.pilot} · ${format().day(phase.opened.flight.startUnixS)} · Venti`
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
			back={{ href: back, label: 'Everyone' }}
			actions={actions(opened)}
			whose={{
				name: opened.row.pilot,
				href: pilotHref(opened.row.handle),
				picture: opened.row.pilotPicture
			}}
			takeoffSite={opened.row.takeoffName
				? { name: opened.row.takeoffName, id: opened.row.takeoffSiteId }
				: undefined}
			{beside}
			onendbeside={() => layBeside(undefined)}
			socialLine={said.filter(Boolean).join(' · ')}
			socialTitle={app.social?.words?.commentsTitle}
		>
			{#snippet social()}
				<FlightSocial
					flightId={opened.row.id}
					flightHandle={opened.row.handle}
					onlines={(cheers, comments) => (said = [cheers, comments])}
				/>
			{/snippet}
		</FlightScreen>
		<CompareChooser
			open={choosing}
			flight={opened.flight}
			publicId={opened.row.id}
			onchoose={layBeside}
			onclose={() => (choosing = false)}
		/>
		<ReportSheet
			open={reporting}
			{...reportedFlight(opened.row.id)}
			onclose={() => (reporting = false)}
		/>
	{/key}
{:else if phase.is === 'failed'}
	<div class="unavailable">
		<span class="symbol"><Icon name="warning" /></span>
		<h1>This flight could not be opened</h1>
		<p>{phase.message}</p>
		<a class="button-plain" href={back}>Back to everyone’s flights</a>
	</div>
{:else}
	<p class="visually-hidden" role="status">Fetching the flight…</p>
{/if}

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

	.button-plain {
		margin-top: 0.75rem;
	}
</style>
