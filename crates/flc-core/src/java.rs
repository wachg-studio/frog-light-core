use crate::download::{Downloader, Task};
use crate::event::{stage, Sink};
use crate::model::JavaVersion;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const RUNTIME_INDEX: &str = "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";
const MARKER: &str = ".flc-ok";

#[derive(Debug, Clone, Serialize)]
pub struct JavaInfo {
    pub path: PathBuf,
    pub major: u32,
}

fn exe_name() -> &'static str {
    if cfg!(windows) { "java.exe" } else { "java" }
}

/// 读取 Java 主目录下的 `release` 文件得到主版本号，比启动 `java -version` 快两个数量级。
pub fn probe(home: &Path) -> Option<JavaInfo> {
    let exe = home.join("bin").join(exe_name());
    if !exe.is_file() {
        return None;
    }
    let release = std::fs::read_to_string(home.join("release")).ok()?;
    let line = release.lines().find(|l| l.starts_with("JAVA_VERSION="))?;
    let v = line.trim_start_matches("JAVA_VERSION=").trim_matches('"');
    let major = if let Some(rest) = v.strip_prefix("1.") {
        rest.split('.').next()?.parse().ok()?
    } else {
        v.split(['.', '-', '+']).next()?.parse().ok()?
    };
    Some(JavaInfo { path: exe, major })
}

fn collect(dir: &Path, depth: u8, out: &mut Vec<JavaInfo>, need_marker: bool) {
    if dir.join("bin").join(exe_name()).is_file() {
        if !need_marker || dir.join(MARKER).exists() {
            out.extend(probe(dir));
        }
        return;
    }
    if depth == 0 {
        return;
    }
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            if e.path().is_dir() {
                collect(&e.path(), depth - 1, out, need_marker);
            }
        }
    }
}

/// 扫描本机已有的 Java：FLC 自己下载的运行时、JAVA_HOME、PATH 以及常见安装目录。
pub fn scan(dirs: &[&Path]) -> Vec<JavaInfo> {
    let mut out = Vec::new();
    for d in dirs {
        collect(&d.join("runtime"), 4, &mut out, true);
    }
    if let Some(h) = std::env::var_os("JAVA_HOME") {
        collect(Path::new(&h), 0, &mut out, false);
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            if let Some(home) = dir.parent() {
                collect(home, 0, &mut out, false);
            }
        }
    }
    for var in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(pf) = std::env::var_os(var) {
            for vendor in ["Java", "Microsoft", "Eclipse Adoptium", "Zulu", "BellSoft", "Amazon Corretto", "Semeru", "Eclipse Foundation"] {
                collect(&Path::new(&pf).join(vendor), 1, &mut out, false);
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|j| seen.insert(j.path.clone()));
    out
}

#[derive(Deserialize)]
struct RuntimeEntry {
    manifest: Remote,
}
#[derive(Deserialize)]
struct Remote {
    url: String,
}
#[derive(Deserialize)]
struct RuntimeManifest {
    files: HashMap<String, RuntimeFile>,
}
#[derive(Deserialize)]
struct RuntimeFile {
    #[serde(rename = "type")]
    kind: String,
    downloads: Option<RuntimeDownloads>,
}
#[derive(Deserialize)]
struct RuntimeDownloads {
    raw: Option<RawFile>,
}
#[derive(Deserialize)]
struct RawFile {
    url: String,
    sha1: String,
    size: u64,
}

fn platforms() -> &'static [&'static str] {
    if cfg!(all(windows, target_arch = "aarch64")) {
        &["windows-arm64", "windows-x64"]
    } else if cfg!(all(windows, target_arch = "x86")) {
        &["windows-x86"]
    } else if cfg!(windows) {
        &["windows-x64"]
    } else if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        &["mac-os-arm64", "mac-os"]
    } else if cfg!(target_os = "macos") {
        &["mac-os"]
    } else {
        &["linux"]
    }
}

/// 从 Mojang 官方运行时仓库下载指定组件，装入 `<root>/runtime/<component>/<platform>/<component>`。
async fn install_runtime(dl: &Downloader, data: &Path, component: &str, sink: &Sink) -> Result<PathBuf> {
    let index: HashMap<String, HashMap<String, Vec<RuntimeEntry>>> =
        serde_json::from_str(&dl.get_text(RUNTIME_INDEX).await?)?;
    for platform in platforms() {
        let Some(entry) = index.get(*platform).and_then(|p| p.get(component)).and_then(|v| v.first()) else {
            continue;
        };
        let manifest: RuntimeManifest = serde_json::from_str(&dl.get_text(&entry.manifest.url).await?)?;
        let home = data.join("runtime").join(component).join(platform).join(component);
        let mut tasks = Vec::new();
        for (rel, f) in &manifest.files {
            if f.kind != "file" {
                continue;
            }
            if let Some(raw) = f.downloads.as_ref().and_then(|d| d.raw.as_ref()) {
                tasks.push(Task { url: raw.url.clone(), dest: home.join(rel), sha1: Some(raw.sha1.clone()), size: Some(raw.size) });
            }
        }
        dl.fetch_all(tasks, "Java 运行时", sink).await?;
        std::fs::write(home.join(MARKER), b"")?;
        return Ok(home.join("bin").join(exe_name()));
    }
    bail!("Mojang 运行时仓库没有适用于本机的 {component}")
}

/// 选出能运行该版本的 Java；本机没有合适版本时自动下载官方运行时。
/// `dirs[0]` 是 FLC 数据目录，下载的运行时放在它下面；其余目录（如旧的游戏目录）只用来查找已有的运行时。
pub async fn ensure(dl: &Downloader, dirs: &[&Path], want: Option<&JavaVersion>, user: Option<&Path>, sink: &Sink) -> Result<PathBuf> {
    if let Some(p) = user {
        return Ok(p.to_path_buf());
    }
    let major = want.map_or(8, |j| j.major_version);
    if let Some(j) = scan(dirs).into_iter().find(|j| j.major == major) {
        return Ok(j.path);
    }
    stage(sink, "下载 Java");
    let component = want.and_then(|j| j.component.as_deref()).unwrap_or("jre-legacy");
    install_runtime(dl, dirs[0], component, sink)
        .await
        .with_context(|| format!("本机没有 Java {major}，且自动下载失败"))
}
