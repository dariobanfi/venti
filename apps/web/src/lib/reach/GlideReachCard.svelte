<!--
	What a glide reaches from a place on the map: the core's sentence, then
	the pilot's three settings, the glide (by half a point), the start
	height (the ground until one is set, by 50 m) and the wind (off until it
	is switched on: where it comes from and how fast), each change asking
	the core again; the core's note of what it does not know at the foot,
	and "Clear", which takes the area off the map. A card at the map's side
	on a wide screen, over the map's foot on a narrow one, as the route
	planner's. Every sentence about the glide is the core's.
-->
<script lang="ts">
	import Icon from '#lib/design/Icon.svelte';
	import { GLIDE_STEP, type GlideReachModel, HEIGHT_STEP_M } from './reach.svelte';

	interface Props {
		reach: GlideReachModel;
	}

	let { reach }: Props = $props();

	const answer = $derived(reach.reach);
	/** The start height shown: the one set, or the ground's, to the step. */
	const startShown = $derived(
		reach.startHeightM ??
			(answer?.groundM === undefined
				? undefined
				: Math.round(answer.groundM / HEIGHT_STEP_M) * HEIGHT_STEP_M)
	);

	function number(event: Event, set: (value: number) => void) {
		const value = (event.currentTarget as HTMLInputElement).valueAsNumber;
		if (Number.isFinite(value) && value >= 0) {
			set(value);
			void reach.show();
		}
	}
</script>

<section class="reach" aria-labelledby="reach-title">
	<header>
		<h2 id="reach-title">What a glide reaches</h2>
		<button type="button" aria-label="Close" title="Close" onclick={() => reach.clear()}>
			<Icon name="close" />
		</button>
	</header>

	{#if reach.problem}
		<p class="problem" role="alert">{reach.problem}</p>
	{:else if answer}
		<p class="words" role="status" data-reach-words>{answer.words}</p>
	{:else if reach.asking}
		<p class="lesser" role="status">Loading…</p>
	{/if}

	<div class="settings">
		<label class="setting">
			<span>Glide</span>
			<input
				type="number"
				inputmode="decimal"
				min="2"
				max="20"
				step={GLIDE_STEP}
				value={reach.glide}
				onchange={(event) => number(event, (value) => (reach.glide = value))}
			/>
		</label>
		<div class="setting">
			<label for="reach-start">Start, m</label>
			<span class="row">
				<input
					id="reach-start"
					type="number"
					inputmode="numeric"
					min="0"
					max="9000"
					step={HEIGHT_STEP_M}
					value={startShown}
					onchange={(event) => number(event, (value) => (reach.startHeightM = value))}
				/>
				{#if reach.startHeightM !== undefined}
					<button
						class="button-plain"
						type="button"
						onclick={() => {
							reach.startHeightM = undefined;
							void reach.show();
						}}
					>
						On the ground
					</button>
				{/if}
			</span>
		</div>
		<label class="setting toggle">
			<span>Wind</span>
			<input
				type="checkbox"
				checked={reach.windOn}
				onchange={(event) => {
					reach.windOn = event.currentTarget.checked;
					void reach.show();
				}}
			/>
		</label>
		{#if reach.windOn}
			<label class="setting">
				<span>From, °</span>
				<input
					type="number"
					inputmode="numeric"
					min="0"
					max="359"
					step="10"
					value={reach.windFromDeg}
					onchange={(event) => number(event, (value) => (reach.windFromDeg = value % 360))}
				/>
			</label>
			<label class="setting">
				<span>Speed, km/h</span>
				<input
					type="number"
					inputmode="numeric"
					min="0"
					max="80"
					step="5"
					value={reach.windKmh}
					onchange={(event) => number(event, (value) => (reach.windKmh = value))}
				/>
			</label>
		{/if}
	</div>

	{#if answer?.note}
		<p class="note">{answer.note}</p>
	{/if}

	<div class="acts">
		<button class="button-plain clear" type="button" onclick={() => reach.clear()}>
			<Icon name="trash" />
			Clear
		</button>
	</div>
</section>

<style>
	.reach {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}

	h2 {
		margin: 0;
		font-size: var(--subheadline);
		font-weight: 600;
	}

	header button {
		display: grid;
		place-items: center;
		width: var(--target);
		height: var(--target);
		margin: calc(var(--space-2) * -1);
		color: var(--text-secondary);
	}

	.words {
		font-size: var(--subheadline);
	}

	.problem {
		font-size: var(--subheadline);
		color: var(--danger);
	}

	.lesser {
		color: var(--text-secondary);
		font-size: var(--subheadline);
	}

	.settings {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2) var(--space-3);
		padding-top: var(--space-2);
		border-top: 1px solid var(--separator);
		font-size: var(--footnote);
	}

	.setting {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		color: var(--text-secondary);
	}

	.row {
		display: flex;
		align-items: center;
		gap: var(--space-1);
	}

	.row .button-plain {
		min-height: 2rem;
		padding-inline: var(--space-2);
		font-size: var(--footnote);
	}

	input[type='number'] {
		width: 4.5rem;
		min-height: 2rem;
		padding: 0 var(--space-2);
		border: 1px solid var(--separator);
		border-radius: var(--radius-control);
		background: var(--surface);
		color: var(--text);
		font: inherit;
	}

	.note {
		color: var(--text-secondary);
		font-size: var(--caption);
	}

	.acts {
		display: flex;
		margin-inline: calc(var(--space-3) * -1);
	}

	.clear {
		gap: 0.375rem;
		margin-left: auto;
		color: var(--danger);
	}

	.clear :global(svg) {
		width: 1.125rem;
		height: 1.125rem;
	}
</style>
