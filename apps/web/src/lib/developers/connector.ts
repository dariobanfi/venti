/**
 * The connector's tools, as the server lists them to an assistant. They
 * are defined once in Rust (`crates/tools`); the server's own test writes
 * the list out into `docs/mcp-tools.json`, which the build reads, so the
 * page asks nothing when it opens. A server that collects less than
 * everything offers fewer of them (docs/mcp.md).
 */

import source from '../../../../../docs/mcp-tools.json?raw';
import type { Schema } from './reference.ts';

/** Where the connector is on the site's own origin. */
export const CONNECTOR_PATH = '/mcp';

export interface Tool {
	name: string;
	title?: string;
	description: string;
	inputSchema: Schema;
	annotations?: { readOnlyHint?: boolean };
}

/** Every tool of the connector, in the order the server lists them. */
export const tools: Tool[] = (JSON.parse(source) as { tools: Tool[] }).tools;
