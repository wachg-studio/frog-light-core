<script>
  import LoaderIcon from './LoaderIcon.svelte';
  import { renderBlock, blockKey } from './blocks.js';

  // 存档图标：原版方块（用原版的方块模型与贴图渲染）、用户自己的图片，或者（icon 为空时）按加载器自动选。
  let { icon = '', url = null, loader = '', size = 40 } = $props();

  // 没选图标又没有加载器时，用草方块
  const key = $derived(icon === 'custom' ? '' : blockKey(icon) || (!loader ? 'grass_block' : ''));
  let src = $state('');

  $effect(() => {
    const k = key;
    src = '';
    if (!k) return;
    let alive = true;
    renderBlock(k).then((u) => { if (alive) src = u; }).catch(() => {});
    return () => { alive = false; };
  });
</script>

{#if icon === 'custom' && url}
  <img class="ii" src={url} width={size} height={size} alt="" style="border-radius:{Math.round(size / 4.5)}px" />
{:else if key}
  <img class="ii block" {src} width={size} height={size} alt="" draggable="false" style:opacity={src ? 1 : 0} />
{:else if loader}
  <LoaderIcon kind={loader} {size} />
{:else}
  <span style="width:{size}px;height:{size}px" aria-hidden="true"></span>
{/if}

<style>
  .ii { flex: none; filter: drop-shadow(0 1px 2px rgba(0, 0, 0, 0.22)); object-fit: cover; }
  .ii.block { object-fit: contain; transition: opacity 0.2s; }
</style>
