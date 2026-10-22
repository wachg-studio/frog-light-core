<script>
  import InstanceIcon from './InstanceIcon.svelte';
  import InstanceSettings from './InstanceSettings.svelte';
  import { LOADER_NAMES } from './loaders.js';
  import { t, locale } from './i18n.svelte.js';

  // “我的存档”：每个存档是一个能直接启动的版本（原版，或原版 + 加载器），带自己的名称、图标和设置。
  let {
    panel = $bindable(null),
    instances,
    selected,
    settings, // 返回随后端调用发送的设置
    onChoose, // (id) => void
    onChanged, // () => Promise，重新读取存档列表
    onDelete, // (id) => Promise<string>，成功返回空串，失败返回错误信息
    onDownload, // 去下载页
    onBrowse, // (kind, id) => void
    onRun, // (label, fn) => Promise，放进下载队列
  } = $props();

  let query = $state('');
  let openId = $state('');
  let confirmId = $state('');
  let delError = $state('');

  async function remove(id) {
    confirmId = '';
    delError = await onDelete(id);
  }

  const q = $derived(query.trim().toLowerCase());
  const list = $derived(instances.filter((i) => !q || i.title.toLowerCase().includes(q) || i.id.toLowerCase().includes(q)));
  const opened = $derived(instances.find((i) => i.id === openId));

  const NAMES = LOADER_NAMES;
  const date = (secs) => new Date(secs * 1000).toLocaleDateString(locale());

  /** 副标题：自动命名时标题本身已经包含版本信息，这里只说上次启动；改过名字时补上版本信息。 */
  function sub(i) {
    const played = i.lastPlayed ? t('上次启动 {d}', { d: date(i.lastPlayed) }) : t('还没启动过');
    if (!i.name.trim()) return played;
    const ver = i.loader ? `${i.mc} · ${NAMES[i.loader]} ${i.loaderVersion}` : i.mc;
    return `${ver} · ${played}`;
  }
</script>

{#if opened}
  <InstanceSettings
    bind:panel
    inst={opened}
    {settings}
    onBack={() => (openId = '')}
    {onChanged}
    {onDelete}
    {onDownload}
    {onBrowse}
    {onRun}
    onDeleted={() => (openId = '')}
  />
{:else}
  <section class="versions" aria-label={t('我的存档')}>
    <div class="vhead">
      <h2>{t('我的存档')}</h2>
      <input class="search" placeholder={t('搜索存档')} bind:value={query} spellcheck="false" />
    </div>
    {#if delError}<p class="note err">{t('删除失败：{err}', { err: delError })}</p>{/if}
    <div class="glass vlist" bind:this={panel}>
      {#if !list.length}
        <div class="empty">
          <p>{instances.length ? t('没有匹配的存档') : t('还没有任何存档，去“下载游戏”里装一个。')}</p>
          {#if !instances.length}<button class="mini" onclick={onDownload}>{t('下载游戏')}</button>{/if}
        </div>
      {:else}
        {#each list as i, n (i.id)}
          <div class="item inst" class:sel={i.id === selected} style="--i:{Math.min(n, 12)}">
            <button class="irow" onclick={() => onChoose(i.id)}>
              <InstanceIcon icon={i.icon} url={i.iconUrl} loader={i.loader} size={38} />
              <span class="itxt">
                <b class="id">{i.title}</b>
                <small>{sub(i)}</small>
              </span>
              {#if i.id === selected}<i class="ok">{t('当前')}</i>{/if}
            </button>
            {#if confirmId === i.id}
              <span class="warn">{t('会一并移除它的世界、模组和配置，且无法恢复。')}</span>
              <button class="mini" onclick={() => (confirmId = '')}>{t('取消')}</button>
              <button class="mini danger" onclick={() => remove(i.id)}>{t('确认删除')}</button>
            {:else}
              <button class="gear trash" aria-label={t('删除此存档')} title={t('删除此存档')} onclick={() => { delError = ''; confirmId = i.id; }}>
                <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 7h16" /><path d="M10 11v6M14 11v6" /><path d="M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12" /><path d="M9 7V4h6v3" /></svg>
              </button>
            {/if}
            <button class="gear" aria-label={t('存档设置')} title={t('存档设置')} onclick={() => (openId = i.id)}>
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" /></svg>
            </button>
          </div>
        {/each}
      {/if}
    </div>
  </section>
{/if}

<style>
  .inst { gap: 4px; padding-right: 8px; }
  .itxt { display: flex; flex-direction: column; min-width: 0; line-height: 1.3; }
  .itxt small { color: var(--text-dim); font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .inst :global(.irow) { min-height: 56px; }
  .gear { width: 34px; height: 34px; border-radius: 10px; display: grid; place-items: center; flex: none; color: var(--text-dim); transition: background 0.2s, color 0.2s, transform 0.3s var(--ease); }
  .gear:hover { background: var(--fill-hover); color: var(--text); transform: rotate(45deg); }
  .gear.trash:hover { color: var(--danger); transform: none; }
  .warn { color: var(--danger); font-size: 12px; max-width: 280px; line-height: 1.35; }
  .mini.danger { color: var(--danger); }
  .empty { display: flex; flex-direction: column; align-items: center; gap: 12px; }
</style>
