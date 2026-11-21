import { useMemo, useRef } from "react";
import { hierarchy, partition, type HierarchyRectangularNode } from "d3-hierarchy";
import { arc } from "d3-shape";
import type { NodeView } from "../api";
import { colorOf, navTarget, opacityOf, setValues, truncate, useSize } from "../chartUtils";
import { formatBytes, formatPercent, nodeLabel } from "../format";

type RNode = HierarchyRectangularNode<NodeView>;

interface Props {
  data: NodeView;
  depth: number;
  hovered: RNode | null;
  onHover: (n: RNode | null) => void;
  onNavigate: (id: number) => void;
  onUp: () => void;
  canGoUp: boolean;
}

export function Sunburst({ data, depth, hovered, onHover, onNavigate, onUp, canGoUp }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const { width, height } = useSize(ref);
  const size = Math.max(0, Math.min(width, height));
  const radius = size / 2 - 4;

  const root = useMemo(() => {
    const h = setValues(hierarchy(truncate(data, depth), (d) => (d.children.length ? d.children : undefined)));
    return partition<NodeView>().size([2 * Math.PI, depth + 1])(h);
  }, [data, depth]);

  const ring = radius / (depth + 1);
  const arcGen = useMemo(
    () =>
      arc<RNode>()
        .startAngle((d) => d.x0)
        .endAngle((d) => d.x1)
        .padAngle(0.003)
        .padRadius(radius / 2)
        .innerRadius((d) => d.y0 * ring + 1)
        .outerRadius((d) => d.y1 * ring - 1)
        .cornerRadius(3),
    [radius, ring],
  );

  const nodes = root.descendants().filter((d) => d.depth > 0 && d.x1 - d.x0 > 0.004);
  const hoveredChain = new Set(hovered ? hovered.ancestors() : []);
  const center = hovered ?? root;

  return (
    <div className="chart" ref={ref}>
      {size > 0 && (
        <svg width={width} height={height} role="img" aria-label="Gráfico sunburst do uso de disco">
          <g transform={`translate(${width / 2},${height / 2})`} onMouseLeave={() => onHover(null)}>
            {nodes.map((d, i) => {
              const target = navTarget(d);
              const dim = hovered && !hoveredChain.has(d);
              return (
                <path
                  key={i}
                  d={arcGen(d) ?? undefined}
                  fill={colorOf(d)}
                  fillOpacity={dim ? opacityOf(d) * 0.35 : opacityOf(d)}
                  className={target != null ? "seg clickable" : "seg"}
                  onMouseEnter={() => onHover(d)}
                  onClick={() => target != null && onNavigate(target)}
                />
              );
            })}
            <circle
              r={Math.max(0, ring - 2)}
              className={canGoUp ? "center clickable" : "center"}
              onClick={canGoUp ? onUp : undefined}
              onMouseEnter={() => onHover(null)}
            />
            <foreignObject x={-ring * 0.8} y={-ring * 0.55} width={ring * 1.6} height={ring * 1.1} pointerEvents="none">
              <div className="center-label">
                <div className="center-name" title={nodeLabel(center.data)}>
                  {nodeLabel(center.data)}
                </div>
                <div className="center-size">{formatBytes(center.data.size)}</div>
                {hovered ? (
                  <div className="center-pct">{formatPercent(hovered.data.size, root.data.size)}</div>
                ) : (
                  canGoUp && <div className="center-pct">clique para subir</div>
                )}
              </div>
            </foreignObject>
          </g>
        </svg>
      )}
    </div>
  );
}
