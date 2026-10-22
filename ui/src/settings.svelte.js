// 设置只存在前端（localStorage），每次调用后端命令时随参数带过去。
const KEY = 'flc.settings';

const defaults = {
  locale: '', // 软件语言：'' 跟随系统，'zh-CN'，'en'
  gameLang: 'auto', // 游戏语言：auto 跟随软件语言，否则是 Minecraft 的语言代码，如 zh_cn
  memMode: 'auto', // auto | manual
  memMb: 4096,
  theme: 'system', // 外观：system | light | dark
  root: '', // 游戏下载位置：.minecraft 目录，空表示默认
  dataDir: '', // FLC 数据位置：Java 运行时、安装器等，空表示默认
  isolate: true, // 版本隔离
  java: '', // 空表示自动选择
  mirror: 'auto', // auto | official | bmcl
  frog: true, // 显示吉祥物青蛙
  log: true, // 记录游戏日志
  skin: 'steve', // 当前皮肤的 id，见 skins.svelte.js
  afterLaunch: 'keep', // 游戏窗口出现后启动器怎么办：keep 不动 | minimize 最小化，游戏关闭后自动回来
  accountType: 'offline', // offline | msa（微软正版）
  accountId: '', // 选中的正版账号（玩家 UUID）
  msClientId: '', // 微软登录用的应用 Client ID；发布版可以构建时内置，留空则用内置的
};

function load() {
  try {
    return { ...defaults, ...JSON.parse(localStorage.getItem(KEY) || '{}') };
  } catch {
    return { ...defaults };
  }
}

export const settings = $state(load());

export function save() {
  try {
    localStorage.setItem(KEY, JSON.stringify(settings));
  } catch {}
}

export const GAME_LANGS = [
  ['zh_cn', '简体中文'],
  ['zh_tw', '繁體中文'],
  ['en_us', 'English (US)'],
  ['ja_jp', '日本語'],
  ['ko_kr', '한국어'],
  ['ru_ru', 'Русский'],
  ['de_de', 'Deutsch'],
  ['fr_fr', 'Français'],
  ['es_es', 'Español'],
  ['pt_br', 'Português (Brasil)'],
  ['it_it', 'Italiano'],
  ['tr_tr', 'Türkçe'],
  ['vi_vn', 'Tiếng Việt'],
  ['pl_pl', 'Polski'],
  ['uk_ua', 'Українська'],
  ['id_id', 'Bahasa Indonesia'],
];
