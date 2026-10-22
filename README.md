<div align="center">

<img src="assets/icon/flc-icon-1024.png" width="160" alt="FLC" />

# FLC · Frog Light Core

**一款快、稳、好看的 Minecraft Java 版启动器**

Tauri 2 + Rust + Svelte · 安装包不到 16 MB · 16 种界面语言

[简体中文](README.md) · [English](README.en.md)

</div>

> FLC 不是 Mojang Studios 或 Microsoft 的官方产品，与它们没有关联。Minecraft 是 Mojang Studios 的商标。详见 [NOTICE.md](NOTICE.md)。

## 特色

**存档，就是一整个游戏**
- 原版和它装的加载器合并成一条“存档”，不会再出现一个版本显示两行。
- 每个存档可以改名、换图标（用原版方块的模型和贴图渲染，也可以用自己的图片），并有独立的内存、Java、JVM 参数、窗口大小。
- 一键复制存档（连同世界、模组和配置）。

**模组与资源，Modrinth 一站式**
- 模组、整合包、光影、资源包、数据包五类内容，搜索、排序、分类筛选。
- 装模组前先看它的前置：已安装 / 将自动安装 / 找不到适配版本 / 可选，必需的前置会自动补齐。
- 安装时选择装到哪些存档，不支持的存档（版本或加载器不对）会置灰并写明原因。
- 整合包（`.mrpack`）一键安装为新存档，也可以导入本地文件。
- 检查模组 / 资源包 / 光影的更新，一键更新。

**下载更稳**
- 并发下载并校验文件完整性，官方源与 BMCLAPI 镜像自动竞速，失败自动换源。
- 下载队列：可以随时取消，点右上角的任务能看到正在下什么、接下来下什么。
- 自动下载并选择合适的 Java，不用自己装。

**加载器**：Fabric、Legacy Fabric、Quilt、Forge、NeoForge。

**账号与皮肤**
- 离线账号，支持换皮肤：3D 预览，导入自己的 64×64 皮肤，内置 Steve、Alex 等 9 个预设。
- 微软正版登录（设备码方式，不需要内置浏览器；令牌用 Windows 系统加密保存）。需要配置应用 Client ID，见下文。

**更多细节**
- 游戏异常退出后，从日志里认出常见原因（内存不足、Java 版本不对、模组冲突、显卡驱动……）并告诉你怎么办。
- 游戏窗口出现后可以自动最小化启动器，游戏关闭后自动回来。
- 16 种界面语言：简体中文、繁體中文、English、日本語、한국어、Русский、Deutsch、Français、Español、Português (Brasil)、Italiano、Türkçe、Tiếng Việt、Polski、Українська、Bahasa Indonesia。
- Windows 11 的 Mica 材质窗口，深浅色跟随系统，尊重“减少动态效果”。

## 预览

<table>
  <tr>
    <td width="50%"><img src="docs/screenshots/home.png" alt="主页" /><br /><sub><b>主页</b>：账号、存档和一个醒目的“启动游戏”</sub></td>
    <td width="50%"><img src="docs/screenshots/download.png" alt="下载游戏" /><br /><sub><b>下载游戏</b>：按系列归类，可顺便装加载器</sub></td>
  </tr>
  <tr>
    <td width="50%"><img src="docs/screenshots/browse.png" alt="模组与资源" /><br /><sub><b>模组与资源</b>：Modrinth 搜索，装到哪些存档由你选</sub></td>
    <td width="50%"><img src="docs/screenshots/skin.png" alt="更换皮肤" /><br /><sub><b>更换皮肤</b>：3D 预览，预设皮肤或导入自己的</sub></td>
  </tr>
</table>

## 下载与使用

到 [Releases](../../releases) 下载最新版本：

- `FLC-*-setup.exe`：**安装包**（推荐），中文安装向导，装到当前用户目录，不需要管理员权限，带开始菜单和桌面快捷方式。
- `FLC-*-portable.exe`：免安装版，双击即用。

需要 Windows 10 / 11 和 WebView2 运行时（Windows 11 自带；安装包会在缺少时自动下载安装，免安装版请到微软官网安装）。

> 目前的安装包没有代码签名，Windows SmartScreen 可能提示“未知发布者”，点“更多信息 → 仍要运行”即可。校验值见 Release 里的 `SHA256SUMS.txt`。

首次使用：**下载游戏** → 选一个版本（可顺便选加载器）→ 下载完成后回到主页点 **启动游戏**。

### 微软正版登录需要 Client ID

微软要求每个登录程序都使用在 Azure 注册、并通过 Minecraft 服务审核的应用 ID。FLC 没有内置公共的 ID：

- 自己用：在 **设置 → 账号 → 微软登录 Client ID** 里填你自己的 ID。
- 发布自己的构建：构建时设置环境变量 `FLC_MS_CLIENT_ID`，ID 会内置进程序。

怎么拿到 ID，见 [docs/构建与发布.md](docs/构建与发布.md)。

### 皮肤是怎么生效的

离线账号没有 Mojang 的皮肤服务可查，所以 FLC 把选中的皮肤打成一个资源包（`resourcepacks/flc-skin.zip`），覆盖游戏内置的默认皮肤。贴图一定是你选的，但手臂粗细（经典 / 纤细）由游戏按玩家 UUID 决定，FLC 管不到。正版账号直接使用你在 Mojang 那边的皮肤。

## 从源码构建

需要：[Rust](https://rustup.rs)（stable）、[Node.js](https://nodejs.org) 20+、Windows 上的 WebView2 与 MSVC 构建工具。

```bash
git clone https://github.com/<你的用户名>/frog-light-core.git
cd frog-light-core
npm install

# 开发（热更新）
npx tauri dev

# 发布构建：界面必须内置进 exe，所以一定要带 custom-protocol
npx vite build
cargo build --release -p flc-app --features tauri/custom-protocol
# 产物：target/release/flc-app.exe
```

测试：

```bash
cargo test --workspace
for c in en zh-TW ja ko ru de fr es pt-BR it tr vi pl uk id; do node tools/check-locale.mjs $c; done
```

命令行版（不依赖界面，方便调试和写脚本）：`cargo run -p flc-cli -- help`。

## 项目结构

```
crates/flc-core   核心库：版本解析、下载、Java、加载器、启动、模组与资源（Modrinth）、账号、皮肤……不依赖任何 UI
crates/flc-cli    命令行壳
src-tauri         桌面壳（Tauri 2）：窗口材质、IPC 命令
ui                界面（Svelte 5 + Vite）
  src/locales     各语言的翻译文件
tools             翻译校验脚本
docs              设计与规划、构建与发布
```

## 参与贡献

欢迎提 Issue 和 PR，详见 [CONTRIBUTING.md](CONTRIBUTING.md)。翻译问题、新语言、新的崩溃诊断规则尤其欢迎。

## 许可证

[GPL-3.0](LICENSE) © 2026 蛙创 Studio

项目里包含从 Minecraft 取出的少量素材（方块贴图、默认皮肤、青蛙模型），它们属于 Mojang Studios，不在 GPL 授权范围内，详见 [NOTICE.md](NOTICE.md)。
