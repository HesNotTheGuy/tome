import { useEffect, useRef, useState } from "react";
import maplibregl, {
  GeoJSONSource,
  MapGeoJSONFeature,
  MapLayerMouseEvent,
  MapMouseEvent,
  StyleSpecification,
} from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";
import * as pmtiles from "pmtiles";

import { tome } from "../service";
import { isTauri, MappedGeotag } from "../types";

interface MapProps {
  onOpen: (title: string) => void;
}

/**
 * World map of every primary-geotagged article we've indexed.
 *
 * - **Renderer:** MapLibre GL (vector + raster).
 * - **Basemap source:** if the user has configured a `.pmtiles` archive in
 *   Settings, the map streams tiles from it via the `tome-pmtiles://` URI
 *   scheme — fully offline, byte-range fetched from disk. Otherwise it falls
 *   back to live OSM raster tiles.
 * - **Pin layer:** a single GeoJSON source feeding three styled layers —
 *   clusters, cluster counts, and individual points. MapLibre handles the
 *   clustering natively, so no separate supercluster index needed.
 *
 * Click handling: clusters zoom in; points open their article in the Reader.
 */
export default function MapPane({ onOpen }: MapProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const mapRef = useRef<maplibregl.Map | null>(null);
  const [phase, setPhase] = useState<"idle" | "loading" | "ready" | "empty" | "error">(
    "idle",
  );
  const [count, setCount] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [usingOffline, setUsingOffline] = useState(false);

  useEffect(() => {
    if (!containerRef.current) return;
    let cancelled = false;

    (async () => {
      // Register the pmtiles protocol so MapLibre can fetch from
      // pmtiles://tome-pmtiles://localhost/world.pmtiles URLs.
      const protocol = new pmtiles.Protocol();
      maplibregl.addProtocol("pmtiles", protocol.tile);

      // Decide the basemap style based on whether a pmtiles file is configured.
      let style: StyleSpecification;
      let offline = false;
      try {
        if (isTauri()) {
          const path = await tome.mapSourcePath();
          if (path) {
            offline = true;
          }
        }
      } catch {
        /* fall through to OSM */
      }
      if (offline) {
        style = pmtilesStyle();
      } else {
        style = osmStyle();
      }
      if (cancelled) return;
      setUsingOffline(offline);

      const map = new maplibregl.Map({
        container: containerRef.current!,
        style,
        center: [0, 20],
        zoom: 1.5,
        attributionControl: { compact: true },
      });
      mapRef.current = map;
      map.addControl(new maplibregl.NavigationControl({}), "top-right");

      map.on("load", () => {
        // Article-pin source. Empty initially; populated when the data load
        // resolves below.
        map.addSource("articles", {
          type: "geojson",
          data: { type: "FeatureCollection", features: [] },
          cluster: true,
          clusterMaxZoom: 14,
          clusterRadius: 50,
        });

        map.addLayer({
          id: "clusters",
          type: "circle",
          source: "articles",
          filter: ["has", "point_count"],
          paint: {
            "circle-color": [
              "step",
              ["get", "point_count"],
              "#6366f1",
              100,
              "#4f46e5",
              1000,
              "#3730a3",
            ],
            "circle-radius": [
              "step",
              ["get", "point_count"],
              16,
              100,
              22,
              1000,
              28,
            ],
            "circle-stroke-width": 2,
            "circle-stroke-color": "#ffffff",
          },
        });
        map.addLayer({
          id: "cluster-count",
          type: "symbol",
          source: "articles",
          filter: ["has", "point_count"],
          layout: {
            "text-field": ["get", "point_count_abbreviated"],
            "text-size": 12,
          },
          paint: {
            "text-color": "#ffffff",
          },
        });
        map.addLayer({
          id: "unclustered-point",
          type: "circle",
          source: "articles",
          filter: ["!", ["has", "point_count"]],
          paint: {
            "circle-color": "#6366f1",
            "circle-radius": 5,
            "circle-stroke-width": 1.5,
            "circle-stroke-color": "#ffffff",
          },
        });

        // Cluster click → zoom in
        map.on("click", "clusters", (e: MapMouseEvent) => {
          const features = map.queryRenderedFeatures(e.point, {
            layers: ["clusters"],
          });
          const feature = features[0];
          if (!feature) return;
          const clusterId = feature.properties?.["cluster_id"] as number;
          const src = map.getSource("articles") as GeoJSONSource | undefined;
          if (!src) return;
          src.getClusterExpansionZoom(clusterId).then((zoom) => {
            const geom = feature.geometry as GeoJSON.Point;
            map.easeTo({ center: geom.coordinates as [number, number], zoom });
          });
        });

        // Pin click → open article
        map.on("click", "unclustered-point", (e: MapLayerMouseEvent) => {
          const f = e.features?.[0] as MapGeoJSONFeature | undefined;
          if (!f) return;
          const title = f.properties?.["title"] as string | undefined;
          if (title) onOpen(title);
        });

        // Cursor feedback
        for (const layer of ["clusters", "unclustered-point"]) {
          map.on("mouseenter", layer, () => {
            map.getCanvas().style.cursor = "pointer";
          });
          map.on("mouseleave", layer, () => {
            map.getCanvas().style.cursor = "";
          });
        }
      });
    })();

    return () => {
      cancelled = true;
      mapRef.current?.remove();
      mapRef.current = null;
      maplibregl.removeProtocol("pmtiles");
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
        const features: GeoJSON.Feature<GeoJSON.Point>[] = rows.map((g) => ({
          type: "Feature",
          geometry: { type: "Point", coordinates: [g.lon, g.lat] },
          properties: { title: g.title, page_id: g.page_id },
        }));
        const data: GeoJSON.FeatureCollection<GeoJSON.Point> = {
          type: "FeatureCollection",
          features,
        };
        const map = mapRef.current;
        if (!map) return;
        const apply = () => {
          const src = map.getSource("articles") as GeoJSONSource | undefined;
          if (src) src.setData(data);
        };
        if (map.isStyleLoaded() && map.getSource("articles")) apply();
        else map.once("load", apply);
        setPhase("ready");
      })
      .catch((e) => {
        setError(String(e));
        setPhase("error");
      });
  }, []);

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
            {phase === "ready" && (
              <>
                {count.toLocaleString()} articles
                {" · "}
                <span className={usingOffline ? "text-tome-success" : ""}>
                  {usingOffline ? "offline basemap" : "online basemap"}
                </span>
              </>
            )}
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

/** Live OpenStreetMap raster basemap. Needs network. */
function osmStyle(): StyleSpecification {
  return {
    version: 8,
    sources: {
      osm: {
        type: "raster",
        tiles: ["https://tile.openstreetmap.org/{z}/{x}/{y}.png"],
        tileSize: 256,
        attribution:
          '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors',
      },
    },
    layers: [{ id: "osm", type: "raster", source: "osm" }],
  };
}

/**
 * Offline vector basemap fed by the user's `.pmtiles` archive via Tauri's
 * custom URI scheme. We don't bundle a proper style sheet — the user's file
 * may be raster, vector, or anything else — so we let MapLibre derive a
 * minimal default by pointing at the source. For richer styling, ship a
 * separate style.json and reference its layers here.
 */
function pmtilesStyle(): StyleSpecification {
  return {
    version: 8,
    glyphs: undefined,
    sources: {
      basemap: {
        type: "raster",
        url: "pmtiles://tome-pmtiles://localhost/basemap.pmtiles",
        tileSize: 256,
        attribution:
          'Offline basemap: user-supplied <a href="https://protomaps.com/">PMTiles</a>',
      },
    },
    layers: [{ id: "basemap", type: "raster", source: "basemap" }],
  };
}
