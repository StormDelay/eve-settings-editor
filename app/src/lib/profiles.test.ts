// Run: npm test  (node --test; Node strips the types itself). No test
// framework and no @types/node on purpose — the frontend dependency list stays
// as scaffolded. A throw is a failing exit code, which is all a runner needs.
import { profileLabels } from "./profiles.ts";
import type { Profile } from "./api.ts";

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

console.log("profiles.test.ts: all checks passed");
