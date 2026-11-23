<!--
	Everyone's flights' old address, from before 2026-11-24. Links to it are out in the
	wild, so it leads on to `/flights/`, with the rest of the query.
-->
<script lang="ts">
	import { browser } from '$app/env';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { movedTo } from '#lib/app/routes.ts';

	if (browser) void goto(movedTo('everyone', page.url), { replaceState: true });
</script>

<svelte:head>
	<title>Flights · Venti</title>
</svelte:head>
