import type { Profile } from "./api";

/**
 * How a profile is labelled in the UI, keyed by its `dir` (the one field that is
 * always unique).
 *
 * `<server> / <profile>` is enough almost always. But two installs can hold the
 * same server AND profile name — a SharedCache dir and a legacy one both with
 * settings_Default — and then the pair alone is ambiguous. In that case, and
 * only in that case, the install name is appended to tell them apart, so the
 * common case stays short.
 */
/**
 * The profile actually in use: the one whose files were touched most recently.
 * `null` when there are no profiles, or when none carries a usable timestamp —
 * callers then have nothing better to guess with. Ties keep the first, which is
 * discovery's alphabetical order.
 */
export function primaryProfileDir(profiles: Profile[]): string | null {
  let best: string | null = null;
  let bestTime = 0;
  for (const p of profiles) {
    const touched = p.files.reduce((max, f) => Math.max(max, f.modified_unix ?? 0), 0);
    if (touched > bestTime) {
      bestTime = touched;
      best = p.dir;
    }
  }
  return best;
}

export function profileLabels(profiles: Profile[]): Map<string, string> {
  const key = (p: Profile) => `${p.server} / ${p.profile}`;
  const seen = new Map<string, number>();
  for (const p of profiles) seen.set(key(p), (seen.get(key(p)) ?? 0) + 1);
  return new Map(
    profiles.map((p) => [
      p.dir,
      seen.get(key(p))! > 1 ? `${key(p)} · ${p.install}` : key(p),
    ]),
  );
}
