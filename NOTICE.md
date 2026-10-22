# 声明与第三方内容 / Notices

## 与 Mojang、Microsoft 的关系

FLC 是第三方启动器，**不是** Mojang Studios 或 Microsoft 的官方产品，也没有得到它们的认可或授权。
Minecraft 是 Mojang Studios 的商标。使用 FLC 玩游戏，你仍然需要拥有 Minecraft 的合法副本，并遵守
[Minecraft EULA](https://www.minecraft.net/eula) 和 [使用准则](https://www.minecraft.net/usage-guidelines)。

FLC is an unofficial launcher. It is not approved by or associated with Mojang Studios or Microsoft.
Minecraft is a trademark of Mojang Studios.

## 包含的 Minecraft 素材

为了让界面里的预览和图标与游戏一致，仓库里放了少量从 Minecraft 取出的素材。**它们属于 Mojang Studios，不在本项目的
GPL-3.0 授权范围内**，仅用于界面展示与识别，不授予你任何再分发权利。

| 位置 | 内容 |
| --- | --- |
| `ui/public/skins/` | 默认皮肤（Steve、Alex、Ari、Efe、Kai、Makena、Noor、Sunny、Zuri） |
| `ui/public/steve.png` | Steve 皮肤 |
| `ui/public/blocks/` | 存档图标用的原版方块贴图 |
| `ui/public/frog/`、`assets/frog/` | 原版青蛙的模型、贴图和动画（界面里的吉祥物） |
| `assets/icon/flc-icon-1024.png`、`src-tauri/icons/` | 应用图标，造型取自 Minecraft 的青蛙与方块 |

如果你要 fork 或再分发 FLC，请自己确认这些素材是否可以随之分发；如果你是权利方并希望移除其中任何内容，请提交 Issue，
我们会尽快处理。

## 使用的在线服务

FLC 运行时会访问下面这些服务，用到的只是它们公开的接口，数据权利归各自所有：

- Mojang / Microsoft：版本清单与游戏文件、微软账号登录、Xbox Live、Minecraft 服务（正版登录时）
- [Modrinth](https://modrinth.com)：模组、整合包、光影、资源包、数据包的搜索与下载。请求会带上 `FLC/<版本>` 的 User-Agent。
- [BMCLAPI](https://bmclapi2.bangbang93.com)：国内下载镜像（可在设置里选择官方源或镜像）
- Fabric、Legacy Fabric、Quilt、Forge、NeoForge 各自的元数据与安装器服务

## 开源依赖

主要依赖及其许可证（完整列表见 `Cargo.lock` 与 `package-lock.json`）：

- [Tauri](https://tauri.app)（MIT / Apache-2.0）、[window-vibrancy](https://github.com/tauri-apps/window-vibrancy)（MIT / Apache-2.0）
- [Svelte](https://svelte.dev)（MIT）、[Vite](https://vitejs.dev)（MIT）、[three.js](https://threejs.org)（MIT）
- Rust 生态：tokio、reqwest、serde、zip、sha1、md5、sysinfo、windows-sys 等（MIT / Apache-2.0）
