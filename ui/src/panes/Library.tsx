import { useCallback, useEffect, useState } from "react";
import { tome } from "../service";
import { InstalledModule, isTauri } from "../types";

interface LibraryProps {
  onOpen: (title: string) => void;
}

export default function Library({ onOpen }: LibraryProps) {
  const [modules, setModules] = useState<InstalledModule[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);

  const refresh = useCallback(() => {
    if (!isTauri()) {
      setLoaded(true);
      return;
    }
    tome
      .listModules()
      .then((list) => setModules(list))
      .catch((e) => setError(String(e)))
      .finally(() => setLoaded(true));
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return (
    <section className="px-6 py-6 max-w-5xl mx-auto">
      <div className="flex items-end justify-between mb-6">
        <div>
          <h2 className="text-2xl font-bold mb-1">Library</h2>
          <p className="text-sm text-tome-muted">
            Modules and installed articles. Open any article in the Reader.
          </p>
        </div>
      </div>

      {!isTauri() && (
        <div className="p-4 mb-6 rounded border border-tome-border bg-tome-surface-2 text-sm">
          Running outside the Tauri shell — backend not connected. Launch via{" "}
          <code className="px-1.5 py-0.5 rounded bg-tome-surface">
            cargo tauri dev
          </code>{" "}
          to load real data.
        </div>
      )}

      {error && (
        <div className="p-4 mb-6 rounded border border-tome-border bg-tome-surface-2 text-sm text-tome-danger">
          {error}
        </div>
      )}

      <ImportSection onComplete={refresh} />

      {loaded && modules.length === 0 && (
        <div className="p-6 rounded border border-dashed border-tome-border text-center text-sm text-tome-muted">
          No modules installed yet.
          <br />
          <span className="text-xs">
            Import a TOML module above to get started. Try{" "}
            <code className="px-1 py-0.5 rounded bg-tome-surface-2">
              samples/science-basics.toml
            </code>
            .
          </span>
        </div>
      )}

      <ul className="grid gap-3 grid-cols-1 sm:grid-cols-2 lg:grid-cols-3">
        {modules.map((m) => (
          <li
            key={m.spec.id}
            className="p-4 rounded border border-tome-border bg-tome-surface hover:border-tome-border-strong"
          >
            <div className="flex items-start justify-between gap-2">
              <h3 className="font-semibold">{m.spec.name}</h3>
              <span className="text-[10px] uppercase tracking-wide px-1.5 py-0.5 rounded bg-tome-surface-2 text-tome-muted">
                {m.spec.default_tier}
              </span>
            </div>
            {m.spec.description && (
              <p className="mt-1 text-sm text-tome-muted line-clamp-2">
                {m.spec.description}
              </p>
            )}
            <p className="mt-3 text-xs text-tome-muted">
              {m.member_count.toLocaleString()} articles
            </p>
            <button
              type="button"
              onClick={() =>
                m.spec.explicit_titles[0] && onOpen(m.spec.explicit_titles[0])
              }
              className="mt-3 text-sm text-tome-link hover:underline"
            >
              Open first article →
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}

function ImportSection({ onComplete }: { onComplete: () => void }) {
  const [path, setPath] = useState("");
  const [phase, setPhase] = useState<"idle" | "running" | "done" | "error">(
    "idle",
  );
  const [error, setError] = useState<string | null>(null);
  const [installed, setInstalled] = useState<InstalledModule | null>(null);

  async function handleImport() {
    if (!isTauri()) {
      setError("import requires the Tauri shell");
      setPhase("error");
      return;
    }
    if (!path.trim()) {
      setError("paste the path to a TOML module file");
      setPhase("error");
      return;
    }
    setPhase("running");
    setError(null);
    setInstalled(null);
    try {
      const result = await tome.importModuleFromPath(path.trim());
      setInstalled(result);
      setPhase("done");
      setPath("");
      onComplete();
    } catch (e) {
      setError(String(e));
      setPhase("error");
    }
  }

  return (
    <div className="mb-6 p-4 rounded border border-tome-border bg-tome-surface space-y-3">
      <h3 className="text-sm font-semibold uppercase tracking-wide text-tome-muted">
        Import module
      </h3>
      <p className="text-xs text-tome-muted">
        Pass the path to a TOML module file. The spec format is documented in{" "}
        <code className="px-1 py-0.5 rounded bg-tome-surface-2">
          samples/science-basics.toml
        </code>
        . For now, modules install with the spec&apos;s explicit_titles as
        members; category resolution lands in a follow-up.
      </p>
      <input
        type="text"
        value={path}
        onChange={(e) => setPath(e.target.value)}
        disabled={phase === "running"}
        placeholder="/path/to/module.toml"
        className="w-full px-2 py-1 text-xs font-mono rounded border border-tome-border bg-tome-bg disabled:opacity-50"
      />
      <div className="flex items-center justify-between gap-3">
        <button
          type="button"
          onClick={handleImport}
          disabled={phase === "running" || !isTauri()}
          className="px-3 py-1 text-sm rounded text-white disabled:opacity-50 disabled:cursor-not-allowed"
          style={{ backgroundColor: "var(--tome-accent)" }}
        >
          {phase === "running" ? "Importing…" : "Import"}
        </button>
        {phase === "done" && installed && (
          <span className="text-xs text-tome-success">
            ✓ Installed “{installed.spec.name}” — {installed.member_count}{" "}
            members
          </span>
        )}
        {phase === "error" && error && (
          <span className="text-xs text-tome-danger">{error}</span>
        )}
      </div>
    </div>
  );
}
