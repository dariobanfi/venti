/**
 * The core's side of the conversation in `protocol.ts`. This file is the
 * worker: it loads the WebAssembly module, opens the browser's store, and
 * answers the page's calls one after another.
 */

import { AccountSource } from './account-source';
import { AirspaceSource } from './airspace-source';
import { AlertsSource } from './alerts-source';
import { CablesSource } from './cables-source';
import { CommunitySource } from './community-source';
import { ForecastSource } from './forecast-source';
import { GroupsSource } from './groups-source';
import { GlideReachSource } from './glide-reach-source';
import { IdbStore } from './idb-store';
import { LiveSource } from './live-source';
import init from './pkg/venti_core_wasm.js';
import wasmUrl from './pkg/venti_core_wasm_bg.wasm?url';
import { type Answering, answer, describeFailure, type Reply, type Request } from './protocol';
import { RadarSource } from './radar-source';
import { RouteWeatherSource } from './route-weather-source';
import { CoreSession } from './session';
import { SiteEditSource } from './site-edit-source';
import { TakeoffsSource } from './takeoffs-source';
import { SitesSource } from './sites-source';
import { SlopesSource } from './slopes-source';
import { AirSource } from './air-source';
import { type FileStore, MemoryStore } from './store';
import { TerrainSource } from './terrain-source';
import { ThermalMapSource } from './thermal-map-source';
import { TodaySource } from './today-source';
import { TokensSource } from './tokens-source';
import { WindSource } from './wind-source';

const send = (reply: Reply) => postMessage(reply);

/**
 * The browser's store, or one in memory if the browser gives the page no
 * storage, as some do in private windows. The logbook then lasts until the
 * page is closed.
 */
async function openStore(): Promise<FileStore> {
	try {
		return await IdbStore.open();
	} catch {
		return new MemoryStore();
	}
}

const starting: Promise<Answering> = (async () => {
	await init({ module_or_path: wasmUrl });
	const session = await CoreSession.open(await openStore());
	session.onChangedElsewhere = () => send({ changedElsewhere: true });
	const wind = new WindSource();
	const sites = new SitesSource(() => wind.present());
	return {
		session,
		wind,
		account: new AccountSource(session),
		tokens: new TokensSource(session),
		alerts: new AlertsSource(session),
		airspace: new AirspaceSource(session, () => wind.present()),
		sites,
		siteEdits: new SiteEditSource(session, () => sites.wrote()),
		takeoffs: new TakeoffsSource(session, () => wind.takesSitesFromPilots()),
		forecast: new ForecastSource(() => wind.present()),
		live: new LiveSource(() => wind.present()),
		community: new CommunitySource(() => wind.present()),
		groups: new GroupsSource(session),
		terrain: new TerrainSource(),
		today: new TodaySource(
			session,
			() => wind.present(),
			() => wind.now()
		),
		thermals: new ThermalMapSource(
			() => wind.present(),
			() => wind.now()
		),
		radar: new RadarSource(
			() => wind.present(),
			() => wind.now()
		),
		routeWeather: new RouteWeatherSource(() => wind.present()),
		glideReach: new GlideReachSource(),
		cables: new CablesSource(() => wind.present()),
		slopes: new SlopesSource(),
		air: new AirSource(() => wind.present())
	};
})();

addEventListener('message', async (event: MessageEvent<Request>) => {
	const { id, call, given } = event.data;
	try {
		send({ id, answer: await answer(await starting, call, given) });
	} catch (error) {
		send({ id, failure: describeFailure(error) });
	}
});
