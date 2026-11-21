<!--
	The filter of the flying sites, in the manner of the forecast layer's
	card: which sites to show as the pack's segmented control, the wind a
	take-off is to work in as a row of the pack's chips ("Any", the wind a
	station measures now where the map has one, then the sixteen points of
	the compass), the height take-offs are to lie at as a second segmented
	control; then what the filter is and what it leaves, and the core's note
	on what a fit says and does not say.

	Every choice and every word is the core's (`sitesFilterChoices`,
	`filterSites`). Under "Landings" the wind and the height are not asked,
	and neither is offered: the core says so with the choice.

	At its foot stands the switch of the crowdsourced layer, "Places from
	pilots", with the core's sentence under it (`withPilotsWords`,
	`withPilotsAbout` of the choices): off until the pilot switches it on,
	and kept as the pilot left it. It is the filter's `withPilots`, by which
	the core chooses what the server is asked for and says that a filter is
	on.

	Beside it the switch of the map's take-offs without a site, with the
	core's sentence (`siteFlightsWords`: `layer`, `layerAbout`): off until
	the pilot switches it on, and kept in this browser.

	Under it, where the page offers it (`onremoved`), a quiet entry in the
	core's words, "Removed lately", opens the removed sites of Venti's tiers.

	"Forecast now" is not offered: the page has no forecast for the map's
	middle at hand. On a narrow screen the chips scroll sideways (`compact`);
	on a wide one they wrap, the pane being narrow and the screen large.
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import Segmented from '#lib/design/Segmented.svelte';
	import type { SitesModel } from './model.svelte';
	import SitesFilterLine from './SitesFilterLine.svelte';

	interface Props {
		sites: SitesModel;
		/** Whether the chips scroll sideways in one row, on a narrow screen. */
		compact?: boolean;
		/** Opens "Removed lately", where the page offers it. */
		onremoved?: () => void;
	}

	let { sites, compact = false, onremoved }: Props = $props();

	const id = $props.id();
	const choices = $derived(sites.choices);
	/** The words of a site's page, the core's: "Removed lately". */
	const words = $derived(getApp().siteEdits?.words);
	/** The layer of take-offs without a site, with the core's words for its switch. */
	const takeoffs = $derived(getApp().takeoffs);
	const choice = $derived(sites.choice);
	/** Whether the wind and the height are asked under what is shown. */
	const asksTakeoffs = $derived(
		choices?.shown.find((one) => one.shown === choice.shown)?.asksTakeoffs ?? false
	);
	/**
	 * The wind now, of the core's two, that the page has a direction for:
	 * a station's. One that was chosen stays offered where the map has no
	 * station, so that it can be let go of.
	 */
	const windsNow = $derived(
		choices?.windsNow.filter(
			(one) =>
				one.windIs === 'measuredNow' &&
				(sites.windNow.measuredDeg !== undefined || choice.windIs === 'measuredNow')
		) ?? []
	);

	/**
	 * Brings the chip that is on to the middle of its row where the row
	 * scrolls sideways. Only the row moves: the sheet around it stays where
	 * it is.
	 */
	function inView(chip: HTMLButtonElement) {
		const row = chip.parentElement;
		if (!row || chip.getAttribute('aria-pressed') !== 'true') return;
		const [inRow, whole] = [chip.getBoundingClientRect(), row.getBoundingClientRect()];
		row.scrollLeft += inRow.left - whole.left - (whole.width - inRow.width) / 2;
	}
</script>

<div class="filter">
	{#if choices}
		<h3 class="label">{choices.shownTitle}</h3>
		<Segmented
			label={choices.shownTitle}
			options={choices.shown.map((one) => one.words)}
			bind:value={
				() => choices.shown.find((one) => one.shown === choice.shown)?.words ?? '',
				(words) => {
					const picked = choices.shown.find((one) => one.words === words);
					if (picked) sites.choose({ shown: picked.shown });
				}
			}
		/>
		{#if asksTakeoffs}
			<h3 class="label" id="{id}-wind">{choices.windTitle}</h3>
			<div class="chips" class:compact role="group" aria-labelledby="{id}-wind">
				<button
					class="chip"
					type="button"
					aria-pressed={choice.windIs === 'chosen' && choice.windFromDeg === undefined}
					onclick={() => sites.choose({ windIs: 'chosen', windFromDeg: undefined })}
					use:inView
				>
					{choices.anyWind}
				</button>
				{#each windsNow as now (now.windIs)}
					<button
						class="chip"
						type="button"
						aria-pressed={choice.windIs === now.windIs}
						onclick={() => sites.choose({ windIs: now.windIs, windFromDeg: undefined })}
						use:inView
					>
						{now.words}
					</button>
				{/each}
				{#each choices.winds as wind (wind.fromDeg)}
					<button
						class="chip numeric"
						type="button"
						aria-pressed={choice.windIs === 'chosen' && choice.windFromDeg === wind.fromDeg}
						onclick={() => sites.choose({ windIs: 'chosen', windFromDeg: wind.fromDeg })}
						use:inView
					>
						{wind.words}
					</button>
				{/each}
			</div>
			<h3 class="label">{choices.heightTitle}</h3>
			<Segmented
				label={choices.heightTitle}
				options={choices.heights.map((one) => one.words)}
				bind:value={
					() => choices.heights.find((one) => one.fromM === choice.takeoffsFromM)?.words ?? '',
					(words) => {
						const picked = choices.heights.find((one) => one.words === words);
						if (picked) sites.choose({ takeoffsFromM: picked.fromM });
					}
				}
			/>
		{/if}
		{#if sites.left}
			<div role="status">
				<SitesFilterLine left={sites.left} counted={sites.state.close} />
			</div>
		{/if}
		<p class="caption">{choices.note}</p>
		<div class="group-card layer">
			<label class="group-row toggle">
				<span>{choices.withPilotsWords}</span>
				<input
					type="checkbox"
					role="switch"
					aria-describedby="{id}-pilots"
					checked={choice.fromPilots}
					onchange={(event) => sites.choose({ fromPilots: event.currentTarget.checked })}
				/>
			</label>
		</div>
		<p class="caption" id="{id}-pilots">{choices.withPilotsAbout}</p>
		{#if takeoffs?.words}
			<!-- The places where pilots take off and no site stands: the core's switch and sentence. -->
			<div class="group-card layer">
				<label class="group-row toggle">
					<span>{takeoffs.words.layer}</span>
					<input
						type="checkbox"
						role="switch"
						aria-describedby="{id}-candidates"
						checked={takeoffs.layerOn}
						onchange={(event) => takeoffs.switchLayer(event.currentTarget.checked)}
					/>
				</label>
			</div>
			<p class="caption" id="{id}-candidates">{takeoffs.words.layerAbout}</p>
		{/if}
	{/if}
	{#if onremoved && words?.removedTitle}
		<div class="group-card layer">
			<button class="group-row" type="button" onclick={onremoved}>{words.removedTitle}</button>
		</div>
	{/if}
</div>

<style>
	.filter {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.label {
		margin: 0;
		color: var(--text-secondary);
		font-size: var(--caption);
		font-weight: 600;
	}

	/* The pack's chips, one of them on. They wrap where the screen is wide
	   and the pane narrow; on a narrow screen the row scrolls sideways. */
	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
		min-width: 0;
	}

	.chips.compact {
		flex-wrap: nowrap;
		overflow-x: auto;
		scrollbar-width: none;
	}

	.chip {
		flex: none;
		min-height: 2.25rem;
		padding: 0 var(--space-3);
		border-radius: var(--radius-pill);
		background: var(--surface);
		box-shadow: inset 0 0 0 1px var(--separator);
		color: var(--text);
		font-size: var(--subheadline);
		font-weight: 600;
		white-space: nowrap;
	}

	.chip[aria-pressed='true'] {
		background: var(--accent);
		color: var(--on-accent);
		box-shadow: none;
	}

	/* Compact on a wide screen: controls about 32 px. */
	@media (min-width: 56rem) {
		.chip {
			min-height: 2rem;
		}
	}

	.layer {
		margin-top: var(--space-2);
	}

	.caption {
		margin: 0;
		color: var(--text-secondary);
		font-size: var(--caption);
	}
</style>
