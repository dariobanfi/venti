<!--
	An old address of "How you fly". The website keeps no logbook of its own (owner decision 20):
	the pilot's flights and what they say are on the account, under My flights' statistics.
	Others link here, so the address leads on.
-->
<script lang="ts">
	import { browser } from '$app/env';
	import { goto } from '$app/navigation';
	import { howYouFlyHref } from '#lib/app/routes.ts';

	if (browser) void goto(howYouFlyHref(), { replaceState: true });
</script>

<svelte:head>
	<title>How you fly · Venti</title>
</svelte:head>
