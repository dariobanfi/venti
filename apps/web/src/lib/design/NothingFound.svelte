<!--
	What a list says when nothing in it matches the text searched for, in
	the core's words (`nothingFoundWords`): "No results for “Brau”" and a
	line under it.
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import type { NothingFoundWords } from '#lib/core/index.ts';
	import EmptyList from './EmptyList.svelte';

	let { searched }: { searched: string } = $props();

	const app = getApp();
	let said = $state.raw<NothingFoundWords>();

	$effect(() => {
		let current = true;
		void app.nothingFound(searched).then((words) => {
			if (current) said = words;
		});
		return () => {
			current = false;
		};
	});
</script>

<EmptyList title={said?.title} line={said?.line} />
