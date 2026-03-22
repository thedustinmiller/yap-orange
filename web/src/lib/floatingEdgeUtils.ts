import { Position } from '@xyflow/svelte';
import type { InternalNode } from '@xyflow/svelte';

/**
 * Finds where a line from a node's center to a target point
 * intersects the node's rectangular boundary.
 */
function getNodeIntersection(
  node: InternalNode,
  targetPoint: { x: number; y: number },
): { x: number; y: number } {
  const w = (node.measured.width ?? 150) / 2;
  const h = (node.measured.height ?? 40) / 2;
  const cx = node.internals.positionAbsolute.x + w;
  const cy = node.internals.positionAbsolute.y + h;

  const dx = targetPoint.x - cx;
  const dy = targetPoint.y - cy;

  if (dx === 0 && dy === 0) {
    return { x: cx, y: cy - h };
  }

  const absDx = Math.abs(dx);
  const absDy = Math.abs(dy);

  // Compare slope of displacement vs node aspect ratio to determine
  // which edge (horizontal or vertical) the line hits first
  if (absDy * w < absDx * h) {
    // Hits left or right edge
    const sign = dx > 0 ? 1 : -1;
    return {
      x: cx + sign * w,
      y: cy + (dy * w) / absDx,
    };
  } else {
    // Hits top or bottom edge
    const sign = dy > 0 ? 1 : -1;
    return {
      x: cx + (dx * h) / absDy,
      y: cy + sign * h,
    };
  }
}

/**
 * Determines which side (Position) of the node an intersection point is on.
 */
function getEdgePosition(
  node: InternalNode,
  point: { x: number; y: number },
): Position {
  const nx = node.internals.positionAbsolute.x;
  const ny = node.internals.positionAbsolute.y;
  const w = node.measured.width ?? 150;
  const h = node.measured.height ?? 40;

  const distTop = Math.abs(point.y - ny);
  const distBottom = Math.abs(point.y - (ny + h));
  const distLeft = Math.abs(point.x - nx);
  const distRight = Math.abs(point.x - (nx + w));

  const min = Math.min(distTop, distBottom, distLeft, distRight);

  if (min === distLeft) return Position.Left;
  if (min === distRight) return Position.Right;
  if (min === distTop) return Position.Top;
  return Position.Bottom;
}

/**
 * Computes floating edge parameters: source/target intersection points
 * and which Position each connects at.
 */
export function getEdgeParams(source: InternalNode, target: InternalNode) {
  const sourceCenter = {
    x: source.internals.positionAbsolute.x + (source.measured.width ?? 150) / 2,
    y: source.internals.positionAbsolute.y + (source.measured.height ?? 40) / 2,
  };
  const targetCenter = {
    x: target.internals.positionAbsolute.x + (target.measured.width ?? 150) / 2,
    y: target.internals.positionAbsolute.y + (target.measured.height ?? 40) / 2,
  };

  const sourceIntersection = getNodeIntersection(source, targetCenter);
  const targetIntersection = getNodeIntersection(target, sourceCenter);

  return {
    sx: sourceIntersection.x,
    sy: sourceIntersection.y,
    tx: targetIntersection.x,
    ty: targetIntersection.y,
    sourcePos: getEdgePosition(source, sourceIntersection),
    targetPos: getEdgePosition(target, targetIntersection),
  };
}
