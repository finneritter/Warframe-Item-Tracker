// Presentation helpers ported from the wireframe.

import { loadPrefs } from "./prefs";
import type { SyncResult } from "./types";

export const clsx = (...a: (string | false | null | undefined)[]) => a.filter(Boolean).join(" ");

// One shared formatter instance — constructing Intl.NumberFormat per call is
// surprisingly costly when `fmt` runs hundreds of times per render (big grids).
const NUM_FMT = new Intl.NumberFormat("en-US");

export const fmt = (n: number | null | undefined): string =>
  n == null ? "—" : NUM_FMT.format(Math.round(n));

// Hard-rounded plat for headline/aggregate numbers — never imply false precision.
// 980 → "980", 6177 → "6.2k", 28757 → "29k".
export const fmtK = (n: number | null | undefined): string => {
  if (n == null) return "—";
  const v = Math.round(n);
  if (v < 1000) return String(v);
  const k = v / 1000;
  return `${k < 10 ? k.toFixed(1) : Math.round(k)}k`;
};

export const pct = (n: number): string => `${n >= 0 ? "+" : ""}${n.toFixed(1)}%`;

/** Human byte size — "1.2 MB" / "640 KB" (min 1 KB). */
export const fmtBytes = (n: number): string =>
  n >= 1_048_576 ? `${(n / 1_048_576).toFixed(1)} MB` : `${Math.max(1, Math.round(n / 1024))} KB`;

/** plat × qty line total, null-safe. */
export const lineTotal = (plat: number | null | undefined, qty: number): number =>
  (plat ?? 0) * qty;

/** A priced row has reached its buy target (price fell to/below target). */
export const atTarget = (r: {
  median_plat: number | null;
  target_plat: number | null;
}): boolean => r.target_plat != null && r.median_plat != null && r.median_plat <= r.target_plat;

export const TIERS = [
  { key: "exotic", min: 120, label: "120p+" },
  { key: "legend", min: 45, label: "45–119p" },
  { key: "rare", min: 15, label: "15–44p" },
  { key: "basic", min: 0, label: "<15p" },
] as const;

export const tier = (p: number | null | undefined): string => {
  const v = p ?? 0;
  return TIERS.find((t) => v >= t.min)?.key ?? "basic";
};

/** Two-letter monogram from a name. */
export const glyph = (name: string): string =>
  name
    .split(/\s+/)
    .slice(0, 2)
    .map((w) => w[0]?.toUpperCase() ?? "")
    .join("");

/** trend class from a delta sign (±1% flat band). */
export const trendOf = (delta: number | null | undefined): "up" | "down" | "flat" => {
  if (delta == null) return "flat";
  if (delta > 1) return "up";
  if (delta < -1) return "down";
  return "flat";
};

/** "today" / "yesterday" / "Nd ago" from an ISO timestamp. */
export const relativeDay = (iso: string): string => {
  const then = new Date(iso);
  const now = new Date();
  const startOf = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const days = Math.round((startOf(now) - startOf(then)) / 86_400_000);
  if (days <= 0) return "today";
  if (days === 1) return "yesterday";
  return `${days}d ago`;
};

/** "synced Nm ago" from an ISO timestamp (or "never"). */
export const syncedAgo = (iso: string | null): string => {
  if (!iso) return "never";
  const secs = Math.max(0, (Date.now() - new Date(iso).getTime()) / 1000);
  if (secs < 60) return "now";
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h`;
  return `${Math.floor(hrs / 24)}d`;
};

/** One-line outcome of a listings sync. A sync that matched nothing must never
 *  read the same as a sync that found nothing — that ambiguity is what made a
 *  broken Sync look like an idle one. */
export const syncListingsNote = (r: SyncResult): string => {
  const s = (n: number) => (n === 1 ? "" : "s");
  if (r.kept)
    return r.fetched === 0
      ? `warframe.market showed no orders, but without a connected session it can only see your visible ones — your ${r.mirrored} listing${s(r.mirrored)} were left as they were. Connect a session to be sure.`
      : `warframe.market returned ${r.fetched} order${s(r.fetched)}, none of them items WFIT tracks — your listings were left as they were`;
  if (r.fetched === 0) return "No open orders on warframe.market";
  const base = `Synced ${r.mirrored} listing${s(r.mirrored)}`;
  return r.untracked > 0 ? `${base} · ${r.untracked} not tracked by WFIT` : base;
};

/** ms remaining until an ISO timestamp (negative if past). */
export const msUntil = (iso: string | null | undefined): number =>
  iso ? new Date(iso).getTime() - Date.now() : Number.NEGATIVE_INFINITY;

/** Short label of the active display zone — "EDT", "GMT+2", … — i.e. the
 *  detected PC zone on "auto", or the configured override. Pairs with hhmm(). */
export const tzLabel = (): string => {
  const tz = loadPrefs().timezone;
  try {
    return (
      new Intl.DateTimeFormat([], {
        timeZone: tz === "auto" ? undefined : tz,
        timeZoneName: "short",
      })
        .formatToParts(new Date())
        .find((p) => p.type === "timeZoneName")?.value ?? "local"
    );
  } catch {
    return "local";
  }
};

/** "HH:MM" clock time in the configured display zone (Prefs.timezone;
 *  "auto" = the PC's zone). An invalid stored zone falls back to local. */
export const hhmm = (iso: string): string => {
  const tz = loadPrefs().timezone;
  const opts: Intl.DateTimeFormatOptions = { hour: "2-digit", minute: "2-digit" };
  if (tz !== "auto") {
    try {
      return new Date(iso).toLocaleTimeString([], { ...opts, timeZone: tz });
    } catch {
      // unknown zone string — fall through to the local clock
    }
  }
  return new Date(iso).toLocaleTimeString([], opts);
};

/** "Sat 03:00" — weekday + clock time in the configured display zone, for
 *  schedule entries that can be days out (arbitration "ones of note"). */
export const dayTime = (iso: string): string => {
  const tz = loadPrefs().timezone;
  const opts: Intl.DateTimeFormatOptions = {
    weekday: "short",
    hour: "2-digit",
    minute: "2-digit",
  };
  if (tz !== "auto") {
    try {
      return new Date(iso).toLocaleString([], { ...opts, timeZone: tz });
    } catch {
      // unknown zone string — fall through to the local clock
    }
  }
  return new Date(iso).toLocaleString([], opts);
};

/** Live countdown to an ISO timestamp: "2d 3h 04m" / "1h 23m 05s" / "45s". */
export const countdown = (iso: string | null | undefined, now: number = Date.now()): string => {
  if (!iso) return "—";
  let s = Math.floor((new Date(iso).getTime() - now) / 1000);
  if (Number.isNaN(s)) return "—";
  if (s <= 0) return "now";
  const pad = (n: number) => String(n).padStart(2, "0");
  const d = Math.floor(s / 86400);
  s -= d * 86400;
  const h = Math.floor(s / 3600);
  s -= h * 3600;
  const m = Math.floor(s / 60);
  s -= m * 60;
  if (d > 0) return `${d}d ${h}h ${pad(m)}m`;
  if (h > 0) return `${h}h ${pad(m)}m ${pad(s)}s`;
  if (m > 0) return `${m}m ${pad(s)}s`;
  return `${s}s`;
};

/** Next occurrence of `hour`:00 UTC (optionally on a given UTC weekday,
 *  0=Sunday) as an ISO string — drives the game-reset countdowns. */
export const nextUtc = (hour: number, weekday?: number): string => {
  const d = new Date();
  d.setUTCHours(hour, 0, 0, 0);
  if (weekday === undefined) {
    if (d.getTime() <= Date.now()) d.setUTCDate(d.getUTCDate() + 1);
  } else {
    let ahead = (weekday - d.getUTCDay() + 7) % 7;
    if (ahead === 0 && d.getTime() <= Date.now()) ahead = 7;
    d.setUTCDate(d.getUTCDate() + ahead);
  }
  return d.toISOString();
};

export const CATEGORY_LABELS: Record<string, string> = {
  warframe: "Warframe",
  weapon: "Weapon",
  set: "Set",
  mod: "Mod",
  arcane: "Arcane",
};
