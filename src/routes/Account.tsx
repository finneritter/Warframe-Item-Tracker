// Account screen — a "Tenno trader profile" (Koala-direction redesign). An identity
// header + three info columns (About/Account/Links) sit above a five-tab body:
// Overview · Resources · Armory · Codex · Stats. Overview is sales-backed (works even
// without a game scan); the rest read the persisted scan snapshot. Resources + Armory +
// Overview are listing tabs wired to the topbar search.
import { open as openExternal } from "@tauri-apps/plugin-shell";
import { useMemo, useState } from "react";
import type { ScreenId } from "../components/Sidebar";
import {
  BlockStatus,
  Chip,
  Glyph,
  ItemName,
  SortTh,
  TableStatus,
  rowAction,
} from "../components/ui";
import {
  useAccountArsenal,
  useAccountCodex,
  useAccountProfile,
  useAccountResources,
  useAccountScan,
  useGameScanStatus,
  useSales,
  useWfmAccount,
} from "../hooks/queries";
import { useColumnSort } from "../hooks/useTable";
import { CATEGORY_LABELS, clsx, fmt, fmtK, relativeDay } from "../lib/format";
import { usePersistedJSON } from "../lib/persist";
import { usePageSearch } from "../lib/searchContext";
import { compileQuery } from "../lib/searchQuery";
import { arsenalSchema, resourcesSchema, soldSchema } from "../lib/searchSchemas";
import type { AccountProfile, GearRow, ResourceRow, SaleRow } from "../lib/types";

type OpenFn = (slug: string) => void;
type NavFn = (s: ScreenId) => void;

const TABS = [
  ["overview", "Overview"],
  ["resources", "Resources"],
  ["armory", "Armory"],
  ["codex", "Codex"],
  ["stats", "Stats"],
] as const;
type TabId = (typeof TABS)[number][0];

const CAT_LABEL: Record<string, string> = {
  warframe: "Warframes",
  primary: "Primaries",
  secondary: "Secondaries",
  melee: "Melee",
  companion: "Companions",
  archwing: "Archwing",
  necramech: "Necramechs",
  amp: "Amps",
  special: "Special",
  railjack: "Railjack",
};
const ARSENAL_CATS = [
  "warframe",
  "primary",
  "secondary",
  "melee",
  "companion",
  "archwing",
  "necramech",
] as const;

// --------------------------------------------------------------------------- shell

export function Account({ onOpen, onNavigate }: { onOpen: OpenFn; onNavigate: NavFn }) {
  const [tab, setTab] = usePersistedJSON<TabId>("account.tab", "overview");
  const { data: profile } = useAccountProfile();
  const { data: wfm } = useWfmAccount();
  const { data: status } = useGameScanStatus();
  const scan = useAccountScan();

  const hasData = profile?.has_data ?? false;
  const canScan = (status?.supported && status?.consented && status?.warframe_running) ?? false;
  const name = wfm?.username ?? profile?.equipped_glyph_name ?? "Tenno";

  return (
    <>
      {/* header: emblem · identity · actions */}
      <div className="acc-head">
        <Emblem size={64} />
        <div className="acc-id">
          <div className="acc-name">{name}</div>
          {/* The URL takes the profile *slug*, not the in-game name — /profile/<name>
              404s for anyone whose name isn't already its own slug. */}
          {wfm?.slug ? (
            <button
              type="button"
              className="acc-handle lnk"
              onClick={() => openExternal(`https://warframe.market/profile/${wfm.slug}`)}
            >
              warframe.market/profile/{wfm.slug} ↗
            </button>
          ) : (
            <span className="acc-handle">not connected to warframe.market</span>
          )}
        </div>
        <div className="acc-actions">
          <button
            type="button"
            className="btn pri sm"
            disabled={!canScan || scan.isPending}
            onClick={() => scan.mutate()}
            title={
              canScan
                ? "Scan the running game — refreshes account, relics and nightwave progress"
                : "Needs consent + a running game (Settings → Game inventory)"
            }
          >
            {scan.isPending ? "Scanning…" : "⟳ Scan account"}
          </button>
          <button
            type="button"
            className="icon-btn"
            title="Settings"
            onClick={() => onNavigate("settings")}
          >
            ⚙
          </button>
        </div>
      </div>

      {/* info columns */}
      <div className="acc-cols">
        <AboutCol profile={profile} ign={name} />
        <AccountCol profile={profile} />
        <LinksCol slug={wfm?.slug ?? null} />
      </div>

      {/* tab bar */}
      <div className="acc-tabs">
        {TABS.map(([id, label]) => (
          <button
            key={id}
            type="button"
            className={clsx("acc-tab", tab === id && "on")}
            onClick={() => setTab(id)}
          >
            {label}
          </button>
        ))}
      </div>

      {scan.isError ? (
        <div className="meta" style={{ padding: "0 0 8px", color: "var(--neg)" }}>
          {(scan.error as Error)?.message ?? "Scan failed."}
        </div>
      ) : null}

      {tab === "overview" ? (
        <Overview onOpen={onOpen} onNavigate={onNavigate} />
      ) : !hasData ? (
        <EmptyScan onNavigate={onNavigate} />
      ) : tab === "resources" ? (
        <ResourcesTab onOpen={onOpen} />
      ) : tab === "armory" ? (
        <ArmoryTab onOpen={onOpen} />
      ) : tab === "codex" ? (
        <CodexTab />
      ) : (
        <StatsTab profile={profile as AccountProfile} />
      )}
    </>
  );
}

function EmptyScan({ onNavigate }: { onNavigate: NavFn }) {
  return (
    <div className="empty">
      No account scan yet. Open Warframe and use <b>⟳ Scan account</b> above (enable the game
      inventory scan first in{" "}
      <button type="button" className="lnk" onClick={() => onNavigate("settings")}>
        Settings
      </button>
      ).
    </div>
  );
}

// --------------------------------------------------------------------------- header bits

/** The hexagon-sigil emblem placeholder (ported from the handoff prototype). */
function Emblem({ size = 64 }: { size?: number }) {
  return (
    <div
      style={{
        width: size,
        height: size,
        flex: "none",
        border: "1px solid var(--line-2)",
        background: "#16131c",
        position: "relative",
        overflow: "hidden",
      }}
    >
      <div
        style={{
          position: "absolute",
          inset: 0,
          background:
            "repeating-linear-gradient(45deg, rgba(124,116,160,.20) 0 6px, transparent 6px 12px)",
        }}
      />
      <svg
        viewBox="0 0 48 48"
        aria-hidden="true"
        style={{
          position: "absolute",
          inset: 0,
          width: "100%",
          height: "100%",
          padding: size * 0.16,
          boxSizing: "border-box",
        }}
      >
        <polygon
          points="24,4 42,14 42,34 24,44 6,34 6,14"
          fill="none"
          stroke="var(--soft)"
          strokeWidth="1.4"
        />
        <polygon
          points="24,14 33,19 33,29 24,34 15,29 15,19"
          fill="none"
          stroke="var(--accent)"
          strokeWidth="1.1"
        />
        <circle cx="24" cy="24" r="2.6" fill="var(--accent)" />
      </svg>
    </div>
  );
}

/** A small 1.7-stroke icon; `color` defaults to --faint (info-row tint). */
function Stroke({
  d,
  size = 14,
  color = "var(--faint)",
}: { d: string; size?: number; color?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      width={size}
      height={size}
      fill="none"
      stroke={color}
      strokeWidth={1.7}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d={d} />
    </svg>
  );
}

function InfoRow({ d, children }: { d: string; children: React.ReactNode }) {
  return (
    <div className="acc-info">
      <Stroke d={d} />
      <span>{children}</span>
    </div>
  );
}

const ICON = {
  user: "M12 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8M4 20c0-4 4-6 8-6s8 2 8 6",
  phone: "M5 4h4l2 5-3 2a12 12 0 0 0 5 5l2-3 5 2v4a2 2 0 0 1-2 2A16 16 0 0 1 3 6a2 2 0 0 1 2-2",
  flag: "M5 4h14v16l-7-3-7 3z",
  trade: "M3 7h13l-3-3M21 17H8l3 3",
  coin: "M3 12a9 9 0 1 0 18 0 9 9 0 0 0-18 0M12 7v10M9 9h4.5a1.5 1.5 0 0 1 0 3H9",
  card: "M3 6h18v12H3zM3 10h18",
  gem: "M12 3l9 5-9 5-9-5z M3 13l9 5 9-5",
  box: "M4 19V5l8 4 8-4v14l-8 4z",
  link: "M10 13a5 5 0 0 0 7 0l3-3a5 5 0 0 0-7-7l-1 1M14 11a5 5 0 0 0-7 0l-3 3a5 5 0 0 0 7 7l1-1",
  ducat: "M3 12a9 9 0 1 0 18 0 9 9 0 0 0-18 0M9 9l6 6M15 9l-6 6",
  bag: "M3 7l9-4 9 4-9 4zM3 7v10l9 4 9-4V7",
};

function AboutCol({ profile, ign }: { profile?: AccountProfile; ign: string }) {
  const p = profile;
  const mr = p?.mastery_rank ?? 0;
  const leg = mr > 30 ? ` · Legendary ${mr - 30}` : "";
  const joinYear = p?.created ? new Date(p.created).getFullYear() : null;
  const joinDays = p?.created
    ? Math.max(0, Math.floor((Date.now() - new Date(p.created).getTime()) / 86_400_000))
    : null;
  return (
    <div>
      <div className="acc-secH">About</div>
      <InfoRow d={ICON.user}>{p?.has_data ? `MR ${mr}${leg}` : "—"}</InfoRow>
      <InfoRow d={ICON.phone}>In-game: {ign}</InfoRow>
      <InfoRow d={ICON.flag}>
        {joinYear ? `Joined ${joinYear} · ${fmt(joinDays)} days` : "Join date unknown"}
      </InfoRow>
      <InfoRow d={ICON.trade}>
        {p?.has_data ? `${p.trades_remaining} trades left today` : "—"}
      </InfoRow>
    </div>
  );
}

function AccountCol({ profile }: { profile?: AccountProfile }) {
  const p = profile;
  return (
    <div>
      <div className="acc-secH">Account</div>
      <InfoRow d={ICON.coin}>
        <span style={{ color: "var(--plat)" }}>{p?.has_data ? fmt(p.platinum) : "—"}p</span>{" "}
        platinum
      </InfoRow>
      <InfoRow d={ICON.card}>{p?.has_data ? fmt(p.credits) : "—"} credits</InfoRow>
      <InfoRow d={ICON.gem}>{p?.has_data ? fmt(p.endo) : "—"} endo</InfoRow>
      <InfoRow d={ICON.box}>{p?.has_data ? fmt(p.regal_aya) : "—"} regal aya</InfoRow>
    </div>
  );
}

function LinksCol({ slug }: { slug: string | null }) {
  return (
    <div>
      <div className="acc-secH">Links</div>
      {slug ? (
        <button
          type="button"
          className="acc-info lnk"
          onClick={() => openExternal(`https://warframe.market/profile/${slug}`)}
        >
          <Stroke d={ICON.link} />
          <span>warframe.market profile ↗</span>
        </button>
      ) : (
        <InfoRow d={ICON.link}>Connect an account in Settings</InfoRow>
      )}
    </div>
  );
}

// --------------------------------------------------------------------------- Overview

const SPANS = ["D", "W", "M"] as const;
type Span = (typeof SPANS)[number];
const SPAN_CAP: Record<Span, string> = { D: "today", W: "last 7 days", M: "last 30 days" };

function StatCard({
  label,
  d,
  color,
  vals,
}: {
  label: string;
  d: string;
  color: string;
  vals: Record<Span, { v: string; u?: string }>;
}) {
  const [span, setSpan] = useState<Span>("W");
  const { v, u } = vals[span];
  return (
    <div className="acc-card">
      <div className="acc-card-h">
        <Stroke d={d} size={15} color={color === "var(--ink)" ? "var(--soft)" : color} />
        <span className="lbl">{label}</span>
        <span className="dwm">
          {SPANS.map((s) => (
            <button
              key={s}
              type="button"
              className={clsx(span === s && "on")}
              onClick={() => setSpan(s)}
            >
              {s}
            </button>
          ))}
        </span>
      </div>
      <div className="acc-val" style={{ color }}>
        {v}
        {u ? <span className="u">{u}</span> : null}
      </div>
      <div className="acc-cap">{SPAN_CAP[span]}</div>
    </div>
  );
}

function Overview({ onOpen, onNavigate }: { onOpen: OpenFn; onNavigate: NavFn }) {
  const { data: sales = [], isLoading, isError } = useSales();
  const search = usePageSearch();
  const { test } = useMemo(() => compileQuery(search, soldSchema), [search]);
  const filtered = useMemo(() => sales.filter(test), [sales, test]);
  const [sel, setSel] = useState<Record<number, boolean>>({});

  const { sort, cycle, apply } = useColumnSort<SaleRow, "id" | "unit" | "when">(
    "account.trades.sort",
    {
      id: (a, b) => a.id - b.id,
      unit: (a, b) => (a.plat_per_unit ?? 0) - (b.plat_per_unit ?? 0),
      when: (a, b) => +new Date(a.sold_at) - +new Date(b.sold_at),
    },
    { key: "id", dir: "desc" },
  );
  const sorted = useMemo(() => apply(filtered), [filtered, apply]);

  // D/W/M aggregates over the same ledger (client-side buckets by sold_at).
  const buckets = useMemo(() => {
    const now = Date.now();
    const startToday = new Date();
    startToday.setHours(0, 0, 0, 0);
    const acc: Record<Span, { plat: number; qty: number }> = {
      D: { plat: 0, qty: 0 },
      W: { plat: 0, qty: 0 },
      M: { plat: 0, qty: 0 },
    };
    for (const s of sales) {
      const t = new Date(s.sold_at).getTime();
      const plat = (s.plat_per_unit ?? 0) * s.qty;
      if (t >= startToday.getTime()) {
        acc.D.plat += plat;
        acc.D.qty += s.qty;
      }
      if (now - t <= 7 * 86_400_000) {
        acc.W.plat += plat;
        acc.W.qty += s.qty;
      }
      if (now - t <= 30 * 86_400_000) {
        acc.M.plat += plat;
        acc.M.qty += s.qty;
      }
    }
    return acc;
  }, [sales]);

  const platVals = mapSpans((s) => ({ v: fmt(buckets[s].plat), u: "p" }));
  const soldVals = mapSpans((s) => ({ v: fmt(buckets[s].qty) }));
  const avgVals = mapSpans((s) => ({
    v: buckets[s].qty > 0 ? fmt(buckets[s].plat / buckets[s].qty) : "—",
    u: "p",
  }));

  return (
    <div>
      <div className="acc-cards">
        <StatCard label="Platinum earned" d={ICON.coin} color="var(--plat)" vals={platVals} />
        <StatCard label="Items sold" d={ICON.bag} color="var(--ink)" vals={soldVals} />
        <StatCard label="Avg sale" d={ICON.ducat} color="var(--ducat)" vals={avgVals} />
      </div>

      <div className="filters" style={{ marginBottom: 0 }}>
        <span style={{ flex: 1 }} />
        <button type="button" className="btn pri sm" onClick={() => onNavigate("listings")}>
          ＋ New listing
        </button>
      </div>

      <div className="tpanel" style={{ borderTop: "none" }}>
        <table className="dtable">
          <thead>
            <tr>
              <th style={{ width: 30 }} />
              <SortTh label="#&nbsp;Trade" col="id" sort={sort} onSort={cycle} />
              <th>Item</th>
              <th>Type</th>
              <SortTh label="Price" col="unit" sort={sort} onSort={cycle} right />
              <th className="r">vs median</th>
              <SortTh label="When" col="when" sort={sort} onSort={cycle} right />
            </tr>
          </thead>
          <tbody>
            {sorted.length === 0 ? (
              <TableStatus
                span={7}
                loading={isLoading}
                error={isError}
                emptyText={
                  sales.length === 0
                    ? "No sales recorded yet. Sell from an item's drawer to log a trade."
                    : "Nothing matches the filter."
                }
              />
            ) : (
              sorted.map((t) => {
                const total = (t.plat_per_unit ?? 0) * t.qty;
                const vs =
                  t.market_median_at_sale_time != null
                    ? (t.plat_per_unit ?? 0) - t.market_median_at_sale_time
                    : null;
                return (
                  <tr
                    key={t.id}
                    className={clsx("rowlink", sel[t.id] && "sel")}
                    {...rowAction(() => onOpen(t.slug))}
                  >
                    <td>
                      <input
                        type="checkbox"
                        checked={!!sel[t.id]}
                        onClick={(e) => e.stopPropagation()}
                        onChange={() => setSel((s) => ({ ...s, [t.id]: !s[t.id] }))}
                      />
                    </td>
                    <td className="num" style={{ color: "var(--soft)" }}>
                      #{t.id}
                    </td>
                    <td>
                      <ItemName
                        name={t.display_name}
                        plat={t.plat_per_unit}
                        thumb={t.thumbnail_url}
                        sub={t.qty > 1 ? `${t.qty}×` : undefined}
                      />
                    </td>
                    <td>
                      <Pill
                        label={CATEGORY_LABELS[t.category] ?? t.category}
                        dot
                        color="var(--soft)"
                      />
                    </td>
                    <td className="r num" style={{ color: "var(--pos)" }}>
                      +{fmt(total)}p
                    </td>
                    <td className="r num">
                      {vs == null ? (
                        <span className="muted">—</span>
                      ) : (
                        <span style={{ color: vs >= 0 ? "var(--pos)" : "var(--neg)" }}>
                          {vs >= 0 ? "+" : "−"}
                          {fmt(Math.abs(vs))}p
                        </span>
                      )}
                    </td>
                    <td className="r when">{relativeDay(t.sold_at)}</td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function mapSpans(
  f: (s: Span) => { v: string; u?: string },
): Record<Span, { v: string; u?: string }> {
  return { D: f("D"), W: f("W"), M: f("M") };
}

/** Colored marker pill — square swatch (value tier) or round dot (status). */
function Pill({ label, color, dot }: { label: string; color: string; dot?: boolean }) {
  return (
    <span className="acc-pill">
      <span className={dot ? "dot" : "sw"} style={{ background: color }} />
      <span>{label}</span>
    </span>
  );
}

// --------------------------------------------------------------------------- Resources

function ResourcesTab({ onOpen }: { onOpen: OpenFn }) {
  const { data: rows = [], isLoading, isError } = useAccountResources();
  const [pinned, setPinned] = usePersistedJSON<string[]>("account.resources.pinned", []);
  const [over, setOver] = useState(false);
  const search = usePageSearch();
  const { test } = useMemo(() => compileQuery(search, resourcesSchema), [search]);
  const filtered = useMemo(() => rows.filter(test), [rows, test]);
  const { sort, cycle, apply } = useColumnSort<ResourceRow, "name" | "count">(
    "account.resources.sort",
    {
      name: (a, b) => a.display_name.localeCompare(b.display_name),
      count: (a, b) => a.count - b.count,
    },
    { key: "count", dir: "desc" },
  );
  const sorted = useMemo(() => apply(filtered), [filtered, apply]);

  const byKey = useMemo(() => new Map(rows.map((r) => [r.unique_name, r])), [rows]);
  const pin = (key: string) => setPinned((p) => (p.includes(key) ? p : [...p, key]));
  const unpin = (key: string) => setPinned((p) => p.filter((k) => k !== key));

  return (
    <div>
      <div className="acc-secH">
        Tracked resources <span className="muted num">· drag a row up here</span>
      </div>
      <div
        className={clsx("acc-tray", over && "over")}
        onDragOver={(e) => {
          e.preventDefault();
          setOver(true);
        }}
        onDragLeave={() => setOver(false)}
        onDrop={(e) => {
          e.preventDefault();
          setOver(false);
          const key = e.dataTransfer.getData("text/plain");
          if (key) pin(key);
        }}
      >
        {pinned.length === 0 ? (
          <div className="ph">Drag a resource here (or click its ⠿) to feature it.</div>
        ) : (
          pinned.map((key) => {
            const r = byKey.get(key);
            if (!r) return null;
            return (
              <div key={key} className="acc-rescard">
                <button type="button" className="x" title="Remove" onClick={() => unpin(key)}>
                  ✕
                </button>
                <Glyph name={r.display_name} plat={null} thumb={r.icon_url} />
                <div className="nm">{r.display_name}</div>
                <div className="qty num">{fmtK(r.count)}</div>
                <div className="sub">{r.kind}</div>
              </div>
            );
          })
        )}
      </div>

      <div className="acc-secH">
        All resources <span className="muted num">{rows.length}</span>
      </div>
      <table className="dtable">
        <thead>
          <tr>
            <th style={{ width: 22 }} />
            <SortTh label="Resource" col="name" sort={sort} onSort={cycle} />
            <th>Kind</th>
            <SortTh label="Count" col="count" sort={sort} onSort={cycle} right />
          </tr>
        </thead>
        <tbody>
          {sorted.length === 0 ? (
            <TableStatus
              span={4}
              loading={isLoading}
              error={isError}
              emptyText={
                rows.length === 0
                  ? "No scan yet — use ⟳ Scan account above."
                  : "Nothing matches the filter."
              }
            />
          ) : (
            sorted.map((r) => (
              <tr
                key={r.unique_name}
                className={clsx(r.slug && "rowlink")}
                draggable
                onDragStart={(e) => e.dataTransfer.setData("text/plain", r.unique_name)}
                {...(r.slug ? rowAction(() => onOpen(r.slug as string)) : {})}
              >
                <td>
                  <button
                    type="button"
                    className="acc-grip"
                    title="Feature this resource"
                    onClick={(e) => {
                      e.stopPropagation();
                      pin(r.unique_name);
                    }}
                  >
                    ⠿
                  </button>
                </td>
                <td>
                  <ItemName name={r.display_name} plat={null} thumb={r.icon_url} />
                </td>
                <td className="muted">{r.kind}</td>
                <td className="r num">{fmt(r.count)}</td>
              </tr>
            ))
          )}
        </tbody>
      </table>
    </div>
  );
}

// --------------------------------------------------------------------------- Armory

function ArmoryTab({ onOpen }: { onOpen: OpenFn }) {
  const { data: rows = [], isLoading, isError } = useAccountArsenal();
  const [cat, setCat] = usePersistedJSON<string>("account.arsenal.cat", "all");
  const search = usePageSearch();
  const { test } = useMemo(() => compileQuery(search, arsenalSchema), [search]);
  const filtered = useMemo(
    () => rows.filter((r) => (cat === "all" || r.category === cat) && test(r)),
    [rows, cat, test],
  );
  const { sort, cycle, apply } = useColumnSort<GearRow, "name" | "rank">(
    "account.arsenal.sort",
    {
      name: (a, b) => a.display_name.localeCompare(b.display_name),
      rank: (a, b) => a.rank - b.rank,
    },
    { key: "name", dir: "asc" },
  );
  const sorted = useMemo(() => apply(filtered), [filtered, apply]);

  const counts = useMemo(() => {
    const m: Record<string, number> = {};
    for (const r of rows) m[r.category] = (m[r.category] ?? 0) + 1;
    return m;
  }, [rows]);

  return (
    <>
      <div className="filters">
        <Chip active={cat === "all"} onClick={() => setCat("all")} count={rows.length}>
          All
        </Chip>
        {ARSENAL_CATS.map((c) =>
          counts[c] ? (
            <Chip key={c} active={cat === c} onClick={() => setCat(c)} count={counts[c]}>
              {CAT_LABEL[c]}
            </Chip>
          ) : null,
        )}
      </div>
      <table className="dtable">
        <thead>
          <tr>
            <SortTh label="Item" col="name" sort={sort} onSort={cycle} />
            <th>Type</th>
            <SortTh label="Rank" col="rank" sort={sort} onSort={cycle} right />
            <th>Mastery</th>
          </tr>
        </thead>
        <tbody>
          {sorted.length === 0 ? (
            <TableStatus
              span={4}
              loading={isLoading}
              error={isError}
              emptyText={
                rows.length === 0
                  ? "No scan yet — use ⟳ Scan account above."
                  : "Nothing matches the filter."
              }
            />
          ) : (
            sorted.map((r) => (
              <tr
                key={`${r.category}:${r.unique_name}`}
                className={clsx(r.slug && "rowlink")}
                {...(r.slug ? rowAction(() => onOpen(r.slug as string)) : {})}
              >
                <td>
                  <ItemName name={r.display_name} plat={null} thumb={r.icon_url} />
                </td>
                <td className="muted">{CAT_LABEL[r.category] ?? r.category}</td>
                <td className="r num">
                  {r.rank}
                  <span className="muted">/{r.max_rank}</span>
                </td>
                <td>
                  {r.mastered ? (
                    <span className="badge at">mastered</span>
                  ) : (
                    <span className="muted">—</span>
                  )}
                </td>
              </tr>
            ))
          )}
        </tbody>
      </table>
    </>
  );
}

// --------------------------------------------------------------------------- Codex

function CodexTab() {
  const { data: codex, isLoading, isError } = useAccountCodex();
  if (isLoading) return <BlockStatus />;
  if (isError) return <BlockStatus error />;
  if (!codex) return <BlockStatus text="No codex data yet — run a game scan from Settings." />;
  const collPct = codex.total_items > 0 ? (codex.total_owned / codex.total_items) * 100 : 0;

  return (
    <div>
      <div className="acc-codex-h">
        <div>
          <span className="acc-bignum">{Math.round(collPct)}%</span>
          <span className="muted" style={{ marginLeft: 10 }}>
            complete · {fmt(codex.total_owned)} / {fmt(codex.total_items)} entries ·{" "}
            {fmt(codex.total_mastered)} mastered
          </span>
        </div>
      </div>

      {codex.categories.map((c) => {
        const pct = c.total > 0 ? (c.owned / c.total) * 100 : 0;
        return (
          <div key={c.category} className="acc-codex-row">
            <span className="lbl">{CAT_LABEL[c.category] ?? c.category}</span>
            <span className="track">
              <i
                className="fill"
                style={{
                  width: `${pct}%`,
                  background: pct >= 70 ? "var(--pos)" : "var(--soft)",
                }}
              />
            </span>
            <span className="val num">{Math.round(pct)}%</span>
          </div>
        );
      })}

      {codex.lore_scans.length ? (
        <div className="tpanel" style={{ marginTop: 16 }}>
          <div className="tpanel-h">Cephalon Fragment / lore scans</div>
          <div className="kv kv--wide" style={{ padding: "8px 12px" }}>
            {codex.lore_scans.map((l) => (
              <div key={l.display_name} className="kv-row">
                <span className="kv-k">{l.display_name}</span>
                <b className="kv-v num">{l.scans}</b>
              </div>
            ))}
          </div>
        </div>
      ) : null}
    </div>
  );
}

// --------------------------------------------------------------------------- Stats

function BarRow({
  name,
  fillPct,
  right,
  mono,
  thumb,
}: {
  name: string;
  fillPct: number;
  right: string;
  mono?: string;
  thumb?: string | null;
}) {
  return (
    <div className={clsx("acc-barrow", mono && "mono")}>
      {mono ? <Glyph name={mono} plat={null} thumb={thumb} /> : null}
      <div>
        <div className="nm">{name}</div>
        <div className="track">
          <i className="fill" style={{ width: `${Math.min(100, fillPct)}%` }} />
        </div>
      </div>
      <span className="rt">{right}</span>
    </div>
  );
}

function StatsTab({ profile }: { profile: AccountProfile }) {
  const { data: codex } = useAccountCodex();
  const p = profile;
  const starPct = p.nodes_total > 0 ? (p.nodes_completed / p.nodes_total) * 100 : 0;
  const career: [string, string, string][] = [
    ["Missions cleared", fmt(p.total_missions), ""],
    ["Star chart", String(Math.round(starPct)), "%"],
    ["Mastery rank", String(p.mastery_rank), ""],
    ["Total mastery", fmtK(p.total_mastery_points), ""],
  ];
  const maxStanding = Math.max(1, ...p.syndicates.map((s) => Math.abs(s.standing)));
  const secondary: [string, string, string][] = [
    ["Trades left", String(p.trades_remaining), ""],
    ["Gifts left", String(p.gifts_remaining), ""],
    ["Login milestones", String(p.login_streak), ""],
    ["Daily focus", fmtK(p.daily_focus), ""],
  ];

  return (
    <div>
      <div className="acc-career">
        {career.map(([k, v, u]) => (
          <div key={k} className="acc-career-cell">
            <div className="v">
              {v}
              {u ? <span className="u">{u}</span> : null}
            </div>
            <div className="k">{k}</div>
          </div>
        ))}
      </div>

      <div className="acc-twocol">
        <div className="tpanel">
          <div className="tpanel-h">
            <h3>Collection by category</h3>
            <span className="meta">owned of masterable</span>
          </div>
          <div style={{ padding: "4px 12px 10px" }}>
            {codex?.categories.length ? (
              codex.categories.map((c) => {
                const pct = c.total > 0 ? (c.owned / c.total) * 100 : 0;
                return (
                  <BarRow
                    key={c.category}
                    name={CAT_LABEL[c.category] ?? c.category}
                    fillPct={pct}
                    right={`${Math.round(pct)}%`}
                  />
                );
              })
            ) : (
              <div className="muted">No codex data.</div>
            )}
          </div>
        </div>

        <div className="tpanel">
          <div className="tpanel-h">
            <h3>Syndicate standing</h3>
            <span className="meta">current</span>
          </div>
          <div style={{ padding: "4px 12px 10px" }}>
            {p.syndicates.length ? (
              p.syndicates.map((s) => (
                <BarRow
                  key={s.tag}
                  name={`${s.label}${s.title ? ` · ${s.title}` : ""}`}
                  fillPct={(Math.abs(s.standing) / maxStanding) * 100}
                  right={fmtK(s.standing)}
                />
              ))
            ) : (
              <div className="muted">No syndicate standing scanned.</div>
            )}
          </div>
        </div>
      </div>

      {p.intrinsics.length ? (
        <div className="tpanel" style={{ marginBottom: 14 }}>
          <div className="tpanel-h">
            <h3>Intrinsics</h3>
          </div>
          <div className="kv kv--wide" style={{ padding: "8px 12px" }}>
            {p.intrinsics.map((i) => (
              <div key={i.skill_key} className="kv-row">
                <span className="kv-k">{i.label}</span>
                <b className="kv-v num">{i.rank}</b>
              </div>
            ))}
          </div>
        </div>
      ) : null}

      <div className="statband" style={{ gridTemplateColumns: "repeat(4, 1fr)", margin: 0 }}>
        {secondary.map(([k, v, u]) => (
          <div className="statbox" key={k}>
            <div className="k">{k}</div>
            <div className="v num">
              {v}
              {u ? <span className="u">{u}</span> : null}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
