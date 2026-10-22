// 皮肤库：内置预设 + 用户导入的皮肤。用户皮肤以 64×64 PNG 的 data URL 存在 localStorage（每张几 KB），当前选中的 id 放在设置里。
import { settings } from './settings.svelte.js';

// slim 表示手臂 3 像素宽的“纤细”模型；预设图片本身就是对应模型的版本。
export const PRESETS = [
  { id: 'steve', name: 'Steve', slim: false, url: '/skins/steve.png', preset: true },
  { id: 'alex', name: 'Alex', slim: true, url: '/skins/alex.png', preset: true },
  { id: 'ari', name: 'Ari', slim: false, url: '/skins/ari.png', preset: true },
  { id: 'efe', name: 'Efe', slim: true, url: '/skins/efe.png', preset: true },
  { id: 'kai', name: 'Kai', slim: false, url: '/skins/kai.png', preset: true },
  { id: 'makena', name: 'Makena', slim: true, url: '/skins/makena.png', preset: true },
  { id: 'noor', name: 'Noor', slim: true, url: '/skins/noor.png', preset: true },
  { id: 'sunny', name: 'Sunny', slim: true, url: '/skins/sunny.png', preset: true },
  { id: 'zuri', name: 'Zuri', slim: false, url: '/skins/zuri.png', preset: true },
];

const KEY = 'flc.skins';

function load() {
  try {
    const list = JSON.parse(localStorage.getItem(KEY) || '[]');
    return Array.isArray(list) ? list.filter((s) => s && typeof s.id === 'string' && typeof s.url === 'string') : [];
  } catch {
    return [];
  }
}

export const userSkins = $state(load());

function persist() {
  try {
    localStorage.setItem(KEY, JSON.stringify(userSkins));
  } catch {}
}

export const allSkins = () => [...PRESETS, ...userSkins];
export const activeSkin = () => allSkins().find((s) => s.id === settings.skin) ?? PRESETS[0];

export function addSkin(skin) {
  const s = { id: `u${Date.now().toString(36)}${Math.random().toString(36).slice(2, 5)}`, preset: false, ...skin };
  userSkins.push(s);
  persist();
  return s;
}

export function updateSkin(id, patch) {
  const s = userSkins.find((x) => x.id === id);
  if (s) {
    Object.assign(s, patch);
    persist();
  }
}

export function removeSkin(id) {
  const i = userSkins.findIndex((x) => x.id === id);
  if (i >= 0) userSkins.splice(i, 1);
  persist();
  if (settings.skin === id) settings.skin = PRESETS[0].id;
}

function loadImage(src) {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error('bad-image'));
    img.src = src;
  });
}

/** 把 64×32 的老格式皮肤补成 64×64：左臂、左腿由右边镜像出来。 */
function legacyTo64(g, img) {
  g.drawImage(img, 0, 0);
  // 把一块区域水平翻转后画到目标位置
  const flip = (sx, sy, w, h, dx, dy) => {
    g.save();
    g.translate(dx + w, dy);
    g.scale(-1, 1);
    g.drawImage(g.canvas, sx, sy, w, h, 0, 0, w, h);
    g.restore();
  };
  // 一个 4×12×4 的肢体：右 / 前 / 左 / 后 四个侧面互换位置并翻转，顶面和底面只翻转。
  const limb = (ox, oy, tx, ty) => {
    flip(ox + 8, oy + 4, 4, 12, tx, ty + 4); // 原左面 → 新右面
    flip(ox + 4, oy + 4, 4, 12, tx + 4, ty + 4); // 前
    flip(ox, oy + 4, 4, 12, tx + 8, ty + 4); // 原右面 → 新左面
    flip(ox + 12, oy + 4, 4, 12, tx + 12, ty + 4); // 后
    flip(ox + 4, oy, 4, 4, tx + 4, ty); // 顶
    flip(ox + 8, oy, 4, 4, tx + 8, ty); // 底
  };
  limb(0, 16, 16, 48); // 右腿 → 左腿
  limb(40, 16, 32, 48); // 右臂 → 左臂
}

/** 手臂外侧两列全透明就是纤细模型（Alex 型）。 */
function looksSlim(g) {
  const d = g.getImageData(54, 20, 2, 12).data;
  for (let i = 3; i < d.length; i += 4) if (d[i] !== 0) return false;
  return true;
}

/** 读取用户选的图片文件，转成标准的 64×64 皮肤。尺寸不对时抛出带中文原文 key 的错误。 */
export async function readSkinFile(file) {
  const url = URL.createObjectURL(file);
  try {
    const img = await loadImage(url).catch(() => {
      throw new Error('这不是有效的图片文件。');
    });
    if (img.width !== 64 || (img.height !== 64 && img.height !== 32)) {
      throw new Error('皮肤图片必须是 64×64（或老格式 64×32）。');
    }
    const c = document.createElement('canvas');
    c.width = c.height = 64;
    const g = c.getContext('2d', { willReadFrequently: true });
    g.imageSmoothingEnabled = false;
    if (img.height === 32) legacyTo64(g, img);
    else g.drawImage(img, 0, 0);
    return { url: c.toDataURL('image/png'), slim: img.height === 64 && looksSlim(g) };
  } finally {
    URL.revokeObjectURL(url);
  }
}

/** 把图片文件裁成正方形并缩到 size×size，用作实例图标。 */
export async function readIconFile(file, size = 128) {
  const url = URL.createObjectURL(file);
  try {
    const img = await loadImage(url).catch(() => {
      throw new Error('这不是有效的图片文件。');
    });
    const side = Math.min(img.width, img.height);
    const c = document.createElement('canvas');
    c.width = c.height = size;
    const g = c.getContext('2d');
    g.imageSmoothingQuality = 'high';
    g.drawImage(img, (img.width - side) / 2, (img.height - side) / 2, side, side, 0, 0, size, size);
    return c.toDataURL('image/png');
  } finally {
    URL.revokeObjectURL(url);
  }
}

const dataUrlCache = new Map();

/** 启动时发给后端的当前皮肤；预设是静态资源，先转成 data URL。 */
export async function skinPayload() {
  const s = activeSkin();
  if (s.url.startsWith('data:')) return { png: s.url };
  if (!dataUrlCache.has(s.url)) {
    const blob = await (await fetch(s.url)).blob();
    const url = await new Promise((resolve, reject) => {
      const r = new FileReader();
      r.onload = () => resolve(r.result);
      r.onerror = reject;
      r.readAsDataURL(blob);
    });
    dataUrlCache.set(s.url, url);
  }
  return { png: dataUrlCache.get(s.url) };
}
