<script lang="ts">
  import type { TreeNode } from '../blockTree.svelte';
  import { resolveFileUrl } from '../blobUrl';

  let { node, isEditing }: { node: TreeNode; isEditing: boolean } = $props();

  let fileHash = $derived(node.properties?.file_hash as string | undefined);
  let filename = $derived((node.properties?.filename as string) ?? node.name);
  let mime = $derived((node.properties?.mime as string) ?? 'image/png');
  let size = $derived(node.properties?.size as number | undefined);
  let missing = $derived(!fileHash);

  // Resolve the file URL (async for WASM, sync for server/desktop)
  let imgSrc = $state<string | null>(null);

  $effect(() => {
    const hash = fileHash;
    const m = mime;
    if (!hash) {
      imgSrc = null;
      return;
    }
    resolveFileUrl(hash, m).then((url) => {
      imgSrc = url || null;
    });
  });

  function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
</script>

{#if isEditing}
  <div class="image-edit">
    {#if imgSrc}
      <img src={imgSrc} alt={filename} class="image-preview" />
    {:else if missing}
      <div class="image-missing">Image file not available</div>
    {:else}
      <div class="image-loading">Loading...</div>
    {/if}
    <div class="image-meta">
      <span class="meta-label">{filename}</span>
      {#if size}
        <span class="meta-size">{formatSize(size)}</span>
      {/if}
    </div>
  </div>
{:else}
  {#if missing}
    <span class="image-missing-inline">Missing: {filename}</span>
  {:else if imgSrc}
    <img src={imgSrc} alt={filename} class="image-inline" loading="lazy" />
  {:else}
    <span class="image-loading-inline">...</span>
  {/if}
{/if}

<style>
  .image-inline {
    max-width: 100%;
    max-height: 200px;
    border-radius: 4px;
    object-fit: contain;
    display: block;
  }

  .image-preview {
    max-width: 100%;
    max-height: 300px;
    border-radius: 4px;
    object-fit: contain;
    display: block;
    margin-bottom: 6px;
  }

  .image-edit {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .image-meta {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: 11px;
    color: var(--text-muted, #7a85b8);
  }

  .meta-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta-size {
    flex-shrink: 0;
    opacity: 0.7;
  }

  .image-missing, .image-missing-inline {
    color: var(--text-muted, #7a85b8);
    font-style: italic;
    font-size: 12px;
  }

  .image-missing {
    padding: 12px;
    border: 1px dashed var(--border-color, #333);
    border-radius: 4px;
    text-align: center;
  }

  .image-loading, .image-loading-inline {
    color: var(--text-muted, #7a85b8);
    font-size: 11px;
  }
</style>
