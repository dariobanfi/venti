// The types of scripts/notices.mjs, for its unit test and vite.config.ts.

export interface Notices {
	/** Each package with the index of its licence text in `texts`. */
	packages: { name: string; version: string; licence: string; text: number }[];
	/** Each distinct licence text once. */
	texts: string[];
}

export const BUNDLED_FROM_DEV: string[];
export const OUT: string;

export function notices(
	lock: {
		packages: Record<
			string,
			{
				version?: string;
				license?: string;
				dev?: boolean;
				devOptional?: boolean;
				optional?: boolean;
			}
		>;
	},
	list: (path: string) => string[],
	read: (path: string) => string,
	extra?: string[]
): Notices;
