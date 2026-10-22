<script>
  import { onMount } from 'svelte';
  import Head from './Head.svelte';
  import SkinViewer from './SkinViewer.svelte';
  import { settings } from './settings.svelte.js';
  import { PRESETS, userSkins, activeSkin, addSkin, updateSkin, removeSkin, readSkinFile } from './skins.svelte.js';
  import { t } from './i18n.svelte.js';

  let { onClose } = $props();

  let walking = $state(false);
  let error = $state('');
  let confirmDel = $state(false);
  let fileInput;

  const skin = $derived(activeSkin());

  async function onFile(e) {
    const file = e.currentTarget.files?.[0];
    e.currentTarget.value = '';
    if (!file) return;
    error = '';
    try {
      const { url, slim } = await readSkinFile(file);
      const s = addSkin({ name: file.name.replace(/\.png$/i, '').slice(0, 24) || t('我的皮肤'), url, slim });
      settings.skin = s.id;
      confirmDel = false;
    } catch (err) {
      error = t(err.message);
    }
  }

  function pick(id) {
    settings.skin = id;
    confirmDel = false;
    error = '';
  }

  function remove() {
    removeSkin(skin.id);
    confirmDel = false;
  }

  onMount(() => {
    const key = (e) => { if (e.key === 'Escape') onClose(); };
    addEventListener('keydown', key);
    return () => removeEventListener('keydown', key);
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" onclick={(e) => { if (e.target === e.currentTarget) onClose(); }}>
  <div class="dlg glass" role="dialog" aria-modal="true" aria-label={t('更换皮肤')}>
    <header>
      <h2>{t('更换皮肤')}</h2>
      <button class="x" onclick={onClose} aria-label={t('关闭')}>
        <svg width="12" height="12" viewBox="0 0 10 10"><path d="M1.5 1.5l7 7M8.5 1.5l-7 7" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" /></svg>
      </button>
    </header>

    <div class="body">
      <section class="stage">
        <div class="view"><SkinViewer url={skin.url} slim={skin.slim} {walking} /></div>
        <div class="meta">
          {#if skin.preset}
            <b>{skin.name}</b>
            <small>{skin.slim ? t('纤细手臂') : t('经典手臂')}</small>
          {:else}
            <input class="txt" value={skin.name} maxlength="24" spellcheck="false" aria-label={t('皮肤名称')} oninput={(e) => updateSkin(skin.id, { name: e.currentTarget.value })} />
            <span class="seg">
              <button class:on={!skin.slim} onclick={() => updateSkin(skin.id, { slim: false })}>{t('经典')}</button>
              <button class:on={skin.slim} onclick={() => updateSkin(skin.id, { slim: true })}>{t('纤细')}</button>
            </span>
          {/if}
        </div>
        <div class="tools">
          <button class="mini" class:on={walking} onclick={() => (walking = !walking)}>{walking ? t('停止') : t('走路')}</button>
          {#if !skin.preset}
            {#if confirmDel}
              <button class="mini" onclick={() => (confirmDel = false)}>{t('取消')}</button>
              <button class="mini danger" onclick={remove}>{t('确认删除')}</button>
            {:else}
              <button class="mini danger" onclick={() => (confirmDel = true)}>{t('删除皮肤')}</button>
            {/if}
          {/if}
        </div>
      </section>

      <section class="pick">
        <h4>{t('预设皮肤')}</h4>
        <div class="grid">
          {#each PRESETS as s (s.id)}
            <button class="card" class:on={s.id === skin.id} onclick={() => pick(s.id)} aria-pressed={s.id === skin.id}>
              <Head size={52} url={s.url} />
              <span>{s.name}</span>
            </button>
          {/each}
        </div>

        <h4>{t('我的皮肤')}</h4>
        <div class="grid">
          {#each userSkins as s (s.id)}
            <button class="card" class:on={s.id === skin.id} onclick={() => pick(s.id)} aria-pressed={s.id === skin.id}>
              <Head size={52} url={s.url} />
              <span>{s.name}</span>
            </button>
          {/each}
          <button class="card add" onclick={() => fileInput.click()}>
            <i aria-hidden="true">
              <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><path d="M12 5v14M5 12h14" /></svg>
            </i>
            <span>{t('导入皮肤')}</span>
          </button>
        </div>
        <input bind:this={fileInput} type="file" accept="image/png" hidden onchange={onFile} />
        {#if error}<p class="err">{error}</p>{/if}
        <p class="foot">{t('导入 64×64 的 PNG 皮肤文件即可。皮肤会在下次启动游戏时生效，仅对离线账号有效。')}</p>
      </section>
    </div>
  </div>
</div>

<style>
  .scrim { backdrop-filter: blur(5px); position: fixed; inset: 0; z-index: 30; display: grid; place-items: center; padding: 56px 32px 32px; background: rgba(0, 0, 0, 0.28); animation: fade 0.2s var(--ease); }
  @keyframes fade { from { opacity: 0; } }
  .dlg { width: min(820px, 100%); height: min(540px, 100%); display: flex; flex-direction: column; background: var(--pop); animation: rise 0.35s var(--ease); overflow: hidden; }
  @keyframes rise { from { opacity: 0; transform: translateY(10px) scale(0.98); } }
  header { display: flex; align-items: center; justify-content: space-between; padding: 16px 20px 0 24px; }
  h2 { font-size: 20px; font-weight: 600; letter-spacing: -0.02em; }
  .x { width: 30px; height: 30px; border-radius: 15px; display: grid; place-items: center; color: var(--text-dim); transition: background 0.2s; }
  .x:hover { background: var(--fill-hover); color: var(--text); }
  .body { flex: 1; min-height: 0; display: grid; grid-template-columns: 300px 1fr; gap: 8px; padding: 8px 20px 20px 20px; }

  .stage { display: flex; flex-direction: column; gap: 10px; min-height: 0; }
  .view { flex: 1; min-height: 0; border-radius: 16px; background: var(--fill); box-shadow: inset 0 0 0 1px var(--hairline); }
  .meta { display: flex; align-items: center; justify-content: space-between; gap: 10px; min-height: 30px; }
  .meta b { font-size: 15px; font-weight: 600; }
  .meta small { color: var(--text-dim); font-size: 12px; }
  .txt { flex: 1; min-width: 0; height: 30px; border: 0; border-radius: 8px; background: var(--glass-solid); font-size: 13px; padding: 0 10px; user-select: text; box-shadow: inset 0 0 0 1px var(--hairline); }
  .tools { display: flex; gap: 8px; }
  .mini.on { background: var(--btn); color: var(--btn-text); }
  .mini.danger { color: var(--danger); }

  .pick { min-height: 0; overflow: auto; padding: 4px 4px 0 12px; }
  .pick::-webkit-scrollbar { width: 8px; }
  .pick::-webkit-scrollbar-thumb { background: var(--fill-hover); border-radius: 8px; }
  h4 { font-size: 12px; font-weight: 600; color: var(--text-dim); margin: 10px 2px 8px; }
  h4:first-child { margin-top: 0; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(86px, 1fr)); gap: 8px; }
  .card { display: flex; flex-direction: column; align-items: center; gap: 6px; padding: 12px 6px 9px; border-radius: 14px; background: var(--fill); font-size: 12px; transition: background 0.2s, transform 0.25s var(--ease), box-shadow 0.2s; }
  .card:hover { background: var(--fill-hover); transform: translateY(-1px); }
  .card.on { background: var(--glass-solid); box-shadow: 0 0 0 2px var(--btn); }
  .card span { max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .card.add { color: var(--text-dim); justify-content: center; }
  .card.add i { width: 52px; height: 52px; border-radius: 14px; display: grid; place-items: center; box-shadow: inset 0 0 0 1.5px var(--hairline); }
  .err { margin-top: 10px; font-size: 12px; color: var(--danger); }
  .foot { margin-top: 14px; font-size: 12px; color: var(--text-dim); line-height: 1.5; }

  @media (max-width: 760px) { .body { grid-template-columns: 1fr; overflow: auto; } .view { min-height: 260px; } }
</style>
