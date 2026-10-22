<script>
  import { untrack, tick } from 'svelte';
  import { call, pickFiles, inTauri, CANCELLED } from './api.js';
  import Select from './Select.svelte';
  import LoaderIcon from './LoaderIcon.svelte';
  import InstallDialog from './InstallDialog.svelte';
  import { renderMarkdown } from './markdown.js';
  import { fmtCount, ago, versionRange, size } from './format.js';
  import { LOADER_NAMES, MODRINTH_LOADER } from './loaders.js';
  import { t } from './i18n.svelte.js';

  // 模组与资源：从 Modrinth 搜索并安装模组、整合包、光影、资源包和数据包。
  let {
    panel = $bindable(null),
    instances,
    kind = $bindable('mod'),
    target = $bindable(''), // 装进哪个存档（版本 id）
    settings, // 返回随后端调用发送的设置
    working, // 正在启动游戏
    current = '', // 正在下载的任务名
    queued = [], // 排队中的任务名
    ring,
    onRun, // (label, fn) => Promise<{ ok, value } | { ok: false, error } | null>
    onChanged, // 装了整合包（多了一个存档）之后刷新存档列表
    onOpenInstance, // (id) => void，去看新装好的存档
    onGoDownload,
  } = $props();

  const SOURCE = 'Modrinth';
  const ALL = '*'; // “安装到”选“全部”：不限定存档
  const KINDS = [['mod', '模组'], ['modpack', '整合包'], ['shader', '光影'], ['resourcepack', '资源包'], ['datapack', '数据包']];
  const KIND_LABEL = Object.fromEntries(KINDS);

  // 每类内容常用的分类标签（Modrinth 的分类名）和对应的中文
  const TAG_LABEL = {
    optimization: '优化', technology: '科技', magic: '魔法', adventure: '冒险', decoration: '装饰', equipment: '装备', food: '食物', library: '前置库',
    mobs: '生物', storage: '存储', utility: '实用', worldgen: '世界生成', 'game-mechanics': '游戏机制', management: '管理', transportation: '交通',
    social: '社交', minigame: '小游戏', economy: '经济', cursed: '奇葩', challenging: '高难度', combat: '战斗', 'kitchen-sink': '大杂烩',
    lightweight: '轻量', multiplayer: '多人', quests: '任务', realistic: '写实', 'semi-realistic': '半写实', 'vanilla-like': '原版风', fantasy: '奇幻',
    cartoon: '卡通', atmosphere: '氛围', bloom: '泛光', 'colored-lighting': '彩色光照', 'path-tracing': '路径追踪', pbr: 'PBR', reflections: '反射',
    shadows: '阴影', foliage: '植被', potato: '土豆配置', low: '低性能消耗', medium: '中等性能消耗', high: '高性能消耗', '8x-': '8x 及以下',
    '16x': '16x', '32x': '32x', '64x': '64x', '128x': '128x', '256x': '256x', '512x+': '512x 及以上', simplistic: '简约', themed: '主题', gui: '界面',
    fonts: '字体', audio: '音效', items: '物品', blocks: '方块', entities: '实体', environment: '环境', models: '模型', tweaks: '微调',
    'core-shaders': '核心着色器', locale: '语言', modded: '模组适配',
  };
  const MOD_TAGS = ['optimization', 'technology', 'magic', 'adventure', 'decoration', 'equipment', 'food', 'library', 'mobs', 'storage', 'utility', 'worldgen', 'game-mechanics', 'management', 'transportation', 'social'];
  const TAGS = {
    mod: MOD_TAGS,
    datapack: MOD_TAGS,
    modpack: ['adventure', 'lightweight', 'optimization', 'technology', 'magic', 'combat', 'quests', 'multiplayer', 'challenging', 'kitchen-sink'],
    shader: ['realistic', 'semi-realistic', 'vanilla-like', 'fantasy', 'cartoon', 'potato', 'low', 'medium', 'high', 'shadows', 'reflections', 'path-tracing', 'colored-lighting'],
    resourcepack: ['16x', '32x', '64x', '128x', '256x', 'vanilla-like', 'realistic', 'simplistic', 'themed', 'gui', 'fonts', 'audio', 'combat', 'decoration', 'items', 'blocks'],
  };
  const SORTS = [['auto', '推荐'], ['downloads', '下载量'], ['follows', '关注数'], ['updated', '最近更新'], ['newest', '最新发布']];
  const VTYPE = { release: '正式', beta: '测试', alpha: '预览' };

  // ---- 某个存档装不装得了某个内容 ----
  // item 是搜索结果或某个具体版本：{ gameVersions, loaders }。适配返回 ''，否则返回不适配的原因。
  const isRelease = (v) => /^\d+(\.\d+)+$/.test(v);
  const acceptedLoaders = (l) => (l === 'quilt' ? ['quilt', 'fabric'] : l === 'legacyfabric' ? ['legacy-fabric'] : [l]);
  function compatOf(i, item, k = kind) {
    if (k === 'mod' && !i.loader) return t('没有模组加载器');
    // 快照这类非正式版本号，搜索结果里只列正式版，没法判断，不拦
    if (item.gameVersions?.length && isRelease(i.mc) && !item.gameVersions.includes(i.mc)) return t('不支持 {v}', { v: i.mc });
    if (k === 'mod' && item.loaders?.length && !item.loaders.some((l) => acceptedLoaders(i.loader).includes(l))) return t('不支持 {name}', { name: LOADER_NAMES[i.loader] ?? i.loader });
    return '';
  }
  let picker = $state(null); // 正在选“装到哪些存档”：{ h, ver }

  let query = $state('');
  let sortKey = $state('auto');
  let cats = $state([]);
  let compat = $state(true); // 只显示适配当前存档的
  let world = $state('');
  let worlds = $state([]);
  let page = $state({ hits: [], total: 0 });
  let loading = $state(false);
  let error = $state('');
  let msg = $state(null); // { k: 'ok' | 'warn' | 'err', text }
  let installedMap = $state({});
  let view = $state({ level: 0, hit: null });
  let detail = $state(null);
  let detailState = $state('idle'); // idle | loading | err
  let vers = $state([]);
  let versAll = $state(false);
  let versState = $state('idle');

  const all = $derived(target === ALL);
  const inst = $derived(all ? undefined : instances.find((i) => i.id === target));
  // 选“全部”时，装进所有装得了的存档：模组要有加载器，资源包和光影都行
  const targets = $derived(all ? instances.filter((i) => kind !== 'mod' || i.loader) : inst ? [inst] : []);
  const needsTarget = $derived(kind !== 'modpack');
  const filterMc = $derived(compat && needsTarget && inst ? inst.mc : '');
  const filterLoader = $derived(compat && inst && (kind === 'mod') ? inst.loader : '');
  const filterMcs = $derived(compat && all && needsTarget ? [...new Set(targets.map((i) => i.mc))] : []);
  const filterLoaders = $derived(compat && all && kind === 'mod' ? [...new Set(targets.map((i) => i.loader))] : []);
  const instOptions = $derived([[ALL, t('全部')], ...instances.map((i) => [i.id, i.title])]);
  const q = $derived(query.trim());
  const effSort = $derived(sortKey === 'auto' ? (q ? 'relevance' : 'downloads') : sortKey);
  const noLoader = $derived(kind === 'mod' && inst && !inst.loader);
  const compatLabel = $derived(inst ? `${inst.mc}${kind === 'mod' && inst.loader ? ` · ${LOADER_NAMES[inst.loader] ?? inst.loader}` : ''}` : '');

  // 目标默认是“全部”；指定的存档被删了也回到“全部”
  $effect(() => {
    if (target !== ALL && !instances.some((i) => i.id === target)) target = ALL;
  });

  // ---- 搜索（输入停顿 0.35 秒后才发请求；结果按请求序号去重，避免旧结果覆盖新结果） ----
  let token = 0;
  let timer;
  async function load(offset = 0) {
    const mine = ++token;
    loading = true;
    error = '';
    try {
      const r = await call('content_search', { args: { kind, query: q, mc: filterMc || null, loader: filterLoader || null, mcs: filterMcs, loaders: filterLoaders, categories: cats, sort: effSort, offset }, settings: settings() });
      if (mine !== token) return;
      page = offset ? { hits: [...page.hits, ...r.hits], total: r.total } : { hits: r.hits, total: r.total };
    } catch (e) {
      if (mine === token) error = String(e);
    } finally {
      if (mine === token) loading = false;
    }
  }
  $effect(() => {
    const key = JSON.stringify([kind, q, effSort, cats, filterMc, filterLoader, filterMcs, filterLoaders]);
    void key;
    clearTimeout(timer);
    timer = setTimeout(() => untrack(() => load(0)), q ? 350 : 0);
    return () => clearTimeout(timer);
  });
  // 换了类型就清掉不适用的分类
  $effect(() => { kind; cats = []; });

  // ---- 数据包要选世界；已安装标记 ----
  $effect(() => {
    const [id, k] = [target, kind];
    if (k !== 'datapack' || !id) { worlds = []; return; }
    call('list_worlds', { id, settings: settings() }).then((w) => { worlds = w; if (!w.includes(world)) world = w[0] ?? ''; }).catch(() => (worlds = []));
  });
  let installedBy = $state({}); // 全部模式：存档 id → 那个存档里已装的项目
  async function refreshInstalled() {
    if (all && needsTarget && kind !== 'datapack') {
      try {
        const maps = await Promise.all(targets.map((i) => call('content_installed', { instance: i.id, kind, world: null, settings: settings() })));
        installedBy = Object.fromEntries(targets.map((i, n) => [i.id, maps[n]]));
      } catch {
        installedBy = {};
      }
      installedMap = {};
      return;
    }
    installedBy = {};
    if (!target || all || !needsTarget || (kind === 'datapack' && !world)) { installedMap = {}; return; }
    try {
      installedMap = await call('content_installed', { instance: target, kind, world: world || null, settings: settings() });
    } catch {
      installedMap = {};
    }
  }
  $effect(() => { [target, kind, world, targets.length]; refreshInstalled(); });
  // 这个项目算不算“已安装”：指定存档就看那个存档；全部模式下，所有适配的存档都装了才算
  function isInstalled(h) {
    if (!all) return installedMap[h.id];
    const fit = targets.filter((i) => !compatOf(i, h));
    return fit.length > 0 && fit.every((i) => installedBy[i.id]?.[h.id]);
  }

  // ---- 安装 ----
  function blocked(h) {
    if (kind === 'modpack') return '';
    if (all) {
      if (!instances.length) return t('还没有存档，先去下载游戏');
      if (kind === 'mod' && !targets.length) return t('没有带模组加载器的存档，装不了模组');
      return ''; // 具体装到哪些存档，在弹出的对话框里选
    }
    if (!inst) return t('还没有存档，先去下载游戏');
    if (kind === 'mod' && !inst.loader) return t('这是原版存档，没有模组加载器，装不了模组');
    if (kind === 'datapack' && !world) return t('这个存档还没有世界，进游戏创建一个再来装数据包');
    return compatOf(inst, h);
  }
  // 某个具体版本能不能装：指定存档时要看这个版本适不适配
  const verBlocked = (v) => blocked(view.hit) || (inst ? compatOf(inst, v) : '');

  /** 点“安装”：选了“全部”就先让玩家选装到哪些存档；指定了存档就直接装。ver 是具体版本，不传就装最新适配版。 */
  function install(h, ver = null) {
    if (kind === 'modpack') return runInstall(h, [], ver?.id ?? null);
    const why = ver ? verBlocked(ver) : blocked(h);
    if (why) { msg = { k: 'warn', text: why }; return; }
    if (all) { picker = { h, ver }; return; }
    return runInstall(h, [{ id: target, world: world || null }], ver?.id ?? null);
  }

  /** picks: [{ id, world }]，要装进去的存档。 */
  async function runInstall(h, picks, versionId = null) {
    msg = null;
    const one = (i, v, w) => call('content_install', { args: { instance: i, kind, project: h.id, title: h.title, version: v, world: w }, settings: settings() });
    const res = await onRun(h.title, async (ctl) => {
      if (kind === 'modpack') return call('modpack_install', { project: h.id, version: versionId, title: h.title, settings: settings() });
      if (picks.length === 1) return one(picks[0].id, versionId, picks[0].world);
      // 装进选中的每个存档；某个存档装失败不影响其他的
      const ok = [];
      const failed = [];
      for (const p of picks) {
        if (ctl.isCancelled()) throw CANCELLED;
        try {
          ok.push(await one(p.id, versionId, p.world));
        } catch (e) {
          if (String(e) === CANCELLED) throw e;
          failed.push(`${instances.find((i) => i.id === p.id)?.title ?? p.id}：${e}`);
        }
      }
      return { many: true, ok: ok.length, failed };
    });
    if (!res) { msg = { k: 'warn', text: t('已经在下载队列里了') }; return; }
    if (!res.ok) { msg = res.cancelled ? { k: 'warn', text: t('已取消') } : { k: 'err', text: t('安装失败：{err}', { err: res.error }) }; return; }
    if (kind === 'modpack') {
      await onChanged();
      msg = { k: 'ok', text: t('整合包“{name}”已安装为新的存档。', { name: h.title }), open: res.value };
    } else if (res.value.many) {
      const { ok, failed } = res.value;
      let text = ok ? t('已把 {name} 安装到 {n} 个存档', { name: h.title, n: ok }) : '';
      if (failed.length) text += `${text ? ' ' : ''}${t('{n} 个存档安装失败：{list}', { n: failed.length, list: failed.join('；') })}`;
      msg = { k: failed.length ? (ok ? 'warn' : 'err') : 'ok', text };
      await refreshInstalled();
    } else {
      const r = res.value;
      let text = t('已安装 {name}', { name: h.title });
      if (r.dependencies.length) text += ' ' + t('（同时自动安装了 {n} 个前置：{list}）', { n: r.dependencies.length, list: r.dependencies.join('、') });
      msg = { k: r.warnings.length ? 'warn' : 'ok', text: [text, ...r.warnings].join(' ') };
      await refreshInstalled();
    }
  }

  async function importPack() {
    const [path] = await pickFiles([{ name: t('Modrinth 整合包'), extensions: ['mrpack'] }]);
    if (!path) return;
    msg = null;
    const res = await onRun(path.split(/[\\/]/).pop(), () => call('modpack_import', { path, settings: settings() }));
    if (!res) return;
    if (!res.ok) { msg = res.cancelled ? { k: 'warn', text: t('已取消') } : { k: 'err', text: t('导入失败：{err}', { err: res.error }) }; return; }
    await onChanged();
    msg = { k: 'ok', text: t('整合包已导入为新的存档。'), open: res.value };
  }

  // ---- 详情页 ----
  let listScroll = 0; // 进详情页之前列表滚到了哪里，返回时还原
  async function openDetail(h) {
    listScroll = panel?.scrollTop ?? 0;
    view = { level: 1, hit: h };
    tick().then(() => { if (panel) panel.scrollTop = 0; });
    detail = null;
    detailState = 'loading';
    vers = [];
    versAll = false;
    versState = 'loading';
    try {
      detail = await call('content_project', { id: h.id, settings: settings() });
      detailState = 'idle';
    } catch (e) {
      detailState = 'err';
      msg = { k: 'err', text: String(e) };
    }
    loadVersions(h);
  }
  async function loadVersions(h) {
    versState = 'loading';
    try {
      const all = versAll || !filterMc;
      vers = await call('content_versions', { project: h.id, kind, mc: all ? null : filterMc, loader: all ? null : (filterLoader || null), settings: settings() });
      versState = 'idle';
    } catch (e) {
      versState = 'err';
    }
  }
  // 详情页：这个模组会带上哪些前置
  let deps = $state([]);
  let depsState = $state('idle'); // idle | loading | err
  $effect(() => {
    const [lvl, hit, id, k] = [view.level, view.hit, target, kind];
    deps = [];
    if (lvl !== 1 || !hit || k !== 'mod' || !id || id === ALL) { depsState = 'idle'; return; }
    const ins = instances.find((i) => i.id === id);
    if (!ins?.loader) { depsState = 'idle'; return; }
    depsState = 'loading';
    let alive = true;
    call('content_dependencies', { args: { instance: id, kind: 'mod', project: hit.id, title: '', version: null, world: null }, settings: settings() })
      .then((r) => { if (alive) { deps = r; depsState = 'idle'; } })
      .catch(() => { if (alive) depsState = 'err'; });
    return () => { alive = false; };
  });
  const DEP_STATUS = { installed: '已安装', auto: '将自动安装', missing: '没有适配版本', optional: '可选' };

  const back = () => {
    view = { level: 0, hit: null };
    tick().then(() => { if (panel) panel.scrollTop = listScroll; });
  };
  // Esc 返回上一级（页面是常驻的，隐藏着的时候不响应）
  function onKey(e) {
    if (e.key === 'Escape' && view.level === 1 && !picker && panel?.offsetParent) back();
  }
  function openLink(e) {
    const a = e.target.closest?.('a[data-href]');
    if (!a) return;
    e.preventDefault();
    openUrl(a.dataset.href);
  }
  function openUrl(url) {
    if (inTauri) call('open_url', { url }).catch((e) => (msg = { k: 'err', text: String(e) }));
    else window.open(url, '_blank', 'noopener');
  }

  const toggleCat = (c) => { cats = cats.includes(c) ? cats.filter((x) => x !== c) : [...cats, c]; };
  const loaderKeys = (h) => [...new Set(h.loaders.map((l) => MODRINTH_LOADER[l]).filter(Boolean))];
  const tagText = (h) => h.tags.filter((x) => TAG_LABEL[x]).slice(0, 3).map((x) => t(TAG_LABEL[x])).join(' · ');
  const installedFile = (id) => installedMap[id];
  const done = $derived(Math.round(ring * 100));
</script>

<svelte:window onkeydown={onKey} />

<section class="versions" aria-label={t('模组与资源')}>
  {#if view.level === 0}
    <div class="vhead">
      <h2>{t('模组与资源')}</h2>
      <input class="search wide" placeholder={t('搜索 {kind}', { kind: t(KIND_LABEL[kind]) })} bind:value={query} spellcheck="false" />
    </div>
    <div class="seg kinds" role="tablist">
      {#each KINDS as [k, label]}
        <button role="tab" class:on={kind === k} aria-selected={kind === k} onclick={() => (kind = k)}>{t(label)}</button>
      {/each}
    </div>

    <div class="filters">
      {#if needsTarget}
        <span class="fl"><span class="dim">{t('安装到')}</span>
          {#if instances.length}<Select bind:value={target} options={instOptions} label={t('安装到')} />{:else}<button class="mini" onclick={onGoDownload}>{t('先去下载游戏')}</button>{/if}
        </span>
        {#if kind === 'datapack' && !all && worlds.length}
          <span class="fl"><span class="dim">{t('世界')}</span><Select bind:value={world} options={worlds.map((w) => [w, w])} label={t('世界')} /></span>
        {/if}
        {#if inst || (all && instances.length)}
          <label class="switch" title={t('关闭后会显示所有版本的内容，其中不少可能装不上')}>
            <span>{all ? t('只看适配我的存档') : t('只看适配 {v}', { v: compatLabel })}</span>
            <input type="checkbox" bind:checked={compat} />
            <i aria-hidden="true"></i>
          </label>
        {/if}
      {:else if inTauri}
        <button class="mini" onclick={importPack}>{t('导入 .mrpack 整合包')}</button>
      {/if}
      <span class="grow"></span>
      <span class="fl"><span class="dim">{t('排序')}</span><Select bind:value={sortKey} options={SORTS.map(([v, l]) => [v, t(l)])} label={t('排序')} /></span>
    </div>

    <div class="chips" role="group" aria-label={t('分类')}>
      <button class="chip" class:on={!cats.length} onclick={() => (cats = [])}>{t('全部')}</button>
      {#each TAGS[kind] as c}
        <button class="chip" class:on={cats.includes(c)} onclick={() => toggleCat(c)} aria-pressed={cats.includes(c)}>{t(TAG_LABEL[c])}</button>
      {/each}
    </div>

    {#if noLoader}
      <p class="note warn">{t('这是原版存档，没有模组加载器，装不了模组。')} <button class="link" onclick={onGoDownload}>{t('去安装加载器')}</button></p>
    {/if}
    {#if msg}
      <p class="note {msg.k}">
        {msg.text}
        {#if msg.open}<button class="link" onclick={() => onOpenInstance(msg.open)}>{t('去看看')}</button>{/if}
        <button class="link x" onclick={() => (msg = null)} aria-label={t('关闭')}>×</button>
      </p>
    {/if}

    <div class="glass vlist results" bind:this={panel}>
      {#if error}
        <div class="empty err"><p>{t('加载失败：{err}', { err: error })}</p><button class="mini" onclick={() => load(0)}>{t('重试')}</button></div>
      {:else if loading && !page.hits.length}
        <div class="skel" role="status" aria-label={t('正在搜索…')}>
          {#each [0, 1, 2, 3, 4] as n}
            <div class="rc sk" style="--i:{n}">
              <span class="ico"></span>
              <div class="main"><b class="skl" style="width:34%"></b><i class="skl" style="width:22%"></i><i class="skl" style="width:88%"></i><i class="skl" style="width:56%"></i></div>
            </div>
          {/each}
        </div>
      {:else if !page.hits.length}
        <p class="empty">{t('没有找到符合条件的内容。试试换个关键词，或关掉“只看适配”。')}</p>
      {:else}
        {#each page.hits as h, n (h.id)}
          {@const have = isInstalled(h)}
          <div class="rc" style="--i:{n % 12}" role="button" tabindex="0" onclick={() => openDetail(h)} onkeydown={(e) => { if (e.key === 'Enter') openDetail(h); }}>
            {#if h.icon}<img class="ico" src={h.icon} alt="" loading="lazy" />{:else}<span class="ico blank" aria-hidden="true"></span>{/if}
            <div class="main">
              <b class="nm">{h.title}</b>
              <span class="type">{t(KIND_LABEL[kind])}{tagText(h) ? ` · ${tagText(h)}` : ''} · {h.author}</span>
              <p class="sum">{h.summary}</p>
              <div class="meta">
                {#each loaderKeys(h) as l}<span class="ld"><LoaderIcon kind={l} size={14} />{LOADER_NAMES[l]}</span>{/each}
                {#if h.gameVersions.length}<span>{versionRange(h.gameVersions)}</span>{/if}
                <span class="dot">·</span><span title={t('下载量')}>↓ {fmtCount(h.downloads)}</span>
                <span class="dot">·</span><span>{ago(h.updated)}</span>
              </div>
            </div>
            <div class="rside">
              <span class="src">{SOURCE}</span>
              {#if have}
                <i class="okpill">{t('已安装')}</i>
              {:else if current === h.title}
                <span class="dlp"><b>{done}%</b></span>
              {:else if queued.includes(h.title)}
                <span class="dlp">{t('排队中')}</span>
              {:else}
                <button class="primary sm" disabled={!!blocked(h)} title={blocked(h)} onclick={(e) => { e.stopPropagation(); install(h); }}>{kind === 'modpack' ? t('安装为存档') : all ? t('安装到…') : t('安装')}</button>
              {/if}
            </div>
          </div>
        {/each}
        {#if page.hits.length < page.total}
          <button class="more" disabled={loading} onclick={() => load(page.hits.length)}>{loading ? t('加载中…') : t('加载更多（已显示 {a} / {b}）', { a: page.hits.length, b: page.total })}</button>
        {:else}
          <p class="end">{t('共 {n} 个结果，已全部显示', { n: page.total })}</p>
        {/if}
      {/if}
    </div>

  {:else}
    <!-- 详情 -->
    <div class="vhead">
      <button class="back" onclick={back} aria-label={t('返回')}>
        <svg width="9" height="15" viewBox="0 0 9 15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M7.5 1.5L2 7.5l5.5 6" /></svg>
        {t('返回')}
      </button>
      <h2 class="ttl">{view.hit.title}</h2>
    </div>
    {#if msg}<p class="note {msg.k}">{msg.text}<button class="link x" onclick={() => (msg = null)} aria-label={t('关闭')}>×</button></p>{/if}
    <div class="glass vlist roomy" bind:this={panel}>
      <div class="dt">
        <div class="hero">
          {#if view.hit.icon}<img class="ico big" src={view.hit.icon} alt="" />{:else}<span class="ico big blank" aria-hidden="true"></span>{/if}
          <div class="htxt">
            <h1>{view.hit.title}</h1>
            <span class="type">{t(KIND_LABEL[kind])} · {detail?.authors?.length ? detail.authors.join('、') : view.hit.author}</span>
            <p class="sum">{view.hit.summary}</p>
            <div class="meta">
              {#each loaderKeys(view.hit) as l}<span class="ld"><LoaderIcon kind={l} size={14} />{LOADER_NAMES[l]}</span>{/each}
              {#if view.hit.gameVersions.length}<span>{versionRange(view.hit.gameVersions)}</span>{/if}
              <span class="dot">·</span><span>↓ {fmtCount(view.hit.downloads)}</span>
              <span class="dot">·</span><span>{ago(view.hit.updated)}</span>
            </div>
          </div>
          <div class="hbtn">
            <span class="src">{SOURCE}</span>
            {#if isInstalled(view.hit)}<i class="okpill">{t('已安装')}</i>
            {:else}<button class="primary" disabled={!!blocked(view.hit)} title={blocked(view.hit)} onclick={() => install(view.hit)}>{current === view.hit.title ? `${done}%` : queued.includes(view.hit.title) ? t('排队中') : kind === 'modpack' ? t('安装为存档') : all ? t('安装到…') : t('安装最新适配版')}</button>{/if}
            {#if detail}<button class="mini" onclick={() => openUrl(detail.url)}>{t('在浏览器中打开')}</button>{/if}
          </div>
        </div>

        {#if kind === 'mod' && !all && inst?.loader}
          <section class="sec">
            <h3>{t('前置模组')}</h3>
            {#if depsState === 'loading'}
              <p class="dim">{t('正在查询前置…')}</p>
            {:else if depsState === 'err'}
              <p class="dim">{t('暂时查不到前置信息')}</p>
            {:else if !deps.length}
              <p class="dim">{t('这个模组不需要额外的前置。')}</p>
            {:else}
              <p class="dim small">{t('安装时会自动补齐必需的前置，可选的不会自动安装。')}</p>
              <div class="deps">
                {#each deps as d (d.project)}
                  <div class="dep">
                    {#if d.icon}<img src={d.icon} alt="" loading="lazy" />{:else}<span class="dico" aria-hidden="true"></span>{/if}
                    <b>{d.title}</b>
                    <i class="dst {d.status}">{d.status === 'auto' ? t('将自动安装 {v}', { v: d.version }) : t(DEP_STATUS[d.status])}</i>
                  </div>
                {/each}
              </div>
            {/if}
          </section>
        {/if}

        {#if detail?.gallery?.length}
          <div class="gallery">{#each detail.gallery.slice(0, 3) as g}<img src={g} alt="" loading="lazy" />{/each}</div>
        {/if}

        <section class="sec">
          <h3>{t('介绍')}</h3>
          {#if detailState === 'loading'}<p class="dim">{t('加载中…')}</p>
          {:else if detail}
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
            <div class="md" onclick={openLink}>{@html renderMarkdown(detail.body || detail.summary)}</div>
            <p class="dim small">
              {#if detail.license}{t('许可证')}：{detail.license} · {/if}{t('客户端')} {detail.clientSide} · {t('服务端')} {detail.serverSide}
              {#if detail.sourceUrl} · <button class="link" onclick={() => openUrl(detail.sourceUrl)}>{t('源代码')}</button>{/if}
              {#if detail.issuesUrl} · <button class="link" onclick={() => openUrl(detail.issuesUrl)}>{t('问题反馈')}</button>{/if}
            </p>
          {/if}
        </section>

        <section class="sec">
          <div class="vtop">
            <h3>{t('版本')}</h3>
            {#if filterMc}
              <label class="switch"><span>{t('显示全部版本')}</span><input type="checkbox" bind:checked={versAll} onchange={() => loadVersions(view.hit)} /><i aria-hidden="true"></i></label>
            {/if}
          </div>
          {#if versState === 'loading'}<p class="dim">{t('加载中…')}</p>
          {:else if !vers.length}<p class="dim">{filterMc ? t('没有适配 {v} 的版本。可以打开“显示全部版本”看看。', { v: compatLabel }) : t('没有可下载的版本。')}</p>
          {:else}
            <div class="vers">
              {#each vers.slice(0, 40) as v, n (v.id)}
                <div class="vrow" style="--i:{Math.min(n, 12)}">
                  <span class="vname"><b>{v.name || v.number}</b><small>{v.number}</small></span>
                  <i class="vt {v.kind}">{t(VTYPE[v.kind] ?? v.kind)}</i>
                  <span class="vmc">{versionRange(v.gameVersions.filter((g) => /^\d+(\.\d+)+$/.test(g))) || v.gameVersions[0]}</span>
                  <span class="vld">{v.loaders.map((l) => LOADER_NAMES[MODRINTH_LOADER[l]] ?? l).join(' ')}</span>
                  <span class="vsz">{v.files[0] ? size(v.files[0].size) : ''}</span>
                  <span class="vdt">{v.date.slice(0, 10)}</span>
                  <button class="mini" disabled={!!verBlocked(v)} title={verBlocked(v)} onclick={() => install(view.hit, v)}>{all ? t('安装到…') : t('安装')}</button>
                </div>
              {/each}
            </div>
          {/if}
        </section>
      </div>
    </div>
  {/if}
</section>

{#if picker}
  <InstallDialog
    title={picker.h.title}
    subtitle={picker.ver ? picker.ver.name || picker.ver.number : ''}
    {kind}
    projectId={picker.h.id}
    item={picker.ver ?? picker.h}
    {instances}
    {settings}
    {compatOf}
    onConfirm={(p) => { const pk = picker; picker = null; runInstall(pk.h, p, pk.ver?.id ?? null); }}
    onClose={() => (picker = null)}
  />
{/if}

<style>
  .grow { flex: 1; }
  .dim { color: var(--text-dim); font-size: 13px; }
  .dim.small { font-size: 12px; margin-top: 10px; }
  .search.wide { width: min(360px, 40vw); }
  .kinds { margin-top: -2px; }
  .filters { display: flex; align-items: center; gap: 14px; flex-wrap: wrap; padding: 0 4px; }
  .fl { display: inline-flex; align-items: center; gap: 8px; }
  .chips { display: flex; gap: 6px; overflow-x: auto; padding: 2px 4px 6px; scrollbar-width: none; }
  .chips::-webkit-scrollbar { display: none; }
  .chip { flex: none; height: 26px; padding: 0 12px; border-radius: 13px; background: var(--fill); font-size: 12px; color: var(--text-dim); transition: background 0.2s, color 0.2s; }
  .chip:hover { background: var(--fill-hover); color: var(--text); }
  .chip.on { background: var(--btn); color: var(--btn-text); }
  .note { display: flex; align-items: center; gap: 10px; padding: 8px 14px; border-radius: 12px; background: var(--fill); font-size: 13px; line-height: 1.45; user-select: text; }
  .note.ok { color: var(--text); }
  .note.warn { color: #b26a00; }
  .note.err { color: var(--danger); }
  .link { color: var(--blue); font-size: inherit; padding: 0; }
  .link:hover { text-decoration: underline; }
  .link.x { margin-left: auto; font-size: 18px; line-height: 1; color: var(--text-dim); }

  /* 结果卡片：图标 | 名称 / 类型 / 简介 / 底部信息 | 来源与安装 */
  .results { padding: 8px; gap: 6px; margin-top: 6px; }
  .rc { display: flex; align-items: flex-start; gap: 14px; padding: 12px 14px; border-radius: 14px; text-align: left; cursor: pointer; transition: background 0.15s; }
  .rc:hover { background: var(--fill); }
  .rc:focus-visible { outline: 2px solid var(--blue); outline-offset: -2px; }
  .ico { width: 56px; height: 56px; border-radius: 12px; object-fit: cover; flex: none; background: var(--fill); }
  .ico.blank { box-shadow: inset 0 0 0 1px var(--hairline); }
  .ico.big { width: 84px; height: 84px; border-radius: 16px; }
  .main { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .nm { font-size: 15px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .type { font-size: 12px; color: var(--text-dim); }
  .sum { font-size: 13px; line-height: 1.45; color: var(--text); opacity: 0.85; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; margin-top: 2px; }
  .meta { display: flex; align-items: center; flex-wrap: wrap; gap: 4px 10px; margin-top: 5px; font-size: 12px; color: var(--text-dim); font-variant-numeric: tabular-nums; }
  .ld { display: inline-flex; align-items: center; gap: 4px; }
  .dot { opacity: 0.5; }
  .rside { display: flex; flex-direction: column; align-items: flex-end; gap: 8px; flex: none; min-width: 84px; }
  .src { font-size: 11px; color: var(--text-dim); padding: 1px 9px; border-radius: 99px; background: var(--fill); }
  .okpill { font-style: normal; font-size: 12px; color: var(--text-dim); padding: 2px 10px; border-radius: 99px; background: var(--fill); }
  .dlp { font-size: 12px; color: var(--text-dim); }
  .primary { height: 36px; padding: 0 22px; border-radius: 18px; background: var(--btn); color: var(--btn-text); font-size: 14px; font-weight: 600; transition: transform 0.2s var(--ease), opacity 0.2s; }
  .primary.sm { height: 30px; padding: 0 16px; font-size: 13px; border-radius: 15px; }
  .primary:hover:not(:disabled) { transform: scale(1.04); }
  .primary:disabled { opacity: 0.35; cursor: not-allowed; }
  .more { margin: 8px auto 4px; height: 34px; padding: 0 22px; border-radius: 17px; background: var(--fill); font-size: 13px; }
  .more:hover:not(:disabled) { background: var(--fill-hover); }
  .end { text-align: center; color: var(--text-dim); font-size: 12px; padding: 8px 0 4px; }
  .empty { display: flex; flex-direction: column; align-items: center; gap: 12px; text-align: center; padding: 0 20px; }

  /* 详情 */
  .back { display: inline-flex; align-items: center; gap: 6px; height: 32px; padding: 0 14px 0 10px; border-radius: 16px; background: var(--fill); font-size: 13px; transition: background 0.2s; flex: none; }
  .back:hover { background: var(--fill-hover); }
  .ttl { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .roomy { padding: 0; }
  .dt { display: flex; flex-direction: column; gap: 24px; padding: 26px 30px 28px; }
  .hero { display: flex; gap: 18px; align-items: flex-start; }
  .htxt { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px; }
  .htxt h1 { font-size: 26px; font-weight: 700; letter-spacing: -0.02em; line-height: 1.2; }
  .hbtn { display: flex; flex-direction: column; align-items: flex-end; gap: 8px; flex: none; }
  .deps { display: flex; flex-direction: column; border-radius: 14px; background: var(--fill); }
  .dep { display: flex; align-items: center; gap: 12px; min-height: 46px; padding: 6px 14px; }
  .dep + .dep { border-top: 1px solid var(--hairline); }
  .dep img, .dico { width: 28px; height: 28px; border-radius: 7px; object-fit: cover; flex: none; background: var(--fill-hover); }
  .dep b { flex: 1; min-width: 0; font-size: 13.5px; font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dst { font-style: normal; font-size: 12px; padding: 2px 10px; border-radius: 99px; background: var(--fill-hover); color: var(--text-dim); flex: none; }
  .dst.auto { background: var(--btn); color: var(--btn-text); }
  .dst.installed { color: #1f8a3c; }
  .dst.missing { color: var(--danger); }
  .gallery { display: flex; gap: 10px; }
  .gallery img { height: 120px; border-radius: 12px; max-width: 32%; object-fit: cover; background: var(--fill); }
  .sec { display: flex; flex-direction: column; gap: 10px; }
  .sec h3 { font-size: 15px; font-weight: 600; }
  .vtop { display: flex; align-items: center; justify-content: space-between; }
  .md { font-size: 13.5px; line-height: 1.65; user-select: text; max-height: 360px; overflow: auto; padding-right: 8px; }
  .md :global(p) { margin: 0 0 8px; }
  .md :global(h2), .md :global(h3), .md :global(h4), .md :global(h5) { margin: 14px 0 6px; font-size: 15px; font-weight: 600; }
  .md :global(ul), .md :global(ol) { margin: 0 0 8px; padding-left: 22px; }
  .md :global(code) { font: 12px 'Cascadia Mono', Consolas, monospace; background: var(--fill); padding: 1px 5px; border-radius: 5px; }
  .md :global(pre) { font: 12px/1.5 'Cascadia Mono', Consolas, monospace; background: var(--fill); padding: 10px 12px; border-radius: 10px; overflow: auto; margin: 0 0 8px; }
  .md :global(blockquote) { margin: 0 0 8px; padding-left: 12px; border-left: 3px solid var(--hairline); color: var(--text-dim); }
  .md :global(a) { color: var(--blue); }
  .md :global(hr) { border: 0; border-top: 1px solid var(--hairline); margin: 12px 0; }
  .vers { display: flex; flex-direction: column; border-radius: 14px; background: var(--fill); }
  .vrow { display: grid; grid-template-columns: minmax(120px, 1.6fr) 44px minmax(70px, 0.8fr) minmax(90px, 1.1fr) 64px 82px auto; align-items: center; gap: 10px; min-height: 48px; padding: 6px 14px; font-size: 12px; color: var(--text-dim); }
  .vrow + .vrow { border-top: 1px solid var(--hairline); }
  .vname { display: flex; flex-direction: column; min-width: 0; line-height: 1.3; }
  .vname b { font-size: 13px; font-weight: 500; color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .vname small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .vt { font-style: normal; text-align: center; padding: 1px 0; border-radius: 99px; background: var(--fill); }
  .vt.release { color: #1f8a3c; }
  .vt.beta { color: #b26a00; }
  .vt.alpha { color: var(--danger); }
  .vld { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .vsz, .vdt { font-variant-numeric: tabular-nums; }
</style>
