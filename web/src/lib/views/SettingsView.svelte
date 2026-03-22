<script lang="ts">
  import type { TreeNode } from '../blockTree.svelte'
  import { getSetting, setSetting, flushSettingsNow } from '../settingsStore.svelte'
  import { isWasmMode, wasmFactoryReset } from '../../sw-register'

  let { node, isEditing = false }: { node: TreeNode; isEditing?: boolean } = $props()

  // Internal keys hidden from the main form (shown as read-only)
  const INTERNAL_KEYS = new Set(['last_location', 'outliner_expanded', 'sidebar_expanded', 'layout_state'])

  interface SettingField {
    key: string
    label: string
    type: 'boolean' | 'enum' | 'number' | 'string'
    default: unknown
    options?: string[]
    min?: number
    max?: number
    placeholder?: string
  }

  const SETTING_FIELDS: SettingField[] = [
    {
      key: 'theme',
      label: 'Theme',
      type: 'enum',
      default: 'dark',
      options: ['dark', 'light', 'system'],
    },
    {
      key: 'font_size',
      label: 'Font Size',
      type: 'number',
      default: 13,
      min: 10,
      max: 24,
    },
    {
      key: 'editor_line_numbers',
      label: 'Show Line Numbers',
      type: 'boolean',
      default: false,
    },
    {
      key: 'dev_mode',
      label: 'Dev Mode',
      type: 'boolean',
      default: false,
    },
    {
      key: 'default_namespace',
      label: 'Default Namespace',
      type: 'string',
      default: '',
      placeholder: 'e.g. notes',
    },
    {
      key: 'max_expand_depth',
      label: 'Max Expand Depth',
      type: 'number',
      default: 0,
      min: 0,
      max: 50,
      placeholder: '0 = unlimited',
    },
  ]

  // Read field values from the reactive settings store (not the node prop)
  function getFieldValue(field: SettingField): unknown {
    return getSetting(field.key) ?? field.default
  }

  let showInternal = $state(false)

  // All writes go through setSetting — reactive cache + debounced server persist
  function handleBooleanChange(key: string, value: boolean) {
    setSetting(key, value, 500)
  }

  function handleEnumChange(key: string, value: string) {
    setSetting(key, value, 500)
  }

  function handleNumberChange(key: string, value: string) {
    const n = parseFloat(value)
    if (!isNaN(n)) setSetting(key, n, 500)
  }

  function handleStringChange(key: string, value: string) {
    setSetting(key, value, 500)
  }

  let resetting = $state(false)

  async function handleFactoryReset() {
    if (!confirm('This will erase all data and restore the default tutorial content. Continue?')) {
      return
    }
    resetting = true
    try {
      await wasmFactoryReset()
      window.location.reload()
    } catch (e: any) {
      console.error('Factory reset failed:', e)
      resetting = false
    }
  }

  let internalEntries = $derived(
    Object.entries(node.properties ?? {}).filter(([k]) => INTERNAL_KEYS.has(k))
  )
</script>

<div class="settings-view" role="presentation" onclick={(e) => e.stopPropagation()}>
  <div class="settings-header">
    <span class="settings-icon">⚙</span>
    <span class="settings-title">Settings</span>
  </div>

  <div class="settings-fields">
    {#each SETTING_FIELDS as field}
      <div class="field-row">
        <label class="field-label" for="setting-{field.key}">{field.label}</label>
        <div class="field-control">
          {#if field.type === 'boolean'}
            <label class="toggle">
              <input
                id="setting-{field.key}"
                type="checkbox"
                checked={getFieldValue(field) as boolean}
                onchange={(e) => handleBooleanChange(field.key, (e.target as HTMLInputElement).checked)}
              />
              <span class="toggle-track"></span>
            </label>
          {:else if field.type === 'enum'}
            <select
              id="setting-{field.key}"
              value={getFieldValue(field) as string}
              onchange={(e) => handleEnumChange(field.key, (e.target as HTMLSelectElement).value)}
            >
              {#each field.options ?? [] as opt}
                <option value={opt}>{opt}</option>
              {/each}
            </select>
          {:else if field.type === 'number'}
            <input
              id="setting-{field.key}"
              type="number"
              value={getFieldValue(field) as number}
              min={field.min}
              max={field.max}
              oninput={(e) => handleNumberChange(field.key, (e.target as HTMLInputElement).value)}
            />
          {:else}
            <input
              id="setting-{field.key}"
              type="text"
              value={getFieldValue(field) as string}
              placeholder={field.placeholder ?? ''}
              oninput={(e) => handleStringChange(field.key, (e.target as HTMLInputElement).value)}
            />
          {/if}
        </div>
      </div>
    {/each}
  </div>

  <div class="settings-actions">
    <button class="reset-layout-btn" onclick={async () => {
      setSetting('layout_state', null, 0);
      await flushSettingsNow();
      window.location.reload();
    }}>Reset Panel Layout</button>
    {#if isWasmMode()}
      <button
        class="reset-layout-btn factory-reset-btn"
        onclick={handleFactoryReset}
        disabled={resetting}
      >{resetting ? 'Resetting...' : 'Factory Reset'}</button>
      <div class="action-hint">Erase all data and restore default tutorial content</div>
    {/if}
  </div>

  {#if internalEntries.length > 0}
    <button class="internal-toggle" onclick={() => (showInternal = !showInternal)} aria-expanded={showInternal}>
      <span class="internal-caret">{showInternal ? '▾' : '▸'}</span>
      Internal ({internalEntries.length})
    </button>
    {#if showInternal}
      <div class="internal-fields">
        {#each internalEntries as [key, value]}
          <div class="internal-row">
            <span class="internal-key">{key}</span>
            <span class="internal-value">{JSON.stringify(value)}</span>
          </div>
        {/each}
      </div>
    {/if}
  {/if}
</div>

<style>
  .settings-view {
    padding: 8px 10px;
    min-width: 0;
  }

  .settings-header {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 10px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-secondary, #888);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .settings-icon {
    font-size: 14px;
    opacity: 0.7;
  }

  .settings-fields {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .field-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 26px;
  }

  .field-label {
    flex: 0 0 160px;
    font-size: 12px;
    color: var(--text-primary, #ccc);
    cursor: default;
    user-select: none;
  }

  .field-control {
    flex: 1;
    min-width: 0;
  }

  .field-control input[type="text"],
  .field-control input[type="number"],
  .field-control select {
    width: 100%;
    max-width: 180px;
    background: var(--bg-secondary, #1a1b26);
    border: 1px solid var(--border-color, #333);
    border-radius: 4px;
    color: var(--text-primary, #ccc);
    font-size: 12px;
    padding: 3px 7px;
    outline: none;
    font-family: inherit;
  }

  .field-control input[type="text"]:focus,
  .field-control input[type="number"]:focus,
  .field-control select:focus {
    border-color: var(--accent-color, #7aa2f7);
  }

  .field-control input[type="number"] {
    max-width: 80px;
  }

  /* Toggle switch */
  .toggle {
    display: inline-flex;
    align-items: center;
    cursor: pointer;
  }

  .toggle input[type="checkbox"] {
    opacity: 0;
    width: 0;
    height: 0;
    position: absolute;
  }

  .toggle-track {
    display: inline-block;
    width: 32px;
    height: 16px;
    background: var(--bg-tertiary, #2d2d3a);
    border-radius: 8px;
    border: 1px solid var(--border-color, #333);
    position: relative;
    transition: background 0.15s;
  }

  .toggle-track::after {
    content: '';
    position: absolute;
    left: 2px;
    top: 50%;
    transform: translateY(-50%);
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--text-muted, #555);
    transition: left 0.15s, background 0.15s;
  }

  .toggle input:checked + .toggle-track {
    background: var(--accent-color, #7aa2f7);
    border-color: var(--accent-color, #7aa2f7);
  }

  .toggle input:checked + .toggle-track::after {
    left: 18px;
    background: white;
  }

  /* Actions */
  .settings-actions {
    margin-top: 12px;
    padding-top: 8px;
    border-top: 1px solid var(--border-color, #333);
  }

  .reset-layout-btn {
    background: var(--bg-secondary, #1a1b26);
    border: 1px solid var(--border-color, #333);
    border-radius: 4px;
    color: var(--text-primary, #ccc);
    font-size: 12px;
    padding: 4px 12px;
    cursor: pointer;
    font-family: inherit;
  }

  .reset-layout-btn:hover:not(:disabled) {
    background: var(--bg-tertiary, #2d2d3a);
    border-color: var(--text-muted, #555);
  }

  .reset-layout-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .factory-reset-btn {
    margin-top: 6px;
    border-color: #f7768e44;
    color: #f7768e;
  }

  .factory-reset-btn:hover:not(:disabled) {
    border-color: #f7768e;
  }

  .action-hint {
    font-size: 11px;
    color: var(--text-muted, #555);
    margin-top: 4px;
  }

  /* Internal section */
  .internal-toggle {
    background: none;
    border: none;
    padding: 0;
    font-family: inherit;
    margin-top: 10px;
    font-size: 11px;
    color: var(--text-muted, #555);
    cursor: pointer;
    user-select: none;
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .internal-toggle:hover {
    color: var(--text-secondary, #888);
  }

  .internal-caret {
    font-size: 10px;
  }

  .internal-fields {
    margin-top: 4px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding-left: 12px;
  }

  .internal-row {
    display: flex;
    gap: 8px;
    font-size: 11px;
    font-family: var(--font-mono, monospace);
  }

  .internal-key {
    color: var(--text-muted, #555);
    flex-shrink: 0;
  }

  .internal-value {
    color: var(--text-secondary, #888);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
