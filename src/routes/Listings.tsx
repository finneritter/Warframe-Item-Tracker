import { useMemo, useState } from "react";
import { Dropdown, type DropdownOption } from "../components/Dropdown";
import { ItemTags } from "../components/ItemTags";
import { ListingForm } from "../components/ListingForm";
import { Chip, Glyph, ItemName, Scrim, StatBox, TableStatus, rowAction } from "../components/ui";
import {
  useInventory,
  useListingRecommendations,
  useListings,
  usePricingProgress,
  useRecommendationsRefresh,
  useSearchCatalog,
  useWfmAccount,
  useWfmConnect,
  useWfmCreateOrder,
  useWfmDeleteOrder,
  useWfmMarkSold,
  useWfmRepriceApply,
  useWfmSetSession,
  useWfmSetStatus,
  useWfmSignout,
  useWfmSync,
  useWfmUpdateOrder,
} from "../hooks/queries";
import { useEscape } from "../hooks/useEscape";
import { wfmRepricePreview } from "../lib/api";
import { CATEGORY_LABELS, clsx, fmt, syncListingsNote } from "../lib/format";
import { usePageSearch } from "../lib/searchContext";
import { compileQuery } from "../lib/searchQuery";
import { listingsSchema, recommendationsSchema } from "../lib/searchSchemas";
import { pushToast } from "../lib/toast";
import type { InventoryRow, ListingRow, RepriceRow } from "../lib/types";

// Sort axes for the Recommended tab.
const REC_SORTS: readonly DropdownOption[] = [
  ["value", "Est. value"],
  ["volume", "Volume"],
  ["price", "Price"],
];

// Category chips for the Recommended tab — same order/labels as the Market screener.
const REC_CATEGORIES = ["all", "warframe", "weapon", "set", "mod", "arcane"] as const;

// UI status segment → warframe.market API status; "Offline" = invisible.
const STATUS_OPTS = [
  { api: "invisible", label: "Invisible", dot: "offline" },
  { api: "online", label: "Online", dot: "online" },
  { api: "ingame", label: "In Game", dot: "ingame" },
] as const;

/** A pickable row (shared by the owned list and the catalog fallback). */
function PickRow({
  slug,
  name,
  sub,
  plat,
  thumb,
  onPick,
}: {
  slug: string;
  name: string;
  sub: string;
  plat: number | null;
  thumb: string | null;
  onPick: (slug: string) => void;
}) {
  return (
    <button type="button" className="sr-row" onClick={() => onPick(slug)}>
      <Glyph name={name} plat={plat} thumb={thumb} />
      <span className="sr-i">
        <span className="sr-n">{name}</span>
        <span className="sr-s">{sub}</span>
      </span>
      <span className="sr-p num">{plat == null ? "—" : `${fmt(plat)}p`}</span>
    </button>
  );
}

/** Pick an item to list — your inventory first (filterable), with a catalog
 *  fallback so you can still list something you don't currently track. */
function NewListingModal({
  onPick,
  onClose,
}: {
  onPick: (slug: string) => void;
  onClose: () => void;
}) {
  const { data: inv = [] } = useInventory();
  const [q, setQ] = useState("");
  const query = q.trim().toLowerCase();
  useEscape(onClose);

  // Owned items, filtered by the query, richest first (what you'd most likely sell).
  const owned = useMemo(() => {
    const worth = (r: InventoryRow) =>
      r.realizable_plat ?? r.value_plat ?? (r.median_plat ?? 0) * r.qty;
    const matches = query
      ? inv.filter(
          (r) =>
            r.display_name.toLowerCase().includes(query) ||
            r.part_type.toLowerCase().includes(query),
        )
      : inv;
    return [...matches].sort((a, b) => worth(b) - worth(a));
  }, [inv, query]);

  // Fallback: catalog matches you don't own, so non-tracked items stay listable.
  const { data: catalog = [] } = useSearchCatalog(query.length >= 2 ? q.trim() : "");
  const others = useMemo(() => {
    if (query.length < 2) return [];
    const ownedSlugs = new Set(inv.map((r) => r.slug));
    return catalog.filter((r) => !ownedSlugs.has(r.slug)).slice(0, 8);
  }, [catalog, inv, query]);

  const empty = owned.length === 0 && others.length === 0;

  return (
    <Scrim onClose={onClose}>
      <div className="modal np-modal">
        <div className="modal-h">
          <h2>New listing</h2>
          <span style={{ flex: 1 }} />
          <button type="button" className="x" onClick={onClose}>
            ✕
          </button>
        </div>
        <div className="search" style={{ margin: 14 }}>
          <input
            autoFocus
            placeholder="Filter your inventory…"
            value={q}
            onChange={(e) => setQ(e.target.value)}
          />
        </div>
        <div className="np-list">
          {empty ? (
            <div className="sr-empty">
              {query ? "Nothing matches — try fewer letters." : "Your inventory is empty."}
            </div>
          ) : null}
          {owned.map((r) => (
            <PickRow
              key={r.slug}
              slug={r.slug}
              name={r.display_name}
              sub={`${r.part_type} · ${CATEGORY_LABELS[r.category]} · own ×${r.qty}`}
              plat={r.median_plat}
              thumb={r.thumbnail_url}
              onPick={onPick}
            />
          ))}
          {others.length ? <div className="np-divider">Not in your inventory</div> : null}
          {others.map((r) => (
            <PickRow
              key={r.slug}
              slug={r.slug}
              name={r.display_name}
              sub={`${r.part_type} · ${CATEGORY_LABELS[r.category]}`}
              plat={r.median_plat}
              thumb={r.thumbnail_url}
              onPick={onPick}
            />
          ))}
        </div>
      </div>
    </Scrim>
  );
}

/** Step 1 of 2: connect by public profile name (resolved + verified against
 *  warframe.market). The API addresses users by their profile *slug*, which isn't
 *  always the in-game name, so pasting the profile URL is offered as the sure path. */
function SignInCard() {
  const connect = useWfmConnect();
  const [username, setUsername] = useState("");

  const submit = () => {
    const u = username.trim();
    if (u && !connect.isPending) connect.mutate(u);
  };

  return (
    <div className="tpanel card" style={{ maxWidth: 520 }}>
      <div className="tpanel-h">
        <h3>Connect warframe.market</h3>
        <span style={{ flex: 1 }} />
        <span className="muted">Step 1 of 2</span>
      </div>
      <div className="content" style={{ padding: 14 }}>
        <p className="muted" style={{ marginTop: 0 }}>
          Mirrors your warframe.market <b>listings</b>, read-only. Enter your profile name — or
          paste your profile URL if the name alone doesn't find you.
        </p>
        <div className="search" style={{ marginBottom: 8 }}>
          <input
            autoFocus
            placeholder="profile name or warframe.market/profile/… URL"
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && submit()}
          />
        </div>
        <button
          type="button"
          className="btn pri"
          disabled={!username.trim() || connect.isPending}
          onClick={submit}
        >
          {connect.isPending ? "Checking…" : "Next →"}
        </button>
        {connect.isError ? (
          <div className="conn-note" style={{ marginTop: 8 }}>
            {(connect.error as Error).message}
          </div>
        ) : null}
      </div>
    </div>
  );
}

/** Step 2 of 2 (optional): paste the JWT cookie to unlock invisible orders + management. */
function SessionCard({ onSkip }: { onSkip: () => void }) {
  const setSession = useWfmSetSession();
  const [jwt, setJwt] = useState("");

  const submit = () => {
    const t = jwt.trim();
    if (t && !setSession.isPending) setSession.mutate(t, { onSuccess: () => setJwt("") });
  };

  return (
    <div className="tpanel card" style={{ maxWidth: 560 }}>
      <div className="tpanel-h">
        <h3>Add a session token</h3>
        <span style={{ flex: 1 }} />
        <span className="muted">Step 2 of 2 · optional</span>
      </div>
      <div className="content" style={{ padding: 14 }}>
        <p className="muted" style={{ marginTop: 0 }}>
          Optional. Paste your warframe.market <b>JWT</b> cookie to manage orders (create, edit,
          delete, status) and see invisible ones. Stored in your OS keychain, never the database.
        </p>
        <div className="grp" style={{ paddingLeft: 0 }}>
          Where to find your JWT
        </div>
        <ol className="muted" style={{ margin: "4px 0 12px", paddingLeft: 18, lineHeight: 1.6 }}>
          <li>
            Log in to <b>warframe.market</b> in your browser.
          </li>
          <li>
            DevTools (<b>F12</b>) → <b>Application</b> / <b>Storage</b> → <b>Cookies</b> →{" "}
            <b>warframe.market</b>.
          </li>
          <li>
            Copy the <b>JWT</b> cookie value (starts <code>eyJ…</code>), paste it below.
          </li>
        </ol>
        <div className="search" style={{ marginBottom: 8 }}>
          <input
            placeholder="paste JWT (eyJ…)"
            value={jwt}
            onChange={(e) => setJwt(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && submit()}
          />
        </div>
        <div className="lf-actions">
          <button
            type="button"
            className="btn pri"
            disabled={!jwt.trim() || setSession.isPending}
            onClick={submit}
          >
            {setSession.isPending ? "Validating…" : "Finish"}
          </button>
          <button type="button" className="btn" onClick={onSkip} disabled={setSession.isPending}>
            Skip — stay read-only
          </button>
        </div>
        {setSession.isError ? (
          <div className="conn-note" style={{ marginTop: 8 }}>
            {(setSession.error as Error).message}
          </div>
        ) : null}
      </div>
    </div>
  );
}

/** Preview + confirm a bulk reprice to the recommended (best) price per listing. */
function RepricePanel({ rows, onClose }: { rows: RepriceRow[]; onClose: () => void }) {
  const apply = useWfmRepriceApply();
  const changes = rows.filter((r) => r.new_price !== r.current_price);
  return (
    <div className="tpanel">
      <div className="tpanel-h">
        <h3>Reprice to best — review changes</h3>
        <span style={{ flex: 1 }} />
        <button type="button" className="x" onClick={onClose}>
          ✕
        </button>
      </div>
      <table className="dtable">
        <thead>
          <tr>
            <th>Item</th>
            <th className="r">Current</th>
            <th className="r">New</th>
            <th className="r">Δ</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => {
            const changed = r.new_price !== r.current_price;
            const delta = r.current_price == null ? null : r.new_price - r.current_price;
            return (
              <tr key={r.order_id} className={changed ? undefined : "row-hidden"}>
                <td>
                  <ItemName
                    name={r.display_name}
                    plat={r.new_price}
                    thumb={r.thumbnail_url}
                    sub={r.part_type}
                  />
                </td>
                <td className="r num">{fmt(r.current_price)}p</td>
                <td className="r num">{fmt(r.new_price)}p</td>
                <td className="r num">
                  {delta == null || delta === 0 ? (
                    <span className="muted">—</span>
                  ) : (
                    <span className={delta > 0 ? "pos" : "neg"}>
                      {delta > 0 ? "+" : ""}
                      {fmt(delta)}p
                    </span>
                  )}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
      <div className="modal-f">
        <div className="info">
          {changes.length} will change · {rows.length - changes.length} unchanged
        </div>
        <span className="sp" style={{ flex: 1 }} />
        {apply.isError ? (
          <span className="muted neg" style={{ marginRight: 8 }}>
            {(apply.error as Error).message}
          </span>
        ) : null}
        <button type="button" className="btn" onClick={onClose} disabled={apply.isPending}>
          Cancel
        </button>
        <button
          type="button"
          className="btn pri"
          disabled={changes.length === 0 || apply.isPending}
          onClick={() =>
            apply.mutate(
              changes.map((r) => ({
                order_id: r.order_id,
                platinum: r.new_price,
                quantity: r.qty,
                visible: r.visible,
              })),
              { onSuccess: onClose },
            )
          }
        >
          {apply.isPending
            ? "Applying…"
            : `Apply ${changes.length} change${changes.length === 1 ? "" : "s"}`}
        </button>
      </div>
    </div>
  );
}

/// The "Recommended" tab: owned items worth listing for plat (liquid, not better
/// ducated, outlier-cleaned, not already up). Each row opens the prefilled
/// ListingForm; "List all" walks them sequentially at the suggested prices.
function RecommendedTable({
  active,
  session,
  writeHint,
  onList,
  onOpen,
}: {
  active: boolean;
  session: boolean;
  writeHint?: string;
  onList: (slug: string, rank?: number) => void;
  onOpen: (slug: string) => void;
}) {
  const { data: rows = [], isLoading, isError } = useListingRecommendations(active);
  const create = useWfmCreateOrder();
  // The full "redo": force-reprice every owned item (fresh stats + order books),
  // then rebuild the list. A backend price sync drives usePricingProgress, which
  // feeds the progress bar below the header.
  const redo = useRecommendationsRefresh();
  const { data: progress } = usePricingProgress();
  const refreshing = redo.isPending || !!progress?.active;
  const refreshPct =
    progress && progress.total > 0 ? `${(progress.priced / progress.total) * 100}%` : undefined;
  const [confirmAll, setConfirmAll] = useState(false);
  const [bulkRunning, setBulkRunning] = useState(false);
  const [bulkErr, setBulkErr] = useState<string | null>(null);
  const [cat, setCat] = useState("all");
  const [sort, setSort] = useState<"value" | "volume" | "price">("value");

  // Topbar query filters the recommendations (same grammar as the rest of the app).
  const search = usePageSearch();
  const { test } = useMemo(() => compileQuery(search, recommendationsSchema), [search]);

  // Per-category counts for the chip row (computed off the unfiltered set).
  const counts = useMemo(() => {
    const m: Record<string, number> = { all: rows.length };
    for (const r of rows) m[r.category] = (m[r.category] ?? 0) + 1;
    return m;
  }, [rows]);

  // Category + search filter, then sort by the chosen axis.
  const view = useMemo(() => {
    const f = rows.filter((r) => (cat === "all" || r.category === cat) && test(r));
    return f.sort((a, b) =>
      sort === "volume"
        ? b.avg_daily_volume - a.avg_daily_volume
        : sort === "price"
          ? b.suggested_price - a.suggested_price
          : b.est_value - a.est_value,
    );
  }, [rows, test, cat, sort]);

  const totalEst = view.reduce((s, r) => s + r.est_value, 0);

  async function listAll() {
    setBulkRunning(true);
    setBulkErr(null);
    // Snapshot the visible set — invalidations during the loop refetch & shrink it.
    const snapshot = view.slice();
    try {
      for (const r of snapshot) {
        await create.mutateAsync({
          slug: r.slug,
          platinum: r.suggested_price,
          quantity: r.owned_qty,
          rank: r.rank,
          visible: true,
        });
      }
      setConfirmAll(false);
    } catch (e) {
      setBulkErr((e as Error).message ?? String(e));
    } finally {
      setBulkRunning(false);
    }
  }

  return (
    <>
      <div className="mkt-filters">
        {REC_CATEGORIES.map((c) => (
          <Chip key={c} active={cat === c} count={counts[c] ?? 0} onClick={() => setCat(c)}>
            {c === "all" ? "All" : CATEGORY_LABELS[c]}
          </Chip>
        ))}
        <span className="sp" style={{ flex: 1 }} />
        <span className="sortlbl">sort</span>
        <Dropdown
          icon="sort"
          value={sort}
          options={REC_SORTS}
          onChange={(v) => setSort(v as "value" | "volume" | "price")}
          align="right"
          title="Sort recommendations"
        />
      </div>
      <div className="tpanel">
        <div className="tpanel-h">
          <h3>
            Recommended to list
            {view.length ? ` · ${view.length} · ~${fmt(totalEst)}p` : ""}
          </h3>
          <span style={{ flex: 1 }} />
          <button
            type="button"
            className="btn sm"
            style={{ marginRight: 8 }}
            disabled={refreshing}
            title="Re-fetch prices for every owned item, then rebuild the recommendations"
            onClick={() => redo.mutate()}
          >
            {refreshing ? "Refreshing…" : "Refresh"}
          </button>
          {confirmAll ? (
            <span className="lf-actions">
              <span className="muted" style={{ marginRight: 8 }}>
                List all {view.length} (~{fmt(totalEst)}p)?
              </span>
              <button type="button" className="btn pri sm" disabled={bulkRunning} onClick={listAll}>
                {bulkRunning ? "Listing…" : "Confirm"}
              </button>
              <button
                type="button"
                className="btn sm"
                disabled={bulkRunning}
                onClick={() => setConfirmAll(false)}
              >
                Cancel
              </button>
            </span>
          ) : (
            <button
              type="button"
              className="btn pri sm"
              disabled={!session || view.length === 0}
              title={writeHint}
              onClick={() => setConfirmAll(true)}
            >
              List all recommended
            </button>
          )}
        </div>
        {refreshing ? (
          <div className="upd-prog" style={{ margin: "0 0 8px" }}>
            <div
              className={clsx("upd-prog-fill", !refreshPct && "indeterminate")}
              style={refreshPct ? { width: refreshPct } : undefined}
            />
          </div>
        ) : null}
        {bulkErr ? (
          <div className="conn-note neg" style={{ margin: "0 0 8px" }}>
            Couldn't list everything: {bulkErr}
          </div>
        ) : null}
        <table className="dtable">
          <thead>
            <tr>
              <th>Item</th>
              <th className="r">Avg/day</th>
              <th className="r">Suggested</th>
              <th className="r">Qty</th>
              <th className="r">Est. value</th>
              <th className="r">List</th>
            </tr>
          </thead>
          <tbody>
            {isLoading || isError || view.length === 0 ? (
              <TableStatus
                span={6}
                loading={isLoading}
                error={isError}
                emptyText={
                  rows.length === 0
                    ? "Nothing to recommend right now. Items show up here once they're owned, liquid (10+/day), worth more as plat than ducats, and not already listed."
                    : "No recommendations match the current filters."
                }
              />
            ) : (
              view.map((r) => (
                <tr key={`${r.slug}-${r.rank ?? "x"}`} {...rowAction(() => onOpen(r.slug))}>
                  <td>
                    <ItemName
                      name={r.rank != null ? `${r.display_name} (rank ${r.rank})` : r.display_name}
                      plat={r.suggested_price}
                      thumb={r.thumbnail_url}
                      sub={r.part_type}
                      tags={<ItemTags trend={r.trend} vaulted={false} />}
                    />
                  </td>
                  <td className="r num">{r.avg_daily_volume.toFixed(1)}</td>
                  <td className="r num">{fmt(r.suggested_price)}p</td>
                  <td className="r num">{r.owned_qty}</td>
                  <td className="r num">{fmt(r.est_value)}p</td>
                  <td
                    className="r"
                    onClick={(e) => e.stopPropagation()}
                    onKeyDown={(e) => e.stopPropagation()}
                  >
                    {!session ? (
                      <span className="muted">—</span>
                    ) : (
                      <button
                        type="button"
                        className="btn sm pri"
                        title="Review and post a sell order at the suggested price"
                        onClick={() => onList(r.slug, r.rank ?? undefined)}
                      >
                        List
                      </button>
                    )}
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </>
  );
}

export function Listings({
  onOpen,
  initialTab = "mine",
}: {
  onOpen: (slug: string) => void;
  initialTab?: "mine" | "recommended";
}) {
  const { data: account } = useWfmAccount();
  const { data: listings = [], isLoading, isError } = useListings();
  const sync = useWfmSync();
  const signout = useWfmSignout();
  const setStatus = useWfmSetStatus();
  const update = useWfmUpdateOrder();
  const del = useWfmDeleteOrder();
  const markSold = useWfmMarkSold();
  const [repriceRows, setRepriceRows] = useState<RepriceRow[] | null>(null);
  const [repricing, setRepricing] = useState(false);
  // The item being created — slug, plus an optional rank when opened from a per-rank
  // recommendation so the form preselects that level.
  const [creating, setCreating] = useState<{ slug: string; rank?: number } | null>(null);
  const [picking, setPicking] = useState(false);
  const [editing, setEditing] = useState<ListingRow | null>(null);
  const [confirmId, setConfirmId] = useState<string | null>(null);
  const [sessionDismissed, setSessionDismissed] = useState(false);
  const [tab, setTab] = useState<"mine" | "recommended">(initialTab);

  // Topbar query filters the table only; the stat band reflects all listings.
  const search = usePageSearch();
  const { test } = useMemo(() => compileQuery(search, listingsSchema), [search]);
  const view = useMemo(() => listings.filter(test), [listings, test]);

  if (!account?.connected) return <SignInCard />;

  // An expired token is treated as "no usable session": writes are gated off and
  // the re-paste card surfaces, exactly like having no token.
  const expired = account.has_session && account.session_expired;
  const session = account.has_session && !account.session_expired;
  const expiresAt = account.session_expires_at ? new Date(account.session_expires_at) : null;
  const daysLeft = expiresAt ? Math.ceil((expiresAt.getTime() - Date.now()) / 86_400_000) : null;
  const expiringSoon = session && daysLeft != null && daysLeft <= 14;
  const writeHint = expired
    ? "Session expired — paste a fresh token to manage orders"
    : session
      ? undefined
      : "Add a session token to manage orders";
  const active = listings.length;
  const listedValue = listings.reduce((s, l) => s + (l.your_price ?? 0) * l.qty, 0);
  const atBest = listings.filter(
    (l) => l.market_low != null && (l.your_price ?? 0) <= l.market_low,
  ).length;
  const undercut = listings.filter(
    (l) => l.market_low != null && (l.your_price ?? 0) > l.market_low,
  ).length;
  const dot = STATUS_OPTS.find((o) => o.api === account.status)?.dot ?? "offline";

  const toggleVisible = (l: ListingRow) =>
    update.mutate({
      orderId: l.order_id,
      platinum: l.your_price ?? 1,
      quantity: l.qty,
      visible: !l.visible,
    });

  return (
    <>
      <div className="conn">
        <span className={clsx("cdot", dot)} />
        <span className="cinfo">
          <b>{account.username}</b>
          {expired ? " · session expired" : session ? " · session active" : " · public · read-only"}
          {session && expiresAt ? (
            <span className="muted"> · expires {expiresAt.toLocaleDateString()}</span>
          ) : null}
        </span>
        <span className="seg" title={writeHint}>
          {STATUS_OPTS.map((o) => (
            <button
              key={o.api}
              type="button"
              className="segb"
              aria-pressed={account.status === o.api}
              disabled={!session || setStatus.isPending}
              onClick={() => setStatus.mutate(o.api)}
            >
              {o.label}
            </button>
          ))}
        </span>
        <span style={{ flex: 1 }} />
        {!session && sessionDismissed ? (
          <button type="button" className="btn sm" onClick={() => setSessionDismissed(false)}>
            Add session token
          </button>
        ) : null}
        <button
          type="button"
          className="btn pri sm"
          disabled={!session}
          title={writeHint}
          onClick={() => setPicking(true)}
        >
          + New listing
        </button>
        <button
          type="button"
          className="btn sm"
          disabled={!session || repricing || listings.length === 0}
          title={writeHint}
          onClick={async () => {
            setRepricing(true);
            try {
              setRepriceRows(await wfmRepricePreview());
            } finally {
              setRepricing(false);
            }
          }}
        >
          {repricing ? "Pricing…" : "Set best prices"}
        </button>
        <button
          type="button"
          className="btn sm"
          onClick={() =>
            sync.mutate(undefined, {
              onSuccess: (r) => pushToast(syncListingsNote(r), "info"),
            })
          }
          disabled={sync.isPending}
        >
          {sync.isPending ? "Syncing…" : "Sync"}
        </button>
        <button type="button" className="btn sm" onClick={() => signout.mutate()}>
          Disconnect
        </button>
      </div>

      {setStatus.isError ? (
        <div className="conn-note">Couldn't set status: {(setStatus.error as Error).message}</div>
      ) : null}

      {repriceRows ? (
        <RepricePanel rows={repriceRows} onClose={() => setRepriceRows(null)} />
      ) : null}

      {expired ? (
        <div className="conn-note">
          Your warframe.market session has expired
          {expiresAt ? ` (${expiresAt.toLocaleDateString()})` : ""}. Paste a fresh JWT below to keep
          creating, editing, and selling orders.
        </div>
      ) : expiringSoon ? (
        <div className="conn-note">
          Your warframe.market session expires in {daysLeft} day{daysLeft === 1 ? "" : "s"} (
          {expiresAt?.toLocaleDateString()}). Disconnect and reconnect to refresh it with a new JWT.
        </div>
      ) : null}

      {!session && !sessionDismissed ? (
        <SessionCard onSkip={() => setSessionDismissed(true)} />
      ) : null}

      <div className="tabband">
        <div className="seg">
          <button
            type="button"
            className="segb"
            aria-pressed={tab === "mine"}
            onClick={() => setTab("mine")}
          >
            My listings
          </button>
          <button
            type="button"
            className="segb"
            aria-pressed={tab === "recommended"}
            onClick={() => setTab("recommended")}
          >
            Recommended
          </button>
        </div>
      </div>

      {tab === "recommended" ? (
        <RecommendedTable
          active={tab === "recommended"}
          session={session}
          writeHint={writeHint}
          onList={(slug, rank) => setCreating({ slug, rank })}
          onOpen={onOpen}
        />
      ) : (
        <>
          <div className="statband">
            <StatBox k="Active listings" v={fmt(active)} />
            <StatBox k="Listed value" v={fmt(listedValue)} unit="p" />
            <StatBox k="At best price" v={fmt(atBest)} dcls="pos" />
            <StatBox k="Undercut" v={fmt(undercut)} dcls="neg" />
          </div>

          <div className="tpanel">
            <table className="dtable">
              <thead>
                <tr>
                  <th>Item</th>
                  <th className="r">Your price</th>
                  <th className="r">Qty</th>
                  <th className="r">Value</th>
                  <th className="r">Market low</th>
                  <th>vs market</th>
                  <th className="r">Manage</th>
                </tr>
              </thead>
              <tbody>
                {isLoading || isError || view.length === 0 ? (
                  <TableStatus
                    span={7}
                    loading={isLoading}
                    error={isError}
                    emptyText={
                      <>
                        No sell orders found. Hit <b>Sync</b> to refresh from warframe.market
                        {session ? (
                          <>
                            , or <b>+ New listing</b> to post one.
                          </>
                        ) : (
                          "."
                        )}
                      </>
                    }
                  />
                ) : (
                  view.map((l) => {
                    const yp = l.your_price ?? 0;
                    const best = l.market_low != null && yp <= l.market_low;
                    const over = l.market_low != null && yp > l.market_low ? yp - l.market_low : 0;
                    const confirming = confirmId === l.order_id;
                    return (
                      <tr
                        key={l.order_id}
                        {...rowAction(() => onOpen(l.slug))}
                        className={l.visible ? undefined : "row-hidden"}
                      >
                        <td>
                          <ItemName
                            name={l.display_name}
                            plat={l.your_price}
                            thumb={l.thumbnail_url}
                            sub={l.part_type}
                            tags={<ItemTags trend={l.trend} vaulted={l.is_vaulted} />}
                          />
                        </td>
                        <td className="r num">{fmt(l.your_price)}p</td>
                        <td className="r num">{l.qty}</td>
                        <td className="r num">{fmt(yp * l.qty)}p</td>
                        <td className="r num">{fmt(l.market_low)}p</td>
                        <td>
                          {!l.visible ? (
                            <span className="badge">hidden</span>
                          ) : l.market_low == null ? (
                            <span className="muted">—</span>
                          ) : best ? (
                            <span className="badge at">best</span>
                          ) : (
                            <span className="badge above">+{fmt(over)}p over</span>
                          )}
                        </td>
                        <td
                          className="r"
                          onClick={(e) => e.stopPropagation()}
                          onKeyDown={(e) => e.stopPropagation()}
                        >
                          {!session ? (
                            <span className="muted">—</span>
                          ) : confirming ? (
                            <span className="lf-actions">
                              <button
                                type="button"
                                className="btn sm warn"
                                disabled={del.isPending}
                                onClick={() =>
                                  del.mutate(l.order_id, { onSuccess: () => setConfirmId(null) })
                                }
                              >
                                {del.isPending ? "…" : "Confirm"}
                              </button>
                              <button
                                type="button"
                                className="btn sm"
                                onClick={() => setConfirmId(null)}
                              >
                                Cancel
                              </button>
                            </span>
                          ) : (
                            <span className="lf-actions">
                              <button
                                type="button"
                                className="btn sm pos"
                                disabled={markSold.isPending}
                                title="Sold one — drops qty by 1 on warframe.market and logs the sale"
                                onClick={() => markSold.mutate(l.order_id)}
                              >
                                {markSold.isPending ? "…" : "Sold"}
                              </button>
                              <button
                                type="button"
                                className="btn sm"
                                disabled={update.isPending}
                                title={l.visible ? "Hide from buyers" : "Show to buyers"}
                                onClick={() => toggleVisible(l)}
                              >
                                {l.visible ? "Hide" : "Show"}
                              </button>
                              <button
                                type="button"
                                className="btn sm"
                                onClick={() => setEditing(l)}
                              >
                                Edit
                              </button>
                              <button
                                type="button"
                                className="btn sm warn"
                                onClick={() => setConfirmId(l.order_id)}
                              >
                                Delete
                              </button>
                            </span>
                          )}
                        </td>
                      </tr>
                    );
                  })
                )}
              </tbody>
            </table>
          </div>
        </>
      )}

      {picking ? (
        <NewListingModal
          onClose={() => setPicking(false)}
          onPick={(slug) => {
            setPicking(false);
            setCreating({ slug });
          }}
        />
      ) : null}
      {creating ? (
        <ListingForm
          slug={creating.slug}
          initialRank={creating.rank}
          onClose={() => setCreating(null)}
        />
      ) : null}
      {editing ? (
        <ListingForm
          slug={editing.slug}
          edit={{
            orderId: editing.order_id,
            price: editing.your_price,
            qty: editing.qty,
            visible: editing.visible,
          }}
          onClose={() => setEditing(null)}
        />
      ) : null}
    </>
  );
}
