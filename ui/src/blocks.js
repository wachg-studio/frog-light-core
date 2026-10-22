// 用原版方块的模型与贴图渲染存档图标。
// 贴图（ui/public/blocks）取自原版游戏资源；每个方块的各面用的是原版 block 模型里指定的贴图。
// 视角与原版物品栏里的方块一致（模型旋转 30°/225°）：能看到顶面、东面（左）和北面（右）。

// 原版平原的草方块着色
const GRASS = '#91bd59';

// up / east / north 对应顶面、左侧面、右侧面；overlay 是叠在上面的第二层贴图（草方块侧面的草边）。
const col = (end, side) => ({ up: { tex: end }, east: { tex: side }, north: { tex: side } });
const all = (tex) => col(tex, tex);

export const BLOCKS = {
  grass_block: {
    up: { tex: 'grass_block_top', tint: GRASS },
    east: { tex: 'grass_block_side', overlay: { tex: 'grass_block_side_overlay', tint: GRASS } },
    north: { tex: 'grass_block_side', overlay: { tex: 'grass_block_side_overlay', tint: GRASS } },
  },
  dirt: all('dirt'),
  stone: all('stone'),
  cobblestone: all('cobblestone'),
  oak_planks: all('oak_planks'),
  sand: all('sand'),
  // crafting_table 模型：north 是正面，east 是侧面
  crafting_table: { up: { tex: 'crafting_table_top' }, east: { tex: 'crafting_table_side' }, north: { tex: 'crafting_table_front' } },
  // furnace 是 orientable 模型：正面在 north
  furnace: { up: { tex: 'furnace_top' }, east: { tex: 'furnace_side' }, north: { tex: 'furnace_front' } },
  bookshelf: col('oak_planks', 'bookshelf'),
  tnt: col('tnt_top', 'tnt_side'),
  diamond_ore: all('diamond_ore'),
  gold_ore: all('gold_ore'),
  redstone_ore: all('redstone_ore'),
  netherrack: all('netherrack'),
  obsidian: all('obsidian'),
  ochre_froglight: col('ochre_froglight_top', 'ochre_froglight_side'),
  verdant_froglight: col('verdant_froglight_top', 'verdant_froglight_side'),
  pearlescent_froglight: col('pearlescent_froglight_top', 'pearlescent_froglight_side'),
};

export const BLOCK_KEYS = Object.keys(BLOCKS);

// 早期版本里用过的图标名
const ALIASES = { grass: 'grass_block', planks: 'oak_planks', nether: 'netherrack', diamond: 'diamond_ore', gold: 'gold_ore', redstone: 'redstone_ore' };
export const blockKey = (icon) => {
  const k = icon.replace('preset:', '');
  return BLOCKS[k] ? k : ALIASES[k] ?? '';
};

const images = new Map();
function image(name) {
  if (!images.has(name)) {
    images.set(
      name,
      new Promise((resolve, reject) => {
        const img = new Image();
        img.onload = () => resolve(img);
        img.onerror = () => reject(new Error(`missing texture ${name}`));
        img.src = `/blocks/${name}.png`;
      }),
    );
  }
  return images.get(name);
}

/** 把贴图按颜色着色（原版里草、树叶这类方块的灰色贴图要乘上生物群系颜色）。 */
function tinted(img, color) {
  const c = document.createElement('canvas');
  c.width = img.width;
  c.height = img.width;
  const g = c.getContext('2d');
  g.drawImage(img, 0, 0, img.width, img.width);
  g.globalCompositeOperation = 'multiply';
  g.fillStyle = color;
  g.fillRect(0, 0, c.width, c.height);
  g.globalCompositeOperation = 'destination-in';
  g.drawImage(img, 0, 0, img.width, img.width);
  return c;
}

const H = Math.sqrt(3) / 2;
// 每个面：贴图原点、u 轴、v 轴（单位是方块边长，屏幕坐标）和明暗。上 1.0，东 0.8，北 0.6。
const FACES = {
  up: { o: [H, -0.5], u: [-H, 0.5], v: [-H, -0.5], shade: 1 },
  east: { o: [-H, -0.5], u: [H, 0.5], v: [0, 1], shade: 0.8 },
  north: { o: [0, 0], u: [H, -0.5], v: [0, 1], shade: 0.6 },
};

const cache = new Map();

/** 渲染方块图标，返回 PNG 的 data URL。 */
export function renderBlock(key, N = 128) {
  const id = `${key}@${N}`;
  if (!cache.has(id)) cache.set(id, draw(key, N));
  return cache.get(id);
}

async function draw(key, N) {
  const def = BLOCKS[key];
  const names = new Set();
  for (const f of Object.values(def)) {
    names.add(f.tex);
    if (f.overlay) names.add(f.overlay.tex);
  }
  const loaded = Object.fromEntries(await Promise.all([...names].map(async (n) => [n, await image(n)])));

  const c = document.createElement('canvas');
  c.width = c.height = N;
  const g = c.getContext('2d');
  g.imageSmoothingEnabled = false;
  const s = (N / 2) * 0.97;
  const cx = N / 2;
  const cy = N / 2;

  const layer = (face, spec, f) => {
    const img = loaded[spec.tex];
    const src = spec.tint ? tinted(img, spec.tint) : img;
    const k = s / img.width;
    g.setTransform(f.u[0] * k, f.u[1] * k, f.v[0] * k, f.v[1] * k, cx + f.o[0] * s, cy + f.o[1] * s);
    g.drawImage(src, 0, 0);
  };

  for (const face of ['up', 'east', 'north']) {
    const f = FACES[face];
    const spec = def[face];
    // 先画一层略微外扩的底色，避免相邻面之间出现缝
    layer(face, spec, f);
    if (spec.overlay) layer(face, spec.overlay, f);
    g.setTransform(1, 0, 0, 1, 0, 0);
    if (f.shade < 1) {
      const p = [f.o, [f.o[0] + f.u[0], f.o[1] + f.u[1]], [f.o[0] + f.u[0] + f.v[0], f.o[1] + f.u[1] + f.v[1]], [f.o[0] + f.v[0], f.o[1] + f.v[1]]];
      g.beginPath();
      p.forEach(([x, y], i) => (i ? g.lineTo(cx + x * s, cy + y * s) : g.moveTo(cx + x * s, cy + y * s)));
      g.closePath();
      g.fillStyle = `rgba(0,0,0,${1 - f.shade})`;
      g.fill();
    }
  }
  return c.toDataURL('image/png');
}
