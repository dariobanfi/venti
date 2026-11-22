<!--
	An old address of the logbook's statistics. The website keeps no logbook of its own (owner
	decision 20): the pilot's flights are on the account, at My flights.
	Others link here, so the address leads on.
-->
<script lang="ts">
	import { browser } from '$app/env';
	import { goto } from '$app/navigation';
	import { myFlightsHref } from '#lib/app/routes.ts';

	if (browser) void goto(myFlightsHref(), { replaceState: true });
</script>

<svelte:head>
	<title>My flights · Venti</title>
</svelte:head>
