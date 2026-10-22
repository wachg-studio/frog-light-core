<script>
  import { call } from './api.js';
  import Select from './Select.svelte';
  import LoaderIcon from './LoaderIcon.svelte';
  import { LOADER_NAMES } from './loaders.js';
  import { t } from './i18n.svelte.js';

  // 下载页分三级：分类（按系列归并）→ 该系列的版本列表 → 整页的版本详情（选模组加载器、下载、删除）。
  let {
    panel = $bindable(null),
    remote,
    remoteState,
    installed,
    dlId, // 正在下载的任务名
    queued = [], // 排队中的任务名
    ring,
    detail,
    working,
    notice,
    dlError,
    settings, // 返回随后端调用发送的设置
    onInstall, // (mc, loader | null) => Promise
    onDelete, // (id) => Promise<string>，成功返回空串，失败返回错误信息
  } = $props();

  const KINDS = ['fabric', 'legacyfabric', 'quilt', 'forge', 'neoforge'];
  const NAMES = LOADER_NAMES;
  const DESC = {
    fabric: '轻量、更新快，新版本和大量现代模组首选',
    legacyfabric: '把 Fabric 带到 1.3–1.13.2 的老版本，玩老版本的 Fabric 模组用它',
    quilt: 'Fabric 的社区分支，兼容大部分 Fabric 模组',
    forge: '历史最悠久、模组数量最多的加载器',
    neoforge: 'Forge 的现代分支，1.20.2 之后的主流选择',
  };
  const TYPES = { release: ['release'], snapshot: ['snapshot'], old: ['old_beta', 'old_alpha'] };
  const TYPE_LABEL = { release: '正式版', snapshot: '快照', old_beta: 'Beta', old_alpha: 'Alpha' };

  let kind = $state('release'); // release | snapshot | old
  let query = $state('');
  let view = $state({ level: 0, cat: '', mc: '' });
  let pick = $state(''); // 选中的加载器，'' 表示不装
  let pickVer = $state('');
  let loaders = $state({}); // `${mc}|${kind}` -> { state: 'loading' | 'ok' | 'err', list }
  let confirmDel = $state(false);
  let delError = $state('');

  /** 把版本归到“系列”：1.20.4 → 1.20，26.1.2 → 26.1，24w14a → 2024 年的周快照，Beta/Alpha 各一类。 */
  function seriesOf(v) {
    if (v.type === 'old_beta') return 'beta';
    if (v.type === 'old_alpha') return 'alpha';
    let m = v.id.match(/^(\d\d)w\d+[a-z]$/);
    if (m) return `20${m[1]}`;
    m = v.id.match(/^1\.(\d+)/);
    if (m) return `1.${m[1]}`;
    m = v.id.match(/^(\d+)\.(\d+)/);
    if (m) return `${m[1]}.${m[2]}`;
    return 'other';
  }
  function seriesLabel(key) {
    if (key === 'beta') return t('Beta 版');
    if (key === 'alpha') return t('Alpha 版');
    if (key === 'other') return t('其他');
    if (/^20\d\d$/.test(key) && kind === 'snapshot') return t('{y} 年快照', { y: key });
    return t('{n} 系列', { n: key });
  }

  const groups = $derived.by(() => {
    const map = new Map();
    for (const v of remote) {
      if (!TYPES[kind].includes(v.type)) continue;
      const k = seriesOf(v);
      if (!map.has(k)) map.set(k, []);
      map.get(k).push(v);
    }
    return [...map].map(([key, list]) => ({ key, list })); // 保持官方清单的新→旧顺序
  });
  const latest = $derived({ release: remote.find((v) => v.type === 'release'), snapshot: remote.find((v) => v.type === 'snapshot') });
  const q = $derived(query.trim().toLowerCase());
  const found = $derived(q ? remote.filter((v) => v.id.toLowerCase().includes(q)).slice(0, 200) : []);
  const group = $derived(groups.find((g) => g.key === view.cat));
  const entry = $derived(remote.find((v) => v.id === view.mc));

  function versionId(mc, k, ver) {
    return { fabric: `fabric-loader-${ver}-${mc}`, legacyfabric: `legacy-fabric-loader-${ver}-${mc}`, quilt: `quilt-loader-${ver}-${mc}`, forge: `${mc}-forge-${ver}`, neoforge: `neoforge-${ver}` }[k];
  }

  function loadLoaders(mc) {
    for (const k of KINDS) {
      const key = `${mc}|${k}`;
      if (loaders[key]) continue;
      loaders[key] = { state: 'loading', list: [] };
      call('list_loader_versions', { mc, loader: k, settings: settings() })
        .then((list) => (loaders[key] = { state: 'ok', list }))
        .catch(() => (loaders[key] = { state: 'err', list: [] }));
    }
  }

  function openCat(key) { view = { level: 1, cat: key, mc: '' }; }
  function openVersion(id, cat = view.cat) {
    view = { level: 2, cat, mc: id };
    pick = ''; pickVer = ''; confirmDel = false; delError = '';
    loadLoaders(id);
  }
  function back() {
    if (view.level === 2 && view.cat) view = { level: 1, cat: view.cat, mc: '' };
    else view = { level: 0, cat: '', mc: '' };
  }

  function choose(k) {
    pick = pick === k ? '' : k;
    const l = loaders[`${view.mc}|${k}`]?.list ?? [];
    pickVer = pick ? ((l.find((v) => v.stable) ?? l[0])?.version ?? '') : '';
  }

  const status = (k) => {
    const s = loaders[`${view.mc}|${k}`];
    if (!s || s.state === 'loading') return 'loading';
    return s.state === 'ok' && s.list.length ? 'ok' : 'none';
  };
  const okKinds = $derived(KINDS.filter((k) => status(k) !== 'none'));
  const noneKinds = $derived(KINDS.filter((k) => status(k) === 'none'));
  const loaderOptions = $derived((loaders[`${view.mc}|${pick}`]?.list ?? []).map((v) => [v.version, v.version + (v.stable ? ` · ${t('推荐')}` : '')]));
  const targetId = $derived(view.mc ? (pick && pickVer ? versionId(view.mc, pick, pickVer) : view.mc) : '');
  const targetName = $derived(pick && pickVer ? `${view.mc} + ${NAMES[pick]} ${pickVer}` : view.mc);
  const targetInstalled = $derived(installed.includes(targetId));

  async function remove() {
    delError = await onDelete(targetId);
    if (!delError) confirmDel = false;
  }

  // 切换标签、搜索时回到第一层
  $effect(() => { kind; q; view = { level: 0, cat: '', mc: '' }; });
</script>

<section class="versions" aria-label={t('下载游戏')}>
  {#if view.level === 0}
    <div class="vhead">
      <h2>{t('下载游戏')}</h2>
      <input class="search" placeholder={t('搜索版本号')} bind:value={query} spellcheck="false" />
    </div>
    {#if !q}
      <div class="seg" role="tablist">
        {#each [['release', '正式版'], ['snapshot', '快照'], ['old', '远古版']] as [k, label]}
          <button role="tab" class:on={kind === k} aria-selected={kind === k} onclick={() => (kind = k)}>{t(label)}</button>
        {/each}
      </div>
    {/if}
  {:else}
    <div class="vhead">
      <button class="back" onclick={back} aria-label={t('返回')}>
        <svg width="9" height="15" viewBox="0 0 9 15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M7.5 1.5L2 7.5l5.5 6" /></svg>
        {t('返回')}
      </button>
      <h2>{view.cat ? seriesLabel(view.cat) : t('下载游戏')}</h2>
    </div>
  {/if}
  {#if notice || dlError}<p class="note" class:err={!!dlError}>{dlError || t(notice.k, notice.v)}</p>{/if}

  <div class="glass vlist" class:roomy={view.level === 2} bind:this={panel}>
    {#if remoteState === 'loading'}
      <p class="empty">{t('正在获取版本清单…')}</p>
    {:else if remoteState === 'error'}
      <p class="empty err">{t('获取版本清单失败：{err}', { err: dlError })}</p>

    {:else if view.level === 2 && entry}
      <!-- 第三层：整页介绍这个版本 -->
      <div class="vd">
        <div class="vd-hero">
          <div>
            <h1>{entry.id}</h1>
            <p>{t(TYPE_LABEL[entry.type] ?? entry.type)}{entry.releaseTime ? ` · ${entry.releaseTime.slice(0, 10)}` : ''}</p>
          </div>
          {#if installed.includes(entry.id)}<i class="okpill">{t('已安装')}</i>{/if}
        </div>

        <section class="vd-sec">
          <h3>{t('游戏本体')}</h3>
          <p class="dim">{t('原版 Minecraft，不带任何模组加载器。')}</p>
        </section>

        <section class="vd-sec">
          <h3>{t('模组加载器')}</h3>
          <p class="dim">{t('可选：装上之后才能加载对应类型的模组。')}</p>
          <div class="lgrid">
            {#each okKinds as k (k)}
              {@const loading = status(k) === 'loading'}
              <button class="lcard" class:on={pick === k} disabled={loading} onclick={() => choose(k)} aria-pressed={pick === k}>
                <LoaderIcon kind={k} size={40} />
                <span class="ltxt">
                  <b>{NAMES[k]}</b>
                  <small>{t(DESC[k])}</small>
                </span>
                {#if loading}<span class="spin" aria-hidden="true"></span>
                {:else if pick === k}<svg class="tick" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>{/if}
              </button>
            {/each}
          </div>
          {#if pick && loaderOptions.length}
            <div class="lver">
              <span class="dim">{t('加载器版本')}</span>
              <Select bind:value={pickVer} options={loaderOptions} label={t('加载器版本')} />
            </div>
          {/if}
        </section>

        {#if noneKinds.length}
        <section class="vd-sec">
          <h3>{t('暂不可用')}</h3>
          <div class="lgrid muted">
            {#each noneKinds as k (k)}
              <div class="lcard off"><LoaderIcon kind={k} size={34} /><span class="ltxt"><b>{NAMES[k]}</b><small>{t('该版本暂无 {name}', { name: NAMES[k] })}</small></span></div>
            {/each}
          </div>
        </section>
        {/if}

        <div class="vd-bar">
          <span class="dim">{t('将要安装')}</span>
          <b class="dname">{targetName}</b>
          <span class="grow"></span>
          {#if targetInstalled}
            {#if confirmDel}
              <span class="warn">{t('删除后会一并移除该版本文件夹里的存档、模组和配置，且无法恢复。')}</span>
              <button class="mini" onclick={() => (confirmDel = false)}>{t('取消')}</button>
              <button class="mini danger" onclick={remove}>{t('确认删除')}</button>
            {:else}
              <i class="okpill">{t('已安装')}</i>
              <button class="mini danger" onclick={() => (confirmDel = true)}>{t('删除此版本')}</button>
            {/if}
          {:else if dlId === targetName}
            <span class="dlp"><b>{Math.round(ring * 100)}%</b>{detail ? ` · ${detail}` : ''}</span>
          {:else if queued.includes(targetName)}
            <span class="dlp">{t('排队中')}</span>
          {:else}
            <button class="primary" disabled={working} onclick={() => onInstall(view.mc, pick && pickVer ? { kind: pick, version: pickVer } : null)}>{dlId ? t('加入队列') : t('下载')}</button>
          {/if}
        </div>
        {#if delError}<p class="note err">{t('删除失败：{err}', { err: delError })}</p>{/if}
      </div>

    {:else if view.level === 1 && group}
      <!-- 第二层：某个系列里的具体版本 -->
      {#each group.list as v (v.id)}
        <button class="item" onclick={() => openVersion(v.id, view.cat)}>
          <span class="id">{v.id}</span>
          <span class="tags">
            {#if v.releaseTime}<em>{v.releaseTime.slice(0, 10)}</em>{/if}
            {#if installed.includes(v.id)}<i class="ok">{t('已安装')}</i>{/if}
            <svg class="chev" width="6" height="10" viewBox="0 0 6 10" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M1 1l4 4-4 4" /></svg>
          </span>
        </button>
      {/each}

    {:else if q}
      <!-- 搜索：不分类，直接列出匹配的版本 -->
      {#if !found.length}<p class="empty">{t('没有匹配的版本')}</p>{/if}
      {#each found as v (v.id)}
        <button class="item" onclick={() => openVersion(v.id, '')}>
          <span class="id">{v.id}</span>
          <span class="tags">
            <i>{t(TYPE_LABEL[v.type] ?? v.type)}</i>
            {#if v.releaseTime}<em>{v.releaseTime.slice(0, 10)}</em>{/if}
            {#if installed.includes(v.id)}<i class="ok">{t('已安装')}</i>{/if}
          </span>
        </button>
      {/each}

    {:else}
      <!-- 第一层：最新版本 + 分类 -->
      <div class="latest">
        {#each [['最新正式版', latest.release], ['最新快照', latest.snapshot]] as [label, v]}
          {#if v}
            <button class="lt" onclick={() => openVersion(v.id, '')}>
              <small>{t(label)}</small>
              <b>{v.id}</b>
              {#if installed.includes(v.id)}<i class="ok">{t('已安装')}</i>{/if}
            </button>
          {/if}
        {/each}
      </div>
      {#each groups as g (g.key)}
        <button class="cat" onclick={() => openCat(g.key)}>
          <span class="ct"><b>{seriesLabel(g.key)}</b><small>{t('共 {n} 个版本 · 最新 {id}', { n: g.list.length, id: g.list[0].id })}</small></span>
          <svg class="chev" width="6" height="10" viewBox="0 0 6 10" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M1 1l4 4-4 4" /></svg>
        </button>
      {/each}
      {#if !groups.length}<p class="empty">{t('没有匹配的版本')}</p>{/if}
    {/if}
  </div>
</section>

<style>
  .back { display: inline-flex; align-items: center; gap: 6px; height: 32px; padding: 0 14px 0 10px; border-radius: 16px; background: var(--fill); font-size: 13px; transition: background 0.2s; }
  .back:hover { background: var(--fill-hover); }
  .chev { opacity: 0.4; }
  .grow { flex: 1; }
  .dim { color: var(--text-dim); font-size: 13px; }

  /* 第一层 */
  .latest { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; padding: 4px 6px 12px; }
  .lt { display: flex; flex-direction: column; align-items: flex-start; gap: 2px; padding: 14px 18px; border-radius: 14px; background: var(--fill); text-align: left; transition: background 0.2s, transform 0.25s var(--ease); }
  .lt:hover { background: var(--fill-hover); transform: translateY(-1px); }
  .lt small { color: var(--text-dim); font-size: 12px; }
  .lt b { font-size: 20px; font-weight: 600; letter-spacing: -0.01em; }
  .lt .ok { font-style: normal; font-size: 12px; color: var(--text-dim); }
  .cat { display: flex; align-items: center; justify-content: space-between; width: 100%; min-height: 62px; padding: 0 20px; border-radius: 12px; text-align: left; transition: background 0.15s; }
  .cat:hover { background: var(--fill); }
  .ct { display: flex; flex-direction: column; gap: 1px; }
  .ct b { font-size: 16px; font-weight: 600; }
  .ct small { color: var(--text-dim); font-size: 12px; }

  /* 第三层：详情页，宽松排版 */
  .vlist.roomy { padding: 0; }
  .vd { display: flex; flex-direction: column; gap: 26px; padding: 30px 34px 24px; }
  .vd-hero { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; }
  .vd-hero h1 { font-size: 34px; font-weight: 700; letter-spacing: -0.02em; line-height: 1.15; }
  .vd-hero p { color: var(--text-dim); margin-top: 4px; }
  .vd-sec { display: flex; flex-direction: column; gap: 10px; }
  .vd-sec h3 { font-size: 15px; font-weight: 600; }
  .lgrid { display: grid; grid-template-columns: repeat(auto-fill, minmax(250px, 1fr)); gap: 12px; margin-top: 4px; }
  .lcard { display: flex; align-items: center; gap: 14px; padding: 14px 16px; border-radius: 14px; background: var(--fill); text-align: left; position: relative; transition: background 0.2s, box-shadow 0.2s, transform 0.25s var(--ease); }
  button.lcard:hover:not(:disabled) { background: var(--fill-hover); transform: translateY(-1px); }
  .lcard.on { background: var(--glass-solid); box-shadow: 0 0 0 2px var(--btn); }
  .lcard:disabled { opacity: 0.6; cursor: progress; }
  .ltxt { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .ltxt b { font-size: 15px; font-weight: 600; }
  .ltxt small { color: var(--text-dim); font-size: 12px; line-height: 1.4; }
  .tick { position: absolute; right: 14px; top: 14px; }
  .lcard .spin { position: absolute; right: 14px; top: 16px; }
  .lcard.off { opacity: 0.55; background: transparent; box-shadow: inset 0 0 0 1px var(--hairline); }
  .lcard.off :global(.licon) { filter: grayscale(1); }
  .lver { display: flex; align-items: center; gap: 14px; margin-top: 4px; }
  .vd-bar { display: flex; align-items: center; gap: 12px; padding: 16px 18px; border-radius: 16px; background: var(--fill); position: sticky; bottom: 0; backdrop-filter: blur(20px); flex-wrap: wrap; }
  .dname { font-size: 15px; font-weight: 600; font-variant-numeric: tabular-nums; }
  .warn { color: var(--danger); font-size: 12px; max-width: 320px; line-height: 1.4; }
  .okpill { font-style: normal; font-size: 12px; color: var(--text-dim); padding: 2px 10px; border-radius: 99px; background: var(--fill); }
  .mini.danger { color: var(--danger); }
  .primary { height: 36px; padding: 0 26px; border-radius: 18px; background: var(--btn); color: var(--btn-text); font-size: 14px; font-weight: 600; transition: transform 0.2s var(--ease), opacity 0.2s; }
  .primary:hover:not(:disabled) { transform: scale(1.04); }
  .primary:disabled { opacity: 0.35; cursor: not-allowed; }
</style>
