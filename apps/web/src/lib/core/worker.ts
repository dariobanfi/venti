/**
 * The core's side of the conversation in `protocol.ts`. This file is the
 * worker: it loads the WebAssembly module, opens the browser's store, and
 * answers the page's calls one after another.
 */

import { AccountSource } from './account-source';
import { AgreementSource } from './agreement-source';
import { AirspaceSource } from './airspace-source';
import { AlertsSource } from './alerts-source';
import { CablesSource } from './cables-source';
import { CommunitySource } from './community-source';
import { FoehnSource } from './foehn-source';
import { ForecastSource } from './forecast-source';
import { GroupsSource } from './groups-source';
import { GlideReachSource } from './glide-reach-source';
import { IdbStore } from './idb-store';
import { LiveSource } from './live-source';
import { OpenPlansSource } from './open-plans-source';
import { OwnFlightsSource } from './own-flights-source';
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
import { SocialSource } from './social-source';
import { AirSource } from './air-source';
import { type FileStore, MemoryStore } from './store';
import { TerrainSource } from './terrain-source';
import { ThermalMapSource } from './thermal-map-source';
import { TodaySource } from './today-source';
import { TokensSource } from './tokens-source';
import { canRunWebAssembly, noWebAssembly } from './webassembly';
import { WindSource } from './wind-source';

const send = (reply: Reply) => postMessage(reply);

/**
 * The browser's store, or one in memory if the browser gives the page no
 * storage, as some do in private windows. What the core keeps (the account,
 * the pilot's own sites, airspace) then lasts until the page is closed.
 */
async function openStore(): Promise<FileStore> {
	try {
		return await IdbStore.open();
	} catch {
		return new MemoryStore();
	}
}

const starting: Promise<Answering> = (async () => {
	// The page asks first and starts no worker without it; this is for a
	// browser that gives a worker less than its page.
	if (!canRunWebAssembly()) throw noWebAssembly();
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
		agreement: new AgreementSource(session, () => wind.serverNow()),
		foehn: new FoehnSource(
			() => wind.present(),
			() => wind.serverNow()
		),
		alerts: new AlertsSource(session),
		social: new SocialSource(session, () => wind.serverNow()),
		airspace: new AirspaceSource(session, () => wind.present()),
		sites,
		siteEdits: new SiteEditSource(session, () => sites.wrote()),
		takeoffs: new TakeoffsSource(session, () => wind.takesSitesFromPilots()),
		forecast: new ForecastSource(() => wind.present()),
		live: new LiveSource(() => wind.present()),
		community: new CommunitySource(
			() => wind.present(),
			undefined,
			() => wind.serverNow()
		),
		own: new OwnFlightsSource(() => session.token()),
		groups: new GroupsSource(session),
		openPlans: new OpenPlansSource(session),
		terrain: new TerrainSource(),
		today: new TodaySource(
			session,
			() => wind.present(),
			() => wind.serverNow()
		),
		thermals: new ThermalMapSource(
			() => wind.present(),
			() => wind.serverNow()
		),
		radar: new RadarSource(
			() => wind.present(),
			() => wind.serverNow()
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
