// Small shared time helpers for the UI.

/**
 * Human-readable relative timestamp ("3 min ago"). Falls back to a locale
 * date string for anything older than a week. `unixSeconds` of 0 (or falsy)
 * means "never read" per the storage convention.
 */
export function relativeTime(unixSeconds: number): string {
  if (!unixSeconds) return "never";
  const now = Math.floor(Date.now() / 1000);
  const diff = now - unixSeconds;
  if (diff < 60) return "just now";
  if (diff < 3600) return `${Math.floor(diff / 60)} min ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} hr ago`;
  if (diff < 604800) return `${Math.floor(diff / 86400)} days ago`;
  return new Date(unixSeconds * 1000).toLocaleDateString();
}
