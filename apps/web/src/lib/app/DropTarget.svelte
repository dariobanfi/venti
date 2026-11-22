<!--
	Makes the whole page a place to drop files, whichever screen is showing:
	flight files everywhere, brought in as uploads to the account, and
	airspace files on the screen that lists them.
	While files are dragged over it, the page says what dropping them will do.
-->
<script lang="ts">
	import { page } from '$app/state';
	import Icon from '#lib/design/Icon.svelte';
	import { carriesFiles, droppedFiles } from '#lib/logbook/flight-file.ts';
	import { getApp } from './app.svelte';

	const app = getApp();

	/** On the airspace screen a dropped file is an airspace file. */
	const forAirspace = $derived(page.route.id === '/settings/airspace');

	/**
	 * How many elements the drag is currently inside of. The browser reports
	 * entering a child before leaving its parent, so a plain flag would
	 * flicker; a count is zero only when the drag has left the page.
	 */
	let inside = $state(0);

	function entered(event: DragEvent) {
		if (!carriesFiles(event.dataTransfer)) return;
		event.preventDefault();
		inside += 1;
	}

	function over(event: DragEvent) {
		if (!carriesFiles(event.dataTransfer) || !event.dataTransfer) return;
		// Without this the browser would open the file instead of handing it over.
		event.preventDefault();
		event.dataTransfer.dropEffect = 'copy';
	}

	function left(event: DragEvent) {
		if (carriesFiles(event.dataTransfer)) inside = Math.max(0, inside - 1);
	}

	function dropped(event: DragEvent) {
		if (!carriesFiles(event.dataTransfer)) return;
		event.preventDefault();
		inside = 0;
		const files = droppedFiles(event.dataTransfer);
		void (forAirspace ? app.importAirspace(files) : app.importFiles(files));
	}
</script>

<svelte:window ondragenter={entered} ondragover={over} ondragleave={left} ondrop={dropped} />

{#if inside > 0 && app.myFlights}
	<div class="veil">
		<p class="glass">
			<Icon name="plus" />
			{forAirspace ? 'Drop airspace files to bring them in' : 'Drop flight files to bring them in'}
		</p>
	</div>
{/if}

<style>
	.veil {
		position: fixed;
		inset: 0;
		z-index: 30;
		display: grid;
		place-items: center;
		padding: 1.5rem;
		background: var(--scrim);
		/* The drag carries on to the page underneath, which takes the drop. */
		pointer-events: none;
	}

	p {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 1rem 1.5rem;
		border-radius: 999px;
		font-weight: 600;
	}
</style>
