<script lang="ts">
  import { onDestroy, untrack } from 'svelte';
  import {
    SvelteFlow,
    Controls,
    Background,
    Panel,
    useSvelteFlow,
    type Node,
    type Edge,
    type EdgeTypes,
  } from '@xyflow/svelte';
  import '@xyflow/svelte/dist/style.css';
  import { stratify, tree } from 'd3-hierarchy';
  import * as d3Force from 'd3-force';
  import collide from './collide';
  import FloatingEdge from './FloatingEdge.svelte';

  import { appState, navigateToAtom, selectBlock, flattenVisibleNodes } from './appState.svelte';
  import { blockTree, type TreeNode } from './blockTree.svelte';
  import { api } from './api';
  import { getSetting, setSetting } from './settingsStore.svelte';
  import type { AtomGraph } from './types';

  let { mode = 'links' }: { mode: 'links' | 'outliner' } = $props();

  const edgeTypes: EdgeTypes = { floating: FloatingEdge as any };

  let simulation: d3Force.Simulation<any, any> | undefined;
  let forceRunning = $state(false);
  let draggingNode: any = $state(null);

  const { fitView } = useSvelteFlow();

  let nodes = $state.raw<Node[]>([]);
  let edges = $state.raw<Edge[]>([]);

  let lastLineageId: string | null | undefined = undefined;
  let outlinerDebounceTimer: ReturnType<typeof setTimeout> | undefined;
  // Track the set of lineage IDs currently in the graph to avoid full rebuild on internal clicks
  let currentGraphLineageIds = new Set<string>();
  // When true, skip fitView after next layout (set by internal node click)
  let suppressFitView = false;

  // --- Build graph data from API response (Links mode) ---
  function buildGraphFromApi(graph: AtomGraph): { nodes: Node[]; edges: Edge[] } {
    const nodeList: Node[] = [];
    const edgeList: Edge[] = [];
    const lineageIds = new Set<string>();

    // Center node: the selected atom
    const centerLineageId = graph.atom.lineage_id;
    lineageIds.add(centerLineageId);
    nodeList.push({
      id: centerLineageId,
      position: { x: 0, y: 0 },
      data: { label: truncateLabel(graph.atom.content) },
      type: 'default',
      style: selectedNodeStyle(),
    });

    // Outlinks (atoms this atom links to via content)
    graph.outlinks.forEach((link, i) => {
      if (!lineageIds.has(link.lineage_id)) {
        lineageIds.add(link.lineage_id);
        nodeList.push({
          id: link.lineage_id,
          position: { x: 200, y: i * 80 },
          data: { label: truncateLabel(link.content || link.namespace || link.lineage_id) },
          type: 'default',
          style: defaultNodeStyle(),
        });
      }
      edgeList.push({
        id: `outlink-${centerLineageId}-${link.lineage_id}-${i}`,
        source: centerLineageId,
        target: link.lineage_id,
        type: 'floating',
        animated: true,
        style: 'stroke: var(--link-color); stroke-width: 2;',
      });
    });

    // Backlinks (atoms that link to this atom via content)
    graph.backlinks.forEach((link, i) => {
      if (!lineageIds.has(link.lineage_id)) {
        lineageIds.add(link.lineage_id);
        nodeList.push({
          id: link.lineage_id,
          position: { x: -200, y: i * 80 },
          data: { label: truncateLabel(link.content || link.namespace || link.lineage_id) },
          type: 'default',
          style: defaultNodeStyle(),
        });
      }
      edgeList.push({
        id: `backlink-${link.lineage_id}-${centerLineageId}-${i}`,
        source: link.lineage_id,
        target: centerLineageId,
        type: 'floating',
        animated: true,
        style: 'stroke: var(--link-color); stroke-width: 2;',
      });
    });

    // Semantic edges (outgoing)
    graph.edges.outgoing.forEach((edge, i) => {
      if (!lineageIds.has(edge.to_lineage_id)) {
        lineageIds.add(edge.to_lineage_id);
        nodeList.push({
          id: edge.to_lineage_id,
          position: { x: 150, y: (graph.outlinks.length + i) * 80 },
          data: { label: edge.to_lineage_id.slice(0, 8) },
          type: 'default',
          style: defaultNodeStyle(),
        });
      }
      edgeList.push({
        id: edge.id,
        source: centerLineageId,
        target: edge.to_lineage_id,
        type: 'floating',
        style: 'stroke: var(--edge-color); stroke-width: 1.5; stroke-dasharray: 5,5;',
        label: edge.edge_type,
      });
    });

    // Semantic edges (incoming)
    graph.edges.incoming.forEach((edge, i) => {
      if (!lineageIds.has(edge.from_lineage_id)) {
        lineageIds.add(edge.from_lineage_id);
        nodeList.push({
          id: edge.from_lineage_id,
          position: { x: -150, y: (graph.backlinks.length + i) * 80 },
          data: { label: edge.from_lineage_id.slice(0, 8) },
          type: 'default',
          style: defaultNodeStyle(),
        });
      }
      edgeList.push({
        id: edge.id,
        source: edge.from_lineage_id,
        target: centerLineageId,
        type: 'floating',
        style: 'stroke: var(--edge-color); stroke-width: 1.5; stroke-dasharray: 5,5;',
        label: edge.edge_type,
      });
    });

    // Hard links (other blocks sharing the same lineage)
    const hardLinks = graph.hard_links ?? [];
    if (hardLinks.length > 0) {
      hardLinks.forEach((hl, i) => {
        // Use block_id as node ID since lineage_id would collide with center node
        const nodeId = `hl-${hl.block_id}`;
        nodeList.push({
          id: nodeId,
          position: { x: 0, y: -(i + 1) * 80 },
          data: { label: hl.namespace || hl.name },
          type: 'default',
          style: hardLinkNodeStyle(),
        });
        edgeList.push({
          id: `hardlink-${centerLineageId}-${hl.block_id}`,
          source: centerLineageId,
          target: nodeId,
          type: 'floating',
          style: 'stroke: var(--accent-color); stroke-width: 1.5; stroke-dasharray: 3,3;',
          label: 'hard link',
        });
      });
    }

    currentGraphLineageIds = lineageIds;
    return { nodes: nodeList, edges: edgeList };
  }

  // --- Build graph from outliner tree (Outliner mode) ---
  async function buildGraphFromOutliner(): Promise<{ nodes: Node[]; edges: Edge[] }> {
    const nsId = appState.activeNamespaceBlockId;
    let treeRoots: TreeNode[];
    if (nsId) {
      const node = blockTree.getNode(nsId);
      treeRoots = node ? [node] : [];
    } else {
      treeRoots = blockTree.roots;
    }

    const flat = flattenVisibleNodes(treeRoots);
    if (flat.length === 0) return { nodes: [], edges: [] };

    const nodeList: Node[] = [];
    const edgeList: Edge[] = [];
    const selectedSet = new Set(appState.selectedBlockIds);

    // Deduplicate by lineage_id (hard links: first block per lineage wins)
    const lineageToBlockId = new Map<string, string>();
    const seenLineages = new Set<string>();

    for (const treeNode of flat) {
      if (seenLineages.has(treeNode.lineage_id)) continue;
      seenLineages.add(treeNode.lineage_id);
      lineageToBlockId.set(treeNode.lineage_id, treeNode.id);

      const isSelected = selectedSet.has(treeNode.id);
      const isRoot = treeNode.id === nsId;

      let style: string;
      if (isSelected) {
        style = selectedNodeStyle();
      } else if (isRoot) {
        style = rootNodeStyle();
      } else {
        style = defaultNodeStyle();
      }

      nodeList.push({
        id: treeNode.lineage_id,
        position: { x: 0, y: 0 },
        data: { label: truncateLabel(treeNode.name || treeNode.id), blockId: treeNode.id },
        type: 'default',
        style,
      });
    }

    // Build parent-child edges from tree structure
    for (const treeNode of flat) {
      if (!lineageToBlockId.has(treeNode.lineage_id) || lineageToBlockId.get(treeNode.lineage_id) !== treeNode.id) continue;
      if (!treeNode.parent_id) continue;
      const parentNode = blockTree.getNode(treeNode.parent_id);
      if (!parentNode || !seenLineages.has(parentNode.lineage_id)) continue;

      edgeList.push({
        id: `parent-${treeNode.lineage_id}`,
        source: parentNode.lineage_id,
        target: treeNode.lineage_id,
        type: 'floating',
        style: 'stroke: var(--border-color); stroke-width: 1.5;',
      });
    }

    // Fetch cross-connections from API
    const lineageIds = Array.from(seenLineages);
    if (lineageIds.length > 0 && lineageIds.length <= 1000) {
      try {
        const subtreeGraph = await api.graph.subtree(lineageIds);

        // Overlay content links
        for (const cl of subtreeGraph.content_links) {
          if (seenLineages.has(cl.from_lineage_id) && seenLineages.has(cl.to_lineage_id)) {
            edgeList.push({
              id: `cl-${cl.from_lineage_id}-${cl.to_lineage_id}`,
              source: cl.from_lineage_id,
              target: cl.to_lineage_id,
              type: 'floating',
              animated: true,
              style: 'stroke: var(--link-color); stroke-width: 2;',
            });
          }
        }

        // Overlay semantic edges
        for (const edge of subtreeGraph.edges) {
          if (seenLineages.has(edge.from_lineage_id) && seenLineages.has(edge.to_lineage_id)) {
            edgeList.push({
              id: `se-${edge.id}`,
              source: edge.from_lineage_id,
              target: edge.to_lineage_id,
              type: 'floating',
              style: 'stroke: var(--edge-color); stroke-width: 1.5; stroke-dasharray: 5,5;',
              label: edge.edge_type,
            });
          }
        }
      } catch (err) {
        console.warn('Failed to load subtree graph:', err);
      }
    }

    return { nodes: nodeList, edges: edgeList };
  }

  function truncateLabel(text: string, max = 30): string {
    // Strip wiki link syntax for display
    const clean = text.replace(/\[\[([^\]]+)\]\]/g, '$1');
    return clean.length > max ? clean.slice(0, max) + '...' : clean;
  }

  function selectedNodeStyle(): string {
    return 'background: var(--accent-color); color: white; border: 2px solid var(--accent-bright); border-radius: 6px; padding: 8px 12px; font-size: 12px;';
  }

  function defaultNodeStyle(): string {
    return 'background: var(--bg-tertiary); color: var(--text-primary); border: 1px solid var(--border-color); border-radius: 6px; padding: 8px 12px; font-size: 12px;';
  }

  function hardLinkNodeStyle(): string {
    return 'background: var(--bg-secondary); color: var(--text-secondary); border: 2px dashed var(--accent-color); border-radius: 6px; padding: 8px 12px; font-size: 12px;';
  }

  function rootNodeStyle(): string {
    return 'background: var(--bg-secondary); color: var(--text-primary); border: 2px solid var(--text-muted); border-radius: 6px; padding: 8px 12px; font-size: 12px;';
  }

  // --- Get the lineage ID to show graph for ---
  function getActiveLineageId(): string | null {
    // Prefer selected block's lineage, then active namespace lineage
    if (appState.selectedBlockIds.length > 0) {
      const node = blockTree.getNode(appState.selectedBlockIds[0]);
      if (node) return node.lineage_id;
    }
    return appState.activeNamespaceLineageId;
  }

  // --- Effect: Links mode — load graph when selection/namespace changes ---
  $effect(() => {
    if (mode !== 'links') return;

    const lineageId = getActiveLineageId();
    if (lineageId === lastLineageId) return;
    lastLineageId = lineageId;

    if (!lineageId) {
      nodes = [];
      edges = [];
      return;
    }

    const wasForceRunning = untrack(() => forceRunning);
    if (wasForceRunning) {
      forceRunning = false;
      simulation?.stop();
    }

    // Fetch graph from API — always start with tree layout, then restore physics if saved
    api.atoms.graph(lineageId).then((graph) => {
      const data = buildGraphFromApi(graph);
      nodes = data.nodes;
      edges = data.edges;
      requestAnimationFrame(() => {
        applyTreeLayout();
        if (getSetting<boolean>('graph_physics')) {
          initForceSimulation();
          forceRunning = true;
          requestAnimationFrame(forceTick);
        }
      });
    }).catch((err) => {
      console.warn('Failed to load graph:', err);
      nodes = [];
      edges = [];
    });
  });

  // --- Effect: Outliner mode — full rebuild on structural tree changes (debounced) ---
  $effect(() => {
    if (mode !== 'outliner') return;

    // Track structural dependencies only (not selection)
    void blockTree.version;
    void appState.expandedBlocks.size;
    void appState.activeNamespaceBlockId;

    // Clear previous debounce
    if (outlinerDebounceTimer) clearTimeout(outlinerDebounceTimer);

    outlinerDebounceTimer = setTimeout(() => {
      const wasForceRunning = untrack(() => forceRunning);
      if (wasForceRunning) {
        forceRunning = false;
        simulation?.stop();
      }

      buildGraphFromOutliner().then((data) => {
        nodes = data.nodes;
        edges = data.edges;
        requestAnimationFrame(() => {
          applyTreeLayout();
          if (getSetting<boolean>('graph_physics')) {
            initForceSimulation();
            forceRunning = true;
            requestAnimationFrame(forceTick);
          }
        });
      }).catch((err) => {
        console.warn('Failed to build outliner graph:', err);
      });
    }, 300);
  });

  // --- Effect: Outliner mode — update node styles on selection change (no layout reset) ---
  $effect(() => {
    if (mode !== 'outliner') return;
    // Subscribe to selection changes
    const selectedIds = appState.selectedBlockIds;
    const nsId = appState.activeNamespaceBlockId;

    // Read nodes WITHOUT subscribing — prevents read/write infinite loop
    const currentNodes = untrack(() => nodes);
    if (currentNodes.length === 0) return;

    const selectedSet = new Set(selectedIds);
    let changed = false;

    const updated = currentNodes.map(n => {
      const blockId = n.data?.blockId as string | undefined;
      if (!blockId) return n;

      const isSelected = selectedSet.has(blockId);
      const isRoot = blockId === nsId;

      let style: string;
      if (isSelected) {
        style = selectedNodeStyle();
      } else if (isRoot) {
        style = rootNodeStyle();
      } else {
        style = defaultNodeStyle();
      }

      if (style === n.style) return n;
      changed = true;
      return { ...n, style };
    });

    if (changed) nodes = updated;
  });

  // Reset state when mode changes
  $effect(() => {
    void mode;
    lastLineageId = undefined;
    nodes = [];
    edges = [];
    if (simulation) {
      simulation.stop();
      forceRunning = false;
    }
  });

  // --- Handle node click ---
  function handleNodeClick(event: any) {
    const node = event.node;
    if (!node) return;

    if (mode === 'outliner') {
      const blockId = node.data?.blockId;
      if (blockId) selectBlock(blockId);
    } else {
      // Skip if clicking the already-active center node
      if (node.id === lastLineageId) return;
      // If re-centering on a node already in the graph, suppress the fitView reset
      if (currentGraphLineageIds.has(node.id)) {
        suppressFitView = true;
      }
      navigateToAtom(node.id);
    }
  }

  // === TREE LAYOUT ===
  const treeLayout = tree<any>();

  function applyTreeLayout() {
    if (nodes.length === 0) return;

    const adjacency = new Map<string, Set<string>>();
    for (const n of nodes) adjacency.set(n.id, new Set());
    for (const e of edges) {
      adjacency.get(e.source as string)?.add(e.target as string);
      adjacency.get(e.target as string)?.add(e.source as string);
    }

    const rootId = pickBestRoot(adjacency);
    if (!rootId) return;

    const visited = new Set<string>();
    const parentMap = new Map<string, string | null>();
    const queue: string[] = [rootId];
    visited.add(rootId);
    parentMap.set(rootId, null);

    while (queue.length > 0) {
      const current = queue.shift()!;
      const neighbors = adjacency.get(current) ?? new Set();
      for (const neighbor of neighbors) {
        if (!visited.has(neighbor)) {
          visited.add(neighbor);
          parentMap.set(neighbor, current);
          queue.push(neighbor);
        }
      }
    }

    const connectedNodes = nodes.filter(n => visited.has(n.id));
    const disconnectedNodes = nodes.filter(n => !visited.has(n.id));

    if (connectedNodes.length > 0) {
      try {
        const virtualRootId = '__vroot__';
        const stratifyData = [
          { id: virtualRootId },
          ...connectedNodes.map(n => ({ id: n.id })),
        ];

        const stratifyParentMap = new Map<string, string>();
        stratifyParentMap.set(rootId, virtualRootId);
        for (const [nodeId, parent] of parentMap) {
          if (parent !== null) {
            stratifyParentMap.set(nodeId, parent);
          }
        }

        const hierarchy = stratify<{ id: string }>()
          .id(d => d.id)
          .parentId(d => stratifyParentMap.get(d.id) ?? null);

        const root = hierarchy(stratifyData);
        const layout = treeLayout.nodeSize([180, 100])(root);

        const positionMap = new Map<string, { x: number; y: number }>();
        layout.descendants().forEach(d => {
          if (d.data.id !== virtualRootId) {
            positionMap.set(d.data.id, { x: d.x, y: d.y });
          }
        });

        const treeYValues = Array.from(positionMap.values()).map(p => p.y);
        const maxTreeY = treeYValues.length > 0 ? Math.max(...treeYValues) : 0;
        const disconnectedY = maxTreeY + 160;

        disconnectedNodes.forEach((n, i) => {
          positionMap.set(n.id, { x: i * 180 - (disconnectedNodes.length * 90), y: disconnectedY });
        });

        nodes = nodes.map(n => ({
          ...n,
          position: positionMap.get(n.id) ?? n.position,
        }));

        if (suppressFitView) {
          suppressFitView = false;
        } else {
          requestAnimationFrame(() => fitView());
        }
      } catch (err) {
        console.warn('Tree layout failed:', err);
      }
    }
  }

  function pickBestRoot(adjacency: Map<string, Set<string>>): string | null {
    // In outliner mode, prefer the active namespace's lineage
    if (mode === 'outliner') {
      const nsLineageId = appState.activeNamespaceLineageId;
      if (nsLineageId && adjacency.has(nsLineageId)) return nsLineageId;
    }

    // In links mode, prefer the center node (selected atom's lineage)
    const lineageId = getActiveLineageId();
    if (lineageId && adjacency.has(lineageId)) return lineageId;

    let bestId: string | null = null;
    let bestCount = -1;
    for (const [id, neighbors] of adjacency) {
      if (neighbors.size > bestCount) {
        bestCount = neighbors.size;
        bestId = id;
      }
    }
    return bestId;
  }

  // === FORCE LAYOUT ===

  function initForceSimulation() {
    if (simulation) simulation.stop();

    simulation = d3Force
      .forceSimulation()
      .force('charge', d3Force.forceManyBody().strength(-800))
      .force('x', d3Force.forceX().x(0).strength(0.05))
      .force('y', d3Force.forceY().y(0).strength(0.05))
      .force('collide', collide() as any)
      .alphaTarget(0.05)
      .stop();

    const simNodes = nodes.map(node => ({
      ...node,
      x: node.position.x,
      y: node.position.y,
      measured: {
        width: node.measured?.width || 150,
        height: node.measured?.height || 40,
      },
    }));

    const simEdges = edges.map(edge => ({
      ...edge,
      source: edge.source,
      target: edge.target,
    }));

    simulation.nodes(simNodes);
    simulation.force(
      'link',
      d3Force
        .forceLink(simEdges)
        .id((d: any) => d.id)
        .strength(0.05)
        .distance(100)
    );
  }

  function forceTick() {
    if (!forceRunning || !simulation) return;

    const simNodes = simulation.nodes();

    simNodes.forEach((node: any, i: number) => {
      const dragging = draggingNode?.id === node.id;
      if (dragging) {
        simNodes[i].fx = draggingNode.position.x;
        simNodes[i].fy = draggingNode.position.y;
      } else {
        delete simNodes[i].fx;
        delete simNodes[i].fy;
      }
    });

    simulation.tick();

    const currentStyles = new Map(nodes.map(n => [n.id, n.style]));
    const currentData = new Map(nodes.map(n => [n.id, n.data]));

    nodes = simNodes.map((simNode: any) => ({
      id: simNode.id,
      type: 'default',
      data: currentData.get(simNode.id) ?? { label: simNode.id },
      style: currentStyles.get(simNode.id) ?? '',
      position: {
        x: simNode.fx ?? simNode.x,
        y: simNode.fy ?? simNode.y,
      },
    }));

    requestAnimationFrame(() => {
      if (forceRunning) forceTick();
    });
  }

  function toggleForce() {
    if (!forceRunning) {
      initForceSimulation();
      forceRunning = true;
      setSetting('graph_physics', true);
      requestAnimationFrame(forceTick);
    } else {
      forceRunning = false;
      simulation?.stop();
      setSetting('graph_physics', false);
    }
  }

  function handleNodeDragStart(event: any) {
    if (forceRunning) {
      draggingNode = event.targetNode;
    }
  }

  function handleNodeDrag(event: any) {
    if (forceRunning) {
      draggingNode = event.targetNode;
    }
  }

  function handleNodeDragStop() {
    draggingNode = null;
  }

  onDestroy(() => {
    simulation?.stop();
    forceRunning = false;
    if (outlinerDebounceTimer) clearTimeout(outlinerDebounceTimer);
  });
</script>

{#if nodes.length > 0}
  <SvelteFlow
    bind:nodes
    bind:edges
    {edgeTypes}
    fitView
    minZoom={0.1}
    maxZoom={4}
    nodesDraggable={true}
    nodesConnectable={false}
    elementsSelectable={true}
    colorMode="dark"
    proOptions={{ hideAttribution: true }}
    onnodeclick={handleNodeClick}
    onnodedragstart={handleNodeDragStart}
    onnodedrag={handleNodeDrag}
    onnodedragstop={handleNodeDragStop}
  >
    <Controls />
    <Background />
    <Panel position="top-right">
      <div class="layout-controls">
        <button class="layout-action" onclick={applyTreeLayout} aria-label="Re-apply tree layout">
          Re-layout
        </button>
        <button
          class="layout-action"
          class:active={forceRunning}
          onclick={toggleForce}
          aria-label={forceRunning ? 'Disable physics simulation' : 'Enable physics simulation'}
        >
          Physics {forceRunning ? 'on' : 'off'}
        </button>
      </div>
    </Panel>
  </SvelteFlow>
{:else}
  <div class="graph-empty">
    <p>{mode === 'outliner' ? 'Navigate to a namespace to see its graph' : 'Select a block to see its graph'}</p>
  </div>
{/if}

<style>
  .graph-empty {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--text-muted);
    font-size: 13px;
  }

  /* Hide default node handles — floating edges compute their own connection points */
  :global(.svelte-flow__handle) {
    opacity: 0;
    width: 0 !important;
    height: 0 !important;
    min-width: 0 !important;
    min-height: 0 !important;
  }

  .layout-controls {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .layout-action {
    padding: 3px 10px;
    font-size: 10px;
    font-weight: 500;
    background: var(--bg-tertiary);
    color: var(--text-secondary);
    border: 1px solid var(--border-color);
    border-radius: 4px;
    cursor: pointer;
    transition: all 0.1s;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .layout-action:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .layout-action.active {
    background: var(--accent-color);
    color: white;
    border-color: var(--accent-bright);
  }
</style>
