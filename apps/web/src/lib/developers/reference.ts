/**
 * The REST API's reference, made at build time from the server's own
 * description, `docs/openapi.json`, which a test of the server holds equal
 * to what `GET /v1/openapi.json` hands out. Nothing of it is written here:
 * this file only puts the operations in groups and in order, and says what
 * a schema is in a few words.
 */

import source from '../../../../../docs/openapi.json?raw';
import { SOURCE_CODE } from '#lib/app/routes.ts';
import { anchorOf } from '#lib/news/markdown.ts';

/** A JSON Schema as the description writes one, as far as the pages read it. */
export interface Schema {
	$ref?: string;
	type?: string | string[];
	format?: string;
	description?: string;
	properties?: Record<string, Schema>;
	required?: string[];
	items?: Schema;
	anyOf?: Schema[];
	oneOf?: Schema[];
	allOf?: Schema[];
	enum?: unknown[];
	minimum?: number;
	maximum?: number;
	'x-venti-values'?: string[];
}

interface MediaType {
	schema?: Schema;
	example?: unknown;
	examples?: Record<string, { summary?: string; value?: unknown }>;
}

interface Answer {
	$ref?: string;
	description?: string;
	content?: Record<string, MediaType>;
}

interface Parameter {
	name: string;
	in: string;
	required?: boolean;
	description?: string;
	schema?: Schema;
	example?: unknown;
}

interface RawOperation {
	operationId: string;
	summary?: string;
	description?: string;
	tags?: string[];
	parameters?: Parameter[];
	requestBody?: { required?: boolean; content?: Record<string, MediaType> };
	responses: Record<string, Answer>;
	security?: Record<string, string[]>[];
	'x-venti-docs'?: string;
}

export interface Description {
	openapi: string;
	info: { title: string; summary?: string; description?: string; version: string };
	paths: Record<string, Record<string, RawOperation>>;
	components: {
		schemas: Record<string, Schema>;
		responses: Record<string, Answer>;
		securitySchemes: Record<string, { description?: string; 'x-venti-scopes'?: string[] }>;
	};
}

/** An example the description carries, by its name. */
export interface Example {
	name: string;
	json: string;
}

/** A body: of the request or of one answer. */
export interface Body {
	mediaType: string;
	schema: Schema | undefined;
	examples: Example[];
}

export interface Response {
	status: string;
	description: string;
	bodies: Body[];
}

export interface Operation {
	/** The anchor of the operation on the page: its `operationId`. */
	id: string;
	method: string;
	path: string;
	summary: string;
	description: string;
	parameters: Parameter[];
	request: Body[];
	responses: Response[];
	/** The heading of docs/api.md that says it in words, as a link. */
	docs: string | undefined;
}

export interface Group {
	id: string;
	name: string;
	operations: Operation[];
}

const METHODS = ['get', 'post', 'put', 'patch', 'delete'] as const;

/**
 * The groups the reference is read in, by the first part of a path after
 * `/v1/`, while the description names no tags. A path of a part not named
 * here gets a group of its own name; an operation with tags is put under
 * its first tag.
 */
const GROUPS: [string, string[]][] = [
	['The server', ['health', 'openapi.json']],
	['Weather', ['stations', 'forecast', 'thermals', 'radar', 'foehn', 'route']],
	['Airspace and maps', ['airspace', 'cables', 'maps', 'imagery']],
	['Sites', ['sites', 'dumps']],
	['Flights and pilots', ['flights', 'pilots', 'ranking', 'reports']],
	['Live tracks', ['live']],
	['Groups and plans', ['groups']],
	['Accounts', ['accounts', 'sessions', 'auth', 'account', 'push', 'oauth', 'dev']]
];

/** The description the site was built with. */
export const description: Description = JSON.parse(source);

/** Where docs/api.md says a heading's part in words. */
export function apiDocsHref(heading: string): string {
	return `${SOURCE_CODE}/blob/main/docs/api.md#${anchorOf(heading)}`;
}

/** The name a `$ref` points at: `#/components/schemas/Site` is `Site`. */
export function refName(ref: string): string {
	return ref.slice(ref.lastIndexOf('/') + 1);
}

function examplesOf(media: MediaType): Example[] {
	const found: Example[] = [];
	if (media.example !== undefined)
		found.push({ name: 'Example', json: JSON.stringify(media.example, null, 2) });
	for (const [name, example] of Object.entries(media.examples ?? {})) {
		found.push({ name: example.summary ?? name, json: JSON.stringify(example.value, null, 2) });
	}
	return found;
}

function bodies(content: Record<string, MediaType> | undefined): Body[] {
	return Object.entries(content ?? {}).map(([mediaType, media]) => ({
		mediaType,
		schema: media.schema,
		examples: examplesOf(media)
	}));
}

/** An answer, with a shared one (`#/components/responses/…`) read where it is. */
function response(document: Description, status: string, answer: Answer): Response {
	const shared = answer.$ref ? document.components.responses[refName(answer.$ref)] : answer;
	return {
		status,
		description: shared?.description ?? '',
		bodies: bodies(shared?.content)
	};
}

/** Every operation of a description, in groups, in the order of the groups and of the paths. */
export function groupsOf(document: Description): Group[] {
	const byName = new Map<string, Operation[]>();
	const order = GROUPS.map(([name]) => name);
	for (const [name] of GROUPS) byName.set(name, []);
	for (const [path, item] of Object.entries(document.paths)) {
		const part = path.split('/')[2] ?? '';
		for (const method of METHODS) {
			const raw = item[method];
			if (!raw) continue;
			const name = raw.tags?.[0] ?? GROUPS.find(([, parts]) => parts.includes(part))?.[0] ?? part;
			if (!byName.has(name)) {
				byName.set(name, []);
				order.push(name);
			}
			byName.get(name)?.push({
				id: raw.operationId,
				method: method.toUpperCase(),
				path,
				summary: raw.summary ?? '',
				description: raw.description ?? '',
				parameters: raw.parameters ?? [],
				request: bodies(raw.requestBody?.content),
				responses: Object.entries(raw.responses).map(([status, answer]) =>
					response(document, status, answer)
				),
				docs: raw['x-venti-docs'] ? apiDocsHref(raw['x-venti-docs']) : undefined
			});
		}
	}
	return order
		.map((name) => ({ id: anchorOf(name), name, operations: byName.get(name) ?? [] }))
		.filter((group) => group.operations.length > 0);
}

/** The schemas the description names, in the order of their names. */
export function schemasOf(document: Description): [string, Schema][] {
	return Object.entries(document.components.schemas).sort(([a], [b]) => a.localeCompare(b));
}

/** The scopes a token for a program can be given, as the description lists them. */
export function scopesOf(document: Description): string[] {
	return document.components.securitySchemes.token?.['x-venti-scopes'] ?? [];
}

/** A schema's kind in a few words, without what a `$ref` or a list holds. */
export function kindOf(schema: Schema): string {
	const types = Array.isArray(schema.type) ? schema.type : schema.type ? [schema.type] : [];
	const named = types.map((type) => (type === 'null' ? 'null' : type));
	const kind = named.join(' or ');
	return schema.format ? `${kind} (${schema.format})` : kind;
}

/** Bounds of a number, as "1 to 100". */
export function boundsOf(schema: Schema): string {
	if (schema.minimum !== undefined && schema.maximum !== undefined)
		return `${schema.minimum} to ${schema.maximum}`;
	if (schema.minimum !== undefined) return `at least ${schema.minimum}`;
	if (schema.maximum !== undefined) return `at most ${schema.maximum}`;
	return '';
}
