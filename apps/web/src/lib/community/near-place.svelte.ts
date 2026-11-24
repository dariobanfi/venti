/**
 * The place Community's lists near a place are for: the pilots near and
 * the open plans near. The browser's position where the pilot has allowed
 * the site to have it already (nothing is asked: the browser is not made
 * to put its question for these lists); else the place the home page's
 * weather is for (`place.ts`: the position the browser gave before, or a
 * take-off chosen there); else the region's middle, which is where the map
 * opens. The server takes the place to about a kilometre and keeps it
 * nowhere.
 */

import type { Coordinate } from '#lib/core/index.ts';
import { rememberedPlace } from '#lib/home/place.ts';
import { watchPosition } from '#lib/map/position.ts';
import { FIRST_REGION } from '#lib/today/model.svelte.ts';

export class NearPlace {
	/** Where the lists are for: what this browser remembers, until the position comes. */
	place = $state.raw<Coordinate>(rememberedPlace()?.at ?? FIRST_REGION);
	/** Whether the place is the browser's position. */
	isPosition = $state(false);

	/**
	 * Takes the browser's position once, where it was allowed already. For a
	 * screen's effect; returns what to call when the screen goes away.
	 */
	watch(): () => void {
		let current = true;
		let stop = () => {};
		navigator.permissions
			?.query({ name: 'geolocation' })
			.then((status) => {
				if (!current || status.state !== 'granted') return;
				stop = watchPosition(
					navigator.geolocation,
					(fix) => {
						stop();
						if (!current) return;
						this.place = { lat: fix.lat, lon: fix.lon };
						this.isPosition = true;
					},
					() => {}
				);
			})
			.catch(() => {});
		return () => {
			current = false;
			stop();
		};
	}
}
