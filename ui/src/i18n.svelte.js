import { settings } from './settings.svelte.js';

// 以简体中文原文作为 key：中文界面直接显示 key，其他语言到 locales/<代码>.js 里查，查不到先退回英文，再退回原文。
// 语言文件按需加载（切换语言时才会取），所以加语言不会拖慢启动。

/** [代码, 语言自己的名字]；第一项是“跟随系统”。 */
export const LANGS = [
  ['', '跟随系统'],
  ['zh-CN', '简体中文'],
  ['zh-TW', '繁體中文'],
  ['en', 'English'],
  ['ja', '日本語'],
  ['ko', '한국어'],
  ['ru', 'Русский'],
  ['de', 'Deutsch'],
  ['fr', 'Français'],
  ['es', 'Español'],
  ['pt-BR', 'Português (Brasil)'],
  ['it', 'Italiano'],
  ['tr', 'Türkçe'],
  ['vi', 'Tiếng Việt'],
  ['pl', 'Polski'],
  ['uk', 'Українська'],
  ['id', 'Bahasa Indonesia'],
];
const CODES = LANGS.map(([c]) => c).filter(Boolean);

// 软件语言对应的游戏内语言代码（“游戏语言”设为“跟随软件语言”时用）
const GAME_LANG = {
  'zh-CN': 'zh_cn', 'zh-TW': 'zh_tw', en: 'en_us', ja: 'ja_jp', ko: 'ko_kr', ru: 'ru_ru', de: 'de_de', fr: 'fr_fr', es: 'es_es',
  'pt-BR': 'pt_br', it: 'it_it', tr: 'tr_tr', vi: 'vi_vn', pl: 'pl_pl', uk: 'uk_ua', id: 'id_id',
};

const files = import.meta.glob('./locales/*.js');
const dicts = $state({});

/** 把系统或用户给的语言标记（zh-HK、pt、en-GB…）对到我们支持的语言。 */
function resolve(raw) {
  const l = (raw || '').toLowerCase();
  const exact = CODES.find((c) => c.toLowerCase() === l);
  if (exact) return exact;
  if (l.startsWith('zh')) return /^zh-(tw|hk|mo)|hant/.test(l) ? 'zh-TW' : 'zh-CN';
  const base = l.split('-')[0];
  if (base === 'pt') return 'pt-BR';
  return CODES.find((c) => c.toLowerCase() === base) ?? 'en';
}

/** 当前生效的软件语言代码。读取 settings 的 $state，所以界面会随之响应。 */
export function locale() {
  return resolve(settings.locale || (typeof navigator !== 'undefined' ? navigator.language : 'zh-CN'));
}

/** 确保当前语言（和作为后备的英文）的语言文件已经加载。App 里用一个 $effect 调它。 */
export async function loadLocale() {
  for (const code of new Set([locale(), 'en'])) {
    if (code === 'zh-CN' || dicts[code] || !files[`./locales/${code}.js`]) continue;
    try {
      dicts[code] = (await files[`./locales/${code}.js`]()).default;
    } catch {
      // 语言文件加载失败就退回英文 / 原文
    }
  }
}

export function t(key, vars) {
  const code = locale();
  let s = code === 'zh-CN' ? key : (dicts[code]?.[key] ?? dicts.en?.[key] ?? key);
  if (vars) for (const k in vars) s = s.replaceAll(`{${k}}`, vars[k]);
  return s;
}

/** 游戏语言设为“自动”时，按软件语言选一个游戏语言代码。 */
export function autoGameLang() {
  return GAME_LANG[locale()] ?? 'en_us';
}
