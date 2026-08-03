// The set drawer: one prime set's completion picture — part-chip grid, per-part
// prices/ownership, and the set-vs-parts economics. Opens from any row in the
// Sets table; part names click through to the item Drawer (owned) or straight to
// the Market screen (missing = "buy this" cue), stacking like the RelicDrawer.
// The header carries the one external link: the *set's* warframe.market page (the
// parts each have their own; the assembled set didn't until a beta report asked).
import { useSets } from "../hooks/queries";
import { useDrawerResize } from "../hooks/useDrawerResize";
import { useEscape } from "../hooks/useEscape";
import { clsx, fmt } from "../lib/format";
import type { SetRow } from "../lib/types";
import { openMarketExternal } from "../lib/wiki";
import type { ScreenId } from "./Sidebar";
import { Scrim } from "./ui";

/// Narrowest the drawer can be dragged: below this the parts table crushes.
const MIN_WIDTH = 400;

export const setPartsSum = (row: SetRow): number | null => {
  if (!row.parts.some((p) => p.median_plat != null)) return null;
  return row.parts.reduce((a, p) => a + (p.median_plat ?? 0) * p.required, 0);
};

export function SetDrawer({
  slug,
  active,
  onClose,
  onOpen,
  onNavigate,
}: {
  slug: string;
  /** false while the item Drawer is stacked on top — gates Escape + scrim close. */
  active: boolean;
  onClose: () => void;
  onOpen: (slug: string) => void;
  onNavigate: (s: ScreenId, opts?: { marketSlug?: string }) => void;
}) {
  const { data: sets = [], isError } = useSets();
  const row = sets.find((s) => s.set_slug === slug);
  useEscape(active ? onClose : () => {});

  // Resizable width — same affordance as the Relic/item drawers, own key.
  const { width, startResize } = useDrawerResize("wfit.setDrawerWidth", MIN_WIDTH, 480);

  const grip = (
    // biome-ignore lint/a11y/useKeyWithClickEvents: pointer-only resize affordance (no keyboard equivalent)
    <div
      className="drawer-grip"
      style={{ right: width }}
      onPointerDown={startResize}
      onClick={(e) => e.stopPropagation()}
      title="Drag to resize"
    />
  );

  if (!row) {
    return (
      <Scrim className="scrim" onClose={onClose}>
        {grip}
        <div className="drawer" style={{ width }}>
          <div className="drawer-h">
            <div className="di">
              <div className="nm">{isError ? "Couldn't load this set." : "Loading…"}</div>
            </div>
            <button type="button" className="x" onClick={onClose}>
              ✕
            </button>
          </div>
        </div>
      </Scrim>
    );
  }

  const oneAway = !row.complete && row.total_parts - row.owned_parts === 1;
  const partsSum = setPartsSum(row);
  const delta = row.set_value != null && partsSum != null ? row.set_value - partsSum : null;
  const goMarket = (s: string) => {
    onClose();
    onNavigate("market", { marketSlug: s });
  };

  return (
    <Scrim className="scrim" onClose={active ? onClose : () => {}}>
      {grip}
      <div className="drawer" style={{ width }}>
        <div className="drawer-h">
          <div className="di">
            <div className="nm">
              {row.set_name}
              {row.complete ? (
                <span className="itag itag-complete" title="all parts owned">
                  COMPLETE
                </span>
              ) : oneAway ? (
                <span className="itag itag-oneaway" title="one part missing">
                  ONE AWAY
                </span>
              ) : null}
            </div>
            {/* The market link lives INSIDE .sub on purpose: `.lnk` is declared later in
                theme.css with equal specificity, so its `font: inherit` only resolves to
                this line's 11.5px when it's nested. */}
            <div className="sub">
              {row.category} · {row.owned_parts}/{row.total_parts} parts owned ·{" "}
              <button
                type="button"
                className="lnk"
                title="Open the full set's warframe.market page in your browser"
                onClick={() => openMarketExternal(row.set_slug)}
              >
                warframe.market ↗
              </button>
            </div>
          </div>
          <button type="button" className="x" onClick={onClose}>
            ✕
          </button>
        </div>

        <div className="drawer-body">
          {/* The part-chip grid — the Sets screen's signature glanceable device. */}
          <div className="pchips" style={{ marginBottom: 12 }}>
            {row.parts.map((p) => (
              <div
                key={p.slug}
                className={clsx("pchip", p.owned ? "have" : "miss")}
                // biome-ignore lint/a11y/useSemanticElements: styled as a div chip; no native-button reset exists in the theme
                role="button"
                tabIndex={0}
                onClick={() => (p.owned ? onOpen(p.slug) : goMarket(p.slug))}
                onKeyDown={(e) => {
                  if ((e.key === "Enter" || e.key === " ") && e.target === e.currentTarget) {
                    e.preventDefault();
                    if (p.owned) onOpen(p.slug);
                    else goMarket(p.slug);
                  }
                }}
                title={
                  (p.required > 1 ? `${p.part_name} ×${p.required} — ` : "") +
                  (p.owned ? p.part_name : `Buy ${p.part_name} on warframe.market`)
                }
              >
                <span className="pa">
                  {p.part_name.slice(0, 3)}
                  {p.required > 1 ? ` ×${p.required}` : ""}
                </span>
                {p.owned ? (
                  <span className="ck">✓</span>
                ) : p.required > 1 && p.owned_qty > 0 ? (
                  <span className="pp">
                    {p.owned_qty}/{p.required}
                  </span>
                ) : (
                  <span className="pp">
                    {p.median_plat == null ? "—" : `${fmt(p.median_plat)}p`}
                  </span>
                )}
              </div>
            ))}
          </div>

          {/* Set-vs-parts economics. */}
          <div className="rankbox">
            <div className="rankbox-h">
              <b>Economics</b>
              <span className="muted"> · full set vs loose parts</span>
            </div>
            <table className="dtable">
              <tbody>
                <tr>
                  <td>Full-set price</td>
                  <td className="r num">
                    {row.set_value != null ? `${fmt(row.set_value)}p` : "—"}
                  </td>
                </tr>
                <tr>
                  <td>Parts sold loose</td>
                  <td className="r num">{partsSum != null ? `${fmt(partsSum)}p` : "—"}</td>
                </tr>
                <tr>
                  <td title="what assembling the set adds over selling parts loose">Set premium</td>
                  <td className={clsx("r num", delta != null && (delta >= 0 ? "pos" : "neg"))}>
                    {delta != null ? `${delta >= 0 ? "+" : ""}${fmt(delta)}p` : "—"}
                  </td>
                </tr>
                {!row.complete ? (
                  <tr>
                    <td>To complete (buy missing)</td>
                    <td className="r num">
                      {row.missing_value != null ? `${fmt(row.missing_value)}p` : "—"}
                    </td>
                  </tr>
                ) : null}
              </tbody>
            </table>
          </div>

          {/* Per-part detail; names open the item Drawer (owned) or Market (missing). */}
          <div className="rankbox">
            <div className="rankbox-h">
              <b>Parts</b>
              <span className="muted"> · owned parts open the item drawer</span>
            </div>
            <table className="dtable">
              <thead>
                <tr>
                  <th>Part</th>
                  <th className="r">Price</th>
                  <th className="r">Owned</th>
                </tr>
              </thead>
              <tbody>
                {row.parts.map((p) => (
                  <tr key={p.slug}>
                    <td>
                      <button
                        type="button"
                        className="crk-link"
                        title={p.owned ? p.part_name : `Buy ${p.part_name} on warframe.market`}
                        onClick={() => (p.owned ? onOpen(p.slug) : goMarket(p.slug))}
                      >
                        {p.part_name}
                      </button>
                    </td>
                    <td className="r num">
                      {p.median_plat != null ? `${fmt(p.median_plat)}p` : "—"}
                      {p.required > 1 ? <span className="muted"> ×{p.required}</span> : null}
                    </td>
                    <td className={clsx("r num", !p.owned && "muted")}>
                      {p.required > 1 ? `${p.owned_qty}/${p.required}` : p.owned ? "✓" : "—"}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </Scrim>
  );
}
