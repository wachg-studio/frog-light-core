//! 模组加载器：列出某个 Minecraft 版本可用的加载器版本，并安装它。
//!
//! Fabric / Quilt 官方直接提供启动配置 JSON，写进 versions 目录即可，依赖库由通用的 `prepare` 补齐。
//! Forge / NeoForge 需要运行官方安装器（里面有补丁处理步骤），这里用 FLC 管理的 Java 以 `--installClient` 方式运行。
use crate::download::{Downloader, Task};
use crate::event::{stage, Event, Sink};
use crate::launch::{self, Config};
use crate::model::JavaVersion;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};
use tokio::io::{AsyncBufReadExt, BufReader};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Loader {
    Fabric,
    /// Legacy Fabric：把 Fabric 带到 1.3–1.13.2 的老版本。
    LegacyFabric,
    Quilt,
    Forge,
    Neoforge,
}

impl Loader {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "fabric" => Some(Self::Fabric),
            "legacyfabric" | "legacy-fabric" => Some(Self::LegacyFabric),
            "quilt" => Some(Self::Quilt),
            "forge" => Some(Self::Forge),
            "neoforge" => Some(Self::Neoforge),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Fabric => "Fabric",
            Self::LegacyFabric => "Legacy Fabric",
            Self::Quilt => "Quilt",
            Self::Forge => "Forge",
            Self::Neoforge => "NeoForge",
        }
    }

    /// 安装完成后在 versions 目录里的版本 id（与各加载器官方的命名一致）。
    pub fn version_id(self, mc: &str, version: &str) -> String {
        match self {
            Self::Fabric => format!("fabric-loader-{version}-{mc}"),
            Self::LegacyFabric => format!("legacy-fabric-loader-{version}-{mc}"),
            Self::Quilt => format!("quilt-loader-{version}-{mc}"),
            Self::Forge => format!("{mc}-forge-{version}"),
            Self::Neoforge => format!("neoforge-{version}"),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct LoaderVersion {
    pub version: String,
    /// 稳定版 / 推荐版。
    pub stable: bool,
}

fn cache() -> &'static Mutex<HashMap<String, String>> {
    static C: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    C.get_or_init(Default::default)
}

/// Forge / NeoForge 的 maven 元数据有几百 KB，一次会话内只取一次。
async fn cached_text(dl: &Downloader, url: &str) -> Result<String> {
    if let Some(t) = cache().lock().unwrap().get(url) {
        return Ok(t.clone());
    }
    let t = dl.get_text(url).await?;
    cache().lock().unwrap().insert(url.to_string(), t.clone());
    Ok(t)
}

fn xml_versions(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find("<version>") {
        rest = &rest[i + 9..];
        let Some(j) = rest.find("</version>") else { break };
        out.push(rest[..j].trim().to_string());
        rest = &rest[j..];
    }
    out
}

#[derive(Deserialize)]
struct MetaEntry {
    loader: MetaLoader,
}
#[derive(Deserialize)]
struct MetaLoader {
    version: String,
    #[serde(default)]
    stable: Option<bool>,
}

const FORGE_META: &str = "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml";
const FORGE_PROMOS: &str = "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
const NEOFORGE_META: &str = "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml";

/// NeoForge 的版本号 `21.1.77` 对应 Minecraft `1.21.1`；1.21 对应 `21.0.x`。1.20.1 用的是旧工件，这里不支持。
fn neoforge_prefix(mc: &str) -> Option<String> {
    let mut it = mc.strip_prefix("1.")?.split('.');
    let minor: u32 = it.next()?.parse().ok()?;
    let patch: u32 = it.next().map_or(Some(0), |p| p.parse().ok())?;
    (minor >= 20 && !(minor == 20 && patch < 2)).then(|| format!("{minor}.{patch}."))
}

/// 列出某个 Minecraft 版本可用的加载器版本，新的在前；不支持该版本时返回空列表。
pub async fn list_versions(dl: &Downloader, loader: Loader, mc: &str) -> Result<Vec<LoaderVersion>> {
    match loader {
        Loader::Fabric | Loader::LegacyFabric | Loader::Quilt => {
            let url = match loader {
                Loader::Fabric => format!("https://meta.fabricmc.net/v2/versions/loader/{mc}"),
                Loader::LegacyFabric => format!("https://meta.legacyfabric.net/v2/versions/loader/{mc}"),
                _ => format!("https://meta.quiltmc.org/v3/versions/loader/{mc}"),
            };
            let entries: Vec<MetaEntry> = serde_json::from_str(&dl.get_text(&url).await?).context("解析加载器版本列表")?;
            Ok(entries
                .into_iter()
                .map(|e| {
                    let stable = e.loader.stable.unwrap_or_else(|| !e.loader.version.contains("beta") && !e.loader.version.contains("alpha"));
                    LoaderVersion { version: e.loader.version, stable }
                })
                .collect())
        }
        Loader::Forge => {
            let xml = cached_text(dl, FORGE_META).await?;
            // 推荐版来自官方的 promotions；取不到就算了，只是少一个“推荐”标记。
            let recommended = async {
                let promos: serde_json::Value = serde_json::from_str(&cached_text(dl, FORGE_PROMOS).await.ok()?).ok()?;
                promos["promos"][format!("{mc}-recommended")].as_str().map(str::to_string)
            }
            .await;
            let prefix = format!("{mc}-");
            let mut v: Vec<LoaderVersion> = xml_versions(&xml)
                .into_iter()
                .filter_map(|x| x.strip_prefix(&prefix).map(str::to_string))
                .map(|version| LoaderVersion { stable: recommended.as_deref() == Some(version.as_str()), version })
                .collect();
            v.reverse();
            Ok(v)
        }
        Loader::Neoforge => {
            let Some(prefix) = neoforge_prefix(mc) else { return Ok(vec![]) };
            let xml = cached_text(dl, NEOFORGE_META).await?;
            let mut v: Vec<LoaderVersion> = xml_versions(&xml)
                .into_iter()
                .filter(|x| x.starts_with(&prefix))
                .map(|version| LoaderVersion { stable: !version.contains("beta") && !version.contains("alpha"), version })
                .collect();
            v.reverse();
            Ok(v)
        }
    }
}

/// 安装“原版 + 加载器”，返回可以直接启动的版本 id。
pub async fn install_loader(cfg: &Config, mc: &str, loader: Loader, version: &str, sink: &Sink) -> Result<String> {
    // 加载器版本继承自原版，所以先把原版整套备齐。
    let had_base = cfg.root.join("versions").join(mc).join(format!("{mc}.json")).is_file();
    launch::install(cfg, mc, sink).await?;
    let id = match loader {
        Loader::Fabric | Loader::LegacyFabric | Loader::Quilt => install_profile(cfg, mc, loader, version, sink).await?,
        Loader::Forge | Loader::Neoforge => run_installer(cfg, mc, loader, version, sink).await?,
    };
    // 补齐加载器自己的依赖库。
    launch::install(cfg, &id, sink).await?;
    // 为加载器顺带装上的原版只是它的组件，不单独显示。
    if !had_base {
        crate::instance::mark_component(&cfg.root, mc)?;
    }
    crate::instance::ensure(&cfg.root, &id)?;
    Ok(id)
}

async fn install_profile(cfg: &Config, mc: &str, loader: Loader, version: &str, sink: &Sink) -> Result<String> {
    stage(sink, "写入加载器配置");
    let dl = Downloader::new(cfg.mirror);
    let url = match loader {
        Loader::Fabric => format!("https://meta.fabricmc.net/v2/versions/loader/{mc}/{version}/profile/json"),
        Loader::LegacyFabric => format!("https://meta.legacyfabric.net/v2/versions/loader/{mc}/{version}/profile/json"),
        _ => format!("https://meta.quiltmc.org/v3/versions/loader/{mc}/{version}/profile/json"),
    };
    let text = dl.get_text(&url).await?;
    let mut json: serde_json::Value = serde_json::from_str(&text).context("解析加载器启动配置")?;
    // Legacy Fabric 的配置沿用 Fabric 的 id，会和同版本的 Fabric 撞名，这里加前缀区分。
    let id = loader.version_id(mc, version);
    json["id"] = serde_json::Value::String(id.clone());
    let text = serde_json::to_string(&json)?;
    let dir = cfg.root.join("versions").join(&id);
    tokio::fs::create_dir_all(&dir).await?;
    tokio::fs::write(dir.join(format!("{id}.json")), text).await?;
    Ok(id)
}

async fn run_installer(cfg: &Config, mc: &str, loader: Loader, version: &str, sink: &Sink) -> Result<String> {
    let dl = Downloader::new(cfg.mirror);
    let (url, file) = match loader {
        Loader::Forge => {
            let full = format!("{mc}-{version}");
            (format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{full}/forge-{full}-installer.jar"), format!("forge-{full}-installer.jar"))
        }
        _ => (format!("https://maven.neoforged.net/releases/net/neoforged/neoforge/{version}/neoforge-{version}-installer.jar"), format!("neoforge-{version}-installer.jar")),
    };
    let jar = cfg.data_dir.join("installers").join(&file);
    dl.fetch_all(vec![Task { url, dest: jar.clone(), sha1: None, size: None }], "加载器安装器", sink).await?;

    // 安装器会读写 launcher_profiles.json，没有它就直接报错。
    let profiles = cfg.root.join("launcher_profiles.json");
    if !profiles.exists() {
        tokio::fs::create_dir_all(&cfg.root).await?;
        tokio::fs::write(&profiles, br#"{"profiles":{},"selectedProfile":"","version":3}"#).await?;
    }

    // 新版安装器要求 Java 17+；游戏本身要求更高就用游戏的。
    let want = crate::version::resolve(&cfg.root, mc).ok().and_then(|r| r.java).filter(|j| j.major_version >= 17).unwrap_or(JavaVersion {
        component: Some("java-runtime-gamma".into()),
        major_version: 17,
    });
    let java = crate::java::ensure(&dl, &[&cfg.data_dir, &cfg.root], Some(&want), cfg.java.as_deref(), sink).await?;

    stage(sink, "运行安装器");
    let before: std::collections::HashSet<String> = crate::version::installed(&cfg.root).into_iter().collect();
    let mut cmd = tokio::process::Command::new(&java);
    cmd.arg("-jar").arg(&jar).arg("--installClient").arg(&cfg.root).current_dir(&cfg.root).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let mut child = cmd.spawn().with_context(|| format!("无法运行 {}", java.display()))?;
    let tail = std::sync::Arc::new(Mutex::new(Vec::<String>::new()));
    let mut readers = Vec::new();
    for pipe in [child.stdout.take().map(|s| Box::new(s) as Box<dyn tokio::io::AsyncRead + Send + Unpin>), child.stderr.take().map(|s| Box::new(s) as _)].into_iter().flatten() {
        let (sink, tail) = (sink.clone(), tail.clone());
        readers.push(tokio::spawn(async move {
            let mut lines = BufReader::new(pipe).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut t = tail.lock().unwrap();
                t.push(line.clone());
                if t.len() > 30 {
                    t.remove(0);
                }
                drop(t);
                sink(Event::Log { line });
            }
        }));
    }
    let status = child.wait().await?;
    for r in readers {
        let _ = r.await;
    }
    if !status.success() {
        let tail = tail.lock().unwrap().join("\n");
        bail!("{} 安装器失败（{status}）:\n{tail}", loader.name());
    }

    let after = crate::version::installed(&cfg.root);
    Ok(after.into_iter().find(|v| !before.contains(v)).unwrap_or_else(|| loader.version_id(mc, version)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neoforge_prefix_maps_minecraft_versions() {
        assert_eq!(neoforge_prefix("1.21.1").as_deref(), Some("21.1."));
        assert_eq!(neoforge_prefix("1.21").as_deref(), Some("21.0."));
        assert_eq!(neoforge_prefix("1.20.2").as_deref(), Some("20.2."));
        assert_eq!(neoforge_prefix("1.20.1"), None);
        assert_eq!(neoforge_prefix("1.12.2"), None);
        assert_eq!(neoforge_prefix("26.3"), None);
    }

    #[test]
    fn maven_metadata_versions_are_extracted() {
        let xml = "<metadata><versioning><versions><version>1.20.1-47.2.0</version>\n<version>1.20.1-47.3.0</version></versions></versioning></metadata>";
        assert_eq!(xml_versions(xml), vec!["1.20.1-47.2.0", "1.20.1-47.3.0"]);
    }

    #[test]
    fn version_ids_match_official_naming() {
        assert_eq!(Loader::Fabric.version_id("1.21.1", "0.16.5"), "fabric-loader-0.16.5-1.21.1");
        assert_eq!(Loader::Forge.version_id("1.20.1", "47.2.0"), "1.20.1-forge-47.2.0");
        assert_eq!(Loader::LegacyFabric.version_id("1.12.2", "0.16.0"), "legacy-fabric-loader-0.16.0-1.12.2");
        assert_eq!(Loader::parse("legacy-fabric"), Some(Loader::LegacyFabric));
        assert_eq!(Loader::Neoforge.version_id("1.21.1", "21.1.77"), "neoforge-21.1.77");
    }
}
