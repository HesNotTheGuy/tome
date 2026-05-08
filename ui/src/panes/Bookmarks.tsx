import { useEffect, useState } from "react";

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
  const [folders, setFolders] = useState<BookmarkFolder[]>([]);
  const [activeFolder, setActiveFolder] = useState<"all" | number | null>("all");
  const [bookmarks, setBookmarks] = useState<Bookmark[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  function refresh() {
    if (!isTauri()) {
      setLoading(false);
      return;
    }
    setLoading(true);
    Promise.all([
      tome.listFolders(),
      activeFolder === "all"
        ? tome.allBookmarks(500)
        : tome.bookmarksInFolder(activeFolder, 500),
    ])
      .then(([fs, bs]) => {
        setFolders(fs);
        setBookmarks(bs);
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
    const name = prompt("Folder name?")?.trim();
    if (!name) return;
    try {
      await tome.createFolder(name, null);
      refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function rename(folder: BookmarkFolder) {
    const name = prompt("Rename folder", folder.name)?.trim();
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
      !confirm(
        `Delete folder "${folder.name}"? Bookmarks inside it will become unfiled.`,
      )
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
            Folders
          </h2>
          <button
            type="button"
            onClick={newFolder}
            disabled={!isTauri()}
            title="New folder"
            className="text-tome-muted hover:text-tome-text disabled:opacity-50"
          >
            +
          </button>
        </div>
        <ul className="p-2">
          <FolderRow
            label="All bookmarks"
            active={activeFolder === "all"}
            onClick={() => setActiveFolder("all")}
          />
          <FolderRow
            label="Unfiled"
            active={activeFolder === null}
            onClick={() => setActiveFolder(null)}
          />
          <li className="border-t border-tome-border my-2" />
          {folders.map((f) => (
            <FolderRow
              key={f.id}
              label={f.name}
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
                : folders.find((f) => f.id === activeFolder)?.name ?? "Folder"}
          </h2>

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
                    title="Move to folder"
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

function FolderRow({
  label,
  active,
  onClick,
  onRename,
  onDelete,
}: {
  label: string;
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
      <span className="truncate">{label}</span>
      {(onRename || onDelete) && (
        <span className="opacity-0 group-hover:opacity-100 flex items-center gap-1 text-xs">
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
