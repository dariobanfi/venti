<!--
	The groups' old address. Since 2026-11-24 the pilot's groups, their
	invites and the groups near are a part of Community (owner decision 26),
	and a group is joined by finding it or by an invite by handle, not with a
	code; links to this address lead on.
-->
<script lang="ts">
	import { browser } from '$app/env';
	import { goto } from '$app/navigation';
	import { groupsHref } from '#lib/app/routes.ts';

	if (browser) void goto(groupsHref(), { replaceState: true });
</script>

<svelte:head>
	<title>Community · Venti</title>
</svelte:head>
