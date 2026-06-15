import { useEffect, useState } from "react";

import { usePrompt } from "./Dialog";
import { tome } from "../service";
import { BookmarkFolder, isTauri } from "../types";

interface BookmarkButtonProps {
  articleTitle: string;
}

/** Sentinel folder key for the root ("Unfiled") group. */
type FolderKey = number | "unfiled";

/**
 * Star button next to the article title in the Reader. Clicking opens a
 * "Save to" menu listing every group with a checkmark for the ones this
 * article is already in — so you can drop the current article into a
 * category (or several) and watch that category fill up as you read.
 *
 * "➕ New group…" creates a folder and saves into it without leaving the
 * article, which is the whole point: start a fresh category mid-read and
 * grow it. The same article can live in multiple groups.
 */
export default function BookmarkButton({ articleTitle }: BookmarkButtonProps) {
  const prompt = usePrompt();
  const [folders, setFolders] = useState<BookmarkFolder[]>([]);
  // folder key -> the bookmark row id, for the groups this article is in.
  const [placements, setPlacements] = useState<Map<FolderKey, number>>(
    new Map(),
  );
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);

  const saved = placements.size > 0;

  async function refresh() {
    if (!isTauri() || !articleTitle) return;
    try {
      const [fs, all] = await Promise.all([
        tome.listFolders(),
        tome.allBookmarks(100000),
      ]);
      setFolders(fs);
      const map = new Map<FolderKey, number>();
      for (const b of all) {
        if (b.article_title === articleTitle) {
          map.set(b.folder_id ?? "unfiled", b.id);
        }
      }
      setPlacements(map);
    } catch {
      // Best-effort — the button isn't load-bearing.
    }
  }

  // Reflect saved state when the article changes (cheap: just the count).
  useEffect(() => {
    if (!isTauri() || !articleTitle) {
      setPlacements(new Map());
      return;
    }
    refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [articleTitle]);

  async function toggle(key: FolderKey) {
    if (busy) return;
    setBusy(true);
    try {
      const existing = placements.get(key);
      if (existing !== undefined) {
        await tome.removeBookmark(existing);
      } else {
        const folderId = key === "unfiled" ? null : key;
        await tome.addBookmark(articleTitle, folderId, null);
      }
      await refresh();
    } catch {
      // best-effort
    } finally {
      setBusy(false);
    }
  }

  async function newGroup() {
    const name = (await prompt({ title: "New group", placeholder: "e.g. Survival skills" }))?.trim();
    if (!name) return;
    setBusy(true);
    try {
      const id = await tome.createFolder(name, null);
      await tome.addBookmark(articleTitle, id, null);
      await refresh();
    } catch {
      // best-effort
    } finally {
      setBusy(false);
    }
  }

  if (!articleTitle) return null;

  return (
    <div className="relative inline-block">
      <button
        type="button"
        onClick={() => {
          if (!open) refresh();
          setOpen((v) => !v);
        }}
        disabled={!isTauri()}
        title={saved ? "Saved — manage groups" : "Save to a group"}
        aria-label={saved ? "Saved — manage groups" : "Save to a group"}
        className={
          "px-2 py-1 text-base rounded transition-colors " +
          (saved
            ? "text-yellow-500 hover:text-yellow-600"
            : "text-tome-muted hover:text-tome-text hover:bg-tome-surface-2")
        }
      >
        {saved ? "★" : "☆"}
      </button>

      {open && (
        <div
          className="absolute right-0 top-full mt-1 w-60 max-h-72 overflow-auto rounded-lg border border-tome-border bg-tome-surface shadow-lg z-30"
          onMouseLeave={() => setOpen(false)}
        >
          <div className="px-3 py-2 border-b border-tome-border text-xs uppercase tracking-wide text-tome-muted">
            Save to group
          </div>
          <ul className="divide-y divide-tome-border">
            <GroupRow
              label="Unfiled"
              icon="📂"
              checked={placements.has("unfiled")}
              onClick={() => toggle("unfiled")}
            />
            {folders.map((f) => (
              <GroupRow
                key={f.id}
                label={f.name}
                icon="📁"
                checked={placements.has(f.id)}
                onClick={() => toggle(f.id)}
              />
            ))}
          </ul>
          <button
            type="button"
            onClick={newGroup}
            disabled={busy}
            className="w-full text-left px-3 py-2 text-sm border-t border-tome-border text-tome-link hover:bg-tome-surface-2 disabled:opacity-50"
          >
            ➕ New group…
          </button>
        </div>
      )}
    </div>
  );
}

function GroupRow({
  label,
  icon,
  checked,
  onClick,
}: {
  label: string;
  icon: string;
  checked: boolean;
  onClick: () => void;
}) {
  return (
    <li
      onClick={onClick}
      className="px-3 py-2 text-sm cursor-pointer hover:bg-tome-surface-2 flex items-center gap-2"
      title={label}
    >
      <span className="w-4 text-tome-success">{checked ? "✓" : ""}</span>
      <span className="text-tome-muted">{icon}</span>
      <span className="truncate flex-1">{label}</span>
    </li>
  );
}
