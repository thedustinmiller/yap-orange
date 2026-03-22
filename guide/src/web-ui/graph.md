# Graph View

The Graph panel provides a visual representation of relationships between blocks using [@xyflow/svelte](https://reactflow.dev) (the Svelte port of React Flow).

<!-- TODO: screenshot of graph view -->

## What the Graph Shows

The graph is centered on the currently selected block (or the currently navigated block if none is selected). It displays:

- **The center node** -- the active block, rendered with a highlighted style.
- **Outlinks** -- blocks that the current block's content links to via wiki links. Shown as animated solid edges.
- **Backlinks** -- blocks whose content links to the current block. Shown as animated solid edges pointing inward.
- **Semantic edges** -- non-hierarchical relationships (created via the edge API). Shown as dashed edges with their edge type label.

## Layout Modes

The graph supports two layout algorithms, toggled via buttons in the panel:

- **Tree layout** (default) -- uses d3-hierarchy to arrange nodes in a hierarchical tree structure. Best for exploring parent-child and link relationships.
- **Force layout** -- uses d3-force simulation with collision detection. Nodes repel each other and are pulled together by their connections. Best for seeing clusters and dense link networks.

## Interacting with the Graph

### Navigation

- **Click a node** to navigate to that block. The outliner and sidebar update to show the clicked block.
- **Pan** by clicking and dragging on the background.
- **Zoom** with the scroll wheel or the zoom controls in the bottom-left corner.

### Controls

The graph includes built-in controls:

- **Zoom in / Zoom out** buttons.
- **Fit view** -- zooms and pans to show all nodes.
- **Lock** -- toggles interactive panning and zooming.

## Legend

The bottom of the Graph panel shows a legend:

- **Solid animated line** -- content link (wiki link in block content).
- **Dashed line** -- semantic edge (explicit relationship created via the edge API).

## When the Graph Updates

The graph re-fetches data whenever the active lineage ID changes -- that is, when you select a different block or navigate to a different namespace. The API call retrieves the atom's full graph (outlinks, backlinks, edges) and rebuilds the node/edge layout.
