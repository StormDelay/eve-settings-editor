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
 * Files only EVE writes. The editor writes `core_char_<id>.dat` and
 * `core_user_<id>.dat`; it never touches the anonymous ones. Verified over the
 * 2026-07-27/28 live captures: 11 editor-only captures touched none of these,
 * and all 4 captures taken after a client run touched three of them.
 */
const EVE_WRITTEN = /^core_(char|user)__\.dat$|^core_public__\.yaml$/;

/**
 * The profile actually in use: the one EVE wrote most recently.
 *
 * Ranking on ANY file — which this used to do — is wrong in the case that
 * matters. Players keep hand-made backups beside the live folder (one machine
 * had nine under a single profile), and editing one through this very editor
 * makes it the most recently touched. On 2026-07-28 that pinned a weeks-stale
 * backup to the top of the sidebar and a full round of work went into it.
 * Ranking on files only EVE writes cannot be moved by our own saves.
 *
 * `null` when there are no profiles, or none carries a usable timestamp —
 * callers then have nothing better to guess with. Ties keep the first, which is
 * discovery's alphabetical order.
 */
export function primaryProfileDir(profiles: Profile[]): string | null {
  const newest = (p: Profile, eveOnly: boolean) =>
    p.files.reduce(
      (max, f) =>
        eveOnly && !EVE_WRITTEN.test(f.file_name) ? max : Math.max(max, f.modified_unix ?? 0),
      0,
    );
  // Prefer the EVE-only signal. Fall back to any file only when no profile has
  // one at all, so a profile the client has never run in is still selectable.
  for (const eveOnly of [true, false]) {
    let best: string | null = null;
    let bestTime = 0;
    for (const p of profiles) {
      const t = newest(p, eveOnly);
      if (t > bestTime) {
        bestTime = t;
        best = p.dir;
      }
    }
    if (best) return best;
  }
  return null;
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
