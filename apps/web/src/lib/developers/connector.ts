/**
 * The connector's tools, asked of the server the site is served by when
 * the page opens: the tools are defined once in Rust (`crates/tools`) and
 * nowhere in a file the site's build can read, and which of them a server
 * offers depends on what it collects. One `tools/list` in the protocol's
 * newest revision, which needs no conversation before it (docs/mcp.md,
 * "The protocol").
 */

import type { Schema } from './reference.ts';

/** The revision of the Model Context Protocol the page speaks. */
const REVISION = '2026-07-28';

/** Where the connector is on the site's own origin. */
export const CONNECTOR_PATH = '/mcp';

export interface Tool {
	name: string;
	title?: string;
	description: string;
	inputSchema: Schema;
	annotations?: { readOnlyHint?: boolean };
}

interface Answer {
	result?: { tools?: Tool[]; nextCursor?: string };
	error?: { message?: string };
}

/** Every tool the connector lists, page after page. */
export async function listTools(ask: typeof fetch = fetch): Promise<Tool[]> {
	const tools: Tool[] = [];
	let cursor: string | undefined;
	for (let page = 0; page < 20; page++) {
		const response = await ask(CONNECTOR_PATH, {
			method: 'POST',
			headers: {
				'Content-Type': 'application/json',
				Accept: 'application/json',
				'MCP-Protocol-Version': REVISION,
				'Mcp-Method': 'tools/list'
			},
			body: JSON.stringify({
				jsonrpc: '2.0',
				id: page + 1,
				method: 'tools/list',
				params: {
					...(cursor ? { cursor } : {}),
					_meta: {
						'io.modelcontextprotocol/protocolVersion': REVISION,
						'io.modelcontextprotocol/clientCapabilities': {}
					}
				}
			})
		});
		const answer = (await response.json().catch(() => ({}))) as Answer;
		if (!response.ok || !answer.result) {
			throw new Error(answer.error?.message ?? `The connector answered ${response.status}.`);
		}
		tools.push(...(answer.result.tools ?? []));
		cursor = answer.result.nextCursor;
		if (!cursor) break;
	}
	return tools;
}
