<script>
  import { onDestroy, untrack } from 'svelte';
  import { call, pickFiles, inTauri } from './api.js';
  import InstanceIcon from './InstanceIcon.svelte';
  import Select from './Select.svelte';
  import { BLOCK_KEYS } from './blocks.js';
  import { readIconFile } from './skins.svelte.js';
  import { size } from './format.js';
  import { t } from './i18n.svelte.js';
  import { LOADER_NAMES } from './loaders.js';

  // 单个存档的设置页：个性化、模组管理、游戏设置、其他。改动会自动保存。
  let {
    panel = $bindable(null),
    inst,
    settings, // 返回随后端调用发送的设置
    onBack,
    onChanged, // () => Promise，重新读取存档列表
    onDelete, // (id) => Promise<string>
    onDownload,
    onBrowse, // (kind, id) => void，去下载页并预选类型和这个存档
    onRun, // (label, fn) => Promise，放进下载队列
    onDeleted,
  } = $props();

  const id = untrack(() => inst.id);
  const NAMES = LOADER_NAMES;
  const TABS = [['look', '个性化'], ['mods', '模组与资源'], ['game', '游戏设置'], ['more', '更多']];
  const ICONS = BLOCK_KEYS.map((k) => `preset:${k}`);

  let tab = $state('look');
  const start = untrack(() => inst);
  const base = { component: start.component, lastPlayed: start.lastPlayed };
  let draft = $state({
    name: start.name, icon: start.icon, note: start.note, memMb: start.memMb, java: start.java,
    jvmArgs: start.jvmArgs, width: start.width, height: start.height,
  });
  let customUrl = $state(start.iconUrl);
  let saveError = $state('');

  const autoTitle = $derived(inst.loader ? `${inst.mc} · ${NAMES[inst.loader]} ${inst.loaderVersion}` : inst.mc);
  const num = (v) => (Number(v) > 0 ? Math.round(Number(v)) : null);
  const payload = () => ({
    ...base,
    name: draft.name.trim(),
    icon: draft.icon,
    note: draft.note,
    memMb: num(draft.memMb),
    java: draft.java.trim(),
    jvmArgs: draft.jvmArgs.trim(),
    width: num(draft.width) && num(draft.height) ? num(draft.width) : null,
    height: num(draft.width) && num(draft.height) ? num(draft.height) : null,
  });

  // 自动保存：停手 0.4 秒后写入；离开页面时把还没写的立刻写掉。
  let lastSaved = JSON.stringify(untrack(payload));
  let pending = null;
  let timer;

  async function flush() {
    clearTimeout(timer);
    const body = pending;
    pending = null;
    if (!body) return;
    try {
      await call('save_instance', { id, meta: body, settings: settings() });
      lastSaved = JSON.stringify(body);
      saveError = '';
      await onChanged();
    } catch (e) {
      saveError = String(e);
    }
  }

  $effect(() => {
    const body = payload();
    if (JSON.stringify(body) === lastSaved) return;
    pending = body;
    clearTimeout(timer);
    timer = setTimeout(flush, 400);
  });
  onDestroy(flush);

  async function onIconFile(e) {
    const file = e.currentTarget.files?.[0];
    e.currentTarget.value = '';
    if (!file) return;
    try {
      const data = await readIconFile(file);
      await call('set_instance_icon', { id, data, settings: settings() });
      customUrl = data;
      draft.icon = 'custom';
      lastSaved = JSON.stringify({ ...payload() }); // 后端已经把 icon 写成 custom，不必再存一遍
      saveError = '';
      await onChanged();
    } catch (err) {
      saveError = String(err?.message ?? err);
    }
  }
  let iconInput = $state();

  // ---- 模组 ----
  let mods = $state([]);
  let modsState = $state('idle'); // idle | loading | err
  let modsError = $state('');
  let modQuery = $state('');
  let confirmRm = $state('');

  let modsLoaded = false;
  async function loadMods() {
    modsLoaded = true;
    modsState = 'loading';
    try {
      mods = await call('list_mods', { id, settings: settings() });
      modsState = 'idle';
    } catch (e) {
      modsError = String(e);
      modsState = 'err';
    }
  }
  async function toggleMod(m) {
    try {
      await call('set_mod_enabled', { id, file: m.file, enabled: !m.enabled, settings: settings() });
    } catch (e) {
      modsError = String(e);
    }
    await loadMods();
  }
  async function removeMod(m) {
    confirmRm = '';
    try {
      await call('remove_mod', { id, file: m.file, settings: settings() });
    } catch (e) {
      modsError = String(e);
    }
    await loadMods();
  }
  async function addMods() {
    const paths = await pickFiles([{ name: t('模组文件'), extensions: ['jar'] }]);
    if (!paths.length) return;
    try {
      await call('add_mods', { id, paths, settings: settings() });
    } catch (e) {
      modsError = String(e);
    }
    await loadMods();
  }
  const openDir = (which) => call('open_instance_dir', { id, which, settings: settings() }).catch((e) => (saveError = String(e)));

  // 模组之外的内容：资源包、光影、数据包
  const SUBS = [['mod', '模组'], ['resourcepack', '资源包'], ['shader', '光影'], ['datapack', '数据包']];
  const FOLDER = { mod: 'mods', resourcepack: 'resourcepacks', shader: 'shaderpacks', datapack: 'saves' };
  let sub = $state('mod');
  let files = $state([]);
  let filesState = $state('idle');
  let fworld = $state('');
  let fworlds = $state([]);
  let confirmFile = $state('');

  async function loadFiles() {
    filesState = 'loading';
    try {
      files = await call('list_content', { id, kind: sub, world: fworld || null, settings: settings() });
      filesState = 'idle';
    } catch (e) {
      modsError = String(e);
      filesState = 'err';
    }
  }
  async function removeFile(f) {
    confirmFile = '';
    try {
      await call('remove_content', { id, kind: sub, file: f.file, world: fworld || null, settings: settings() });
    } catch (e) {
      modsError = String(e);
    }
    await loadFiles();
  }
  $effect(() => {
    if (tab !== 'mods' || sub === 'mod') return;
    if (sub !== 'datapack') { untrack(loadFiles); return; }
    call('list_worlds', { id, settings: settings() }).then((w) => {
      fworlds = w;
      if (!w.includes(fworld)) fworld = w[0] ?? '';
      loadFiles();
    });
  });
  $effect(() => { const w = fworld; if (tab === 'mods' && sub === 'datapack' && w) untrack(loadFiles); });

  // ---- 检查更新 / 更新 ----
  let updates = $state({}); // 文件名（不带 .disabled）→ { title, current, latest }
  let checking = $state(false);
  let updMsg = $state('');
  const baseName = (f) => f.replace(/\.disabled$/, '');
  const updateCount = $derived(Object.keys(updates).length);

  async function checkUpdates() {
    if (checking) return;
    checking = true;
    updMsg = '';
    modsError = '';
    try {
      const list = await call('content_check_updates', { instance: id, kind: sub, settings: settings() });
      updates = Object.fromEntries(list.map((u) => [u.file, u]));
      updMsg = list.length ? t('有 {n} 个可以更新', { n: list.length }) : t('都是最新版本');
    } catch (e) {
      modsError = String(e);
    } finally {
      checking = false;
    }
  }
  async function updateOne(file) {
    const key = baseName(file);
    const u = updates[key];
    if (!u) return;
    const res = await onRun(`${t('更新')} ${u.title}`, () => call('content_update', { instance: id, kind: sub, file: key, settings: settings() }));
    if (!res) return;
    if (res.ok) {
      delete updates[key];
      updMsg = updateCount ? t('有 {n} 个可以更新', { n: updateCount }) : '';
      await (sub === 'mod' ? loadMods() : loadFiles());
    } else if (!res.cancelled) {
      modsError = res.error;
    }
  }
  async function updateAll() {
    for (const f of Object.keys(updates)) await updateOne(f);
  }

  // ---- 复制存档 ----
  let duplicating = $state(false);
  let dupMsg = $state('');
  async function duplicate() {
    if (duplicating) return;
    duplicating = true;
    dupMsg = '';
    try {
      const name = `${inst.title} ${t('副本').trim()}`;
      await call('duplicate_instance', { id, name, settings: settings() });
      await onChanged();
      dupMsg = t('已复制为“{name}”', { name });
    } catch (e) {
      dupMsg = t('复制失败：{err}', { err: String(e) });
    } finally {
      duplicating = false;
    }
  }

  const mq = $derived(modQuery.trim().toLowerCase());
  const shownMods = $derived(mods.filter((m) => !mq || m.name.toLowerCase().includes(mq) || m.file.toLowerCase().includes(mq)));
  const enabledCount = $derived(mods.filter((m) => m.enabled).length);
  

  $effect(() => { if (tab === 'mods' && !modsLoaded) loadMods(); });

  // ---- 删除 ----
  let confirmDel = $state(false);
  let delError = $state('');
  async function remove() {
    delError = await onDelete(id);
    if (!delError) onDeleted();
    confirmDel = false;
  }
</script>

<section class="versions" aria-label={t('存档设置')}>
  <div class="vhead">
    <button class="back" onclick={onBack} aria-label={t('返回')}>
      <svg width="9" height="15" viewBox="0 0 9 15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M7.5 1.5L2 7.5l5.5 6" /></svg>
      {t('返回')}
    </button>
    <h2 class="ttl">{draft.name.trim() || autoTitle}</h2>
  </div>

  <div class="seg lvl1" role="tablist">
    {#each TABS as [k, label]}
      <button role="tab" class:on={tab === k} aria-selected={tab === k} onclick={() => (tab = k)}>{t(label)}</button>
    {/each}
  </div>
  {#if saveError}<p class="note err">{t('保存失败：{err}', { err: saveError })}</p>{/if}

  <div class="glass vlist setpane" bind:this={panel}>
    {#if tab === 'look'}
      <div class="hero">
        <InstanceIcon icon={draft.icon} url={customUrl} loader={inst.loader} size={64} />
        <div class="htxt">
          <b>{draft.name.trim() || autoTitle}</b>
          <small>{autoTitle}</small>
        </div>
      </div>

      <h4>{t('名称')}</h4>
      <div class="rows">
        <div class="srow">
          <b>{t('存档名称')}</b>
          <span class="ctl grow"><input class="txt" bind:value={draft.name} placeholder={autoTitle} maxlength="40" spellcheck="false" aria-label={t('存档名称')} /></span>
        </div>
      </div>

      <h4>{t('图标')}</h4>
      <div class="rows icons">
        <button class="ic" class:on={draft.icon === ''} onclick={() => (draft.icon = '')} title={t('自动')} aria-pressed={draft.icon === ''}>
          <InstanceIcon icon="" loader={inst.loader} size={36} />
          <span>{t('自动')}</span>
        </button>
        {#each ICONS as key (key)}
          <button class="ic" class:on={draft.icon === key} onclick={() => (draft.icon = key)} aria-pressed={draft.icon === key} aria-label={key.replace('preset:', '')}>
            <InstanceIcon icon={key} size={36} />
          </button>
        {/each}
        {#if customUrl}
          <button class="ic" class:on={draft.icon === 'custom'} onclick={() => (draft.icon = 'custom')} aria-pressed={draft.icon === 'custom'} title={t('自定义图片')}>
            <InstanceIcon icon="custom" url={customUrl} size={36} />
          </button>
        {/if}
        <button class="ic upload" onclick={() => iconInput.click()} title={t('选择图片')}>
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3.5" y="4.5" width="17" height="15" rx="3" /><circle cx="9" cy="10" r="1.6" /><path d="M4 17l5-4.5 3.5 3L15 13l5 4.5" /></svg>
          <span>{t('自定义')}</span>
        </button>
      </div>
      <input bind:this={iconInput} type="file" accept="image/png,image/jpeg,image/webp" hidden onchange={onIconFile} />

      <h4>{t('备注')}</h4>
      <div class="rows">
        <div class="note-row"><textarea bind:value={draft.note} rows="3" maxlength="300" placeholder={t('写点什么，比如这个存档是跟谁玩的、装了哪些整合包…')} aria-label={t('备注')}></textarea></div>
      </div>

    {:else if tab === 'mods'}
      <div class="seg subs" role="tablist">
        {#each SUBS as [k, label]}
          <button role="tab" class:on={sub === k} aria-selected={sub === k} onclick={() => { sub = k; modsError = ''; confirmFile = ''; confirmRm = ''; updates = {}; updMsg = ''; }}>{t(label)}</button>
        {/each}
      </div>
      {#if sub === 'mod' && !inst.loader}
        <div class="banner">
          <span>{t('这是原版存档，没有模组加载器，放进去的模组不会生效。')}</span>
          <button class="mini" onclick={onDownload}>{t('去安装加载器')}</button>
        </div>
      {/if}
      <div class="toolbar">
        {#if sub === 'mod'}
          <input class="search" placeholder={t('搜索模组')} bind:value={modQuery} spellcheck="false" />
          <span class="count">{t('已启用 {a} / 共 {b}', { a: enabledCount, b: mods.length })}</span>
        {:else if sub === 'datapack'}
          {#if fworlds.length}<Select bind:value={fworld} options={fworlds.map((w) => [w, w])} label={t('世界')} />{/if}
          <span class="count">{t('共 {n} 个', { n: files.length })}</span>
        {:else}
          <span class="count">{t('共 {n} 个', { n: files.length })}</span>
        {/if}
        <span class="grow"></span>
        {#if sub !== 'datapack'}
          {#if updateCount > 1}<button class="mini" onclick={updateAll}>{t('全部更新（{n}）', { n: updateCount })}</button>{/if}
          <button class="mini" onclick={checkUpdates} disabled={checking}>{checking ? t('正在检查…') : t('检查更新')}</button>
        {/if}
        <button class="mini" onclick={() => onBrowse(sub, id)}>{t('去下载')}</button>
        {#if sub === 'mod' && inTauri}<button class="mini" onclick={addMods}>{t('添加模组')}</button>{/if}
        <button class="mini" onclick={() => openDir(FOLDER[sub])}>{t('打开文件夹')}</button>
        <button class="mini" onclick={() => (sub === 'mod' ? loadMods() : loadFiles())} disabled={modsState === 'loading' || filesState === 'loading'}>{t('刷新')}</button>
      </div>
      {#if updMsg}<p class="note">{updMsg}</p>{/if}
      {#if modsError}<p class="note err">{modsError}</p>{/if}

      {#if sub === 'mod'}
        <div class="rows modlist">
          {#if modsState === 'loading' && !mods.length}
            <p class="none">{t('正在读取模组…')}</p>
          {:else if !shownMods.length}
            <p class="none">{mods.length ? t('没有匹配的模组') : t('mods 文件夹里还没有模组。点“去下载”，或把 .jar 文件放进文件夹。')}</p>
          {:else}
            {#each shownMods as m (m.file)}
              <div class="srow mod" class:off={!m.enabled}>
                <label class="switch" title={m.enabled ? t('点击停用') : t('点击启用')}>
                  <input type="checkbox" checked={m.enabled} onchange={() => toggleMod(m)} aria-label={m.name} />
                  <i aria-hidden="true"></i>
                </label>
                <span class="mtxt">
                  <b>{m.name}{#if m.version}<em>{m.version}</em>{/if}</b>
                  <small>{m.description || m.file}</small>
                </span>
                {#if m.project}<span class="src" title={t('通过 FLC 从 Modrinth 安装')}>Modrinth</span>{/if}
                {#if updates[baseName(m.file)]}
                  <button class="mini upd" onclick={() => updateOne(m.file)} title={t('更新到 {v}', { v: updates[baseName(m.file)].latest })}>{t('更新')} · {updates[baseName(m.file)].latest}</button>
                {/if}
                <span class="msize">{size(m.size)}</span>
                {#if confirmRm === m.file}
                  <button class="mini" onclick={() => (confirmRm = '')}>{t('取消')}</button>
                  <button class="mini danger" onclick={() => removeMod(m)}>{t('确认删除')}</button>
                {:else}
                  <button class="mini danger" onclick={() => (confirmRm = m.file)}>{t('删除')}</button>
                {/if}
              </div>
            {/each}
          {/if}
        </div>
      {:else}
        <div class="rows modlist">
          {#if sub === 'datapack' && !fworlds.length}
            <p class="none">{t('这个存档还没有世界。进游戏创建一个世界后，数据包才有地方放。')}</p>
          {:else if filesState === 'loading' && !files.length}
            <p class="none">{t('正在读取…')}</p>
          {:else if !files.length}
            <p class="none">{t('这里还没有内容。点“去下载”，或把文件放进文件夹。')}</p>
          {:else}
            {#each files as f (f.file)}
              <div class="srow mod">
                <span class="mtxt"><b>{f.name}</b><small>{f.isDir ? t('文件夹') : f.file}</small></span>
                {#if f.project}<span class="src" title={t('通过 FLC 从 Modrinth 安装')}>Modrinth</span>{/if}
                {#if updates[f.file]}
                  <button class="mini upd" onclick={() => updateOne(f.file)} title={t('更新到 {v}', { v: updates[f.file].latest })}>{t('更新')} · {updates[f.file].latest}</button>
                {/if}
                <span class="msize">{size(f.size)}</span>
                {#if confirmFile === f.file}
                  <button class="mini" onclick={() => (confirmFile = '')}>{t('取消')}</button>
                  <button class="mini danger" onclick={() => removeFile(f)}>{t('确认删除')}</button>
                {:else}
                  <button class="mini danger" onclick={() => (confirmFile = f.file)}>{t('删除')}</button>
                {/if}
              </div>
            {/each}
          {/if}
        </div>
      {/if}

    {:else if tab === 'game'}
      <p class="hint">{t('这里的设置只对这个存档生效；留空就沿用“设置”页里的全局设置。')}</p>
      <div class="rows">
        <div class="srow">
          <b>{t('内存')}</b>
          <span class="ctl">
            {#if draft.memMb}
              <input class="num" type="number" min="512" max="65536" step="256" bind:value={draft.memMb} aria-label={t('内存')} /> <em>MB</em>
            {/if}
            <span class="seg">
              <button class:on={!draft.memMb} onclick={() => (draft.memMb = null)}>{t('跟随全局')}</button>
              <button class:on={!!draft.memMb} onclick={() => (draft.memMb = draft.memMb || 4096)}>{t('自定义')}</button>
            </span>
          </span>
        </div>
        <div class="srow">
          <b>{t('Java 路径')}</b>
          <span class="ctl grow"><input class="txt" bind:value={draft.java} placeholder={t('跟随全局')} spellcheck="false" aria-label={t('Java 路径')} /></span>
        </div>
        <div class="srow">
          <span class="lbl"><b>{t('额外 JVM 参数')}</b><small>{t('用空格分隔，例如 -Dfoo=bar -XX:+UseZGC')}</small></span>
          <span class="ctl grow"><input class="txt" bind:value={draft.jvmArgs} placeholder="-Dkey=value" spellcheck="false" aria-label={t('额外 JVM 参数')} /></span>
        </div>
        <div class="srow">
          <span class="lbl"><b>{t('窗口大小')}</b><small>{t('宽和高都填了才生效')}</small></span>
          <span class="ctl">
            <input class="num" type="number" min="320" max="7680" bind:value={draft.width} placeholder="854" aria-label={t('宽')} />
            <em>×</em>
            <input class="num" type="number" min="240" max="4320" bind:value={draft.height} placeholder="480" aria-label={t('高')} />
          </span>
        </div>
      </div>

    {:else}
      <h4>{t('文件夹')}</h4>
      <div class="rows">
        {#each [['game', '游戏目录'], ['saves', '世界存档'], ['mods', '模组'], ['resourcepacks', '资源包']] as [which, label]}
          <div class="srow">
            <b>{t(label)}</b>
            <button class="mini" onclick={() => openDir(which)}>{t('打开')}</button>
          </div>
        {/each}
      </div>

      <h4>{t('信息')}</h4>
      <div class="rows">
        <div class="srow"><b>{t('版本 ID')}</b><em class="sel">{inst.id}</em></div>
        <div class="srow"><b>{t('游戏版本')}</b><em class="sel">{inst.mc}</em></div>
        <div class="srow"><b>{t('模组加载器')}</b><em>{inst.loader ? `${NAMES[inst.loader]} ${inst.loaderVersion}` : t('无')}</em></div>
      </div>

      <h4>{t('操作')}</h4>
      <div class="rows">
        <div class="srow">
          <span class="lbl"><b>{t('复制此存档')}</b><small>{t('会连同世界、模组和配置一起复制')}</small></span>
          <button class="mini" onclick={duplicate} disabled={duplicating}>{duplicating ? t('正在复制…') : t('复制')}</button>
        </div>
      </div>
      {#if dupMsg}<p class="note">{dupMsg}</p>{/if}

      <h4>{t('危险操作')}</h4>
      <div class="rows">
        <div class="srow">
          <span class="lbl">
            <b>{t('删除此存档')}</b>
            <small>{t('会一并移除它的世界、模组和配置，且无法恢复。')}</small>
          </span>
          {#if confirmDel}
            <span class="ctl">
              <button class="mini" onclick={() => (confirmDel = false)}>{t('取消')}</button>
              <button class="mini danger" onclick={remove}>{t('确认删除')}</button>
            </span>
          {:else}
            <button class="mini danger" onclick={() => (confirmDel = true)}>{t('删除')}</button>
          {/if}
        </div>
      </div>
      {#if delError}<p class="note err">{t('删除失败：{err}', { err: delError })}</p>{/if}
    {/if}
  </div>
</section>

<style>
  .back { display: inline-flex; align-items: center; gap: 6px; height: 32px; padding: 0 14px 0 10px; border-radius: 16px; background: var(--fill); font-size: 13px; transition: background 0.2s; flex: none; }
  .back:hover { background: var(--fill-hover); }
  .ttl { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .lvl1 { margin-top: -2px; }
  .grow { flex: 1; }
  .hint { font-size: 12px; color: var(--text-dim); margin: 0 6px 8px; }

  .hero { display: flex; align-items: center; gap: 16px; padding: 6px 6px 10px; }
  .htxt { display: flex; flex-direction: column; min-width: 0; }
  .htxt b { font-size: 20px; font-weight: 600; letter-spacing: -0.01em; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .htxt small { color: var(--text-dim); font-size: 12px; }

  .icons { display: flex; flex-wrap: wrap; gap: 8px; padding: 12px; }
  .ic { width: 62px; min-height: 62px; border-radius: 14px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 2px; font-size: 11px; color: var(--text-dim); transition: background 0.2s, box-shadow 0.2s, transform 0.25s var(--ease); }
  .ic:hover { background: var(--fill-hover); transform: translateY(-1px); }
  .ic.on { background: var(--glass-solid); box-shadow: 0 0 0 2px var(--btn); color: var(--text); }
  .ic.upload { box-shadow: inset 0 0 0 1.5px var(--hairline); }

  .note-row { padding: 10px 12px; }
  textarea { width: 100%; border: 0; border-radius: 10px; background: var(--glass-solid); box-shadow: inset 0 0 0 1px var(--hairline); padding: 8px 10px; font: inherit; font-size: 13px; resize: vertical; min-height: 64px; max-height: 200px; user-select: text; color: inherit; }
  textarea:focus-visible { outline: 2px solid var(--blue); outline-offset: 1px; }

  .banner { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 10px 14px; margin-bottom: 8px; border-radius: 12px; background: var(--fill); font-size: 13px; color: var(--text-dim); }
  .toolbar { display: flex; align-items: center; gap: 8px; margin: 0 0 8px; flex-wrap: wrap; }
  .toolbar .search { width: 200px; height: 30px; border-radius: 8px; }
  .count { font-size: 12px; color: var(--text-dim); }
  .modlist { overflow: hidden; }
  .none { padding: 26px 16px; text-align: center; color: var(--text-dim); font-size: 13px; }
  .mod { gap: 12px; }
  .mod.off .mtxt { opacity: 0.5; }
  .mtxt { flex: 1; min-width: 0; display: flex; flex-direction: column; line-height: 1.35; }
  .mtxt b { font-size: 14px; font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .mtxt em { color: var(--text-dim); font-size: 12px; font-style: normal; margin-left: 6px; }
  .mtxt small { color: var(--text-dim); font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .subs { margin-bottom: 10px; }
  .mini.upd { background: var(--btn); color: var(--btn-text); }
  .src { font-size: 11px; color: var(--text-dim); padding: 1px 9px; border-radius: 99px; background: var(--fill); flex: none; }
  .msize { font-size: 12px; color: var(--text-dim); font-variant-numeric: tabular-nums; flex: none; }
  .mini.danger { color: var(--danger); }
  em.sel { user-select: text; }
</style>
