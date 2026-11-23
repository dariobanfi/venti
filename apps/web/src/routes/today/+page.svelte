<!--
	Today's old address. Today is the home page's first part since the owner
	merged the two (2026-11-23), and links to `/today/` are out in the wild,
	so the address leads there.
-->
<script lang="ts">
	import { browser } from '$app/env';
	import { goto } from '$app/navigation';
	import { homeHref } from '#lib/app/routes.ts';

	if (browser) void goto(homeHref(), { replaceState: true });
</script>

<svelte:head>
	<title>Venti</title>
</svelte:head>
