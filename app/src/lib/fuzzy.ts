// Subsequence scoring for the palette. Hand-rolled and about forty lines,
// because a fuzzy library is not worth a dependency for a candidate set that
// peaks around seventy commands plus a few dozen characters and presets — at
// that size an O(n·m) scan is free, and it runs once per keystroke over a list
// that fits on one screen.

/** Diacritics fold away before comparing: EVE character names carry them, and a
 *  user typing `Renee` should still find `Renée`. */
const fold = (s: string): string =>
  s
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase();

const BOUNDARY = /[\s\-.]/;

/**
 * How well `query` matches, or `-Infinity` for no match at all.
 *
 * `extra` is the secondary haystack — keywords and the group name. Matching the
 * group is what makes typing `overv` surface every Overview command, and
 * `keywords` is where the old label of anything this phase renamed goes, so
 * muscle memory keeps working for a release or two.
 *
 * A match that landed ONLY in `extra` scores 40% lower than the same match in
 * the label: a command whose own name you typed should beat one that merely
 * shares a group with it.
 */
export function score(query: string, label: string, extra = ""): number {
  const q = fold(query).replace(/\s+/g, "");
  if (q === "") return 0;
  const lab = fold(label);
  const hay = extra ? `${lab} ${fold(extra)}` : lab;

  let total = 0;
  let qi = 0;
  let prev = -2;
  let first = -1;
  let inLabel = false;

  for (let i = 0; i < hay.length && qi < q.length; i++) {
    if (hay[i] !== q[qi]) continue;
    if (first < 0) first = i;
    total += 8;
    if (i === prev + 1) total += 10;
    if (i === 0 || BOUNDARY.test(hay[i - 1])) total += 12;
    if (i < lab.length) inLabel = true;
    prev = i;
    qi++;
  }

  if (qi < q.length) return -Infinity; // an unmatched query char is not a match
  if (lab.startsWith(q)) total += 20;
  total -= first; // one point per character skipped before the first hit
  return inLabel ? total : total * 0.6;
}

/** Sort helper: highest score first, ties broken by the caller's original order.
 *  `Array.prototype.sort` is stable in every engine this ships on, so registry
 *  order survives a tie without being re-stated here. */
export function rank<T>(items: T[], of: (t: T) => number): T[] {
  return items
    .map((item) => ({ item, s: of(item) }))
    .filter((x) => x.s > -Infinity)
    .sort((a, b) => b.s - a.s)
    .map((x) => x.item);
}
