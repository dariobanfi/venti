<!--
	An old address of the logbook's statistics. The website keeps no logbook of its own (owner
	decision 20): the pilot's flights and their statistics are on the account, under My flights.
	Others link here, so the address leads on.
-->
<script lang="ts">
	import { browser } from '$app/env';
	import { goto } from '$app/navigation';
	import { myStatisticsHref } from '#lib/app/routes.ts';

	if (browser) void goto(myStatisticsHref(), { replaceState: true });
</script>

<svelte:head>
	<title>Statistics · Venti</title>
</svelte:head>
