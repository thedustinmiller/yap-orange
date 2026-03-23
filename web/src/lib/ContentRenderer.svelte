<script lang="ts">
  import { renderMarkdown } from './editor/renderMarkdown';
  import { mount, unmount } from 'svelte';
  import EmbedRenderer from './views/EmbedRenderer.svelte';

  let {
    content,
    onNavigateLink,
    embedDepth = 0,
  }: {
    content: string;
    onNavigateLink?: (path: string) => void;
    embedDepth?: number;
  } = $props();

  let html = $derived(renderMarkdown(content));

  let containerEl: HTMLElement | undefined = $state();
  let mountedEmbeds: any[] = [];

  // Mount embed components into .block-embed placeholder spans after HTML renders
  $effect(() => {
    // Touch html to re-run when content changes
    void html;

    // Clean up previous embeds
    for (const embed of mountedEmbeds) {
      try { unmount(embed); } catch { /* already unmounted */ }
    }
    mountedEmbeds = [];

    if (!containerEl) return;

    // Wait a tick for {@html} to render
    requestAnimationFrame(() => {
      if (!containerEl) return;
      const placeholders = containerEl.querySelectorAll('.block-embed[data-embed-path]');
      for (const el of placeholders) {
        const path = (el as HTMLElement).dataset.embedPath;
        if (!path) continue;

        const component = mount(EmbedRenderer, {
          target: el,
          props: {
            path,
            depth: embedDepth + 1,
            onNavigateLink,
          },
        });
        mountedEmbeds.push(component);
      }
    });
  });

  function handleClick(e: MouseEvent) {
    const target = e.target as HTMLElement;

    // Handle wiki link clicks
    const link = target.closest('.wiki-link') as HTMLElement | null;
    if (link) {
      e.stopPropagation();
      const path = link.dataset.path;
      if (path) {
        onNavigateLink?.(path);
      }
      return;
    }

  }
</script>

<span class="content-rendered" role="presentation" onclick={handleClick} bind:this={containerEl}>
  {@html html}
</span>

<style>
  .content-rendered {
    word-break: break-word;
  }

  /* Markdown element styling */
  .content-rendered :global(p) {
    margin: 0;
  }

  .content-rendered :global(p + p) {
    margin-top: 0.4em;
  }

  .content-rendered :global(code) {
    font-family: var(--font-mono);
    font-size: 0.9em;
    background: var(--bg-tertiary);
    padding: 1px 4px;
    border-radius: 3px;
    color: #9ece6a;
  }

  .content-rendered :global(pre) {
    background: var(--bg-tertiary);
    padding: 8px;
    border-radius: 4px;
    overflow-x: auto;
    margin: 4px 0;
  }

  .content-rendered :global(pre code) {
    background: none;
    padding: 0;
  }

  .content-rendered :global(strong) {
    font-weight: bold;
  }

  .content-rendered :global(em) {
    font-style: italic;
  }

  .content-rendered :global(a) {
    color: var(--link-color);
    text-decoration: underline;
  }

  .content-rendered :global(blockquote) {
    border-left: 3px solid var(--border-color);
    margin: 4px 0;
    padding-left: 8px;
    color: var(--text-secondary);
  }

  .content-rendered :global(ul),
  .content-rendered :global(ol) {
    margin: 2px 0;
    padding-left: 20px;
  }

  .content-rendered :global(hr) {
    border: none;
    border-top: 1px solid var(--border-color);
    margin: 8px 0;
  }

  /* Wiki link styling */
  .content-rendered :global(.wiki-link) {
    color: var(--link-color);
    cursor: pointer;
    border-bottom: 1px solid transparent;
    transition: border-color 0.1s;
  }

  .content-rendered :global(.wiki-link:hover) {
    border-bottom-color: var(--link-color);
  }

</style>
