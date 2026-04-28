import { useEffect, useRef, useState } from "react";
import L from "leaflet";
import "leaflet/dist/leaflet.css";
import Supercluster from "supercluster";

import { tome } from "../service";
import { isTauri, MappedGeotag } from "../types";

interface MapProps {
  onOpen: (title: string) => void;
}

type PointFeature = Supercluster.PointFeature<{ title: string; page_id: number }>;

/**
 * World map of every primary-geotagged article we've indexed.
 *
 * Implementation notes:
 *
 * - **Leaflet + OpenStreetMap raster tiles**. Tiles need network; if offline
 *   the basemap is empty but the cluster pins still render. That's the price
 *   of not bundling 5+ GB of tiles.
 * - **Supercluster** does viewport-aware clustering on the JS side. We load
 *   every point once (cheap — simplewiki ~7k, enwiki ~1.6M still fits in
 *   memory) and recompute visible clusters whenever the map moves.
 * - **Cluster markers** are styled with a div-icon so we can show counts.
 *   Individual markers are circle-markers — Leaflet's default png-based pin
 *   doesn't survive bundling cleanly and circles are more honest about a
 *   coord being "approximate" anyway.
 */
export default function Map({ onOpen }: MapProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const mapRef = useRef<L.Map | null>(null);
  const layerRef = useRef<L.LayerGroup | null>(null);
  const indexRef = useRef<Supercluster<{ title: string; page_id: number }> | null>(
    null,
  );
  const [phase, setPhase] = useState<"idle" | "loading" | "ready" | "empty" | "error">(
    "idle",
  );
  const [count, setCount] = useState(0);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!containerRef.current) return;
    const map = L.map(containerRef.current, {
      worldCopyJump: true,
      zoomControl: true,
    }).setView([20, 0], 2);

    L.tileLayer("https://tile.openstreetmap.org/{z}/{x}/{y}.png", {
      attribution:
        '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors',
      maxZoom: 19,
    }).addTo(map);

    const layer = L.layerGroup().addTo(map);
    mapRef.current = map;
    layerRef.current = layer;

    const onMoveEnd = () => render();
    map.on("moveend", onMoveEnd);
    map.on("zoomend", onMoveEnd);

    return () => {
      map.off("moveend", onMoveEnd);
      map.off("zoomend", onMoveEnd);
      map.remove();
      mapRef.current = null;
      layerRef.current = null;
      indexRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!isTauri()) return;
    setPhase("loading");
    tome
      .allPrimaryGeotags()
      .then((rows: MappedGeotag[]) => {
        setCount(rows.length);
        if (rows.length === 0) {
          setPhase("empty");
          return;
        }
        const features: PointFeature[] = rows.map((g) => ({
          type: "Feature",
          geometry: { type: "Point", coordinates: [g.lon, g.lat] },
          properties: { title: g.title, page_id: g.page_id },
        }));
        const idx = new Supercluster<{ title: string; page_id: number }>({
          radius: 60,
          maxZoom: 16,
        });
        idx.load(features);
        indexRef.current = idx;
        setPhase("ready");
        render();
      })
      .catch((e) => {
        setError(String(e));
        setPhase("error");
      });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  function render() {
    const map = mapRef.current;
    const layer = layerRef.current;
    const idx = indexRef.current;
    if (!map || !layer || !idx) return;
    layer.clearLayers();

    const b = map.getBounds();
    const bbox: [number, number, number, number] = [
      b.getWest(),
      b.getSouth(),
      b.getEast(),
      b.getNorth(),
    ];
    const zoom = Math.round(map.getZoom());
    const clusters = idx.getClusters(bbox, zoom);

    for (const c of clusters) {
      const coords = c.geometry.coordinates as [number, number];
      const lon = coords[0];
      const lat = coords[1];
      const props = c.properties as
        | Supercluster.ClusterProperties
        | { title: string; page_id: number };

      if ("cluster" in props && props.cluster) {
        const n = props.point_count;
        const size = n < 10 ? 28 : n < 100 ? 36 : n < 1000 ? 44 : 52;
        const html = `<div class="tome-cluster" style="width:${size}px;height:${size}px;line-height:${size}px;">${
          n < 1000 ? n : `${(n / 1000).toFixed(1)}k`
        }</div>`;
        const icon = L.divIcon({
          html,
          className: "",
          iconSize: [size, size],
        });
        const m = L.marker([lat, lon], { icon });
        m.on("click", () => {
          const expandTo = Math.min(idx.getClusterExpansionZoom(props.cluster_id), 16);
          map.flyTo([lat, lon], expandTo);
        });
        layer.addLayer(m);
      } else {
        const p = props as { title: string; page_id: number };
        const m = L.circleMarker([lat, lon], {
          radius: 5,
          weight: 1.5,
          color: "var(--tome-accent, #6366f1)",
          fillColor: "var(--tome-accent, #6366f1)",
          fillOpacity: 0.9,
        });
        m.bindTooltip(p.title, { direction: "top", offset: [0, -4] });
        m.on("click", () => onOpen(p.title));
        layer.addLayer(m);
      }
    }
  }

  return (
    <section className="h-full flex flex-col">
      <div className="px-6 py-4 border-b border-tome-border">
        <div className="flex items-baseline justify-between gap-3 max-w-5xl mx-auto">
          <div>
            <h2 className="text-2xl font-bold">Map</h2>
            <p className="text-sm text-tome-muted">
              Every geotagged article in your library. Click a pin to open.
            </p>
          </div>
          <div className="text-xs text-tome-muted whitespace-nowrap">
            {phase === "loading" && "loading…"}
            {phase === "ready" && `${count.toLocaleString()} articles`}
            {phase === "empty" && "no geotags ingested yet"}
            {phase === "error" && (
              <span className="text-tome-danger">{error}</span>
            )}
          </div>
        </div>
      </div>

      {!isTauri() && (
        <div className="p-4 m-4 rounded border border-tome-border bg-tome-surface-2 text-sm">
          Running outside the Tauri shell — no data available.
        </div>
      )}

      {phase === "empty" && (
        <div className="p-4 m-4 rounded border border-tome-border bg-tome-surface text-sm text-tome-muted">
          Ingest a <code>geo_tags.sql.gz</code> in Settings → Geotag ingestion
          and the map will populate.
        </div>
      )}

      <div ref={containerRef} className="flex-1 min-h-0 tome-map" />
    </section>
  );
}
