import { useMemo, useRef, useState } from "react";
import { ComposableMap, Geographies, Geography, Marker } from "react-simple-maps";
// Bundled offline TopoJSON (no tile servers / network) — see geo.ts for why.
import worldTopo from "world-atlas/countries-110m.json";
import type { ClusterType, Environment } from "../types";

/** Health of a connection on the map (derived from the dashboard's probes). */
export type MapStatus = "online" | "unreachable" | "unknown";

/** One plottable connection. */
export interface MapPoint {
  id: string;
  name: string;
  /** [longitude, latitude]. */
  coordinates: [number, number];
  environment: Environment | null;
  clusterType: ClusterType | null;
  plant: string | null;
  /** "City, Country" label. */
  location: string | null;
  status: MapStatus;
  active: boolean;
}

interface Props {
  points: MapPoint[];
  /** Total registered connections (to message how many lack a location). */
  totalConnections: number;
  onSelect: (id: string) => void;
}

/** Fill color by environment; ring color by reachability. Kept distinct so a
 *  marker conveys both dimensions at a glance. */
const ENV_COLOR: Record<Environment, string> = {
  Dev: "#38bdf8",
  Val: "#fbbf24",
  Prod: "#a78bfa",
};
const ENV_FALLBACK = "#94a3b8";

const STATUS_COLOR: Record<MapStatus, string> = {
  online: "#22c55e",
  unreachable: "#ef4444",
  unknown: "#64748b",
};

const envColor = (e: Environment | null) => (e ? ENV_COLOR[e] : ENV_FALLBACK);

/** Fan out connections that resolve to the same coordinate (e.g. a shared country
 *  centroid) so they don't fully overlap. Deterministic — no Math.random. */
function spread(points: MapPoint[]): MapPoint[] {
  const groups = new Map<string, MapPoint[]>();
  for (const p of points) {
    const key = `${p.coordinates[0].toFixed(2)},${p.coordinates[1].toFixed(2)}`;
    const g = groups.get(key);
    if (g) g.push(p);
    else groups.set(key, [p]);
  }
  const out: MapPoint[] = [];
  for (const g of groups.values()) {
    if (g.length === 1) {
      out.push(g[0]);
      continue;
    }
    // Arrange duplicates on a small circle (~1.2° radius) around the centroid.
    const r = 1.2;
    g.forEach((p, i) => {
      const angle = (2 * Math.PI * i) / g.length;
      out.push({
        ...p,
        coordinates: [
          p.coordinates[0] + r * Math.cos(angle),
          p.coordinates[1] + r * Math.sin(angle),
        ],
      });
    });
  }
  return out;
}

export function WorldMap({ points, totalConnections, onSelect }: Props) {
  const wrapRef = useRef<HTMLDivElement>(null);
  const [hover, setHover] = useState<{ x: number; y: number; point: MapPoint } | null>(null);

  const placed = useMemo(() => spread(points), [points]);
  const missing = totalConnections - points.length;

  function onEnter(e: React.MouseEvent, point: MapPoint) {
    const rect = wrapRef.current?.getBoundingClientRect();
    setHover({
      x: e.clientX - (rect?.left ?? 0),
      y: e.clientY - (rect?.top ?? 0),
      point,
    });
  }

  return (
    <div className="worldmap" ref={wrapRef}>
      <ComposableMap
        projection="geoEqualEarth"
        projectionConfig={{ scale: 165 }}
        width={980}
        height={440}
        style={{ width: "100%", height: "auto" }}
      >
        <Geographies geography={worldTopo as unknown as Record<string, unknown>}>
          {({ geographies }) =>
            geographies.map((geo) => (
              <Geography
                key={geo.rsmKey}
                geography={geo}
                style={{
                  default: {
                    fill: "var(--map-land, #2a3344)",
                    stroke: "var(--map-stroke, #3a455c)",
                    strokeWidth: 0.5,
                    outline: "none",
                  },
                  hover: { fill: "var(--map-land, #2a3344)", outline: "none" },
                  pressed: { fill: "var(--map-land, #2a3344)", outline: "none" },
                }}
              />
            ))
          }
        </Geographies>

        {placed.map((p) => (
          <Marker
            key={p.id}
            coordinates={p.coordinates}
            onMouseEnter={(e) => onEnter(e, p)}
            onMouseMove={(e) => onEnter(e, p)}
            onMouseLeave={() => setHover(null)}
            onClick={() => onSelect(p.id)}
            style={{
              default: { cursor: "pointer" },
              hover: { cursor: "pointer" },
              pressed: { cursor: "pointer" },
            }}
          >
            {p.active && (
              <circle r={11} fill="none" stroke={envColor(p.environment)} strokeWidth={1.5} opacity={0.5}>
                <animate attributeName="r" from="8" to="15" dur="1.6s" repeatCount="indefinite" />
                <animate attributeName="opacity" from="0.5" to="0" dur="1.6s" repeatCount="indefinite" />
              </circle>
            )}
            <circle
              r={6}
              fill={envColor(p.environment)}
              stroke={STATUS_COLOR[p.status]}
              strokeWidth={2.5}
            />
          </Marker>
        ))}
      </ComposableMap>

      {hover && (
        <div
          className="worldmap-tip"
          style={{ left: hover.x + 14, top: hover.y + 14 }}
          // Don't let the tooltip steal the mouse and flicker.
          onMouseEnter={() => setHover(null)}
        >
          <div className="worldmap-tip-title">{hover.point.name}</div>
          {hover.point.location && <div className="dim">{hover.point.location}</div>}
          <div className="worldmap-tip-meta">
            {hover.point.clusterType && <span className="pill">{hover.point.clusterType}</span>}
            {hover.point.environment && (
              <span className="pill" style={{ background: envColor(hover.point.environment), color: "#0b1220" }}>
                {hover.point.environment}
              </span>
            )}
          </div>
          {hover.point.plant && <div className="dim">Plant: {hover.point.plant}</div>}
          <div className="dim" style={{ color: STATUS_COLOR[hover.point.status] }}>
            {hover.point.status === "online"
              ? "● Online"
              : hover.point.status === "unreachable"
                ? "● Unreachable"
                : "● Status unknown"}
          </div>
        </div>
      )}

      <div className="worldmap-legend">
        <span className="lg"><i style={{ background: ENV_COLOR.Dev }} /> Dev</span>
        <span className="lg"><i style={{ background: ENV_COLOR.Val }} /> Val</span>
        <span className="lg"><i style={{ background: ENV_COLOR.Prod }} /> Prod</span>
        <span className="lg"><i style={{ background: ENV_FALLBACK }} /> Unspecified</span>
        <span className="lg sep"><i className="ring" style={{ borderColor: STATUS_COLOR.online }} /> Online</span>
        <span className="lg"><i className="ring" style={{ borderColor: STATUS_COLOR.unreachable }} /> Unreachable</span>
        {missing > 0 && (
          <span className="lg dim" style={{ marginLeft: "auto" }}>
            {missing} connection{missing === 1 ? "" : "s"} without a location
          </span>
        )}
      </div>
    </div>
  );
}
