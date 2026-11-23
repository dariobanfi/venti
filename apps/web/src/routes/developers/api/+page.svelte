<!--
	The REST API's reference, written out at build time from the server's
	own description (docs/openapi.json): every operation in its group, folded
	to its method, path and summary. Who may ask is in each operation's
	words; the scopes a token can have are the description's.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { API_DESCRIPTION, developersHref } from '#lib/app/routes.ts';
	import OperationView from '#lib/developers/OperationView.svelte';
	import { description, groupsOf, scopesOf } from '#lib/developers/reference.ts';

	const groups = groupsOf(description);
	const scopes = scopesOf(description);
	const count = groups.reduce((sum, group) => sum + group.operations.length, 0);

	/** An operation the address names is shown unfolded: a link to one leads to what it says. */
	function unfoldNamed(): void {
		const named = location.hash
			? document.getElementById(decodeURIComponent(location.hash.slice(1)))
			: null;
		if (named instanceof HTMLDetailsElement) {
			named.open = true;
			named.scrollIntoView();
		}
	}

	onMount(() => {
		unfoldNamed();
		addEventListener('hashchange', unfoldNamed);
		return () => removeEventListener('hashchange', unfoldNamed);
	});
</script>

<svelte:head>
	<title>REST API · Developers · Venti</title>
</svelte:head>

<h1>REST API</h1>
<p class="lead">
	{count} operations under <code>/v1</code>, from the server’s own
	<a class="text-link" href={API_DESCRIPTION} rel="external">description</a>, version
	{description.info.version}.
	<a class="text-link" href={developersHref('api/schemas')}>Schemas</a>
</p>

<nav class="groups" aria-label="Groups of the API">
	<ul>
		{#each groups as group (group.id)}
			<li><a href="#{group.id}">{group.name}</a></li>
		{/each}
	</ul>
</nav>

<section class="scopes" aria-labelledby="api-scopes">
	<h2 id="api-scopes">Scopes of a token</h2>
	<p>
		{#each scopes as scope (scope)}<code>{scope}</code>{/each}
	</p>
</section>

{#each groups as group (group.id)}
	<section class="group" aria-labelledby={group.id}>
		<h2 id={group.id}>{group.name}</h2>
		<div class="operations">
			{#each group.operations as operation (operation.id)}
				<OperationView {operation} />
			{/each}
		</div>
	</section>
{/each}

<style>
	code {
		padding: 0 var(--space-1);
		border-radius: var(--radius-s);
		background: var(--fill);
		font-size: var(--subheadline);
	}

	.groups ul {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-1) var(--space-4);
		margin-bottom: var(--space-5);
	}

	.groups a {
		display: inline-flex;
		align-items: center;
		min-height: var(--target);
		color: var(--link);
	}

	.scopes p {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
	}

	h2 {
		margin-bottom: var(--space-2);
		font-size: var(--title);
		scroll-margin-top: calc(var(--nav-top) + var(--space-4));
	}

	.scopes,
	.group {
		margin-bottom: var(--space-6);
	}

	.operations {
		border-bottom: 1px solid var(--separator);
	}

	.operations :global(details) {
		scroll-margin-top: calc(var(--nav-top) + var(--space-4));
	}
</style>
