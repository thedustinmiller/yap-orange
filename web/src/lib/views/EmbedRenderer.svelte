<script lang="ts">
  import { api } from '../api';
  import { resolveFileUrl } from '../blobUrl';
  import { onMount } from 'svelte';

  let {
    path,
    depth = 0,
    maxDepth = 3,
    onNavigateLink,
  }: {
    path: string;
    depth?: number;
    maxDepth?: number;
    onNavigateLink?: (path: string) => void;
  } = $props();

  let state: 'loading' | 'loaded' | 'error' | 'depth-exceeded' = $state('loading');
  let block: any = $state(null);
  let errorMessage = $state('');
  let mediaUrl = $state<string | null>(null);

  onMount(async () => {
    if (depth >= maxDepth) {
      state = 'depth-exceeded';
      return;
    }

    try {
      // Resolve path to block
      const resolved = await api.resolve({ path });
      block = await api.blocks.get(resolved.block_id);

      // Pre-resolve media URL if this is a media block
      if (block.properties?.file_hash) {
        mediaUrl = await resolveFileUrl(
          block.properties.file_hash as string,
          block.properties.mime as string | undefined,
        );
      }

      state = 'loaded';
    } catch {
      state = 'error';
      errorMessage = path;
    }
  });
</script>

<span class="embed-container" role="presentation">
  {#if state === 'loading'}
    <span class="embed-loading">...</span>
  {:else if state === 'depth-exceeded'}
    <span class="embed-depth-exceeded" title="Max embed depth reached">[[{path}]]</span>
  {:else if state === 'error'}
    <span class="embed-error">embed not found: {errorMessage}</span>
  {:else if block}
    {#if block.content_type === 'image' && mediaUrl}
      <img
        src={mediaUrl}
        alt={block.name}
        class="embed-image"
        loading="lazy"
      />
    {:else if block.content_type === 'pdf' && block.properties?.file_hash}
      <span class="embed-file-chip">
        <span class="embed-file-badge">PDF</span>
        <span class="embed-file-name">{block.properties.filename ?? block.name}</span>
      </span>
    {:else if block.content_type === 'file' && block.properties?.file_hash}
      <span class="embed-file-chip">
        <span class="embed-file-badge">FILE</span>
        <span class="embed-file-name">{block.properties.filename ?? block.name}</span>
      </span>
    {:else if block.content}
      <!-- Text block transclusion — render as inline content -->
      <!-- Note: we import renderMarkdown lazily to avoid circular deps -->
      {@html renderContent(block.content)}
    {:else}
      <span class="embed-empty">{block.name || 'Empty block'}</span>
    {/if}
  {/if}
</span>

<script lang="ts" module>
  import { renderMarkdown } from '../editor/renderMarkdown';

  function renderContent(content: string): string {
    return renderMarkdown(content);
  }
</script>

<style>
  .embed-container {
    display: inline;
  }

  .embed-image {
    max-width: 100%;
    max-height: 200px;
    border-radius: 4px;
    object-fit: contain;
    vertical-align: middle;
  }

  .embed-file-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: var(--bg-tertiary, #24283b);
    border-radius: 3px;
    padding: 1px 6px;
    font-size: 11px;
    vertical-align: middle;
  }

  .embed-file-badge {
    font-size: 8px;
    font-weight: 700;
    color: var(--text-muted, #7a85b8);
    letter-spacing: 0.5px;
  }

  .embed-file-name {
    color: var(--text-primary, #ccc);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 150px;
  }

  .embed-loading {
    color: var(--text-muted, #7a85b8);
    font-style: italic;
    font-size: 11px;
  }

  .embed-error {
    color: #f7768e;
    font-style: italic;
    font-size: 11px;
  }

  .embed-depth-exceeded {
    color: var(--text-muted, #7a85b8);
    font-style: italic;
  }

  .embed-empty {
    color: var(--text-muted, #7a85b8);
    font-style: italic;
    font-size: 11px;
  }
</style>
