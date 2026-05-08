import { useEffect, useState } from "react";

import { tome } from "../service";
import { BookmarkFolder, isTauri } from "../types";

interface BookmarkButtonProps {
  articleTitle: string;
}

/**
 * Star button shown next to the article title in the Reader. Click to
 * bookmark; click again to unbookmark. The dropdown lets the user pick
 * a folder (or default to unfiled). Folder list refreshes lazily on open.
 */
export default function BookmarkButton({ articleTitle }: BookmarkButtonProps) {
  const [bookmarked, setBookmarked] = useState<boolean>(false);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [folders, setFolders] = useState<BookmarkFolder[]>([]);

  useEffect(() => {
    if (!isTauri() || !articleTitle) return;
    tome
      .isBookmarked(articleTitle)
      .then(setBookmarked)
      .catch(() => setBookmarked(false));
  }, [articleTitle]);

  async function loadFolders() {
    if (!isTauri()) return;
    try {
      const fs = await tome.listFolders();
      setFolders(fs);
    } catch {
      setFolders([]);
    }
  }

  async function quickToggle() {
    if (!isTauri() || !articleTitle) return;
    if (bookmarked) {
      // Toggle off: find the bookmark(s) for this title and remove them
      // all. We don't currently have a "remove by title" facade — list
      // and delete each.
      try {
        const all = await tome.allBookmarks(1000);
        for (const b of all) {
          if (b.article_title === articleTitle) {
            await tome.removeBookmark(b.id);
          }
        }
        setBookmarked(false);
      } catch {
        // best-effort
      }
    } else {
      // Open the folder picker so the user can choose where to save.
      await loadFolders();
      setPickerOpen(true);
    }
  }

  async function saveTo(folderId: number | null) {
    setPickerOpen(false);
    if (!isTauri() || !articleTitle) return;
    try {
      await tome.addBookmark(articleTitle, folderId, null);
      setBookmarked(true);
    } catch {
      // best-effort
    }
  }

  if (!articleTitle) return null;

  return (
    <div className="relative inline-block">
      <button
        type="button"
        onClick={quickToggle}
        title={bookmarked ? "Remove bookmark" : "Bookmark this article"}
        aria-label={bookmarked ? "Remove bookmark" : "Bookmark"}
        className={
          "px-2 py-1 text-base rounded transition-colors " +
          (bookmarked
            ? "text-yellow-500 hover:text-yellow-600"
            : "text-tome-muted hover:text-tome-text hover:bg-tome-surface-2")
        }
      >
        {bookmarked ? "★" : "☆"}
      </button>

      {pickerOpen && (
        <div
          className="absolute right-0 top-full mt-1 w-56 max-h-64 overflow-auto rounded-lg border border-tome-border bg-tome-surface shadow-lg z-30"
          onMouseLeave={() => setPickerOpen(false)}
        >
          <div className="px-3 py-2 border-b border-tome-border text-xs uppercase tracking-wide text-tome-muted">
            Save to
          </div>
          <ul className="divide-y divide-tome-border">
            <li
              onClick={() => saveTo(null)}
              className="px-3 py-2 text-sm cursor-pointer hover:bg-tome-surface-2"
            >
              <span className="text-tome-muted">📂 </span>Unfiled
            </li>
            {folders.map((f) => (
              <li
                key={f.id}
                onClick={() => saveTo(f.id)}
                className="px-3 py-2 text-sm cursor-pointer hover:bg-tome-surface-2 truncate"
                title={f.name}
              >
                <span className="text-tome-muted">📁 </span>
                {f.name}
              </li>
            ))}
            {folders.length === 0 && (
              <li className="px-3 py-2 text-xs text-tome-muted italic">
                No folders yet. Save unfiled, or create folders in the
                Bookmarks pane.
              </li>
            )}
          </ul>
        </div>
      )}
    </div>
  );
}
