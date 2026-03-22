# Settings

Settings in yap-orange are stored as a regular block at `settings::ui` with `content_type = "setting"`. This means settings are persisted through the same atom/lineage system as all other content.

<!-- TODO: screenshot of settings view -->

## Accessing Settings

There are two ways to reach the settings:

1. **Sidebar** -- click the `settings` block, then navigate to the `ui` child.
2. **URL** -- go to `/#/settings::ui` directly.

When you navigate to the settings block, the outliner renders the Settings View (an inline form) instead of the normal editor.

## Available Settings

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| **Theme** | enum | `dark` | Color theme: `dark`, `light`, or `system` (follows OS preference). |
| **Font Size** | number | `13` | Editor font size in pixels. Range: 10--24. |
| **Show Line Numbers** | boolean | `false` | Whether to show line numbers in the CodeMirror editor. |
| **Dev Mode** | boolean | `false` | Enables the debug info bar in the outliner header and the Debug Log panel. |
| **Default Namespace** | string | (empty) | Default parent namespace for new blocks created via the sidebar. |
| **Max Expand Depth** | number | `0` | Maximum depth for auto-expand and expand-all operations. 0 means unlimited. |

## Changing a Setting

1. Navigate to `settings::ui`.
2. Find the setting in the form.
3. Change the value:
   - **Boolean** -- toggle the switch.
   - **Enum** -- select from the dropdown.
   - **Number** -- type a value or use the spinner.
   - **String** -- type in the text field.
4. Changes are saved automatically after a 500ms debounce. There is no save button.

## Reset Panel Layout

Below the settings fields, there is a **Reset Panel Layout** button. Clicking it:

1. Clears the saved `layout_state` setting.
2. Reloads the page.
3. The Dockview layout returns to its default arrangement.

Use this if panels get into a broken or undesirable state.

## Internal Settings

Some settings are managed automatically by the application and are not shown in the main form. They can be viewed by clicking the **Internal** toggle at the bottom of the settings view:

| Key | Purpose |
|-----|---------|
| `last_location` | The last navigated block ID. Used to restore your position on page reload. |
| `outliner_expanded` | Set of expanded block IDs in the outliner. Restored on load. |
| `sidebar_expanded` | Set of expanded block IDs in the sidebar. Restored on load. |
| `layout_state` | Serialized Dockview panel layout. Restored on load. |

These are stored as key-value pairs in the settings block's properties, just like the user-facing settings.

## How Settings Persistence Works

The settings store (`settingsStore.svelte.ts`) provides two functions:

- `getSetting(key)` -- reads the current value from the reactive in-memory cache.
- `setSetting(key, value, debounceMs)` -- updates the cache immediately (so the UI reacts instantly) and schedules a debounced save to the server.

On startup, the application loads all settings from the `settings::ui` block before initializing the router or restoring navigation state. This ensures that settings like `last_location` and `outliner_expanded` are available before any navigation occurs.
