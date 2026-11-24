<!--
	A group's old address, `?id=`. Since 2026-11-24 a group with its chat is
	at `/community/groups/<id>/` (owner decision 26); links to this address
	lead on, and one without a group leads to Community's groups.
-->
<script lang="ts">
	import { browser } from '$app/env';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { groupHref, groupsHref } from '#lib/app/routes.ts';

	if (browser) {
		const id = page.url.searchParams.get('id');
		void goto(id ? groupHref(id) : groupsHref(), { replaceState: true });
	}
</script>

<svelte:head>
	<title>Community · Venti</title>
</svelte:head>
