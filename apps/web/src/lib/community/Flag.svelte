<!--
	A country's flag, drawn by the reader's system from the two letters of
	its code (`country.ts`), and named for a screen reader. Nothing without
	a code.
-->
<script lang="ts">
	import { countryName, flag } from './country';

	interface Props {
		/** ISO 3166-1 alpha-2, as the server gives it. */
		code: string | undefined;
	}

	let { code }: Props = $props();

	const drawn = $derived(code ? flag(code) : '');
</script>

{#if code && drawn}
	<span class="flag" role="img" aria-label={countryName(code)} title={countryName(code)}
		>{drawn}</span
	>
{/if}

<style>
	.flag {
		flex: none;
		font-style: normal;
		line-height: 1;
	}
</style>
