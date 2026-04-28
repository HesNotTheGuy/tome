import { Revision } from "../types";

interface TimelineProps {
  revisions: Revision[];
}

const HEIGHT = 64;
const BAR_WIDTH = 3;
const VIEWBOX_WIDTH = 1000;

/**
 * Timeline visualization for revision history.
 *
 * X-axis: time (earliest to latest, left to right).
 * Bar height: article size at that revision, normalized to the largest size
 *             observed in the window.
 * Color: minor edits in muted gray, major edits in the link accent.
 */
export default function Timeline({ revisions }: TimelineProps) {
  if (revisions.length === 0) {
    return (
      <div className="text-xs text-tome-muted italic">
        No revisions to display.
      </div>
    );
  }

  // Sort ascending by timestamp for left-to-right rendering.
  const sorted = [...revisions].sort((a, b) =>
    a.timestamp.localeCompare(b.timestamp),
  );

  const maxSize = Math.max(1, ...sorted.map((r) => r.size));
  const earliest = new Date(sorted[0]!.timestamp).getTime();
  const latest = new Date(sorted[sorted.length - 1]!.timestamp).getTime();
  const span = Math.max(1, latest - earliest);

  return (
    <div className="w-full">
      <svg
        width="100%"
        height={HEIGHT}
        viewBox={`0 0 ${VIEWBOX_WIDTH} ${HEIGHT}`}
        preserveAspectRatio="none"
        className="block"
      >
        {/* Baseline */}
        <line
          x1={0}
          y1={HEIGHT - 0.5}
          x2={VIEWBOX_WIDTH}
          y2={HEIGHT - 0.5}
          stroke="var(--tome-border)"
          strokeWidth={1}
        />
        {sorted.map((rev) => {
          const t = new Date(rev.timestamp).getTime();
          const x = ((t - earliest) / span) * VIEWBOX_WIDTH;
          const h = (rev.size / maxSize) * (HEIGHT - 4) + 2;
          return (
            <rect
              key={rev.revision_id}
              x={x - BAR_WIDTH / 2}
              y={HEIGHT - h}
              width={BAR_WIDTH}
              height={h}
              fill={rev.minor ? "var(--tome-text-muted)" : "var(--tome-link)"}
              opacity={rev.minor ? 0.5 : 0.85}
            >
              <title>
                rev {rev.revision_id} by {rev.user || "anon"} —{" "}
                {rev.timestamp.slice(0, 10)} — {rev.size.toLocaleString()} bytes
                {rev.minor ? " (minor)" : ""}
                {rev.comment ? `\n${rev.comment}` : ""}
              </title>
            </rect>
          );
        })}
      </svg>
      <div className="flex justify-between text-[10px] text-tome-muted mt-1 px-1">
        <span>{sorted[0]!.timestamp.slice(0, 10)}</span>
        <span>
          {sorted.length} revision{sorted.length === 1 ? "" : "s"}
        </span>
        <span>{sorted[sorted.length - 1]!.timestamp.slice(0, 10)}</span>
      </div>
    </div>
  );
}
