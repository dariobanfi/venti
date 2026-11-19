/**
 * The core's side of the conversation in `protocol.ts`. This file is the
 * worker: it loads the WebAssembly module, opens the browser's store, and
 * answers the page's calls one after another.
 */

import { AccountSource } from './account-source';
import { AirspaceSource } from './airspace-source';
import { ForecastSource } from './forecast-source';
import { IdbStore } from './idb-store';
import init from './pkg/venti_core_wasm.js';
import wasmUrl from './pkg/venti_core_wasm_bg.wasm?url';
import { type Answering, answer, describeFailure, type Reply, type Request } from './protocol';
import { CoreSession } from './session';
import { SitesSource } from './sites-source';
import { type FileStore, MemoryStore } from './store';
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
	return {
		session,
		wind,
		account: new AccountSource(session),
		airspace: new AirspaceSource(session, () => wind.present()),
		sites: new SitesSource(() => wind.present()),
		forecast: new ForecastSource(() => wind.present())
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
