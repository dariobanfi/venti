<!--
	A place's page's old address, from before 2026-11-24. Links to it are out in the
	wild, so it leads on to `/places/<lat>,<lon>/`, with the rest of the query.
-->
<script lang="ts">
	import { browser } from '$app/env';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { movedTo } from '#lib/app/routes.ts';

	if (browser) void goto(movedTo('place', page.url), { replaceState: true });
</script>

<svelte:head>
	<title>Place · Venti</title>
</svelte:head>
