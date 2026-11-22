<!--
	The board's old address. The plans of the pilot's groups are the first
	part of Community since 2026-11-23; others link here, so the address
	leads on. A plan's and a group's pages keep their addresses under it.
-->
<script lang="ts">
	import { browser } from '$app/env';
	import { goto } from '$app/navigation';
	import { communityHref } from '#lib/app/routes.ts';

	if (browser) void goto(communityHref(), { replaceState: true });
</script>

<svelte:head>
	<title>Community · Venti</title>
</svelte:head>
