<!--
	The app's symbols, drawn as outlines in the colour of the text around
	them. Where the design pack has a flying icon (design/assets/icons) it is
	that one, read from the pack at build time; the general ones the pack
	leaves to Lucide are the app's own, drawn at the pack's 2 px stroke.

	An icon says nothing to a screen reader. Whatever holds it carries the words.
-->
<script lang="ts" module>
	const shapes = {
		book: 'M6 4h10.5A1.5 1.5 0 0 1 18 5.5V17H7a2 2 0 0 0 0 4h11v-4M6 4a1.5 1.5 0 0 0-1.5 1.5V19',
		plus: 'M12 5v14M5 12h14',
		minus: 'M5 12h14',
		back: 'M15 5 8 12l7 7',
		forward: 'm9 5 7 7-7 7',
		info: 'M12 3.5a8.5 8.5 0 1 0 0 17 8.5 8.5 0 0 0 0-17ZM12 11v5.5M12 7.6v.2',
		close: 'm7 7 10 10M17 7 7 17',
		more: 'M6 12h.01M12 12h.01M18 12h.01',
		trash: 'M4.5 7h15M9.5 7V4.5h5V7M6.5 7l1 12.5h9l1-12.5M10 10.5v5.5M14 10.5v5.5',
		download: 'M12 4v11M7.5 10.5 12 15l4.5-4.5M5 19.5h14',
		play: 'M8 5.5v13l10.5-6.5L8 5.5Z',
		pause: 'M8.5 5.5v13M15.5 5.5v13',
		rain: 'M7 15.5a4.5 4.5 0 1 1 1.3-8.8 5.5 5.5 0 0 1 10.4 1.8A3.6 3.6 0 0 1 18 15.5M8.5 18.5l-1 2.5M12.5 18.5l-1 2.5M16.5 18.5l-1 2.5',
		location: 'M20 4 4.5 10.3l6.6 2.6 2.6 6.6L20 4Z',
		mountain: 'm2.5 19 7-12 3.8 6.5 2.4-3.4L21.5 19h-19Z',
		triangle: 'M12 5 3.5 19h17L12 5Z',
		// Lucide's "list-filter": the pack has no icon for a filter.
		filter: 'M4 7h16M7 12h10M10 17h4',
		settings: 'M4 7h8M16 7h4M4 12h2M10 12h10M4 17h10M18 17h2M14 4.5v5M8 9.5v5M16 14.5v5',
		link: 'M10.5 13.5a4 4 0 0 0 5.66 0l2.84-2.84a4 4 0 0 0-5.66-5.66l-.84.84M13.5 10.5a4 4 0 0 0-5.66 0L5 13.34A4 4 0 0 0 10.66 19l.84-.84',
		check: 'M5 12.5 10 17.5 19 7',
		copy: 'M9 8.5h10.5V20H9V8.5ZM15 5H4.5v11.5',
		person: 'M12 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8ZM4.5 20a7.5 7.5 0 0 1 15 0',
		calendar: 'M4.5 6.5h15v13h-15v-13ZM4.5 10.5h15M8.5 4v4M15.5 4v4',
		key: 'M8 11a3.5 3.5 0 1 0 0 7 3.5 3.5 0 0 0 0-7ZM10.6 12.2 18.5 4.5M15.5 7.5l2.5 2.5M13 10l1.8 1.8',
		verified: 'M12 3.5a8.5 8.5 0 1 0 0 17 8.5 8.5 0 0 0 0-17ZM8 12.3l2.8 2.8L16 9.5',
		nosign: 'M12 3.5a8.5 8.5 0 1 0 0 17 8.5 8.5 0 0 0 0-17ZM6 6l12 12',
		question:
			'M12 3.5a8.5 8.5 0 1 0 0 17 8.5 8.5 0 0 0 0-17ZM9.6 9.7a2.5 2.5 0 1 1 3.7 2.2c-.8.5-1.3 1-1.3 2M12 16.6v.1'
	} as const;

	const PACK = '../../../../../design/assets/icons/';
	const files = import.meta.glob('../../../../../design/assets/icons/*.svg', {
		query: '?raw',
		import: 'default',
		eager: true
	}) as Record<string, string>;

	/** What is inside the `svg` element of one of the pack's icons: its own drawing, as the pack has it. */
	function drawing(file: string): string {
		return files[`${PACK}${file}.svg`]
			.replace(/^[\s\S]*?<svg[^>]*>/, '')
			.replace(/<\/svg>\s*$/, '');
	}

	/** The pack's flying icons, by the names the app calls them. */
	const pack = {
		map: drawing('map'),
		layers: drawing('layers'),
		wind: drawing('wind'),
		warning: drawing('caution'),
		glider: drawing('wing'),
		pin: drawing('site'),
		track: drawing('track'),
		route: drawing('triangle'),
		league: drawing('league'),
		forecast: drawing('forecast'),
		good: drawing('good'),
		danger: drawing('danger'),
		arrowWind: drawing('arrow-wind'),
		takeoff: drawing('takeoff'),
		airspace: drawing('airspace'),
		thermal: drawing('thermal')
	} as const;

	export type IconName = keyof typeof shapes | keyof typeof pack;
</script>

<script lang="ts">
	interface Props {
		name: IconName;
		/** Filled in, for a control that is switched on. */
		filled?: boolean;
	}

	let { name, filled = false }: Props = $props();
</script>

<svg
	viewBox="0 0 24 24"
	width="1.5em"
	height="1.5em"
	fill={filled ? 'currentColor' : 'none'}
	stroke="currentColor"
	stroke-width={name === 'more' ? 2.8 : 2}
	stroke-linecap="round"
	stroke-linejoin="round"
	aria-hidden="true"
	focusable="false"
>
	{#if name in pack}
		<!-- The pack's own file, read at build time: no one else's text. -->
		{@html pack[name as keyof typeof pack]}
	{:else}
		<path d={shapes[name as keyof typeof shapes]} />
	{/if}
</svg>

<style>
	svg {
		display: block;
		flex: none;
	}
</style>
