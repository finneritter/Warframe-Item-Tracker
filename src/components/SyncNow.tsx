import { useState } from "react";
import { useGameScanPreview, useGameScanStatus, useWfmAccount, useWfmSync } from "../hooks/queries";
import { clsx, syncListingsNote } from "../lib/format";
import type { ScanDiffRow } from "../lib/types";
import { ReviewPanel, errMessage } from "./GameScanPanel";
import { Icon } from "./Icon";

/**
 * Topbar "Sync now": one click runs everything Settings/Listings can sync —
 * your warframe.market listings (if connected) and the consent-gated game
 * inventory scan (if enabled and Warframe is running). The scan result still
 * goes through the same ReviewPanel before anything is written. Disabled with
 * an explanatory tooltip when neither source is set up.
 */
export function SyncNow() {
  const { data: account } = useWfmAccount();
  const { data: scan } = useGameScanStatus();
  const wfmSync = useWfmSync();
  const preview = useGameScanPreview();
  const [diff, setDiff] = useState<ScanDiffRow[] | null>(null);
  // Outcome of the last run — a muted summary or a neg error; replaced on next sync.
  const [note, setNote] = useState<{ text: string; neg: boolean } | null>(null);

  const canListings = !!account?.connected;
  const canScan = !!scan?.supported && !!scan.consented && !!scan.warframe_running;
  const pending = wfmSync.isPending || preview.isPending;

  const parts = [canListings && "warframe.market listings", canScan && "game inventory"]
    .filter(Boolean)
    .join(" + ");
  const title = parts
    ? `Sync ${parts}`
    : "Nothing to sync — connect warframe.market and/or enable the game scan in Settings";

  const run = async () => {
    setNote(null);
    // Each job resolves to a short summary fragment (or null when the review
    // modal carries the result). Both run concurrently; one failing doesn't
    // stop the other.
    const jobs: Promise<string | null>[] = [];
    if (canListings) jobs.push(wfmSync.mutateAsync().then(syncListingsNote));
    if (canScan)
      jobs.push(
        preview.mutateAsync().then((rows) => {
          if (rows.length === 0) return "inventory in sync";
          setDiff(rows);
          return null;
        }),
      );
    const results = await Promise.allSettled(jobs);
    const failed = results.find((r): r is PromiseRejectedResult => r.status === "rejected");
    if (failed) {
      setNote({ text: errMessage(failed.reason), neg: true });
      return;
    }
    const texts = results
      .map((r) => (r.status === "fulfilled" ? r.value : null))
      .filter((v): v is string => !!v);
    if (texts.length) setNote({ text: texts.join(" · "), neg: false });
  };

  return (
    <>
      {note ? (
        <span
          className={clsx("muted", note.neg && "neg")}
          title={note.text}
          style={{
            maxWidth: 240,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {note.text}
        </span>
      ) : null}
      <button
        type="button"
        className={clsx("icon-btn", pending && "pulsing")}
        title={title}
        onClick={run}
        disabled={pending || !parts}
      >
        <Icon name="sync" />
      </button>
      {diff ? <ReviewPanel rows={diff} onClose={() => setDiff(null)} /> : null}
    </>
  );
}
