<!--
	The frame around every screen: the navigation, the page, and what can
	happen on any page: a message for the pilot, and files dropped on the window.
-->
<script lang="ts">
	import '../app.css';
	import { onMount, type Snippet } from 'svelte';
	import { App, setApp } from '#lib/app/app.svelte.ts';
	import DropTarget from '#lib/app/DropTarget.svelte';
	import Navigation from '#lib/app/Navigation.svelte';
	import NoticeDialog from '#lib/design/NoticeDialog.svelte';

	let { children }: { children: Snippet } = $props();

	const app = setApp(new App());

	// The core is WebAssembly and its store is the browser's: neither exists
	// while the pages are written out at build time.
	onMount(() => {
		void app.start();
	});
</script>

<Navigation />
<main>
	{@render children()}
</main>
<DropTarget />
<NoticeDialog notice={app.notice} onclose={() => (app.notice = undefined)} />

<style>
	main {
		/* The page fills the window beside the rail of a wide screen. Each
		   screen decides for itself whether it scrolls. */
		min-height: 100dvh;
		margin-left: var(--nav-side);
	}
</style>
