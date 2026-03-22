<script lang="ts">
  import {
    BaseEdge,
    getBezierPath,
    useInternalNode,
    type EdgeProps,
  } from '@xyflow/svelte';
  import { getEdgeParams } from './floatingEdgeUtils';

  let {
    id,
    source,
    target,
    style,
    label,
    labelStyle,
    markerStart,
    markerEnd,
    interactionWidth,
  }: EdgeProps = $props();

  const edgePath = $derived.by(() => {
    // Access source/target inside derived to track reactively
    const sNode = useInternalNode(source);
    const tNode = useInternalNode(target);
    const s = sNode.current;
    const t = tNode.current;
    if (!s || !t) return '';

    const { sx, sy, tx, ty, sourcePos, targetPos } = getEdgeParams(s, t);
    const [path] = getBezierPath({
      sourceX: sx,
      sourceY: sy,
      targetX: tx,
      targetY: ty,
      sourcePosition: sourcePos,
      targetPosition: targetPos,
    });
    return path;
  });
</script>

{#if edgePath}
  <BaseEdge
    {id}
    path={edgePath}
    {style}
    {label}
    {labelStyle}
    {markerStart}
    {markerEnd}
    {interactionWidth}
  />
{/if}
