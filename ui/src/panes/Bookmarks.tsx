import { useEffect, useState } from "react";

import { useConfirm, usePrompt } from "../components/Dialog";
import PathField from "../components/PathField";
import { tome } from "../service";
import { Bookmark, BookmarkFolder, isTauri } from "../types";

interface BookmarksProps {
  onOpen: (title: string) => void;
}

/**
 * Bookmarks pane.
 *
 * Two-column layout: folder sidebar on the left, bookmark list on the
 * right. The schema supports nested folders via `parent_id`, but the v1
 * UI only surfaces a single level — adding nesting later doesn't need a
 * migration.
 *
 * Special pseudo-folders:
 *   `null`  → "Unfiled" (folder_id IS NULL)
 *   `"all"` → "All bookmarks" across folders (uses all_bookmarks)
 */
export default function Bookmarks({ onOpen }: BookmarksProps) {
  const confirm = useConfirm();
  const prompt = usePrompt();
  const [folders, setFolders] = useState<BookmarkFolder[]>([]);
  const [activeFolder, setActiveFolder] = useState<"all" | number | null>("all");
  const [bookmarks, setBookmarks] = useState<Bookmark[]>([]);
  // Article count per group key (folder id, or "unfiled" for root), plus the
  // grand total — drives the "watch a category fill up" badges in the sidebar.
  const [counts, setCounts] = useState<Map<number | "unfiled", number>>(
    new Map(),
  );
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  function refresh() {
    if (!isTauri()) {
      setLoading(false);
      return;
    }
    setLoading(true);
    // One fetch of every bookmark: the displayed list is just a client-side
    // filter of it, and the per-group counts a tally — no second round trip.
    Promise.all([tome.listFolders(), tome.allBookmarks(100000)])
      .then(([fs, all]) => {
        setFolders(fs);
        const list =
          activeFolder === "all"
            ? all
            : activeFolder === null
              ? all.filter((b) => b.folder_id == null)
              : all.filter((b) => b.folder_id === activeFolder);
        setBookmarks(list);
        const c = new Map<number | "unfiled", number>();
        for (const b of all) {
          const k = b.folder_id ?? "unfiled";
          c.set(k, (c.get(k) ?? 0) + 1);
        }
        setCounts(c);
        setTotal(all.length);
        setError(null);
      })
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }

  useEffect(() => {
    refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeFolder]);

  async function newFolder() {
    const name = (await prompt({ title: "Group name" }))?.trim();
    if (!name) return;
    try {
      await tome.createFolder(name, null);
      refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function rename(folder: BookmarkFolder) {
    const name = (
      await prompt({ title: "Rename group", defaultValue: folder.name })
    )?.trim();
    if (!name || name === folder.name) return;
    try {
      await tome.renameFolder(folder.id, name);
      refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function removeFolder(folder: BookmarkFolder) {
    if (
      !(await confirm({
        message: `Delete group "${folder.name}"? Bookmarks inside it become unfiled.`,
        danger: true,
      }))
    ) {
      return;
    }
    try {
      await tome.deleteFolder(folder.id);
      if (activeFolder === folder.id) setActiveFolder("all");
      else refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function removeBookmark(b: Bookmark) {
    try {
      await tome.removeBookmark(b.id);
      refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function moveBookmark(b: Bookmark, folderId: number | null) {
    try {
      await tome.moveBookmark(b.id, folderId);
      refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <section className="h-full flex">
      {/* Sidebar */}
      <aside className="w-64 shrink-0 border-r border-tome-border bg-tome-surface overflow-y-auto">
        <div className="p-3 border-b border-tome-border flex items-center justify-between">
          <h2 className="text-sm font-bold uppercase tracking-wide text-tome-muted">
            Groups
          </h2>
          <button
            type="button"
            onClick={newFolder}
            disabled={!isTauri()}
            title="New group"
            className="text-tome-muted hover:text-tome-text disabled:opacity-50"
          >
            +
          </button>
        </div>
        <ul className="p-2">
          <FolderRow
            label="All bookmarks"
            count={total}
            active={activeFolder === "all"}
            onClick={() => setActiveFolder("all")}
          />
          <FolderRow
            label="Unfiled"
            count={counts.get("unfiled") ?? 0}
            active={activeFolder === null}
            onClick={() => setActiveFolder(null)}
          />
          <li className="border-t border-tome-border my-2" />
          {folders.map((f) => (
            <FolderRow
              key={f.id}
              label={f.name}
              count={counts.get(f.id) ?? 0}
              active={activeFolder === f.id}
              onClick={() => setActiveFolder(f.id)}
              onRename={() => rename(f)}
              onDelete={() => removeFolder(f)}
            />
          ))}
        </ul>
      </aside>

      {/* Main */}
      <div className="flex-1 overflow-y-auto">
        <div className="px-6 py-6 max-w-4xl mx-auto">
          <h2 className="text-2xl font-bold mb-4">
            {activeFolder === "all"
              ? "All bookmarks"
              : activeFolder === null
                ? "Unfiled"
                : folders.find((f) => f.id === activeFolder)?.name ?? "Group"}
          </h2>

          <BackupSection onChanged={refresh} />

          {!isTauri() && (
            <div className="p-4 mb-4 rounded border border-tome-border bg-tome-surface-2 text-sm">
              Running outside the Tauri shell — no data available.
            </div>
          )}

          {error && (
            <div className="p-3 mb-3 rounded border border-tome-danger/50 bg-tome-danger/10 text-sm text-tome-danger">
              {error}
            </div>
          )}

          {loading && <div className="text-sm text-tome-muted">Loading…</div>}

          {!loading && bookmarks.length === 0 && !error && (
            <div className="p-6 rounded border border-dashed border-tome-border text-center text-sm text-tome-muted">
              No bookmarks here yet. Click the ⭐ in the Reader to save an
              article.
            </div>
          )}

          {bookmarks.length > 0 && (
            <ul className="rounded border border-tome-border overflow-hidden divide-y divide-tome-border">
              {bookmarks.map((b) => (
                <li
                  key={b.id}
                  className="p-3 hover:bg-tome-surface-2 flex items-start gap-3"
                >
                  <div
                    onClick={() => onOpen(b.article_title)}
                    className="flex-1 min-w-0 cursor-pointer"
                  >
                    <div className="text-sm font-medium truncate">
                      {b.article_title}
                    </div>
                    {b.note && (
                      <div className="text-xs text-tome-muted mt-0.5 italic">
                        {b.note}
                      </div>
                    )}
                  </div>
                  <select
                    value={b.folder_id ?? ""}
                    onChange={(e) =>
                      moveBookmark(
                        b,
                        e.target.value === "" ? null : Number(e.target.value),
                      )
                    }
                    className="text-xs px-1 py-0.5 rounded border border-tome-border bg-tome-bg"
                    title="Move to group"
                  >
                    <option value="">(unfiled)</option>
                    {folders.map((f) => (
                      <option key={f.id} value={f.id}>
                        {f.name}
                      </option>
                    ))}
                  </select>
                  <button
                    type="button"
                    onClick={() => removeBookmark(b)}
                    title="Remove bookmark"
                    className="text-xs text-tome-muted hover:text-tome-danger"
                  >
                    ✕
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </section>
  );
}

/**
 * Backup & restore. Exports all bookmarks + folders to a portable, versioned
 * JSON file, and imports one back. Paths are pasted (same convention as the
 * dump/module paths elsewhere in the app). Import defaults to a safe
 * non-destructive merge; "replace" is opt-in and confirmed.
 */
function BackupSection({ onChanged }: { onChanged: () => void }) {
  const confirm = useConfirm();
  const [open, setOpen] = useState(false);
  const [exportPath, setExportPath] = useState("");
  const [importPath, setImportPath] = useState("");
  const [replace, setReplace] = useState(false);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);

  async function doExport() {
    if (!exportPath.trim()) {
      setErr("Enter a folder or a .json file path to save the backup to.");
      return;
    }
    setBusy(true);
    setErr(null);
    setMsg(null);
    try {
      const s = await tome.exportBookmarks(exportPath.trim());
      setMsg(`Saved ${s.bookmarks} bookmark(s) and ${s.folders} group(s) → ${s.path}`);
      setExportPath("");
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function doImport() {
    if (!importPath.trim()) {
      setErr("Enter the path to a backup .json file.");
      return;
    }
    if (
      replace &&
      !(await confirm({
        message:
          "Replace ALL current bookmarks and groups with this backup? This cannot be undone.",
        danger: true,
      }))
    ) {
      return;
    }
    setBusy(true);
    setErr(null);
    setMsg(null);
    try {
      const s = await tome.importBookmarks(importPath.trim(), replace);
      let m = `Added ${s.bookmarks_added} bookmark(s)`;
      if (s.bookmarks_skipped > 0) m += ` (${s.bookmarks_skipped} already present)`;
      m += `, ${s.folders_created} new group(s).`;
      if (s.from_newer_version) {
        m +=
          " Note: this backup was made by a newer version of Tome — imported everything this version understands.";
      }
      setMsg(m);
      setImportPath("");
      onChanged();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="mb-4 rounded border border-tome-border bg-tome-surface">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        disabled={!isTauri()}
        className="w-full flex items-center justify-between px-3 py-2 text-sm text-tome-muted hover:text-tome-text disabled:opacity-50"
      >
        <span className="font-semibold uppercase tracking-wide">Backup &amp; restore</span>
        <span>{open ? "▾" : "▸"}</span>
      </button>

      {open && (
        <div className="px-3 pb-3 space-y-4 border-t border-tome-border pt-3">
          {/* Export */}
          <div className="space-y-1.5">
            <label className="text-xs font-medium text-tome-muted">
              Export to a backup file
            </label>
            <div className="flex gap-2">
              <div className="flex-1">
                <PathField
                  value={exportPath}
                  onChange={setExportPath}
                  mode="saveFile"
                  filters={[{ name: "JSON", extensions: ["json"] }]}
                  defaultFileName="tome-bookmarks.json"
                  placeholder="folder path, or full path ending in .json"
                  disabled={busy}
                />
              </div>
              <button
                type="button"
                onClick={doExport}
                disabled={busy}
                className="px-3 py-1 text-sm rounded text-white disabled:opacity-50"
                style={{ backgroundColor: "var(--tome-accent)" }}
              >
                Export
              </button>
            </div>
          </div>

          {/* Import */}
          <div className="space-y-1.5">
            <label className="text-xs font-medium text-tome-muted">
              Restore from a backup file
            </label>
            <div className="flex gap-2">
              <div className="flex-1">
                <PathField
                  value={importPath}
                  onChange={setImportPath}
                  mode="openFile"
                  filters={[{ name: "JSON", extensions: ["json"] }]}
                  placeholder="/path/to/tome-bookmarks.json"
                  disabled={busy}
                />
              </div>
              <button
                type="button"
                onClick={doImport}
                disabled={busy}
                className="px-3 py-1 text-sm rounded border border-tome-border hover:bg-tome-surface-2 disabled:opacity-50"
              >
                Import
              </button>
            </div>
            <label className="flex items-center gap-1.5 text-xs text-tome-muted">
              <input
                type="checkbox"
                checked={replace}
                onChange={(e) => setReplace(e.target.checked)}
                disabled={busy}
              />
              Replace everything (wipe current bookmarks first) — otherwise merge
            </label>
          </div>

          {msg && <div className="text-xs text-tome-success">{msg}</div>}
          {err && <div className="text-xs text-tome-danger">{err}</div>}
        </div>
      )}
    </div>
  );
}

function FolderRow({
  label,
  count,
  active,
  onClick,
  onRename,
  onDelete,
}: {
  label: string;
  count?: number;
  active: boolean;
  onClick: () => void;
  onRename?: () => void;
  onDelete?: () => void;
}) {
  return (
    <li
      onClick={onClick}
      className={
        "px-3 py-1.5 rounded text-sm cursor-pointer flex items-center justify-between gap-2 group " +
        (active
          ? "bg-tome-surface-2 text-tome-text"
          : "text-tome-muted hover:bg-tome-surface-2 hover:text-tome-text")
      }
    >
      <span className="truncate flex-1">{label}</span>
      {count !== undefined && (
        <span className="text-xs text-tome-muted tabular-nums shrink-0">
          {count}
        </span>
      )}
      {(onRename || onDelete) && (
        <span className="opacity-0 group-hover:opacity-100 flex items-center gap-1 text-xs shrink-0">
          {onRename && (
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                onRename();
              }}
              title="Rename"
              className="hover:text-tome-text"
            >
              ✎
            </button>
          )}
          {onDelete && (
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                onDelete();
              }}
              title="Delete"
              className="hover:text-tome-danger"
            >
              ✕
            </button>
          )}
        </span>
      )}
    </li>
  );
}
