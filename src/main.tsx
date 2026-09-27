import { MutationCache, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { RecoveryScreen } from "./components/RecoveryScreen";
import "./index.css";
import { startupStatus } from "./lib/api";
import { applyPrefs, loadPrefs } from "./lib/prefs";
import { errorMessage, pushToast } from "./lib/toast";
import type { StartupStatus } from "./lib/types";

// Apply saved theme/density before first paint so there's no flash.
applyPrefs(loadPrefs());

// One client for the whole app. Everything is local SQLite via invoke(), so
// cached data is cheap to re-read; keep it fresh-ish but not chatty.
// Failed MUTATIONS surface as a toast — a write the user asked for must never
// fail silently. (Query failures stay per-screen: TableStatus/BlockStatus
// already render them, and polled queries would spam a global toast.)
const queryClient = new QueryClient({
  mutationCache: new MutationCache({
    onError: (e) => pushToast(errorMessage(e)),
  }),
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      refetchOnWindowFocus: false,
      retry: 1,
    },
  },
});

// Boot gate: nothing mounts (and no AppState-backed command fires) until the
// backend reports which mode it's in. A failed startup renders the recovery
// screen instead of the app — State-taking commands would panic without state.
// The page can load before backend setup has run (Windows/WebView2, issue #4),
// so a `pending` answer is polled until setup lands — never shown as a failure
// unless it outlasts BOOT_TIMEOUT_MS.
const BOOT_POLL_MS = 100;
const BOOT_TIMEOUT_MS = 60_000;

function Boot() {
  const [status, setStatus] = useState<StartupStatus | null>(null);
  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const started = Date.now();
    const poll = () => {
      startupStatus()
        .then((s) => {
          if (cancelled) return;
          if (!s.pending) return setStatus(s);
          if (Date.now() - started > BOOT_TIMEOUT_MS) {
            return setStatus({
              ok: false,
              error: "backend startup timed out (setup never finished)",
              db_path: null,
            });
          }
          timer = setTimeout(poll, BOOT_POLL_MS);
        })
        .catch((e) => {
          if (!cancelled) setStatus({ ok: false, error: String(e), db_path: null });
        });
    };
    poll();
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, []);
  if (!status) return null; // usually sub-frame; not worth a splash
  if (!status.ok)
    return <RecoveryScreen error={status.error ?? "unknown error"} dbPath={status.db_path} />;
  return <App />;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <ErrorBoundary>
        <Boot />
      </ErrorBoundary>
    </QueryClientProvider>
  </React.StrictMode>,
);
