<!--
	A group's invite code, shown once: when the group is made, and when its
	keeper has a new code made. The server keeps no copy that can be read, so
	this sheet is the keeper's one chance to hand it on.

	The iPhone hands the code to the system's share sheet. A browser has none
	to count on, so a button copies the code, and the code can be selected.
	It is kept nowhere: not in the address, not in the browser's storage.
-->
<script lang="ts">
	import Icon from '#lib/design/Icon.svelte';
	import Sheet from '#lib/design/Sheet.svelte';
	import { ABOUT_THE_CODE } from './words';

	interface Props {
		/** The group and its code, while the sheet is open. */
		shown: { group: string; code: string } | undefined;
		onclose: () => void;
	}

	let { shown, onclose }: Props = $props();

	const id = $props.id();
	/** What became of copying, in a line. */
	let copied = $state.raw<string>();

	$effect(() => {
		if (shown) copied = undefined;
	});

	async function copy(code: string) {
		try {
			await navigator.clipboard.writeText(code);
			copied = 'Copied.';
		} catch {
			// Browsers hand the clipboard only to pages served securely.
			copied = 'This browser did not let the page copy. Select the code and copy it.';
		}
	}
</script>

<Sheet open={shown !== undefined} title="Invite code" {onclose}>
	{#if shown}
		<section class="group first" aria-labelledby="{id}-group">
			<h3 class="group-title" id="{id}-group">{shown.group}</h3>
			<div class="group-card">
				<p class="code numeric" aria-describedby="{id}-about">{shown.code}</p>
				<button class="group-row" type="button" onclick={() => copy(shown.code)}>
					Copy the code
					<Icon name="copy" />
				</button>
			</div>
			{#if copied}
				<p class="group-note" role="status">{copied}</p>
			{/if}
			<p class="group-note" id="{id}-about">{ABOUT_THE_CODE}</p>
		</section>
	{/if}
</Sheet>

<style>
	.first {
		margin-top: 0.25rem;
	}

	/* Large and in letters of one width, to be read out and typed off. It
	   can be selected with one click. */
	.code {
		padding: 1.25rem 1rem;
		font-family: ui-monospace, 'SF Mono', Menlo, Consolas, monospace;
		font-size: var(--title);
		font-weight: 600;
		text-align: center;
		letter-spacing: 0.02em;
		overflow-wrap: anywhere;
		-webkit-user-select: all;
		user-select: all;
	}

	.group-row :global(svg) {
		font-size: 0.875em;
	}
</style>
