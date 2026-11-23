/**
 * The sources Venti's server is built with, each with its licence and
 * credit, as the server's own test writes them out from their manifests
 * (`manifest.toml` in each `crates/sources-*`) into `docs/sources.json`,
 * which the build reads. Nothing of it is written here; this file only
 * names the fields as the pages read them.
 */

import source from '../../../../../docs/sources.json?raw';

/** A licence as a manifest gives it. */
export interface Licence {
	name: string;
	/** The credit line, worded as the provider asks. */
	credit: string;
	link: string;
	notice: string | undefined;
	commercial: boolean;
	shareAlike: boolean;
	/** The day somebody last read the provider's terms. */
	verifiedOn: string;
}

export interface Source {
	id: string;
	provider: string;
	kinds: string[];
	countries: string[];
	licence: Licence;
	/** Data of others an adapter is built with, each under its own licence. */
	assets: { what: string; provider: string; licence: Licence }[];
}

interface RawLicence {
	name: string;
	credit: string;
	link: string;
	notice?: string;
	commercial: boolean;
	share_alike: boolean;
	verified_on: string;
}

interface RawSource {
	id: string;
	provider: string;
	kinds: string[];
	countries: string[];
	licence: RawLicence;
	assets?: { name: string; provider: string; licence: RawLicence }[];
}

function licenceOf(licence: RawLicence): Licence {
	return {
		name: licence.name,
		credit: licence.credit,
		link: licence.link,
		notice: licence.notice || undefined,
		commercial: licence.commercial,
		shareAlike: licence.share_alike,
		verifiedOn: licence.verified_on
	};
}

/** A source as `docs/sources.json` lists it. */
export function sourceOf(source: RawSource): Source {
	return {
		id: source.id,
		provider: source.provider,
		kinds: source.kinds,
		countries: source.countries,
		licence: licenceOf(source.licence),
		assets: (source.assets ?? []).map((asset) => ({
			what: asset.name,
			provider: asset.provider,
			licence: licenceOf(asset.licence)
		}))
	};
}

/** Every source the server is built with, by its provider's name. */
export const sources: Source[] = (JSON.parse(source) as { sources: RawSource[] }).sources
	.map(sourceOf)
	.sort((a, b) => a.provider.localeCompare(b.provider) || a.id.localeCompare(b.id));
