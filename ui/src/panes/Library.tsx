import { useEffect, useState } from "react";
import { tome } from "../service";
import { InstalledModule, IS_TAURI } from "../types";

interface LibraryProps {
  onOpen: (title: string) => void;
}

export default function Library({ onOpen }: LibraryProps) {
  const [modules, setModules] = useState<InstalledModule[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    if (!IS_TAURI) {
      setLoaded(true);
      return;
    }
    tome
      .listModules()
      .then((list) => setModules(list))
      .catch((e) => setError(String(e)))
      .finally(() => setLoaded(true));
  }, []);

  return (
    <section className="px-6 py-6 max-w-5xl mx-auto">
      <h2 className="text-2xl font-bold mb-1">Library</h2>
      <p className="text-sm text-zinc-500 dark:text-zinc-400 mb-6">
        Modules and installed articles. Open any article in the Reader.
      </p>

      {!IS_TAURI && (
        <div className="p-4 mb-6 rounded border border-amber-300 dark:border-amber-700 bg-amber-50 dark:bg-amber-950 text-sm">
          Running outside the Tauri shell — backend not connected. Launch via
          <code className="mx-1 px-1.5 py-0.5 rounded bg-amber-100 dark:bg-amber-900">
            cargo tauri dev
          </code>
          to load real data.
        </div>
      )}

      {error && (
        <div className="p-4 mb-6 rounded border border-red-300 dark:border-red-800 bg-red-50 dark:bg-red-950 text-sm text-red-700 dark:text-red-300">
          {error}
        </div>
      )}

      {loaded && modules.length === 0 && (
        <div className="p-6 rounded border border-dashed border-zinc-300 dark:border-zinc-700 text-center text-sm text-zinc-500 dark:text-zinc-400">
          No modules installed yet.
          <br />
          <span className="text-xs">
            Browse Wikipedia categories or import a TOML module to get started.
          </span>
        </div>
      )}

      <ul className="grid gap-3 grid-cols-1 sm:grid-cols-2 lg:grid-cols-3">
        {modules.map((m) => (
          <li
            key={m.spec.id}
            className="p-4 rounded border border-zinc-200 dark:border-zinc-800 bg-white dark:bg-zinc-900 hover:border-zinc-300 dark:hover:border-zinc-700"
          >
            <div className="flex items-start justify-between gap-2">
              <h3 className="font-semibold">{m.spec.name}</h3>
              <span className="text-[10px] uppercase tracking-wide px-1.5 py-0.5 rounded bg-zinc-100 dark:bg-zinc-800 text-zinc-500 dark:text-zinc-400">
                {m.spec.default_tier}
              </span>
            </div>
            {m.spec.description && (
              <p className="mt-1 text-sm text-zinc-600 dark:text-zinc-400 line-clamp-2">
                {m.spec.description}
              </p>
            )}
            <p className="mt-3 text-xs text-zinc-500 dark:text-zinc-500">
              {m.member_count.toLocaleString()} articles
            </p>
            <button
              type="button"
              onClick={() => onOpen(m.spec.name)}
              className="mt-3 text-sm text-blue-600 dark:text-blue-400 hover:underline"
            >
              Open module →
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}
