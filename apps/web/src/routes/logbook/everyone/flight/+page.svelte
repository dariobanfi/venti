<!--
	Another pilot's flight's old address, from before 2026-11-24. Links to it are out in the
	wild, so it leads on to `/flights/<id>/`, with the rest of the query.
-->
<script lang="ts">
	import { browser } from '$app/env';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { movedTo } from '#lib/app/routes.ts';

	if (browser) void goto(movedTo('flight', page.url), { replaceState: true });
</script>

<svelte:head>
	<title>Flight · Venti</title>
</svelte:head>
