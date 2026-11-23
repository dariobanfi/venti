/**
 * The dumps of Venti's own sites, as the server lists them
 * (`GET /v1/dumps`, docs/api.md "Dumps of Venti's own sites"): asked when
 * the page opens, since what a layer holds is the server's.
 */

export interface DumpFile {
	content: string;
	format: string;
	path: string;
	media_type: string;
	bytes: number;
	sha256: string;
}

export interface DumpLayer {
	layer: string;
	provider: string;
	licence: string;
	licence_url: string;
	notice?: string;
	generated_at: string;
	cursor: string;
	sites: number;
	removed: number;
	revisions: number;
	files: DumpFile[];
}

export async function listDumps(ask: typeof fetch = fetch): Promise<DumpLayer[]> {
	const response = await ask('/v1/dumps', { headers: { Accept: 'application/json' } });
	if (!response.ok) throw new Error(`The server answered ${response.status}.`);
	const answer = (await response.json()) as { layers?: DumpLayer[] };
	return answer.layers ?? [];
}
