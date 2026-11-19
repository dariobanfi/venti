/**
 * The public endpoints the map's tiles come from, until the project has its
 * own tile server. `docs/architecture.md` has their terms.
 *
 * They stand in a file of their own because two things need them: the map
 * style (`style.ts`) and the site's content security policy
 * (`../app/security-policy.ts`), which lets the app talk to these hosts and
 * to no others. The build reads the policy, so this file imports nothing.
 */
export const ONLINE = {
	/** OpenFreeMap: the base map, as a description of where its tiles are. */
	basemap: 'https://tiles.openfreemap.org/planet',
	/** Mapterhorn: elevation, for the relief shading and the terrain in 3D. */
	terrainTiles: 'https://tiles.mapterhorn.com/{z}/{x}/{y}.webp',
	/**
	 * Mapterhorn serves terrain to this zoom everywhere; finer tiles exist
	 * only for some countries. Asking for no more keeps the relief free of
	 * holes.
	 */
	terrainMaxZoom: 12,
	terrainAttribution: '<a href="https://mapterhorn.com/attribution">© Mapterhorn</a>'
} as const;
