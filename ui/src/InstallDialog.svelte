<script>
  import { onMount } from 'svelte';
  import { call } from './api.js';
  import InstanceIcon from './InstanceIcon.svelte';
  import Select from './Select.svelte';
  import { LOADER_NAMES } from './loaders.js';
  import { t } from './i18n.svelte.js';

  // 选择把一个模组 / 光影 / 资源包 / 数据包装进哪些存档。
  // 不适配的存档（游戏版本或加载器不对）是灰色的，不能选，并写明原因；已经装过的也不能再选。
  let {
    title, // 要装的东西叫什么
    subtitle = '', // 例如具体的版本名
    kind,
    projectId,
    item, // { gameVersions, loaders }：用来判断每个存档适不适配
    instances,
    settings, // 返回随后端调用发送的设置
    compatOf, // (instance, item, kind) => 不适配的原因，适配返回 ''
    onConfirm, // ([{ id, world }]) => void
    onClose,
  } = $props();

  const rows = $derived(instances.map((inst) => ({ inst, why: compatOf(inst, item, kind) })));
  let installed = $state({}); // 存档 id → 已经装过
  let worlds = $state({}); // 数据包：存档 id → 世界列表
  let world = $state({}); // 数据包：存档 id → 选的世界
  let sel = $state({}); // 存档 id → 勾选了
  let loading = $state(true);

  // 每一行最终的状态：灰掉的原因（不适配 / 已安装 / 没有世界），空串表示可以选
  const blockedBy = (r) => r.why || (installed[r.inst.id] ? t('已安装') : kind === 'datapack' && !(worlds[r.inst.id]?.length) ? t('还没有世界') : '');
  const pickable = $derived(rows.filter((r) => !blockedBy(r)));
  const picked = $derived(pickable.filter((r) => sel[r.inst.id]));

  onMount(async () => {
    const ok = rows.filter((r) => !r.why);
    await Promise.all(
      ok.map(async (r) => {
        try {
          if (kind === 'datapack') {
            const w = await call('list_worlds', { id: r.inst.id, settings: settings() });
            worlds[r.inst.id] = w;
            world[r.inst.id] = w[0] ?? '';
          } else {
            const map = await call('content_installed', { instance: r.inst.id, kind, world: null, settings: settings() });
            installed[r.inst.id] = projectId in map;
          }
        } catch {
          // 查不到就当没装过
        }
      }),
    );
    // 默认勾上所有能装的
    for (const r of ok) if (!blockedBy(r)) sel[r.inst.id] = true;
    loading = false;
  });

  onMount(() => {
    const key = (e) => { if (e.key === 'Escape') onClose(); };
    addEventListener('keydown', key);
    return () => removeEventListener('keydown', key);
  });

  const toggleAll = () => {
    const every = pickable.length > 0 && pickable.every((r) => sel[r.inst.id]);
    for (const r of pickable) sel[r.inst.id] = !every;
  };
  const sub = (i) => `${i.mc}${i.loader ? ` · ${LOADER_NAMES[i.loader] ?? i.loader} ${i.loaderVersion}` : ''}`;
  const confirm = () => onConfirm(picked.map((r) => ({ id: r.inst.id, world: kind === 'datapack' ? world[r.inst.id] || null : null })));
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" onclick={(e) => { if (e.target === e.currentTarget) onClose(); }}>
  <div class="dlg glass" role="dialog" aria-modal="true" aria-label={t('选择安装到哪些存档')}>
    <header>
      <div>
        <h2>{t('安装到哪些存档')}</h2>
        <p class="what"><b>{title}</b>{#if subtitle}<span> · {subtitle}</span>{/if}</p>
      </div>
      <button class="x" onclick={onClose} aria-label={t('关闭')}>
        <svg width="12" height="12" viewBox="0 0 10 10"><path d="M1.5 1.5l7 7M8.5 1.5l-7 7" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" /></svg>
      </button>
    </header>

    <p class="hint">{t('灰色的存档不支持这个内容，不能选择。')}</p>

    <div class="list" role="group" aria-label={t('存档')}>
      {#each rows as r, n (r.inst.id)}
        {@const why = blockedBy(r)}
        <label class="row" class:off={!!why} style="--i:{Math.min(n, 10)}">
          <input type="checkbox" disabled={!!why} bind:checked={sel[r.inst.id]} />
          <i class="box" aria-hidden="true"></i>
          <InstanceIcon icon={r.inst.icon} url={r.inst.iconUrl} loader={r.inst.loader} size={32} />
          <span class="txt">
            <b>{r.inst.title}</b>
            <small>{sub(r.inst)}</small>
          </span>
          {#if why}
            <span class="why">{why}</span>
          {:else if kind === 'datapack' && worlds[r.inst.id]?.length}
            <Select bind:value={world[r.inst.id]} options={worlds[r.inst.id].map((w) => [w, w])} label={t('世界')} />
          {/if}
        </label>
      {/each}
      {#if loading && rows.some((r) => !r.why)}<p class="load">{t('正在检查每个存档的情况…')}</p>{/if}
    </div>

    {#if kind === 'mod'}<p class="hint">{t('安装模组时会自动补齐它必需的前置。')}</p>{/if}

    <footer>
      <button class="link" onclick={toggleAll} disabled={!pickable.length}>{pickable.length > 0 && pickable.every((r) => sel[r.inst.id]) ? t('取消全选') : t('全选可安装的')}</button>
      <span class="grow"></span>
      <button class="ghost" onclick={onClose}>{t('取消')}</button>
      <button class="primary" disabled={!picked.length} onclick={confirm}>{picked.length ? t('安装到 {n} 个存档', { n: picked.length }) : t('请选择存档')}</button>
    </footer>
  </div>
</div>

<style>
  .scrim { backdrop-filter: blur(5px); position: fixed; inset: 0; z-index: 30; display: grid; place-items: center; padding: 48px 24px 24px; background: rgba(0, 0, 0, 0.28); animation: fade 0.2s var(--ease); }
  @keyframes fade { from { opacity: 0; } }
  .dlg { width: min(520px, 100%); max-height: 100%; display: flex; flex-direction: column; gap: 12px; padding: 20px 22px 18px; background: var(--pop); animation: rise 0.3s var(--ease); }
  @keyframes rise { from { opacity: 0; transform: translateY(10px) scale(0.98); } }
  header { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px; }
  h2 { font-size: 18px; font-weight: 600; letter-spacing: -0.02em; }
  .what { font-size: 13px; color: var(--text-dim); margin-top: 2px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 380px; }
  .what b { color: var(--text); font-weight: 600; }
  .x { width: 28px; height: 28px; border-radius: 14px; display: grid; place-items: center; color: var(--text-dim); flex: none; }
  .x:hover { background: var(--fill-hover); color: var(--text); }
  .hint { font-size: 12px; color: var(--text-dim); }
  .list { display: flex; flex-direction: column; gap: 4px; overflow: auto; min-height: 0; padding-right: 2px; }
  .list::-webkit-scrollbar { width: 8px; }
  .list::-webkit-scrollbar-thumb { background: var(--fill-hover); border-radius: 8px; }
  .row { animation: slide 0.3s var(--ease) both; animation-delay: calc(var(--i) * 28ms); display: flex; align-items: center; gap: 12px; min-height: 52px; padding: 6px 12px; border-radius: 12px; background: var(--fill); cursor: pointer; transition: background 0.15s; }
  .row:hover:not(.off) { background: var(--fill-hover); }
  .row.off { cursor: not-allowed; opacity: 0.5; filter: grayscale(1); }
  .row input { display: none; }
  .box { width: 18px; height: 18px; border-radius: 6px; box-shadow: inset 0 0 0 1.5px var(--text-dim); flex: none; display: grid; place-items: center; transition: background 0.15s, box-shadow 0.15s; }
  .row input:checked + .box { background: var(--btn); box-shadow: none; }
  .row input:checked + .box::after { content: ''; width: 8px; height: 4px; border-left: 2px solid var(--btn-text); border-bottom: 2px solid var(--btn-text); transform: translateY(-1px) rotate(-45deg); }
  .row.off .box { opacity: 0.5; }
  .txt { flex: 1; min-width: 0; display: flex; flex-direction: column; line-height: 1.3; }
  .txt b { font-size: 14px; font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .txt small { font-size: 12px; color: var(--text-dim); }
  .why { font-size: 12px; color: var(--text-dim); flex: none; }
  .load { text-align: center; font-size: 12px; color: var(--text-dim); padding: 6px; }
  footer { display: flex; align-items: center; gap: 10px; padding-top: 4px; }
  .grow { flex: 1; }
  .link { font-size: 12px; color: var(--blue); }
  .link:hover:not(:disabled) { text-decoration: underline; }
  .link:disabled { opacity: 0.4; }
  .primary { height: 36px; padding: 0 22px; border-radius: 18px; background: var(--btn); color: var(--btn-text); font-size: 14px; font-weight: 600; transition: transform 0.2s var(--ease), opacity 0.2s; }
  .primary:hover:not(:disabled) { transform: scale(1.04); }
  .primary:disabled { opacity: 0.35; cursor: not-allowed; }
</style>
