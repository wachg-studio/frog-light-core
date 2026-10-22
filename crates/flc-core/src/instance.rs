//! 实例（界面里叫“存档”）：一个可以直接启动的版本，加上 FLC 为它保存的名称、图标和个性化设置。
//!
//! 数据放在版本目录里（`versions/<id>/flc-instance.json`），所以移动、备份或删除版本目录时它们一起走。
//! 装了加载器的版本会继承原版，原版只是它的“组件”：不单独显示，加载器实例被删掉时一起清理。
use crate::launch::Config;
use crate::version;
use anyhow::{bail, Context, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

const META_FILE: &str = "flc-instance.json";
const ICON_FILE: &str = "flc-icon.png";
const MAX_ICON_BYTES: usize = 1 << 20;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Meta {
    /// 用户起的名字，空表示用自动生成的标题。
    pub name: String,
    /// 空：按加载器自动选；`preset:<key>`：内置图标；`custom`：版本目录里的自定义图片。
    pub icon: String,
    pub note: String,
    /// 只是别的实例的依赖（比如加载器底下的原版），不在列表里显示。
    pub component: bool,
    /// 以下为这个实例自己的游戏设置，留空则沿用全局设置。
    pub mem_mb: Option<u32>,
    pub java: String,
    pub jvm_args: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub last_played: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    pub id: String,
    pub title: String,
    /// 原版（继承链的根）版本号。
    pub mc: String,
    /// fabric | quilt | forge | neoforge，原版为空。
    pub loader: String,
    pub loader_version: String,
    pub icon_url: Option<String>,
    #[serde(flatten)]
    pub meta: Meta,
}

fn dir(root: &Path, id: &str) -> PathBuf {
    root.join("versions").join(id)
}

pub fn load_meta(root: &Path, id: &str) -> Option<Meta> {
    let text = std::fs::read_to_string(dir(root, id).join(META_FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn meta(root: &Path, id: &str) -> Meta {
    load_meta(root, id).unwrap_or_default()
}

pub fn save_meta(root: &Path, id: &str, meta: &Meta) -> Result<()> {
    version::check_id(id)?;
    let d = dir(root, id);
    if !d.is_dir() {
        bail!("版本 {id} 不存在");
    }
    std::fs::write(d.join(META_FILE), serde_json::to_vec_pretty(meta)?).with_context(|| format!("保存 {id} 的设置失败"))
}

/// 用户明确安装（或重新安装）的版本：登记为实例，已经是组件的转为可见。
pub fn ensure(root: &Path, id: &str) -> Result<()> {
    let mut m = meta(root, id);
    if load_meta(root, id).is_none() || m.component {
        m.component = false;
        save_meta(root, id, &m)?;
    }
    Ok(())
}

/// 安装加载器时顺带装上的原版：只做组件，不单独显示。
pub fn mark_component(root: &Path, id: &str) -> Result<()> {
    let mut m = meta(root, id);
    m.component = true;
    save_meta(root, id, &m)
}

pub fn touch_played(root: &Path, id: &str) -> Result<()> {
    let mut m = meta(root, id);
    m.last_played = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs());
    save_meta(root, id, &m)
}

fn loader_name(kind: &str) -> &'static str {
    match kind {
        "fabric" => "Fabric",
        "legacyfabric" => "Legacy Fabric",
        "quilt" => "Quilt",
        "forge" => "Forge",
        "neoforge" => "NeoForge",
        _ => "",
    }
}

/// 从版本 id 认出加载器及其版本；`mc` 用来剥掉 Fabric / Quilt 末尾的游戏版本。
pub fn parse_loader(id: &str, mc: &str) -> (String, String) {
    for (prefix, kind) in [("legacy-fabric-loader-", "legacyfabric"), ("fabric-loader-", "fabric"), ("quilt-loader-", "quilt")] {
        if let Some(rest) = id.strip_prefix(prefix) {
            let v = rest.strip_suffix(&format!("-{mc}")).unwrap_or(rest);
            return (kind.into(), v.into());
        }
    }
    if let Some(v) = id.strip_prefix("neoforge-") {
        return ("neoforge".into(), v.into());
    }
    if let Some((_, v)) = id.split_once("-forge-") {
        return ("forge".into(), v.into());
    }
    // 老版 Forge：1.12.2-forge1.12.2-14.23.5.2860
    if let Some((_, rest)) = id.split_once("-forge") {
        return ("forge".into(), rest.rsplit('-').next().unwrap_or(rest).into());
    }
    (String::new(), String::new())
}

fn auto_title(mc: &str, loader: &str, loader_version: &str) -> String {
    match loader_name(loader) {
        "" => mc.to_string(),
        name if loader_version.is_empty() => format!("{mc} · {name}"),
        name => format!("{mc} · {name} {loader_version}"),
    }
}

fn icon_data_url(root: &Path, id: &str) -> Option<String> {
    let bytes = std::fs::read(dir(root, id).join(ICON_FILE)).ok()?;
    Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

/// 所有要在界面里显示的实例。
pub fn list(root: &Path) -> Vec<Instance> {
    let ids = version::installed(root);
    let parents_of: HashMap<&str, Option<String>> =
        ids.iter().map(|id| (id.as_str(), version::read_json(root, id).ok().and_then(|j| j.inherits_from))).collect();
    let parents: HashSet<&str> = parents_of.values().flatten().map(String::as_str).collect();

    let mut out: Vec<Instance> = ids
        .iter()
        .filter_map(|id| {
            let saved = load_meta(root, id);
            // 有登记就听登记的；老版本没有登记，被别的版本继承的当作组件。
            let visible = match &saved {
                Some(m) => !m.component,
                None => !parents.contains(id.as_str()),
            };
            if !visible {
                return None;
            }
            let meta = saved.unwrap_or_default();
            // 沿继承链往上走：整合包存档自己的 id 认不出加载器，但它继承的加载器版本可以
            let mut chain = vec![id.as_str()];
            for _ in 0..8 {
                match parents_of.get(chain[chain.len() - 1]).and_then(|p| p.as_deref()) {
                    Some(p) => chain.push(p),
                    None => break,
                }
            }
            let mc = chain[chain.len() - 1];
            let (loader, loader_version) = chain.iter().map(|c| parse_loader(c, mc)).find(|(l, _)| !l.is_empty()).unwrap_or_default();
            let title = if meta.name.trim().is_empty() { auto_title(mc, &loader, &loader_version) } else { meta.name.trim().to_string() };
            let icon_url = (meta.icon == "custom").then(|| icon_data_url(root, id)).flatten();
            Some(Instance { id: id.clone(), title, mc: mc.to_string(), loader, loader_version, icon_url, meta })
        })
        .collect();
    out.sort_by_key(|i| (Reverse(i.meta.last_played), Reverse(i.id.clone())));
    out
}

/// 把名字变成一个能当文件夹名的版本 id，并避开已有的（重名就加 “ (2)”）。
pub fn new_version_id(root: &Path, name: &str) -> String {
    let clean: String = name.chars().map(|c| if r#"\/:*?"<>|"#.contains(c) || c.is_control() { ' ' } else { c }).collect();
    let clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    let base = if clean.is_empty() || clean.starts_with('.') { "Instance".to_string() } else { clean.chars().take(48).collect() };
    let taken = |id: &str| root.join("versions").join(id).exists();
    if !taken(&base) {
        return base;
    }
    (2..).map(|n| format!("{base} ({n})")).find(|id| !taken(id)).expect("infinite")
}

fn copy_tree(from: &Path, to: &Path, skip: &dyn Fn(&Path) -> bool) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)?.flatten() {
        let p = e.path();
        if skip(&p) {
            continue;
        }
        let dest = to.join(e.file_name());
        if p.is_dir() {
            copy_tree(&p, &dest, skip)?;
        } else {
            std::fs::copy(&p, &dest).with_context(|| format!("复制 {} 失败", p.display()))?;
        }
    }
    Ok(())
}

/// 复制一个实例（连同它的世界、模组、配置），返回新实例的版本 id。natives 会在启动时重新解出，不复制。
pub fn duplicate(root: &Path, id: &str, name: &str) -> Result<String> {
    version::check_id(id)?;
    let src = dir(root, id);
    let json = src.join(format!("{id}.json"));
    if !json.is_file() {
        bail!("版本 {id} 不存在");
    }
    let new_id = new_version_id(root, name);
    let dst = dir(root, &new_id);
    let result = (|| -> Result<()> {
        let old_json = json.clone();
        let old_jar = src.join(format!("{id}.jar"));
        let skip = |p: &Path| p == old_json || p == old_jar || p.file_name().is_some_and(|n| n == "natives" || n == META_FILE);
        copy_tree(&src, &dst, &skip)?;
        // 版本 JSON 里写着自己的 id，要跟着改；客户端 jar 按新名字放
        let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&old_json)?)?;
        v["id"] = Value::String(new_id.clone());
        std::fs::write(dst.join(format!("{new_id}.json")), serde_json::to_vec_pretty(&v)?)?;
        if old_jar.is_file() {
            std::fs::copy(&old_jar, dst.join(format!("{new_id}.jar")))?;
        }
        let mut meta = meta(root, id);
        meta.name = name.trim().to_string();
        meta.component = false;
        meta.last_played = None;
        save_meta(root, &new_id, &meta)
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&dst);
        return Err(e);
    }
    Ok(new_id)
}

/// 删除一个实例；如果它底下的原版只是为它装的组件，并且没有别的版本在用，一并清理。
pub fn delete(root: &Path, id: &str) -> Result<()> {
    let parent = version::read_json(root, id).ok().and_then(|j| j.inherits_from);
    version::delete(root, id)?;
    if let Some(p) = parent {
        if load_meta(root, &p).is_some_and(|m| m.component) {
            let _ = version::delete(root, &p); // 还有别人依赖它时会拒绝，保留即可
        }
    }
    Ok(())
}

/// 保存自定义图标（PNG 字节），并把实例的图标设为 custom。
pub fn set_icon(root: &Path, id: &str, png: &[u8]) -> Result<()> {
    version::check_id(id)?;
    if png.len() > MAX_ICON_BYTES {
        bail!("图标太大了（上限 1 MB）");
    }
    if !png.starts_with(b"\x89PNG\r\n\x1a\n") {
        bail!("图标必须是 PNG 图片");
    }
    std::fs::write(dir(root, id).join(ICON_FILE), png).context("保存图标失败")?;
    let mut m = meta(root, id);
    m.icon = "custom".into();
    save_meta(root, id, &m)
}

/// 把实例自己的设置叠加到全局配置上。
pub fn apply_overrides(cfg: &mut Config, m: &Meta) {
    if let Some(mem) = m.mem_mb {
        cfg.max_mem_mb = Some(mem);
    }
    if !m.java.trim().is_empty() {
        cfg.java = Some(PathBuf::from(m.java.trim()));
    }
    cfg.extra_jvm.extend(m.jvm_args.split_whitespace().map(str::to_string));
    if let (Some(w), Some(h)) = (m.width, m.height) {
        cfg.window = Some((w, h));
    }
}

/// 这个实例的游戏目录（存档、模组、配置所在）。
pub fn game_dir(cfg: &Config, id: &str) -> PathBuf {
    if cfg.isolate {
        dir(&cfg.root, id)
    } else {
        cfg.root.clone()
    }
}

/// 这个实例的原版版本号、加载器和加载器版本；版本不存在时报错。
pub fn describe(root: &Path, id: &str) -> Result<(String, String, String)> {
    version::check_id(id)?;
    let mut chain = vec![id.to_string()];
    for _ in 0..8 {
        match version::read_json(root, &chain[chain.len() - 1])?.inherits_from {
            Some(p) => chain.push(p),
            None => break,
        }
    }
    let mc = chain[chain.len() - 1].clone();
    let (loader, lv) = chain.iter().map(|c| parse_loader(c, &mc)).find(|(l, _)| !l.is_empty()).unwrap_or_default();
    Ok((mc, loader, lv))
}

/// 游戏目录下的世界（有 level.dat 的存档文件夹）。
pub fn list_worlds(game_dir: &Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(game_dir.join("saves")) else { return vec![] };
    let mut out: Vec<String> = rd.flatten().filter(|e| e.path().join("level.dat").is_file()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    out.sort();
    out
}

// ---------- FLC 安装记录 ----------
// 每个内容文件夹里一份 .flc-index.json，记下哪些文件是从哪个项目装的，用来标注“已安装”并跳过重复的依赖。

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct IndexEntry {
    pub project: String,
    pub version: String,
    pub title: String,
}

const INDEX_FILE: &str = ".flc-index.json";

pub fn read_index(dir: &Path) -> HashMap<String, IndexEntry> {
    std::fs::read_to_string(dir.join(INDEX_FILE)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn write_index(dir: &Path, index: &HashMap<String, IndexEntry>) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join(INDEX_FILE), serde_json::to_vec_pretty(index)?).context("写入安装记录失败")
}

/// 文件夹里已装的项目：项目 id → 文件名（只算文件还在的）。
pub fn installed_projects(dir: &Path) -> HashMap<String, String> {
    read_index(dir)
        .into_iter()
        .filter(|(f, _)| dir.join(f).exists() || dir.join(format!("{f}.disabled")).exists())
        .map(|(f, e)| (e.project, f))
        .collect()
}

// ---------- 资源包 / 光影 / 数据包 ----------

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub file: String,
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub project: Option<String>,
}

fn dir_size(p: &Path) -> u64 {
    std::fs::read_dir(p).map(|rd| rd.flatten().map(|e| e.metadata().map(|m| if m.is_dir() { dir_size(&e.path()) } else { m.len() }).unwrap_or(0)).sum()).unwrap_or(0)
}

/// 列出文件夹里的包（zip 或解开的文件夹），跳过 FLC 自己的记录和皮肤包。
pub fn list_files(dir: &Path) -> Vec<FileInfo> {
    let Ok(rd) = std::fs::read_dir(dir) else { return vec![] };
    let index = read_index(dir);
    let mut out: Vec<FileInfo> = rd
        .flatten()
        .filter_map(|e| {
            let file = e.file_name().to_string_lossy().into_owned();
            if file.starts_with('.') || file == crate::skin::PACK_FILE {
                return None;
            }
            let is_dir = e.path().is_dir();
            let size = if is_dir { dir_size(&e.path()) } else { e.metadata().map(|m| m.len()).unwrap_or(0) };
            let name = file.trim_end_matches(".zip").to_string();
            Some(FileInfo { project: index.get(&file).map(|i| i.project.clone()), file, name, size, is_dir })
        })
        .collect();
    out.sort_by_key(|f| f.name.to_lowercase());
    out
}

pub fn remove_entry(dir: &Path, file: &str) -> Result<()> {
    if file.is_empty() || file.starts_with('.') || file.contains(['/', '\\']) || file.contains("..") {
        bail!("无效的文件名: {file}");
    }
    let p = dir.join(file);
    if p.is_dir() {
        std::fs::remove_dir_all(&p)
    } else {
        std::fs::remove_file(&p)
    }
    .with_context(|| format!("无法删除 {file}"))?;
    let mut index = read_index(dir);
    if index.remove(file).is_some() {
        write_index(dir, &index)?;
    }
    Ok(())
}

// ---------- 模组 ----------

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModInfo {
    pub file: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub enabled: bool,
    pub size: u64,
    /// 通过 FLC 从 Modrinth 装上的，这里是项目 id；手动放进去的为空。
    pub project: Option<String>,
}

fn mod_file_ok(file: &str) -> Result<()> {
    if file.is_empty() || file.contains(['/', '\\']) || file.contains("..") || !(file.ends_with(".jar") || file.ends_with(".jar.disabled")) {
        bail!("无效的模组文件名: {file}");
    }
    Ok(())
}

fn text_of(v: &Value) -> String {
    v.as_str().unwrap_or_default().trim().to_string()
}

fn entry(z: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Option<String> {
    let mut f = z.by_name(name).ok()?;
    let mut s = String::new();
    f.read_to_string(&mut s).ok()?;
    Some(s.trim_start_matches('\u{feff}').to_string())
}

fn toml_field(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let rest = line.trim().strip_prefix(key)?.trim_start().strip_prefix('=')?;
        Some(rest.trim().trim_matches(|c| c == '"' || c == '\'').to_string())
    })
}

/// 读出模组自己声明的名称、版本和简介；读不出来的字段为空。
fn read_info(path: &Path) -> (String, String, String) {
    let Ok(file) = std::fs::File::open(path) else { return Default::default() };
    let Ok(mut z) = zip::ZipArchive::new(file) else { return Default::default() };
    let mut out = (String::new(), String::new(), String::new());
    if let Some(v) = entry(&mut z, "fabric.mod.json").and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
        out = (text_of(&v["name"]), text_of(&v["version"]), text_of(&v["description"]));
    } else if let Some(v) = entry(&mut z, "quilt.mod.json").and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
        let q = &v["quilt_loader"];
        out = (text_of(&q["metadata"]["name"]), text_of(&q["version"]), text_of(&q["metadata"]["description"]));
    } else if let Some(t) = entry(&mut z, "META-INF/neoforge.mods.toml").or_else(|| entry(&mut z, "META-INF/mods.toml")) {
        let head = t.split("[[dependencies").next().unwrap_or(&t);
        out = (toml_field(head, "displayName").unwrap_or_default(), toml_field(head, "version").unwrap_or_default(), String::new());
    } else if let Some(v) = entry(&mut z, "mcmod.info").and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
        let first = v.get("modList").unwrap_or(&v).get(0).cloned().unwrap_or_default();
        out = (text_of(&first["name"]), text_of(&first["version"]), text_of(&first["description"]));
    }
    // mods.toml 里的版本常写成占位符，真正的值在 MANIFEST 里。
    if out.1.is_empty() || out.1.contains("${") {
        out.1 = entry(&mut z, "META-INF/MANIFEST.MF")
            .and_then(|m| m.lines().find_map(|l| l.strip_prefix("Implementation-Version:").map(|v| v.trim().to_string())))
            .unwrap_or_default();
    }
    out
}

pub fn list_mods(mods_dir: &Path) -> Vec<ModInfo> {
    let Ok(rd) = std::fs::read_dir(mods_dir) else { return vec![] };
    let index = read_index(mods_dir);
    let mut out: Vec<ModInfo> = rd
        .flatten()
        .filter_map(|e| {
            let file = e.file_name().to_string_lossy().into_owned();
            let enabled = file.ends_with(".jar");
            if !enabled && !file.ends_with(".jar.disabled") || !e.path().is_file() {
                return None;
            }
            let (name, version, description) = read_info(&e.path());
            let stem = file.trim_end_matches(".disabled").trim_end_matches(".jar").to_string();
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            let project = index.get(file.trim_end_matches(".disabled")).map(|e| e.project.clone());
            Some(ModInfo { file, name: if name.is_empty() { stem } else { name }, version, description, enabled, size, project })
        })
        .collect();
    out.sort_by_key(|m| m.name.to_lowercase());
    out
}

pub fn set_mod_enabled(mods_dir: &Path, file: &str, enabled: bool) -> Result<()> {
    mod_file_ok(file)?;
    let base = file.strip_suffix(".disabled").unwrap_or(file);
    let to = if enabled { base.to_string() } else { format!("{base}.disabled") };
    if to == file {
        return Ok(());
    }
    std::fs::rename(mods_dir.join(file), mods_dir.join(&to)).with_context(|| format!("无法重命名 {file}"))
}

pub fn remove_mod(mods_dir: &Path, file: &str) -> Result<()> {
    mod_file_ok(file)?;
    std::fs::remove_file(mods_dir.join(file)).with_context(|| format!("无法删除 {file}"))
}

/// 复制若干 jar 到模组目录，返回成功的个数。
pub fn add_mods(mods_dir: &Path, files: &[PathBuf]) -> Result<usize> {
    std::fs::create_dir_all(mods_dir)?;
    let mut n = 0;
    for f in files {
        let Some(name) = f.file_name().map(|n| n.to_string_lossy().into_owned()) else { continue };
        if !name.to_lowercase().ends_with(".jar") {
            continue;
        }
        std::fs::copy(f, mods_dir.join(&name)).with_context(|| format!("无法复制 {name}"))?;
        n += 1;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("flc-inst-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write_version(root: &Path, id: &str, inherits: Option<&str>) {
        let d = root.join("versions").join(id);
        std::fs::create_dir_all(&d).unwrap();
        let inh = inherits.map(|p| format!(r#","inheritsFrom":"{p}""#)).unwrap_or_default();
        std::fs::write(d.join(format!("{id}.json")), format!(r#"{{"id":"{id}","type":"release","mainClass":"m"{inh}}}"#)).unwrap();
    }

    #[test]
    fn loader_is_recognised_from_id() {
        assert_eq!(parse_loader("fabric-loader-0.16.5-1.21.1", "1.21.1"), ("fabric".into(), "0.16.5".into()));
        assert_eq!(parse_loader("quilt-loader-0.30.0-beta.9-1.21.2-rc1", "1.21.2-rc1"), ("quilt".into(), "0.30.0-beta.9".into()));
        assert_eq!(parse_loader("1.20.1-forge-47.2.0", "1.20.1"), ("forge".into(), "47.2.0".into()));
        assert_eq!(parse_loader("legacy-fabric-loader-0.16.0-1.12.2", "1.12.2"), ("legacyfabric".into(), "0.16.0".into()));
        assert_eq!(parse_loader("neoforge-21.1.77", "1.21.1"), ("neoforge".into(), "21.1.77".into()));
        assert_eq!(parse_loader("1.21.1", "1.21.1"), (String::new(), String::new()));
    }

    #[test]
    fn loader_instance_hides_its_vanilla_base() {
        let root = temp("list");
        write_version(&root, "1.21.1", None);
        write_version(&root, "fabric-loader-0.16.5-1.21.1", Some("1.21.1"));
        write_version(&root, "1.8.9", None);
        let list = list(&root);
        let ids: Vec<_> = list.iter().map(|i| i.id.as_str()).collect();
        assert!(ids.contains(&"fabric-loader-0.16.5-1.21.1") && ids.contains(&"1.8.9") && !ids.contains(&"1.21.1"), "{ids:?}");
        let fabric = list.iter().find(|i| i.loader == "fabric").unwrap();
        assert_eq!((fabric.mc.as_str(), fabric.title.as_str()), ("1.21.1", "1.21.1 · Fabric 0.16.5"));

        // 明确登记过的原版仍然是一个独立实例。
        ensure(&root, "1.21.1").unwrap();
        assert!(super::list(&root).iter().any(|i| i.id == "1.21.1"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn deleting_loader_instance_cleans_its_component_base() {
        let root = temp("delete");
        write_version(&root, "1.21.1", None);
        mark_component(&root, "1.21.1").unwrap();
        write_version(&root, "fabric-loader-0.16.5-1.21.1", Some("1.21.1"));
        delete(&root, "fabric-loader-0.16.5-1.21.1").unwrap();
        assert!(!root.join("versions/1.21.1").exists());

        // 没有登记的老版本原版不会被自动删除，并且会作为实例重新出现。
        write_version(&root, "1.20.1", None);
        write_version(&root, "fabric-loader-0.16.5-1.20.1", Some("1.20.1"));
        delete(&root, "fabric-loader-0.16.5-1.20.1").unwrap();
        assert!(list(&root).iter().any(|i| i.id == "1.20.1"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn duplicating_copies_worlds_and_mods_but_not_natives() {
        let root = temp("dup");
        write_version(&root, "1.21.1", None);
        let d = root.join("versions/1.21.1");
        std::fs::write(d.join("1.21.1.jar"), b"jar").unwrap();
        std::fs::create_dir_all(d.join("saves/w1")).unwrap();
        std::fs::write(d.join("saves/w1/level.dat"), b"x").unwrap();
        std::fs::create_dir_all(d.join("natives")).unwrap();
        std::fs::write(d.join("natives/a.dll"), b"x").unwrap();
        save_meta(&root, "1.21.1", &Meta { name: "原名".into(), icon: "preset:dirt".into(), last_played: Some(5), ..Default::default() }).unwrap();

        let id = duplicate(&root, "1.21.1", "原名 副本").unwrap();
        assert_eq!(id, "原名 副本");
        let n = root.join("versions").join(&id);
        assert!(n.join("saves/w1/level.dat").is_file() && n.join("原名 副本.jar").is_file() && !n.join("natives").exists());
        let json: Value = serde_json::from_str(&std::fs::read_to_string(n.join("原名 副本.json")).unwrap()).unwrap();
        assert_eq!(json["id"], "原名 副本");
        let m = meta(&root, &id);
        assert_eq!((m.name.as_str(), m.icon.as_str(), m.last_played), ("原名 副本", "preset:dirt", None));
        // 原来的没动
        assert!(d.join("1.21.1.json").is_file() && d.join("natives/a.dll").is_file());
        // 重名会自动加序号
        assert_eq!(duplicate(&root, "1.21.1", "原名 副本").unwrap(), "原名 副本 (2)");
        assert!(duplicate(&root, "nope", "x").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn overrides_only_replace_what_is_set() {
        let mut cfg = Config::new(PathBuf::from("x"));
        cfg.max_mem_mb = Some(2048);
        apply_overrides(&mut cfg, &Meta { jvm_args: "-Dfoo=1  -Dbar=2".into(), ..Default::default() });
        assert_eq!((cfg.max_mem_mb, cfg.extra_jvm.len(), cfg.window), (Some(2048), 2, None));
        apply_overrides(&mut cfg, &Meta { mem_mb: Some(8192), width: Some(1280), height: Some(720), ..Default::default() });
        assert_eq!((cfg.max_mem_mb, cfg.window), (Some(8192), Some((1280, 720))));
    }

    #[test]
    fn mods_can_be_listed_disabled_and_removed() {
        let root = temp("mods");
        let jar = root.join("sodium.jar");
        let mut z = zip::ZipWriter::new(std::fs::File::create(&jar).unwrap());
        z.start_file("fabric.mod.json", zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(br#"{"id":"sodium","name":"Sodium","version":"0.6.0","description":"fast"}"#).unwrap();
        z.finish().unwrap();

        let mods = root.join("mods");
        assert_eq!(add_mods(&mods, &[jar, root.join("notes.txt")]).unwrap(), 1);
        let m = &list_mods(&mods)[0];
        assert_eq!((m.name.as_str(), m.version.as_str(), m.enabled), ("Sodium", "0.6.0", true));

        set_mod_enabled(&mods, "sodium.jar", false).unwrap();
        assert!(mods.join("sodium.jar.disabled").is_file() && !list_mods(&mods)[0].enabled);
        set_mod_enabled(&mods, "sodium.jar.disabled", true).unwrap();
        assert!(mods.join("sodium.jar").is_file());

        assert!(remove_mod(&mods, "../x.jar").is_err());
        remove_mod(&mods, "sodium.jar").unwrap();
        assert!(list_mods(&mods).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn toml_fields_ignore_dependency_versions() {
        let t = "modLoader=\"javafml\"\n[[mods]]\nmodId=\"x\"\ndisplayName=\"Example\"\nversion=\"1.2.3\"\n";
        assert_eq!(toml_field(t, "displayName").as_deref(), Some("Example"));
        assert_eq!(toml_field(t, "version").as_deref(), Some("1.2.3"));
        assert_eq!(toml_field("versionRange=\"[1,2)\"", "version"), None);
    }
}
