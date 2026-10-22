// 检查一个语言文件：是否覆盖了 keys.json 里的所有键，占位符（{name}）是否原样保留，有没有多余的键或空翻译。
// 用法：node tools/check-locale.mjs <语言代码>，例如 node tools/check-locale.mjs ja
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';

const code = process.argv[2];
if (!code) {
  console.error('用法: node tools/check-locale.mjs <语言代码>');
  process.exit(2);
}
const keys = JSON.parse(readFileSync(resolve('keys.json'), 'utf8'));
const dict = (await import(pathToFileURL(resolve(`ui/src/locales/${code}.js`)).href)).default;
const holes = (s) => [...s.matchAll(/\{[a-zA-Z]+\}/g)].map((m) => m[0]).sort().join(',');

const problems = [];
for (const k of keys) {
  const v = dict[k];
  if (typeof v !== 'string' || !v.trim()) problems.push(`缺少翻译: ${k}`);
  else if (holes(v) !== holes(k)) problems.push(`占位符不一致: ${k}  =>  ${v}`);
}
const extra = Object.keys(dict).filter((k) => !keys.includes(k));
if (extra.length) problems.push(`多余的键 ${extra.length} 个，例如: ${extra.slice(0, 3).join(' | ')}`);

if (problems.length) {
  console.log(`${code}: ${problems.length} 个问题`);
  console.log(problems.slice(0, 40).join('\n'));
  process.exit(1);
}
console.log(`${code}: OK（${keys.length} 条）`);
