// 极简 Markdown 渲染，只用来显示 Modrinth 项目的介绍。
// 先把所有 HTML 标签剥掉、再转义，然后才套自己生成的标签，所以输出里不会有来自项目作者的脚本或样式。
// 链接只保留 https，点击时由界面交给系统浏览器打开。

const esc = (s) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

function inline(s) {
  let out = esc(s);
  out = out.replace(/`([^`]+)`/g, '<code>$1</code>');
  out = out.replace(/\*\*([^*]+)\*\*/g, '<b>$1</b>').replace(/__([^_]+)__/g, '<b>$1</b>');
  out = out.replace(/(^|[^*])\*([^*\s][^*]*)\*/g, '$1<i>$2</i>');
  // 图片只留说明文字（徽章、横幅放进去会很乱）
  out = out.replace(/!\[([^\]]*)\]\([^)]*\)/g, '$1');
  out = out.replace(/\[([^\]]+)\]\((https:\/\/[^)\s]+)\)/g, (_, text, url) => `<a href="#" data-href="${url}">${text}</a>`);
  out = out.replace(/(^|[\s(])(https:\/\/[^\s<)]+)/g, (_, pre, url) => `${pre}<a href="#" data-href="${url}">${url}</a>`);
  return out;
}

export function renderMarkdown(src) {
  const text = (src ?? '')
    .replace(/\r/g, '')
    .replace(/<br\s*\/?>/gi, '\n')
    .replace(/<\/(p|div|h[1-6]|li|tr)>/gi, '\n')
    .replace(/<!--[\s\S]*?-->/g, '')
    .replace(/<[^>]*>/g, '');
  const html = [];
  let list = null;
  let code = null;
  let para = [];
  const flushPara = () => { if (para.length) { html.push(`<p>${inline(para.join(' '))}</p>`); para = []; } };
  const flushList = () => { if (list) { html.push(`<${list.tag}>${list.items.map((i) => `<li>${inline(i)}</li>`).join('')}</${list.tag}>`); list = null; } };

  for (const raw of text.split('\n')) {
    if (code) {
      if (raw.trim().startsWith('```')) { html.push(`<pre>${esc(code.join('\n'))}</pre>`); code = null; } else code.push(raw);
      continue;
    }
    const line = raw.trimEnd();
    if (line.trim().startsWith('```')) { flushPara(); flushList(); code = []; continue; }
    if (!line.trim()) { flushPara(); flushList(); continue; }
    const h = line.match(/^(#{1,6})\s+(.*)$/);
    if (h) { flushPara(); flushList(); const n = Math.min(h[1].length + 1, 5); html.push(`<h${n}>${inline(h[2])}</h${n}>`); continue; }
    if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(line)) { flushPara(); flushList(); html.push('<hr>'); continue; }
    const li = line.match(/^\s*([-*+]|\d+\.)\s+(.*)$/);
    if (li) {
      flushPara();
      const tag = /\d/.test(li[1]) ? 'ol' : 'ul';
      if (!list || list.tag !== tag) { flushList(); list = { tag, items: [] }; }
      list.items.push(li[2]);
      continue;
    }
    const q = line.match(/^>\s?(.*)$/);
    if (q) { flushPara(); flushList(); html.push(`<blockquote>${inline(q[1])}</blockquote>`); continue; }
    flushList();
    para.push(line.trim());
  }
  if (code) html.push(`<pre>${esc(code.join('\n'))}</pre>`);
  flushPara();
  flushList();
  return html.join('');
}
