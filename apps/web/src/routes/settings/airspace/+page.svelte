<!--
	The pilot's airspace: what the app keeps from this site's server, and the
	files the pilot brings in. Where each comes from, how old it is, and what
	it left out. The iPhone app's `AirspaceScreen.swift` is its twin.

	The page says plainly what the app does not know: it knows this airspace
	and no other, and what it draws is advice.
-->
<script lang="ts">
	import { countryNames, fileDetail, keptDetail, leftOutWarning } from '#lib/airspace/words.ts';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { settingsHref } from '#lib/app/routes.ts';
	import type { AirspaceFile } from '#lib/core/index.ts';
	import ConfirmDialog from '#lib/design/ConfirmDialog.svelte';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';

	const app = getApp();
	const text = format();
	const airspace = $derived(app.airspace);
	const hasServer = $derived(app.account?.hasServer);

	let picker: HTMLInputElement;
	let fileToDelete = $state.raw<AirspaceFile>();

	/** The files the pilot chose. The core says which of them hold airspace. */
	function picked() {
		const files = [...(picker.files ?? [])];
		// Cleared so that choosing the same file again is noticed.
		picker.value = '';
		void app.importAirspace(files);
	}

	function useServer(use: boolean) {
		if (use) airspace?.useServer();
		else void airspace?.stopUsingServer();
	}

	/** The site a credit links to, without the rest of its address. */
	function host(url: string): string {
		try {
			return new URL(url).host;
		} catch {
			return url;
		}
	}

	async function deleteFile(file: AirspaceFile) {
		fileToDelete = undefined;
		await airspace?.delete(file);
	}

	/** What the server's part is doing, under its rows. */
	const serverNote = $derived.by(() => {
		if (!airspace) return '';
		if (!airspace.usesServer) return 'Switched off: only your own files are used.';
		if (airspace.kept.length === 0) {
			return 'Fetched the next time a map is open, for about 200 km around what it shows, and kept in this browser.';
		}
		return 'Kept in this browser for about 200 km around where it was fetched, so the maps have it without a network. Fetched again once a day, and when you look somewhere else.';
	});
</script>

<svelte:head>
	<title>Airspace · Venti</title>
</svelte:head>

<div class="settings-page">
	<header>
		<a class="back-link" href={settingsHref()}>
			<Icon name="back" />
			Settings
		</a>
	</header>
	<h1>Airspace</h1>

	{#if app.failure}
		<p class="group-note">Venti could not open its data. {app.failure}</p>
	{:else if airspace}
		{#if airspace.problem}
			<section class="group">
				<div class="group-card">
					<p class="group-row group-warning" role="alert">
						<Icon name="warning" />
						<span>{airspace.problem}</span>
					</p>
				</div>
			</section>
		{/if}

		<section class="group" aria-labelledby="airspace-server">
			<h2 class="group-title" id="airspace-server">From Venti</h2>
			{#if hasServer === false}
				<p class="group-note">
					This copy of the app is served without Venti behind it, so there is no airspace to fetch.
					Bring in your own files below.
				</p>
			{:else}
				<ul class="group-card" aria-labelledby="airspace-server">
					{#each airspace.kept as layer (layer.source)}
						<li class="entry">
							<span>{layer.provider}</span>
							<span class="lesser numeric">{keptDetail(layer, text)}</span>
							{#if layer.countries.length > 0}
								<!-- The kept area reaches across borders: say whose airspace this is. -->
								<span class="plain">
									The airspace of {countryNames(layer.countries, text)}. Of other countries none is
									kept, also where the area fetched reaches into them.
								</span>
							{/if}
							{#if !layer.saysWhatItCovers}
								<span class="plain group-warning">
									<Icon name="warning" />
									<span>
										Venti does not say for which area this is complete. Where the map shows no
										airspace, that need not mean there is none.
									</span>
								</span>
							{/if}
							{#if layer.credit}
								{#if layer.credit.notice}
									<span class="plain notice">
										<Icon name="warning" />
										<span>{layer.credit.notice}</span>
									</span>
								{/if}
								<span class="lesser">{layer.credit.text} Licence: {layer.credit.licence}.</span>
								<a class="lesser" href={layer.credit.url} target="_blank" rel="noopener noreferrer">
									{host(layer.credit.url)}
								</a>
							{/if}
						</li>
					{/each}
					{#if airspace.serverProblem}
						<li class="entry">
							<span class="lesser" role="status">{airspace.serverProblem}</span>
						</li>
					{/if}
					<li>
						<label class="group-row toggle">
							<span>Airspace from Venti</span>
							<input
								type="checkbox"
								role="switch"
								checked={airspace.usesServer}
								onchange={(event) => useServer(event.currentTarget.checked)}
							/>
						</label>
					</li>
				</ul>
				<p class="group-note">{serverNote}</p>
			{/if}
		</section>

		<section class="group" aria-labelledby="airspace-files">
			<h2 class="group-title" id="airspace-files">Your files</h2>
			<ul class="group-card" aria-labelledby="airspace-files">
				{#each airspace.files as file (file.storedName)}
					{@const warning = leftOutWarning(file)}
					<li class="file">
						<span class="entry">
							<span>{file.name}</span>
							<span class="lesser numeric">{fileDetail(file, text)}</span>
							{#if warning}
								<span class="plain group-warning">
									<Icon name="warning" />
									<span>{warning}</span>
								</span>
							{/if}
						</span>
						<button
							class="remove"
							type="button"
							aria-label="Remove {file.name}"
							title="Remove"
							onclick={() => (fileToDelete = file)}
						>
							<Icon name="trash" />
						</button>
					</li>
				{/each}
				<li>
					<button class="group-row" type="button" onclick={() => picker.click()}>
						Add an airspace file
						<Icon name="plus" />
					</button>
				</li>
			</ul>
			<p class="group-note">
				Venti reads airspace files in OpenAir format, the kind flight instruments use, and draws
				their airspace on its maps beside Venti’s.
				<span class="with-mouse">Files dragged onto this page are brought in too.</span>
			</p>
			<p class="group-note">
				It knows the airspace listed here and no other. Where none of it reaches, the map shows no
				airspace, and that does not mean there is none. An old file it cannot tell from a new one,
				it does not know which areas are active today, and on the map of a past flight it draws the
				airspace as it is now. What it draws is advice: you remain responsible for where you fly.
				The warnings in flight are the phone app’s.
			</p>
		</section>
	{/if}
</div>

<!-- Airspace files have no type a browser knows, so any file can be picked. -->
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
	open={fileToDelete !== undefined}
	title="Remove this file?"
	message="Its airspace is no longer drawn. The file you brought in from is not touched."
	confirm="Remove file"
	onconfirm={() => fileToDelete && deleteFile(fileToDelete)}
	oncancel={() => (fileToDelete = undefined)}
/>

<style>
	/* A row that says several things about one source, one under the other. */
	.entry {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		min-width: 0;
		padding: 0.75rem 1rem;
	}

	.lesser {
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	a.lesser {
		align-self: flex-start;
		color: var(--link);
	}

	.plain {
		font-size: var(--footnote);
	}

	/* What the provider asks to be said: said, without the alarm of a warning. */
	.notice {
		display: flex;
		align-items: flex-start;
		gap: 0.5rem;
	}

	.notice :global(svg) {
		font-size: 0.875em;
		margin-top: 0.1em;
	}

	.file {
		display: flex;
		align-items: center;
		gap: 0.25rem;
	}

	.file .entry {
		flex: 1;
	}

	.remove {
		flex: none;
		display: grid;
		place-items: center;
		width: var(--target);
		height: var(--target);
		margin-right: 0.375rem;
		border-radius: 50%;
		color: var(--text-secondary);
	}

	.group-row :global(svg) {
		font-size: 0.875em;
	}

	/* Dragging files is something a mouse does. */
	.with-mouse {
		display: none;
	}

	@media (hover: hover) {
		.with-mouse {
			display: inline;
		}

		.remove:hover {
			background: var(--fill);
			color: var(--danger);
		}
	}
</style>
