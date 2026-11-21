/**
 * The order of a table whose headings sort, for the tables of a wide
 * screen: a click on a heading orders by its column, a second click turns
 * the order round. Equal values keep the order the rows came in.
 */

export interface Sort<Key extends string> {
	key: Key;
	/** Largest or latest first. */
	descending: boolean;
}

/**
 * A click on a column's heading: the same column turns its order round, a
 * new one starts with the largest or latest first, a column of words from A.
 */
export function sortBy<Key extends string>(
	sort: Sort<Key> | undefined,
	key: Key,
	words: readonly Key[] = []
): Sort<Key> {
	if (sort?.key === key) return { key, descending: !sort.descending };
	return { key, descending: !words.includes(key) };
}

/** The rows in the order asked for; without one, as they came. */
export function sorted<Row, Key extends string>(
	rows: readonly Row[],
	sort: Sort<Key> | undefined,
	value: (row: Row, key: Key) => number | string
): Row[] {
	if (!sort) return [...rows];
	const direction = sort.descending ? -1 : 1;
	return rows
		.map((row, index) => ({ row, index, of: value(row, sort.key) }))
		.toSorted((one, other) => {
			const order =
				typeof one.of === 'string' && typeof other.of === 'string'
					? one.of.localeCompare(other.of)
					: (one.of as number) - (other.of as number);
			return order * direction || one.index - other.index;
		})
		.map(({ row }) => row);
}

/** What a heading tells a screen reader of the order: `aria-sort`. */
export function ariaSort<Key extends string>(
	sort: Sort<Key> | undefined,
	key: Key
): 'ascending' | 'descending' | 'none' {
	if (sort?.key !== key) return 'none';
	return sort.descending ? 'descending' : 'ascending';
}
