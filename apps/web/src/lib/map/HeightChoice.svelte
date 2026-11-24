<!--
	The height the forecast's wind is shown at (the owner, 2026-11-23, after
	Burnair's weather map: "you can select at multiple altitudes"): one small
	pill at the map's side that names the height shown ("250 m AGL",
	"Ground", "2000 m"), and when pressed the list of heights under it, the
	ground, the heights above it, then those above the sea, each by the
	core's name. Choosing one closes the list. A list always open took a
	third of a phone's map (docs/design.md, "The wind").

	The list is the browser's own popover, as `Menu.svelte`'s: a click beside
	it or Escape closes it, and focus goes back to the pill.
-->
<script lang="ts">
	import type { ThermalMapLevel } from '#lib/core/index.ts';
	import { isLevel, type WindLevel } from '#lib/forecast/thermal-map.ts';

	interface Props {
		levels: readonly ThermalMapLevel[];
		chosen: WindLevel;
		onchoose: (level: WindLevel) => void;
	}

	let { levels, chosen, onchoose }: Props = $props();

	const id = $props.id();
	let pill: HTMLButtonElement;
	let list: HTMLDivElement;
	let open = $state(false);

	/** A height the map no longer has is shown as the ground, as it is asked for. */
	const shown = $derived(
		levels.find((level) => isLevel(level, chosen)) ? chosen : { heightM: undefined }
	);
	const shownLevel = $derived(levels.find((level) => isLevel(level, shown)) ?? levels[0]);

	const entries = () => [...list.querySelectorAll('button')];

	/** Puts the list under the pill, their right edges in line, before it shows. */
	function opening(event: ToggleEvent) {
		if (event.newState !== 'open') return;
		const place = pill.getBoundingClientRect();
		list.style.top = `${place.bottom}px`;
		list.style.right = `${document.documentElement.clientWidth - place.right}px`;
	}

	/** The keyboard starts on the height shown. */
	function opened(event: ToggleEvent) {
		open = event.newState === 'open';
		if (!open) return;
		const all = entries();
		(all.find((entry) => entry.getAttribute('aria-checked') === 'true') ?? all[0])?.focus();
	}

	function keyed(event: KeyboardEvent) {
		const all = entries();
		const at = all.indexOf(document.activeElement as HTMLButtonElement);
		const to: Record<string, number> = {
			ArrowDown: (at + 1) % all.length,
			ArrowUp: (at - 1 + all.length) % all.length,
			Home: 0,
			End: all.length - 1
		};
		if (event.key in to) {
			event.preventDefault();
			all[to[event.key]].focus();
		} else if (event.key === 'Tab') {
			list.hidePopover();
		}
	}

	function choose(level: ThermalMapLevel) {
		list.hidePopover();
		pill.focus();
		onchoose({ heightM: level.heightM, aboveGroundM: level.aboveGroundM });
	}
</script>

<button
	bind:this={pill}
	class="pill"
	type="button"
	popovertarget={id}
	aria-haspopup="true"
	aria-expanded={open}
	aria-label="Height, {shownLevel?.name ?? ''}"
	title={shownLevel?.words}
>
	<span class="drawn">{shownLevel?.name}</span>
</button>

<div
	{id}
	bind:this={list}
	class="heights"
	popover
	role="radiogroup"
	tabindex="-1"
	aria-label="Height"
	onbeforetoggle={opening}
	ontoggle={opened}
	onkeydown={keyed}
>
	{#each levels as level (level.name)}
		{@const on = isLevel(level, shown)}
		<button
			type="button"
			role="radio"
			aria-checked={on}
			title={level.words}
			class:on
			onclick={() => choose(level)}>{level.name}</button
		>
	{/each}
</div>

<style>
	/* The pack's `.v-chip`: 36 px drawn, with its tap area 48. */
	.pill {
		display: grid;
		place-items: center end;
		min-height: var(--target);
		pointer-events: auto;
	}

	.drawn {
		display: inline-flex;
		align-items: center;
		height: 2.25rem;
		padding: 0 var(--space-3);
		border-radius: var(--radius-pill);
		background: var(--surface);
		color: var(--text);
		box-shadow:
			inset 0 0 0 1px var(--separator),
			var(--shadow-float);
		font-size: var(--subheadline);
		font-weight: 600;
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}

	.pill[aria-expanded='true'] .drawn {
		box-shadow:
			inset 0 0 0 2px var(--accent),
			var(--shadow-float);
	}

	/* The pack's segmented control, stood on end: one surface, the chosen one in the primary colour. */
	.heights {
		/* The browser would put a popover in the middle of the window. */
		position: fixed;
		inset: auto;
		margin: 0;
		flex-direction: column;
		padding: var(--space-1);
		border: 0;
		border-radius: var(--radius-control);
		background: var(--surface);
		box-shadow:
			inset 0 0 0 1px var(--separator),
			var(--shadow-float);
		/* The place over the map it stands in lets clicks through to the map. */
		pointer-events: auto;
	}

	.heights:popover-open {
		display: flex;
	}

	.heights button {
		min-height: var(--target);
		padding: 0 var(--space-3);
		border-radius: calc(var(--radius-control) - var(--space-1));
		color: var(--text);
		font-size: var(--subheadline);
		font-weight: 600;
		font-variant-numeric: tabular-nums;
		text-align: end;
		white-space: nowrap;
	}

	.heights button.on {
		background: var(--accent);
		color: var(--on-accent);
	}
</style>
