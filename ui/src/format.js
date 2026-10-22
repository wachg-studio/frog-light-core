import { t, locale } from './i18n.svelte.js';

// 东亚语言按“万 / 亿”计数，其他语言用 K / M / B
const CJK_UNITS = { 'zh-CN': ['万', '亿'], 'zh-TW': ['萬', '億'], ja: ['万', '億'], ko: ['만', '억'] };

/** 下载量的简写。 */
export function fmtCount(n) {
  const x = Number(n) || 0;
  const u = CJK_UNITS[locale()];
  if (u) {
    if (x >= 1e8) return `${(x / 1e8).toFixed(1)} ${u[1]}`;
    if (x >= 1e4) return `${(x / 1e4).toFixed(x >= 1e5 ? 0 : 1)} ${u[0]}`;
    return String(x);
  }
  if (x >= 1e9) return `${(x / 1e9).toFixed(1)}B`;
  if (x >= 1e6) return `${(x / 1e6).toFixed(1)}M`;
  if (x >= 1e3) return `${(x / 1e3).toFixed(x >= 1e5 ? 0 : 1)}K`;
  return String(x);
}

/** “3 天前更新”这样的相对时间。 */
export function ago(iso) {
  const ms = Date.now() - new Date(iso).getTime();
  if (!Number.isFinite(ms)) return '';
  const days = Math.max(0, Math.floor(ms / 864e5));
  if (days < 1) return t('今天更新');
  if (days < 30) return t('{n} 天前更新', { n: days });
  if (days < 365) return t('{n} 个月前更新', { n: Math.floor(days / 30) });
  return t('{n} 年前更新', { n: Math.floor(days / 365) });
}

/** 支持的游戏版本：只有一个就写它，否则写“最旧 – 最新”。 */
export function versionRange(list) {
  if (!list?.length) return '';
  return list.length === 1 ? list[0] : `${list[0]} – ${list[list.length - 1]}`;
}

export function size(n) {
  return n >= 1e6 ? `${(n / 1e6).toFixed(1)} MB` : `${Math.max(1, Math.round(n / 1e3))} KB`;
}
