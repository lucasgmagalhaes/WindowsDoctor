import { useMemo, useRef } from "react";
import { hierarchy, treemap, treemapSquarify, type HierarchyRectangularNode } from "d3-hierarchy";
import type { NodeView } from "../api";
import { colorOf, isRepeatClick, navTarget, opacityOf, setValues, truncate, useSize } from "../chartUtils";
import { formatBytes, nodeLabel } from "../format";

type RNode = HierarchyRectangularNode<NodeView>;

interface Props {
  data: NodeView;
  depth: number;
  hovered: RNode | null;
  onHover: (n: RNode | null) => void;
  onNavigate: (id: number) => void;
}

const HEADER = 18;

export function Treemap({ data, depth, hovered, onHover, onNavigate }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const { width, height } = useSize(ref);

  const root = useMemo(() => {
    if (width === 0 || height === 0) return null;
    const h = setValues(hierarchy(truncate(data, depth), (d) => (d.children.length ? d.children : undefined)));
    return treemap<NodeView>()
      .tile(treemapSquarify.ratio(1.3))
      .size([width, height])
      .paddingOuter(2)
      .paddingInner(1)
      .paddingTop((n) => (n.depth > 0 && n.children ? HEADER : 2))
      .round(true)(h);
  }, [data, depth, width, height]);

  const nodes = root ? root.descendants().filter((d) => d.depth > 0 && d.x1 - d.x0 >= 2 && d.y1 - d.y0 >= 2) : [];

  return (
    <div className="chart" ref={ref}>
      {root && (
        <svg width={width} height={height} role="img" aria-label="Treemap do uso de disco" onMouseLeave={() => onHover(null)}>
          {nodes.map((d, i) => {
            const w = d.x1 - d.x0;
            const h = d.y1 - d.y0;
            const target = navTarget(d);
            const isHover = hovered === d;
            const showLabel = w > 46 && h > 15;
            const isParent = !!d.children;
            return (
              <g
                key={i}
                transform={`translate(${d.x0},${d.y0})`}
                className={target != null ? "clickable" : undefined}
                onMouseEnter={(e) => {
                  e.stopPropagation();
                  onHover(d);
                }}
                onClick={(e) => {
                  e.stopPropagation();
                  if (target != null && !isRepeatClick(e)) onNavigate(target);
                }}
              >
                <rect
                  width={w}
                  height={h}
                  rx={3}
                  fill={colorOf(d)}
                  fillOpacity={isParent ? opacityOf(d) * 0.45 : opacityOf(d)}
                  className={isHover ? "tm-rect hover" : "tm-rect"}
                />
                {showLabel && (
                  <text x={5} y={isParent ? 13 : 14} className="tm-label">
                    <tspan>{clip(nodeLabel(d.data), w - 10)}</tspan>
                    {(isParent ? w > 140 : h > 30) && w > 70 && (
                      <tspan x={isParent ? undefined : 5} dx={isParent ? 6 : 0} dy={isParent ? 0 : 14} className="tm-size">
                        {formatBytes(d.data.size)}
                      </tspan>
                    )}
                  </text>
                )}
              </g>
            );
          })}
        </svg>
      )}
    </div>
  );
}

// Corta o texto para caber aproximadamente na largura disponível (~6.6px por caractere).
function clip(s: string, px: number) {
  const max = Math.floor(px / 6.6);
  return s.length > max ? s.slice(0, Math.max(1, max - 1)) + "…" : s;
}
