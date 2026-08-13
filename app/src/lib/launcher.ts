// Pure merge of the launcher's proposals onto the account cards. Kept out of the
// component so the routing rule — which card shows a ghost, which shows a
// dispute — is testable without mounting anything.
import type { Proposal } from "./api";

export interface CardProposals {
  /** Characters the launcher puts on this account that the store does not. */
  ghosts: number[];
  /** Chips ON this card the launcher disputes, and the account it names instead. */
  conflicts: { charId: number; target: number }[];
}

/**
 * Group proposals by the card that should show them.
 *
 * A disputed proposal is deliberately routed to `conflict` — the account whose
 * card holds the chip today — not to the account the launcher names. The user
 * needs to see the claim beside the thing it contradicts; showing it on the
 * target card would be a second, unexplained ghost.
 */
export function proposalsByCard(
  proposals: Proposal[],
  dismissed: ReadonlySet<number>,
): Map<number, CardProposals> {
  const out = new Map<number, CardProposals>();
  const card = (id: number) => {
    let c = out.get(id);
    if (!c) out.set(id, (c = { ghosts: [], conflicts: [] }));
    return c;
  };
  for (const p of proposals) {
    if (dismissed.has(p.char_id)) continue;
    if (p.conflict === null) card(p.user_id).ghosts.push(p.char_id);
    else card(p.conflict).conflicts.push({ charId: p.char_id, target: p.user_id });
  }
  return out;
}

/** Every undisputed proposal, shaped as `confirm_pairings`' argument. */
export function acceptAllPairs(
  proposals: Proposal[],
  dismissed: ReadonlySet<number>,
): [number, number][] {
  return proposals
    .filter((p) => p.conflict === null && !dismissed.has(p.char_id))
    .map((p) => [p.char_id, p.user_id]);
}
