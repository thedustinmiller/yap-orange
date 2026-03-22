# Panel Layout

The yap-orange web UI uses [Dockview](https://dockview.dev) for its panel layout. Every panel can be resized, rearranged, and tabbed together by dragging panel headers.

## Default Panels

The interface ships with seven panels, arranged in a three-column default layout:

| Panel | Tab Title | Position | Purpose |
|-------|-----------|----------|---------|
| Sidebar | Blocks | Left column | Namespace tree navigation |
| Outliner | Outliner | Center column | Hierarchical block editing |
| Graph | Graph | Right column, top | Relationship visualization |
| Links | Links | Right column, bottom (tabbed) | Backlinks, outlinks, edges |
| Properties | Properties | Right column, bottom (tabbed with Links) | JSON metadata editor |
| Debug Log | Debug Log | Below outliner | Server log stream (dev mode) |
| Import/Export | Import/Export | Added to existing group | Data transfer UI |

<!-- TODO: screenshot of the default layout -->

## Rearranging Panels

To rearrange panels:

1. **Drag a panel tab** to another location. Drop zones appear as you drag: above, below, left, right, or within an existing tab group.
2. **Resize columns and rows** by dragging the dividers between panels.
3. **Tab panels together** by dropping one panel's tab onto another panel's tab bar. The Links and Properties panels are tabbed together by default.

Layout changes are saved automatically. The current layout is serialized to the `layout_state` setting and restored on next load.

## Panel Shortcuts

You can switch between panels without the mouse:

- **Alt+1 through Alt+5** -- toggle focus on Navigator, Bookmarks, Links, Properties, and Graph. Press the same shortcut again to return to the outliner.
- **Ctrl+K** -- open the Quick Switcher, which lists all panels in most-recently-used order. Arrow keys to select, Enter to switch, Escape to cancel.

See [Keyboard Shortcuts](./keyboard-shortcuts.md) for the full reference.

## Resetting the Layout

If the layout gets into a bad state:

1. Navigate to `settings::ui` in the sidebar.
2. Click the **Reset Panel Layout** button.
3. The page reloads with the default panel arrangement.

Alternatively, clear the `layout_state` setting from the Properties panel on the settings block.

## Initial Panel Sizing

On first load (no saved layout), the sidebar is set to 220px wide and the outliner takes the majority of remaining space. The right column shares whatever width is left.

## Adding New Panels After Updates

When new panels are added in a software update, they are automatically injected into an existing layout that was saved before the panel existed. You do not need to reset your layout to see newly added panels.

## Dev Mode

Enabling **dev_mode** in settings activates two additional UI elements:

- **Debug info bar** -- appears in the outliner header below the breadcrumbs, showing the current block's `id`, `lineage_id`, `namespace`, and `name`.
- **Debug Log panel** -- a panel (below the outliner by default) that polls `GET /api/debug/logs` every second and displays recent server log entries from the ring buffer.

To enable dev mode:

1. Navigate to `settings::ui` in the sidebar.
2. Toggle **Dev Mode** on.

## Status Bar

The outliner panel includes a status bar at the bottom showing:

- **Mode indicator** -- `NAV` (navigation mode) or `EDIT` (edit mode, highlighted in accent color).
- **Block count** -- number of visible blocks in the current tree view.
- **Selection info** -- the namespace path of the selected block, or count if multiple blocks are selected.
- **Keyboard hints** (edit mode only) -- shows available shortcuts: `Esc` done, `Enter` new block, `Shift+Enter` newline, `Tab` indent, `Shift+Tab` outdent, `[[` link.
