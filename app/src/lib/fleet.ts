// EVE's Broadcast Settings dialog, row by row: the internal type token (the
// suffix of `listenBroadcast_<Type>` and `fleet_broadcastcolor_<Type>`) and
// the label, sentence-cased per the copy standard. "Broadcast:" is dropped —
// every row bar one would carry it. The order is EVE's (spec §2.2).
export const BROADCASTS: { type: string; label: string }[] = [
  { type: "HealArmor", label: "Need armor" },
  { type: "HealCapacitor", label: "Need capacitor" },
  { type: "NeedBackup", label: "Need backup" },
  { type: "Target", label: "Target" },
  { type: "HealShield", label: "Need shield" },
  { type: "WarpTo", label: "Warp to" },
  { type: "TravelTo", label: "Travel to" },
  { type: "Event", label: "Fleet event" },
  { type: "JumpTo", label: "Jump to" },
  { type: "AlignTo", label: "Align to" },
  { type: "HealTarget", label: "Repair target" },
  { type: "InPosition", label: "In position at" },
  { type: "EnemySpotted", label: "Spotted an enemy" },
  { type: "HoldPosition", label: "Request that the fleet hold position" },
  { type: "JumpBeacon", label: "Jump to beacon" },
  { type: "Location", label: "At location" },
];

/** The dialog's top checkbox. One field; the backend writes its two keys. */
export const SHOW_OWN = { field: "listen_show_own", label: "Always show my own broadcasts" };

/** The projection's field name for a type's checkbox. */
export const listenField = (type: string): string => `listen_${type}`;
