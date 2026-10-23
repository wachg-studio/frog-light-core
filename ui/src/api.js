// 与 Tauri 后端通信。在普通浏览器里（没有 Tauri）退化为模拟数据，方便单独调界面。
export const inTauri = '__TAURI_INTERNALS__' in window;

/** 后端任务被用户取消时抛出的错误文本。 */
export const CANCELLED = '已取消';

export async function call(cmd, args) {
  if (inTauri) {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke(cmd, args);
  }
  return mockCall(cmd, args);
}

export async function onEvent(cb) {
  if (inTauri) {
    const { listen } = await import('@tauri-apps/api/event');
    return listen('flc://event', (e) => cb(e.payload));
  }
  mockSink = cb;
  return () => {};
}

/** 弹出系统资源管理器的“选择文件夹”对话框；取消返回 null。浏览器预览里没有这个能力，同样返回 null。 */
export async function pickFolder(defaultPath) {
  if (!inTauri) return null;
  const { open } = await import('@tauri-apps/plugin-dialog');
  const dir = await open({ directory: true, multiple: false, defaultPath: defaultPath || undefined });
  return typeof dir === 'string' ? dir : null;
}

/** 弹出系统的“选择文件”对话框，返回所选路径数组；取消或浏览器预览里返回空数组。 */
export async function pickFiles(filters) {
  if (!inTauri) return [];
  const { open } = await import('@tauri-apps/plugin-dialog');
  const files = await open({ multiple: true, directory: false, filters });
  return Array.isArray(files) ? files : typeof files === 'string' ? [files] : [];
}

/** 让窗口材质（Mica）跟着主题走：'light' | 'dark' | null（跟随系统）。 */
export async function setWindowTheme(theme) {
  if (!inTauri) return;
  const { getCurrentWindow } = await import('@tauri-apps/api/window');
  await getCurrentWindow().setTheme(theme);
}
/** 把最小化的窗口还原并拿到焦点（游戏关闭后让启动器回来）。 */
export async function restoreWindow() {
  if (!inTauri) return;
  const { getCurrentWindow } = await import('@tauri-apps/api/window');
  const w = getCurrentWindow();
  await w.unminimize();
  await w.setFocus();
}
export async function windowControl(action) {
  if (!inTauri) return;
  const { getCurrentWindow } = await import('@tauri-apps/api/window');
  const w = getCurrentWindow();
  if (action === 'minimize') w.minimize();
  else if (action === 'toggle') w.toggleMaximize();
  else if (action === 'close') w.close();
}

let mockSink = () => {};
const mockInst = (id, mc, loader = '', loaderVersion = '', extra = {}) => ({
  id, mc, loader, loaderVersion, iconUrl: null, title: '', name: '', icon: '', note: '', component: false,
  memMb: null, java: '', jvmArgs: '', width: null, height: null, lastPlayed: null, ...extra,
});
let mockInstances = [
  mockInst('1.8.9', '1.8.9'),
  mockInst('fabric-loader-0.19.5-1.21.1', '1.21.1', 'fabric', '0.19.5', { lastPlayed: Math.floor(Date.now() / 1000) - 3600 * 5 }),
  mockInst('neoforge-21.1.264', '1.21.1', 'neoforge', '21.1.264', { name: '和朋友的生存服', icon: 'preset:diamond_ore' }),
  mockInst('26.3', '26.3'),
];

// ---- 内容库的模拟数据（浏览器预览用） ----
const mockHit = (id, title, summary, extra = {}) => ({
  id, slug: id, title, summary, author: 'someone', icon: null, downloads: 1_234_567, follows: 3456, updated: new Date(Date.now() - 3 * 864e5).toISOString(),
  loaders: ['fabric', 'quilt', 'neoforge'], tags: ['optimization', 'utility'], gameVersions: ['1.20.1', '1.20.4', '1.21.1', '1.21.4'], clientSide: 'required', serverSide: 'optional', ...extra,
});
const MOCK_CONTENT = {
  mod: [
    mockHit('AANobbMI', 'Sodium', 'A high-performance rendering engine replacement for Minecraft, which greatly improves frame rates and reduces micro-stutter.', { downloads: 98_000_000, author: 'jellysquid3' }),
    mockHit('P7dR8mSH', 'Fabric API', '为 Fabric 模组提供基础接口的前置库。', { downloads: 120_000_000, tags: ['library'], loaders: ['fabric', 'quilt'], author: 'modmuss50' }),
    mockHit('gvQqBUqZ', 'Lithium', 'No-compromises game logic and server optimization mod.', { downloads: 64_000_000, author: 'jellysquid3', updated: new Date(Date.now() - 40 * 864e5).toISOString() }),
  ],
  modpack: [mockHit('1KVo5zza', 'Fabulously Optimized', 'Beautiful graphics, speedy performance and a collection of top-notch mods.', { loaders: ['fabric'], tags: ['optimization', 'lightweight'], downloads: 19_000_000 })],
  shader: [mockHit('HVnmMxH1', 'Complementary Shaders - Reimagined', 'Preserving the elements of Minecraft while enhancing the visuals.', { loaders: [], tags: ['vanilla-like', 'shadows'], downloads: 72_000_000 })],
  resourcepack: [mockHit('XNLEBJmD', 'Faithful 32x', 'Faithful keeps the original feel at a higher resolution.', { loaders: [], tags: ['32x', 'vanilla-like'], downloads: 40_000_000 })],
  datapack: [mockHit('HFE7B8bM', 'Veinminer', 'Mine the whole vein by mining a single ore.', { tags: ['utility'], downloads: 3_100_000 })],
};
const mockVersionsOf = (project) => ['1.4.2', '1.4.1', '1.3.9'].map((n, i) => ({
  id: `${project}-v${i}`, projectId: project, name: `Release ${n}`, number: n, kind: i === 2 ? 'beta' : 'release', date: new Date(Date.now() - (i * 20 + 3) * 864e5).toISOString(),
  gameVersions: ['1.21.1', '1.21'], loaders: ['fabric', 'quilt'], downloads: 1000, files: [{ filename: `${project}-${n}.jar`, url: '', size: 1_400_000 - i * 90_000, primary: true }],
}));
const mockInstalledContent = {};

const LOADER_NAMES = { fabric: 'Fabric', quilt: 'Quilt', forge: 'Forge', neoforge: 'NeoForge' };
const mockTitle = (i) => i.name.trim() || (i.loader ? `${i.mc} · ${LOADER_NAMES[i.loader]} ${i.loaderVersion}` : i.mc);
const mockMods = [
  { file: 'sodium-fabric-0.6.0.jar', name: 'Sodium', version: '0.6.0', description: '现代化的渲染引擎，大幅提升帧率。', enabled: true, size: 1_240_000 },
  { file: 'lithium-fabric-0.14.3.jar', name: 'Lithium', version: '0.14.3', description: '优化游戏逻辑与服务端性能。', enabled: true, size: 790_000 },
  { file: 'modmenu-11.0.0.jar.disabled', name: 'Mod Menu', version: '11.0.0', description: '', enabled: false, size: 260_000 },
];
let mockTimer;
let mockAbort = null; // 让预览里的“取消”能中止当前的模拟任务
let mockAccounts = [];

function mockCall(cmd, args) {
  const inst = () => mockInstances.find((i) => i.id === args.id);
  if (cmd === 'ms_client_id_builtin') return Promise.resolve(false);
  if (cmd === 'list_instances') return Promise.resolve(mockInstances.map((i) => ({ ...i, title: mockTitle(i) })));
  if (cmd === 'list_remote') return Promise.resolve(mockVersions());
  if (cmd === 'delete_instance') {
    if (args.id.startsWith('1.21.1') && mockInstances.some((x) => x.loader && x.mc === '1.21.1' && x.id !== args.id)) return Promise.reject('这些版本依赖它，请先删除它们: fabric-loader-0.19.5-1.21.1');
    mockInstances = mockInstances.filter((x) => x.id !== args.id);
    return Promise.resolve();
  }
  if (cmd === 'save_instance') {
    const i = inst();
    if (i) Object.assign(i, args.meta);
    return Promise.resolve();
  }
  if (cmd === 'set_instance_icon') {
    const i = inst();
    if (i) Object.assign(i, { icon: 'custom', iconUrl: args.data });
    return Promise.resolve();
  }


  if (cmd === 'cancel_task') { mockAbort?.(); mockAbort = null; return Promise.resolve(); }
  if (cmd === 'list_accounts') return Promise.resolve(mockAccounts.map((a) => ({ ...a })));
  if (cmd === 'remove_account') { mockAccounts = mockAccounts.filter((a) => a.id !== args.id); return Promise.resolve(); }
  if (cmd === 'account_skin') return Promise.resolve(null);
  if (cmd === 'ms_login_start') return new Promise((r) => setTimeout(() => r({ userCode: 'AB12CD34', verificationUri: 'https://www.microsoft.com/link', deviceCode: 'x', interval: 5, expiresIn: 900 }), 400));
  if (cmd === 'ms_login_finish') {
    return new Promise((resolve, reject) => {
      const t = setTimeout(() => { mockAbort = null; const a = { id: 'a1b2c3', name: 'Notch', kind: 'msa' }; mockAccounts = [a]; resolve(a); }, 5000);
      mockAbort = () => { clearTimeout(t); reject('已取消'); };
    });
  }
  if (cmd === 'duplicate_instance') {
    const src = mockInstances.find((i) => i.id === args.id);
    mockInstances.push({ ...src, id: args.name, name: args.name, lastPlayed: null });
    return new Promise((r) => setTimeout(() => r(args.name), 400));
  }
  if (cmd === 'content_dependencies') {
    return new Promise((r) => setTimeout(() => r([
      { project: 'P7dR8mSH', title: 'Fabric API', icon: null, status: 'auto', version: '0.116.17' },
      { project: 'eXts2L7r', title: 'Text Placeholder API', icon: null, status: 'installed', version: '' },
      { project: 'x1', title: 'Cloth Config', icon: null, status: 'optional', version: '' },
    ]), 300));
  }
  if (cmd === 'content_check_updates') return new Promise((r) => setTimeout(() => r([{ file: 'sodium-fabric-0.6.0.jar', project: 'AANobbMI', title: 'Sodium', current: '0.6.0', latest: '0.6.13' }]), 700));
  if (cmd === 'content_update') return mockProgress(false).then(() => ({ files: ['sodium-fabric-0.6.13.jar'], dependencies: [], warnings: [] }));
  if (cmd === 'content_search') {
    const a = args.args;
    const q = (a.query || '').toLowerCase();
    const hits = (MOCK_CONTENT[a.kind] ?? []).filter((h) => !q || h.title.toLowerCase().includes(q));
    return new Promise((r) => setTimeout(() => r({ hits, total: hits.length, offset: 0 }), 350));
  }
  if (cmd === 'content_project') {
    const h = Object.values(MOCK_CONTENT).flat().find((x) => x.id === args.id) ?? mockHit(args.id, args.id, '');
    return Promise.resolve({
      ...h, kind: 'mod', url: `https://modrinth.com/mod/${h.slug}`, authors: ['jellysquid3', 'IMS'], license: 'LGPL-3.0', gallery: [], sourceUrl: 'https://github.com/example/repo', issuesUrl: null,
      body: '## 简介\n\nSodium is a **free and open-source** rendering engine.\n\n- 大幅提升帧率\n- 兼容大多数模组\n\n访问 [官网](https://modrinth.com) 了解更多。\n\n```\nfps = 999\n```',
    });
  }
  if (cmd === 'content_versions') return new Promise((r) => setTimeout(() => r(mockVersionsOf(args.project)), 250));
  if (cmd === 'content_installed') return Promise.resolve({ ...(mockInstalledContent[`${args.instance}|${args.kind}`] ?? {}) });
  if (cmd === 'list_worlds') return Promise.resolve(['新的世界', '生存存档 1']);
  if (cmd === 'content_install') {
    return mockProgress(false).then(() => {
      (mockInstalledContent[`${args.args.instance}|${args.args.kind}`] ??= {})[args.args.project] = 'x.jar';
      return { files: ['x.jar', 'fabric-api.jar'], dependencies: ['fabric-api.jar'], warnings: [] };
    });
  }
  if (cmd === 'modpack_install' || cmd === 'modpack_import') return mockProgress(false).then(() => { const id = `${args.title ?? 'Modpack'}`; mockInstances.push(mockInst(id, '1.20.1', 'fabric', '0.15.0', { name: id })); return id; });
  if (cmd === 'list_content') return Promise.resolve([{ file: 'Faithful 32x.zip', name: 'Faithful 32x', size: 5_200_000, isDir: false, project: 'XNLEBJmD' }, { file: 'MyPack', name: 'MyPack', size: 120_000, isDir: true, project: null }]);
  if (cmd === 'remove_content' || cmd === 'open_url') return Promise.resolve();
  if (cmd === 'list_mods') return new Promise((r) => setTimeout(() => r(mockMods.map((m) => ({ ...m }))), 250));
  if (cmd === 'set_mod_enabled') {
    const m = mockMods.find((x) => x.file === args.file);
    if (m) {
      m.enabled = args.enabled;
      m.file = m.file.replace(/\.disabled$/, '') + (args.enabled ? '' : '.disabled');
    }
    return Promise.resolve();
  }
  if (cmd === 'remove_mod') {
    const i = mockMods.findIndex((x) => x.file === args.file);
    if (i >= 0) mockMods.splice(i, 1);
    return Promise.resolve();
  }
  if (cmd === 'add_mods' || cmd === 'open_instance_dir') return Promise.resolve(0);
  if (cmd === 'default_root') return Promise.resolve('C:\\Users\\you\\AppData\\Roaming\\.minecraft');
  if (cmd === 'default_data_dir') return Promise.resolve('C:\\Users\\you\\AppData\\Roaming\\FLC');
  if (cmd === 'list_loader_versions') {
    const v = (version, stable = true) => ({ version, stable });
    const demo = { fabric: [v('0.19.5'), v('0.19.4'), v('0.19.3')], quilt: [v('0.30.0-beta.9', false), v('0.29.2')], forge: [v('47.4.28'), v('47.4.0', true)], neoforge: [v('21.1.264'), v('21.1.263')] };
    // 老版本没有 NeoForge / Fabric，演示“暂不支持”的灰色状态
    const old = args.mc.startsWith('1.7') || args.mc.startsWith('1.8') || args.mc.startsWith('b1');
    return new Promise((r) => setTimeout(() => r(old && args.loader !== 'forge' ? [] : demo[args.loader]), 300));
  }
  if (cmd === 'install_game') {
    return mockProgress(false).then(() => {
      const l = args.loader;
      const id = l ? `${l.kind}-loader-${l.version}-${args.id}` : args.id;
      if (!mockInstances.some((x) => x.id === id)) mockInstances.push(mockInst(id, args.id, l?.kind ?? '', l?.version ?? ''));
      return id;
    });
  }
  if (cmd === 'launch_game') return mockProgress(true);
  if (cmd === 'kill_game') {
    clearInterval(mockTimer);
    mockSink({ type: 'exited', code: 1, ready: false, secs: 3, tail: [] });
    return Promise.resolve();
  }
  return Promise.resolve();
}

// 浏览器预览用的模拟：下载进度，launch 时再模拟启动、加载、就绪。
function mockProgress(launch) {
  const e = (x) => mockSink(x);
  e({ type: 'stage', name: '解析版本', ms: 0 });
  let done = 0;
  const total = 920e6;
  const files = ['client.jar', 'asm-9.6.jar', 'lwjgl-3.3.3.jar', 'icon_16x16.png', 'netty-all.jar'];
  return new Promise((resolve, reject) => {
    mockAbort = () => { clearInterval(mockTimer); reject('已取消'); };
    mockTimer = setInterval(() => {
      done += 30e6;
      if (done < total) {
        const k = Math.floor(done / 30e6) % files.length;
        e({ type: 'progress', label: '游戏文件', done, total, filesDone: Math.floor(done / 2e5), filesTotal: 3959, active: [files[k], files[(k + 1) % files.length]] });
        return;
      }
      clearInterval(mockTimer);
      mockAbort = null;
      e({ type: 'stage', name: '准备运行环境', ms: 46 });
      if (!launch) return resolve();
      e({ type: 'stage', name: '启动游戏', ms: 48 });
      e({ type: 'stage', name: '进程已创建', ms: 54 });
      e({ type: 'started', pid: 1234 });
      e({ type: 'log', line: '[FLC] java = jdk-21  内存 = 6144M' });
      e({ type: 'log', line: '[21:30:29] [Render thread/INFO]: Setting user: Player' });
      mockTimer = setTimeout(() => {
        e({ type: 'log', line: '[21:30:32] [Render thread/INFO]: Sound engine started' });
        e({ type: 'ready' });
      }, 4000);
      resolve(1234);
    }, 120);
  });
}

// 浏览器预览用的版本清单：覆盖各个系列、快照和远古版，用来演示分类。字段名与后端序列化结果一致（type / releaseTime）。
function mockVersions() {
  const out = [];
  const add = (id, type, d) => out.push({ id, type, url: '', releaseTime: d });
  add('26.4-snapshot-5', 'snapshot', '2026-10-14'); add('26.3', 'release', '2026-09-30'); add('26.2', 'release', '2026-06-24'); add('26.1.2', 'release', '2026-04-02'); add('26.1', 'release', '2026-03-10');
  for (const v of ['1.21.5', '1.21.4', '1.21.3', '1.21.1', '1.21']) add(v, 'release', '2024-08-08');
  add('25w02a', 'snapshot', '2025-01-08'); add('24w40a', 'snapshot', '2024-10-02'); add('1.21.2-rc1', 'snapshot', '2024-10-17');
  for (const v of ['1.20.6', '1.20.4', '1.20.2', '1.20.1', '1.20']) add(v, 'release', '2023-06-12');
  for (const v of ['1.19.4', '1.19.2', '1.18.2', '1.17.1', '1.16.5', '1.14.4', '1.12.2', '1.8.9', '1.7.10']) add(v, 'release', '2015-12-09');
  add('b1.7.3', 'old_beta', '2011-07-08'); add('b1.6.6', 'old_beta', '2011-04-04'); add('a1.2.6', 'old_alpha', '2010-12-03');
  return out;
}