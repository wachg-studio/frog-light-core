# 参与贡献

谢谢你愿意帮忙！这个项目由 蛙创 Studio 维护，欢迎 Issue 和 Pull Request。

## 提 Issue

- **Bug**：说明 FLC 版本、Windows 版本、复现步骤；游戏崩溃请附上游戏日志（主页右侧的“复制日志”）。
- **功能建议**：说清楚想解决什么问题，比说“做一个 XX”更有用。
- 涉及登录令牌、日志的内容，贴出来之前请自己检查有没有包含 access token。

## 开发环境

```bash
npm install
npx tauri dev          # 开发（热更新）
cargo test --workspace # Rust 测试
```

界面在 `ui/src`（Svelte 5，使用 runes）；核心逻辑在 `crates/flc-core`，它不依赖任何 UI，请保持这一点——新增的功能先在核心库里写好并加测试，再接到 `src-tauri` 的命令和界面上。

浏览器里直接打开 `npm run dev` 的页面时，`ui/src/api.js` 会用模拟数据代替后端，方便单独调界面。

## 代码风格

- 向周围的代码看齐：命名、注释密度、写法。注释只写代码本身表达不了的约束和原因。
- 改动保持克制：做被要求的事，不顺手重构。
- 新增行为尽量带测试；涉及网络的逻辑把可以离线验证的部分（解析、判断）拆出来测。
- 删除、覆盖、对外发布这类不可逆操作，界面里要有确认。

## 界面语言

界面文字以**简体中文原文作为键**，其他语言在 `ui/src/locales/<代码>.js`。新增或修改了界面文字后：

1. 把新的中文原文加进根目录的 `keys.json`；
2. 给每个语言文件补上翻译（保留 `{占位符}` 原样）；
3. 逐个运行 `node tools/check-locale.mjs <代码>`，直到都输出 OK。

想新增一种语言：在 `ui/src/i18n.svelte.js` 的 `LANGS`、`GAME_LANG` 和 `ui/src/settings.svelte.js` 的 `GAME_LANGS` 里加一项，再写一个 `locales/<代码>.js`。

后端（Rust）返回的错误信息目前只有中文，还没有随界面语言翻译——这是个很适合上手的改进。

## 提交 PR 之前

```bash
cargo test --workspace
npx vite build
for c in en zh-TW ja ko ru de fr es pt-BR it tr vi pl uk id; do node tools/check-locale.mjs $c; done
```

发布构建要带 `--features tauri/custom-protocol`，否则 exe 会去连开发服务器，详见 [docs/构建与发布.md](docs/构建与发布.md)。

## 许可证

提交代码即表示你同意它以 [GPL-3.0](LICENSE) 发布。
