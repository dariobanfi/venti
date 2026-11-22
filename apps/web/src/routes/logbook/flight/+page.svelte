<!--
	A flight's old address in the logbook. The website keeps no logbook of
	its own (owner decision 20): the flight is the account's, and opens at
	its address there, `?id=` and all.
-->
<script lang="ts">
	import { browser } from '$app/env';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { myFlightHref, myFlightsHref } from '#lib/app/routes.ts';

	if (browser) {
		const id = page.url.searchParams.get('id');
		void goto(id ? myFlightHref(id) : myFlightsHref(), { replaceState: true });
	}
</script>

<svelte:head>
	<title>Flight · Venti</title>
</svelte:head>
