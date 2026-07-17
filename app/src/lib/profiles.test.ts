// Run: npm test  (node --test; Node strips the types itself). No test
// framework and no @types/node on purpose — the frontend dependency list stays
// as scaffolded. A throw is a failing exit code, which is all a runner needs.
import { primaryProfileDir, profileLabels } from "./profiles.ts";
import type { Profile, SettingsFile } from "./api.ts";

const check = (name: string, ok: boolean) => {
  if (!ok) throw new Error(`FAIL: ${name}`);
  console.log(`  ok - ${name}`);
};

const profile = (install: string, server: string, name: string): Profile => ({
  install,
  server,
  profile: name,
  dir: `/roots/${install}_${server}/settings_${name}`,
  files: [],
});

// The common case: distinct server/profile pairs need no install noise.
{
  const tq = profile("c_eve_sharedcache_tq", "tranquility", "Default");
  const sisi = profile("c_eve_sharedcache_sisi", "singularity", "Default");
  const labels = profileLabels([tq, sisi]);
  check(
    "unique pair labels as '<server> / <profile>'",
    labels.get(tq.dir) === "tranquility / Default",
  );
  check(
    "the other unique pair is unaffected",
    labels.get(sisi.dir) === "singularity / Default",
  );
}

// The whole point: two installs, same server AND profile name. Without the
// install suffix both render "tranquility / Default" and the user cannot tell
// which one they are picking.
{
  const a = profile("c_eve_sharedcache_tq", "tranquility", "Default");
  const b = profile("g_eve_shared_cache_sharedcache_tq", "tranquility", "Default");
  const labels = profileLabels([a, b]);
  check(
    "colliding pair appends the install name (a)",
    labels.get(a.dir) === "tranquility / Default · c_eve_sharedcache_tq",
  );
  check(
    "colliding pair appends the install name (b)",
    labels.get(b.dir) === "tranquility / Default · g_eve_shared_cache_sharedcache_tq",
  );
  check("colliding labels are distinct", labels.get(a.dir) !== labels.get(b.dir));
}

// A collision must not drag an unrelated profile into verbose mode.
{
  const a = profile("install_a", "tranquility", "Default");
  const b = profile("install_b", "tranquility", "Default");
  const other = profile("install_a", "singularity", "Default");
  const labels = profileLabels([a, b, other]);
  check(
    "non-colliding profile stays short despite a collision elsewhere",
    labels.get(other.dir) === "singularity / Default",
  );
}

// Same profile name on different servers is not a collision.
{
  const a = profile("i", "tranquility", "Omega");
  const b = profile("i", "singularity", "Omega");
  const labels = profileLabels([a, b]);
  check(
    "same profile name on different servers needs no install",
    labels.get(a.dir) === "tranquility / Omega" && labels.get(b.dir) === "singularity / Omega",
  );
}

{
  check("no profiles yields no labels", profileLabels([]).size === 0);
}

// primaryProfileDir: the profile actually in use is the most recently touched.
const file = (name: string, modified: number | null): SettingsFile => ({
  path: `/x/${name}`,
  file_name: name,
  kind: "char",
  id: 1,
  size: 1,
  modified_unix: modified,
});

const withFiles = (dir: string, files: SettingsFile[]): Profile => ({
  install: "i",
  server: "tranquility",
  profile: "Default",
  dir,
  files,
});

{
  const older = withFiles("/a", [file("core_char_1.dat", 100)]);
  const newer = withFiles("/b", [file("core_char_2.dat", 500)]);
  check(
    "picks the most recently touched profile",
    primaryProfileDir([older, newer]) === "/b",
  );
  check("order does not matter", primaryProfileDir([newer, older]) === "/b");
}

{
  // A profile's recency is its newest file, not its first or its average.
  const a = withFiles("/a", [file("x.dat", 10), file("y.dat", 900)]);
  const b = withFiles("/b", [file("z.dat", 500)]);
  check("a profile is as recent as its newest file", primaryProfileDir([a, b]) === "/a");
}

{
  check("no profiles yields null", primaryProfileDir([]) === null);
  check(
    "profiles without timestamps yield null rather than a false guess",
    primaryProfileDir([withFiles("/a", [file("x.dat", null)]), withFiles("/b", [])]) === null,
  );
}

console.log("profiles.test.ts: all checks passed");
