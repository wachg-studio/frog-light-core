//! 内容库：从 Modrinth 搜索并安装模组、整合包、光影、资源包和数据包。
//!
//! 模组会自动补齐必需的前置依赖；整合包（.mrpack）会新建一个独立存档：安装好需要的原版与加载器，
//! 下载包里列出的全部文件，再解开 overrides。
use crate::download::{Downloader, Task};
use crate::event::{stage, Sink};
use crate::instance::{self, new_version_id, IndexEntry};
use crate::launch::{self, Config};
use crate::loader::{self, Loader};
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

const API: &str = "https://api.modrinth.com/v2";
const PAGE: u32 = 20;
const MAX_DEPS: usize = 40;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Mod,
    Modpack,
    Shader,
    Resourcepack,
    Datapack,
}

impl Kind {
    /// 装进游戏目录下的哪个文件夹（数据包在世界里，整合包是新存档，没有固定位置）。
    fn folder(self) -> Option<&'static str> {
        match self {
            Kind::Mod => Some("mods"),
            Kind::Shader => Some("shaderpacks"),
            Kind::Resourcepack => Some("resourcepacks"),
            Kind::Modpack | Kind::Datapack => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Kind::Mod => "模组",
            Kind::Modpack => "整合包",
            Kind::Shader => "光影",
            Kind::Resourcepack => "资源包",
            Kind::Datapack => "数据包",
        }
    }
}

// ---------- 搜索 ----------

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchArgs {
    pub kind: Option<Kind>,
    pub query: String,
    pub mc: Option<String>,
    pub loader: Option<String>,
    /// 多个游戏版本 / 加载器：命中任意一个即可（“安装到全部存档”时，按所有存档合起来过滤）。
    pub mcs: Vec<String>,
    pub loaders: Vec<String>,
    pub categories: Vec<String>,
    /// relevance | downloads | follows | newest | updated
    pub sort: String,
    pub offset: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub author: String,
    pub icon: Option<String>,
    pub downloads: u64,
    pub follows: u64,
    /// 最后更新时间（ISO 8601）。
    pub updated: String,
    /// 支持的加载器。
    pub loaders: Vec<String>,
    /// 其余分类标签。
    pub tags: Vec<String>,
    /// 支持的正式版游戏版本，从旧到新。
    pub game_versions: Vec<String>,
    pub client_side: String,
    pub server_side: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub hits: Vec<Hit>,
    pub total: u32,
    pub offset: u32,
}

#[derive(Deserialize)]
struct RawPage {
    hits: Vec<RawHit>,
    #[serde(default)]
    total_hits: u32,
}

#[derive(Deserialize)]
struct RawHit {
    project_id: String,
    slug: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    author: String,
    icon_url: Option<String>,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    follows: u64,
    #[serde(default)]
    date_modified: String,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    versions: Vec<String>,
    #[serde(default)]
    client_side: String,
    #[serde(default)]
    server_side: String,
}

const LOADERS: [&str; 9] = ["fabric", "quilt", "forge", "neoforge", "legacy-fabric", "liteloader", "modloader", "rift", "babric"];

/// 正式版版本号：1.21.1、26.3 这种；快照（24w40a、1.21-pre1）不算。
fn is_release(v: &str) -> bool {
    !v.is_empty() && v.split('.').all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())) && v.contains('.')
}

impl From<RawHit> for Hit {
    fn from(h: RawHit) -> Self {
        let (loaders, tags): (Vec<String>, Vec<String>) = h.categories.into_iter().partition(|c| LOADERS.contains(&c.as_str()));
        Hit {
            id: h.project_id,
            slug: h.slug,
            title: h.title,
            summary: h.description,
            author: h.author,
            icon: h.icon_url,
            downloads: h.downloads,
            follows: h.follows,
            updated: h.date_modified,
            loaders,
            tags,
            game_versions: h.versions.into_iter().filter(|v| is_release(v)).collect(),
            client_side: h.client_side,
            server_side: h.server_side,
        }
    }
}

/// 存档用的加载器对应 Modrinth 上哪些加载器的模组能用（Quilt 能加载 Fabric 模组）。
pub fn loader_filter(loader: &str) -> Vec<String> {
    match loader {
        "quilt" => vec!["quilt".into(), "fabric".into()],
        "" => vec![],
        l => vec![l.replace("legacyfabric", "legacy-fabric")],
    }
}

fn facets(a: &SearchArgs, kind: Kind) -> String {
    let mut f: Vec<Vec<String>> = vec![];
    let one = |s: &str| vec![s.to_string()];
    match kind {
        Kind::Mod => {
            f.push(one("project_type:mod"));
            f.push(one("categories!=datapack")); // 数据包在 Modrinth 上也算 mod 类，单独成一类
        }
        Kind::Datapack => {
            f.push(one("project_type:mod"));
            f.push(one("categories:datapack"));
        }
        Kind::Modpack => f.push(one("project_type:modpack")),
        Kind::Shader => f.push(one("project_type:shader")),
        Kind::Resourcepack => f.push(one("project_type:resourcepack")),
    }
    let ok_tag = |s: &&String| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.' || b == b'+' || b == b'_');
    let mut mcs: Vec<String> = a.mcs.iter().filter(ok_tag).cloned().collect();
    if let Some(mc) = a.mc.as_deref().filter(|m| !m.is_empty()) {
        mcs.push(mc.to_string());
    }
    if !mcs.is_empty() {
        f.push(mcs.into_iter().map(|v| format!("versions:{v}")).collect());
    }
    if matches!(kind, Kind::Mod | Kind::Modpack) {
        let mut ls = loader_filter(a.loader.as_deref().unwrap_or(""));
        ls.extend(a.loaders.iter().filter(ok_tag).flat_map(|l| loader_filter(l)));
        ls.sort();
        ls.dedup();
        if !ls.is_empty() {
            f.push(ls.into_iter().map(|l| format!("categories:{l}")).collect());
        }
    }
    for c in a.categories.iter().filter(|c| c.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'+')) {
        f.push(vec![format!("categories:{c}")]);
    }
    serde_json::to_string(&f).unwrap_or_else(|_| "[]".into())
}

async fn get<T: serde::de::DeserializeOwned>(dl: &Downloader, path: &str, params: &[(&str, String)]) -> Result<T> {
    let url = reqwest::Url::parse_with_params(&format!("{API}{path}"), params)?;
    let text = dl.get_text(url.as_str()).await?;
    serde_json::from_str(&text).with_context(|| format!("解析 Modrinth 返回的数据失败: {path}"))
}

/// 项目 id / slug / 版本 id 会拼进请求路径，只允许字母数字和 `-_`。
fn safe_id(s: &str) -> Result<&str> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
        bail!("无效的项目编号: {s}");
    }
    Ok(s)
}

pub async fn search(dl: &Downloader, a: &SearchArgs) -> Result<Page> {
    let kind = a.kind.unwrap_or(Kind::Mod);
    let index = match a.sort.as_str() {
        s @ ("relevance" | "downloads" | "follows" | "newest" | "updated") => s,
        _ => "relevance",
    };
    let mut params = vec![("facets", facets(a, kind)), ("index", index.to_string()), ("limit", PAGE.to_string()), ("offset", a.offset.to_string())];
    if !a.query.trim().is_empty() {
        params.push(("query", a.query.trim().to_string()));
    }
    let raw: RawPage = get(dl, "/search", &params).await?;
    Ok(Page { hits: raw.hits.into_iter().map(Hit::from).collect(), total: raw.total_hits, offset: a.offset })
}

// ---------- 项目详情 ----------

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detail {
    pub id: String,
    pub slug: String,
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub body: String,
    pub icon: Option<String>,
    pub downloads: u64,
    pub follows: u64,
    pub updated: String,
    pub loaders: Vec<String>,
    pub tags: Vec<String>,
    pub game_versions: Vec<String>,
    pub client_side: String,
    pub server_side: String,
    pub license: String,
    pub authors: Vec<String>,
    pub gallery: Vec<String>,
    pub url: String,
    pub source_url: Option<String>,
    pub issues_url: Option<String>,
}

#[derive(Deserialize)]
struct RawProject {
    id: String,
    slug: String,
    project_type: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    body: String,
    icon_url: Option<String>,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    followers: u64,
    #[serde(default)]
    updated: String,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    additional_categories: Vec<String>,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    client_side: String,
    #[serde(default)]
    server_side: String,
    license: Option<RawLicense>,
    #[serde(default)]
    gallery: Vec<RawGallery>,
    source_url: Option<String>,
    issues_url: Option<String>,
}

#[derive(Deserialize)]
struct RawLicense {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
struct RawGallery {
    url: String,
}

#[derive(Deserialize)]
struct RawMember {
    user: RawUser,
}

#[derive(Deserialize)]
struct RawUser {
    username: String,
}

pub async fn project(dl: &Downloader, id: &str) -> Result<Detail> {
    let id = safe_id(id)?;
    let (path, members_path) = (format!("/project/{id}"), format!("/project/{id}/members"));
    let (p, members) = tokio::join!(get::<RawProject>(dl, &path, &[]), get::<Vec<RawMember>>(dl, &members_path, &[]));
    let p = p?;
    let (loaders, tags): (Vec<String>, Vec<String>) = p.categories.into_iter().chain(p.additional_categories).partition(|c| LOADERS.contains(&c.as_str()));
    let license = p.license.map(|l| if l.name.is_empty() { l.id } else { l.name }).unwrap_or_default();
    Ok(Detail {
        url: format!("https://modrinth.com/{}/{}", p.project_type, p.slug),
        id: p.id,
        slug: p.slug,
        kind: p.project_type,
        title: p.title,
        summary: p.description,
        body: p.body,
        icon: p.icon_url,
        downloads: p.downloads,
        follows: p.followers,
        updated: p.updated,
        loaders,
        tags,
        game_versions: p.game_versions.into_iter().filter(|v| is_release(v)).collect(),
        client_side: p.client_side,
        server_side: p.server_side,
        license,
        authors: members.map(|m| m.into_iter().map(|m| m.user.username).collect()).unwrap_or_default(),
        gallery: p.gallery.into_iter().take(6).map(|g| g.url).collect(),
        source_url: p.source_url,
        issues_url: p.issues_url,
    })
}

// ---------- 版本与文件 ----------

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ver {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub number: String,
    /// release | beta | alpha
    pub kind: String,
    pub date: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub downloads: u64,
    pub files: Vec<VFile>,
    #[serde(skip)]
    pub deps: Vec<Dep>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VFile {
    #[serde(rename = "filename")]
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub primary: bool,
    #[serde(default, skip_serializing)]
    pub hashes: HashMap<String, String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Dep {
    pub project_id: Option<String>,
    pub version_id: Option<String>,
    pub dependency_type: String,
}

#[derive(Deserialize)]
struct RawVer {
    id: String,
    project_id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    version_number: String,
    #[serde(default)]
    version_type: String,
    #[serde(default)]
    date_published: String,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    loaders: Vec<String>,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    files: Vec<VFile>,
    #[serde(default)]
    dependencies: Vec<Dep>,
}

impl From<RawVer> for Ver {
    fn from(v: RawVer) -> Self {
        Ver {
            id: v.id,
            project_id: v.project_id,
            name: v.name,
            number: v.version_number,
            kind: v.version_type,
            date: v.date_published,
            game_versions: v.game_versions,
            loaders: v.loaders,
            downloads: v.downloads,
            files: v.files,
            deps: v.dependencies,
        }
    }
}

impl Ver {
    fn primary(&self) -> Option<&VFile> {
        self.files.iter().find(|f| f.primary).or(self.files.first())
    }
}

/// 某个项目的版本列表，新的在前；可按游戏版本、加载器过滤。
pub async fn versions(dl: &Downloader, project: &str, mc: Option<&str>, loaders: &[String]) -> Result<Vec<Ver>> {
    let project = safe_id(project)?;
    let mut params = vec![];
    if let Some(mc) = mc.filter(|m| !m.is_empty()) {
        params.push(("game_versions", serde_json::to_string(&[mc])?));
    }
    if !loaders.is_empty() {
        params.push(("loaders", serde_json::to_string(loaders)?));
    }
    let raw: Vec<RawVer> = get(dl, &format!("/project/{project}/version"), &params).await?;
    Ok(raw.into_iter().map(Ver::from).collect())
}

async fn version_by_id(dl: &Downloader, id: &str) -> Result<Ver> {
    Ok(get::<RawVer>(dl, &format!("/version/{}", safe_id(id)?), &[]).await?.into())
}

/// 正式版优先，再按发布时间取最新。
fn best(mut v: Vec<Ver>) -> Option<Ver> {
    let rank = |t: &str| match t {
        "release" => 0,
        "beta" => 1,
        _ => 2,
    };
    v.sort_by(|a, b| rank(&a.kind).cmp(&rank(&b.kind)).then_with(|| b.date.cmp(&a.date)));
    v.into_iter().next()
}

// ---------- 安装 ----------

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InstallArgs {
    pub instance: String,
    pub kind: Option<Kind>,
    pub project: String,
    pub title: String,
    pub version: Option<String>,
    pub world: Option<String>,
}

#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Installed {
    pub files: Vec<String>,
    /// 顺带装上的前置依赖。
    pub dependencies: Vec<String>,
    /// 需要提醒用户的事：没有精确匹配的版本、某个依赖找不到等。
    pub warnings: Vec<String>,
}

/// 内容该放的目录。
pub fn dest_dir(cfg: &Config, instance_id: &str, kind: Kind, world: Option<&str>) -> Result<PathBuf> {
    let game = instance::game_dir(cfg, instance_id);
    match kind {
        Kind::Datapack => {
            let w = world.filter(|w| !w.is_empty()).ok_or_else(|| anyhow!("请先选择要装进哪个世界"))?;
            if w.contains(['/', '\\']) || w == ".." || w == "." {
                bail!("无效的世界名");
            }
            let dir = game.join("saves").join(w);
            if !dir.join("level.dat").is_file() {
                bail!("找不到世界 {w}");
            }
            Ok(dir.join("datapacks"))
        }
        Kind::Modpack => bail!("整合包会创建新的存档，不能装进已有存档"),
        k => Ok(game.join(k.folder().unwrap_or("mods"))),
    }
}

/// 选出适合这个存档的版本；找不到完全匹配的时候，光影/资源包/数据包退而求其次并给出提醒。
async fn pick(dl: &Downloader, project: &str, kind: Kind, mc: &str, loader: &str) -> Result<(Ver, Option<String>)> {
    let loaders = if kind == Kind::Mod { loader_filter(loader) } else { vec![] };
    if let Some(v) = best(versions(dl, project, Some(mc), &loaders).await?) {
        return Ok((v, None));
    }
    if kind == Kind::Mod {
        bail!("这个模组没有适配 {mc} {} 的版本", if loader.is_empty() { "" } else { loader });
    }
    let any = best(versions(dl, project, None, &[]).await?).ok_or_else(|| anyhow!("这个项目还没有任何可下载的版本"))?;
    let note = format!("没有专门适配 {mc} 的版本，已选用最新版（{}），可能不兼容。", any_label(&any));
    Ok((any, Some(note)))
}

fn any_label(v: &Ver) -> String {
    match v.game_versions.last() {
        Some(g) => format!("{} · {g}", v.number),
        None => v.number.clone(),
    }
}

/// 下载地址只信任 Modrinth 的 CDN 和常见的代码托管站，整合包里的链接同样要过这一关。
fn trusted_url(u: &str) -> bool {
    ["https://cdn.modrinth.com/", "https://github.com/", "https://raw.githubusercontent.com/", "https://gitlab.com/", "https://api.modrinth.com/"].iter().any(|p| u.starts_with(p))
}

fn file_task(dir: &Path, f: &VFile) -> Result<Task> {
    if !trusted_url(&f.url) {
        bail!("下载地址不在受信任的站点上: {}", f.url);
    }
    if f.name.is_empty() || f.name.contains(['/', '\\']) || f.name.starts_with('.') {
        bail!("无效的文件名: {}", f.name);
    }
    Ok(Task { url: f.url.clone(), dest: dir.join(&f.name), sha1: f.hashes.get("sha1").cloned(), size: (f.size > 0).then_some(f.size) })
}

pub async fn install(cfg: &Config, dl: &Downloader, a: &InstallArgs, sink: &Sink) -> Result<Installed> {
    let kind = a.kind.unwrap_or(Kind::Mod);
    let (mc, loader, _) = instance::describe(&cfg.root, &a.instance)?;
    if kind == Kind::Mod && loader.is_empty() {
        bail!("这是原版存档，没有模组加载器，装不了模组。请先给它安装 Fabric、Forge 等加载器。");
    }
    let dir = dest_dir(cfg, &a.instance, kind, a.world.as_deref())?;
    let have = instance::installed_projects(&dir);
    let mut out = Installed::default();

    let (root, warn) = match &a.version {
        Some(v) => (version_by_id(dl, v).await?, None),
        None => pick(dl, safe_id(&a.project)?, kind, &mc, &loader).await?,
    };
    out.warnings.extend(warn);

    // 必需的前置依赖（只对模组做；光影、资源包的依赖通常是别的模组，由用户自己决定）
    let mut chosen: Vec<(Ver, String)> = vec![(root.clone(), a.title.clone())];
    if kind == Kind::Mod {
        let plan = plan_deps(dl, &root, &mc, &loader, &have).await;
        chosen.extend(plan.resolved.into_iter().map(|v| (v, String::new())));
        out.warnings.extend(plan.missing.iter().map(|p| format!("前置模组 {p} 没有适配 {mc} 的版本，需要你自己处理。")));
    }

    let mut tasks = vec![];
    for (v, _) in &chosen {
        let f = v.primary().ok_or_else(|| anyhow!("版本 {} 没有可下载的文件", v.number))?;
        tasks.push(file_task(&dir, f)?);
    }
    dl.fetch_all(tasks, kind.label(), sink).await?;

    let mut index = instance::read_index(&dir);
    for (i, (v, title)) in chosen.iter().enumerate() {
        let f = v.primary().expect("checked above");
        index.insert(f.name.clone(), IndexEntry { project: v.project_id.clone(), version: v.id.clone(), title: if title.is_empty() { v.name.clone() } else { title.clone() } });
        out.files.push(f.name.clone());
        if i > 0 {
            out.dependencies.push(f.name.clone());
        }
    }
    instance::write_index(&dir, &index)?;
    Ok(out)
}


// ---------- 前置依赖 ----------

#[derive(Default)]
struct DepPlan {
    /// 需要自动装上的前置（不含已经装了的），按依赖顺序。
    resolved: Vec<Ver>,
    /// 必需、已经装了的前置的项目 id。
    present: Vec<String>,
    /// 必需，但找不到适配这个存档的版本。
    missing: Vec<String>,
    /// 可选的前置（不会自动装）。
    optional: Vec<String>,
}

/// 沿着依赖关系往下找：必需的前置要么已经装了，要么挑一个适配的版本，要么记为找不到。
async fn plan_deps(dl: &Downloader, root: &Ver, mc: &str, loader: &str, have: &HashMap<String, String>) -> DepPlan {
    let mut plan = DepPlan::default();
    let mut seen: HashSet<String> = HashSet::from([root.project_id.clone()]);
    let mut queue = vec![root.clone()];
    while let Some(v) = queue.pop() {
        for d in &v.deps {
            let Some(pid) = d.project_id.clone() else { continue };
            match d.dependency_type.as_str() {
                "optional" => {
                    if seen.insert(pid.clone()) {
                        plan.optional.push(pid);
                    }
                }
                "required" => {
                    if !seen.insert(pid.clone()) || plan.resolved.len() >= MAX_DEPS {
                        continue;
                    }
                    if have.contains_key(&pid) {
                        plan.present.push(pid);
                        continue;
                    }
                    let found = match &d.version_id {
                        Some(id) => version_by_id(dl, id).await.ok(),
                        None => best(versions(dl, &pid, Some(mc), &loader_filter(loader)).await.unwrap_or_default()),
                    };
                    match found {
                        Some(dv) => {
                            queue.push(dv.clone());
                            plan.resolved.push(dv);
                        }
                        None => plan.missing.push(pid),
                    }
                }
                _ => {}
            }
        }
    }
    plan
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DepInfo {
    pub project: String,
    pub title: String,
    pub icon: Option<String>,
    /// installed 已安装 | auto 安装时会自动装上 | missing 找不到适配的版本 | optional 可选，不会自动装
    pub status: String,
    /// auto 时是将要装的版本号。
    pub version: String,
}

#[derive(Deserialize)]
struct RawProjectLite {
    id: String,
    title: String,
    icon_url: Option<String>,
}

/// 装这个模组会带上哪些前置：给界面在安装之前展示。
pub async fn dependencies(cfg: &Config, dl: &Downloader, a: &InstallArgs) -> Result<Vec<DepInfo>> {
    let (mc, loader, _) = instance::describe(&cfg.root, &a.instance)?;
    if a.kind.unwrap_or(Kind::Mod) != Kind::Mod || loader.is_empty() {
        return Ok(vec![]);
    }
    let dir = dest_dir(cfg, &a.instance, Kind::Mod, None)?;
    let have = instance::installed_projects(&dir);
    let root = match &a.version {
        Some(v) => version_by_id(dl, v).await?,
        None => pick(dl, safe_id(&a.project)?, Kind::Mod, &mc, &loader).await?.0,
    };
    let plan = plan_deps(dl, &root, &mc, &loader, &have).await;

    let mut rows: Vec<(String, &str, String)> = vec![]; // (项目 id, 状态, 版本号)
    rows.extend(plan.resolved.iter().map(|v| (v.project_id.clone(), "auto", v.number.clone())));
    rows.extend(plan.present.iter().map(|p| (p.clone(), "installed", String::new())));
    rows.extend(plan.missing.iter().map(|p| (p.clone(), "missing", String::new())));
    rows.extend(plan.optional.iter().map(|p| (p.clone(), if have.contains_key(p) { "installed" } else { "optional" }, String::new())));
    if rows.is_empty() {
        return Ok(vec![]);
    }
    let ids = serde_json::to_string(&rows.iter().map(|r| r.0.as_str()).collect::<Vec<_>>())?;
    let info: Vec<RawProjectLite> = get(dl, "/projects", &[("ids", ids)]).await.unwrap_or_default();
    let by_id: HashMap<&str, &RawProjectLite> = info.iter().map(|p| (p.id.as_str(), p)).collect();
    Ok(rows
        .into_iter()
        .map(|(project, status, version)| {
            let p = by_id.get(project.as_str());
            DepInfo { title: p.map(|p| p.title.clone()).unwrap_or_else(|| project.clone()), icon: p.and_then(|p| p.icon_url.clone()), project, status: status.to_string(), version }
        })
        .collect())
}

// ---------- 整合包 ----------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackIndex {
    #[serde(default)]
    name: String,
    #[serde(default)]
    files: Vec<PackFile>,
    #[serde(default)]
    dependencies: HashMap<String, String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackFile {
    path: String,
    #[serde(default)]
    hashes: HashMap<String, String>,
    env: Option<HashMap<String, String>>,
    #[serde(default)]
    downloads: Vec<String>,
    #[serde(default)]
    file_size: u64,
}

/// 整合包里的相对路径必须留在游戏目录里。
fn safe_rel(p: &str) -> Option<PathBuf> {
    let path = Path::new(p);
    let ok = !p.is_empty() && path.components().all(|c| matches!(c, Component::Normal(_)));
    ok.then(|| path.to_path_buf())
}

fn extract_overrides(pack: &Path, game_dir: &Path) -> Result<usize> {
    let mut z = zip::ZipArchive::new(std::fs::File::open(pack)?)?;
    let mut n = 0;
    for i in 0..z.len() {
        let mut f = z.by_index(i)?;
        if f.is_dir() {
            continue;
        }
        let Some(name) = f.enclosed_name() else { continue };
        let rel = match name.strip_prefix("overrides").or_else(|_| name.strip_prefix("client-overrides")) {
            Ok(r) if safe_rel(&r.to_string_lossy()).is_some() => r.to_path_buf(),
            _ => continue,
        };
        let to = game_dir.join(rel);
        if let Some(p) = to.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::io::copy(&mut f, &mut std::fs::File::create(&to)?)?;
        n += 1;
    }
    Ok(n)
}

fn read_pack_index(pack: &Path) -> Result<PackIndex> {
    let mut z = zip::ZipArchive::new(std::fs::File::open(pack).context("打不开整合包文件")?).context("这不是有效的 .mrpack 整合包")?;
    let mut s = String::new();
    z.by_name("modrinth.index.json").context("整合包里没有 modrinth.index.json（目前只支持 Modrinth 的 .mrpack）")?.read_to_string(&mut s)?;
    serde_json::from_str(&s).context("解析整合包清单失败")
}

/// 安装一个本地的 .mrpack：建一个新存档，返回它的版本 id。
pub async fn install_mrpack(cfg: &Config, dl: &Downloader, pack: &Path, name: Option<&str>, sink: &Sink) -> Result<String> {
    if !cfg.isolate {
        bail!("安装整合包需要先在设置里打开“版本隔离”，否则整合包的文件会和别的存档混在一起。");
    }
    let (p, pack_owned) = (read_pack_index(pack)?, pack.to_path_buf());
    let mc = p.dependencies.get("minecraft").cloned().ok_or_else(|| anyhow!("整合包没有声明游戏版本"))?;
    let loader = [("fabric-loader", Loader::Fabric), ("quilt-loader", Loader::Quilt), ("forge", Loader::Forge), ("neoforge", Loader::Neoforge)]
        .into_iter()
        .find_map(|(k, l)| p.dependencies.get(k).map(|v| (l, v.clone())));
    let title = name.filter(|n| !n.trim().is_empty()).map(str::to_string).or_else(|| (!p.name.is_empty()).then(|| p.name.clone())).unwrap_or_else(|| "Modpack".into());

    // 这次安装新建的版本目录（原版、加载器、整合包存档本身）：中途失败或被取消时整体删掉，不留半成品
    let before: HashSet<String> = crate::version::installed(&cfg.root).into_iter().collect();
    let mut guard = CleanupDirs { dirs: vec![], armed: true };
    let base = match &loader {
        Some((l, v)) => loader::install_loader(cfg, &mc, *l, v, sink).await?,
        None => {
            launch::install(cfg, &mc, sink).await?;
            mc.clone()
        }
    };
    // 新装上的原版和加载器只是整合包存档的组件，不单独出现在列表里
    let created: Vec<String> = crate::version::installed(&cfg.root).into_iter().filter(|v| !before.contains(v)).collect();
    guard.dirs.extend(created.iter().map(|v| cfg.root.join("versions").join(v)));
    if created.contains(&base) {
        instance::mark_component(&cfg.root, &base)?;
    }

    let id = new_version_id(&cfg.root, &title);
    let dir = cfg.root.join("versions").join(&id);
    guard.dirs.push(dir.clone());
    let game = instance::game_dir(cfg, &id);
    tokio::fs::create_dir_all(&game).await?;

    let mut tasks = vec![];
    for f in &p.files {
        if f.env.as_ref().and_then(|e| e.get("client")).is_some_and(|c| c == "unsupported") {
            continue;
        }
        let rel = safe_rel(&f.path).ok_or_else(|| anyhow!("整合包里有不安全的路径: {}", f.path))?;
        let url = f.downloads.iter().find(|u| trusted_url(u)).ok_or_else(|| anyhow!("{} 没有可信的下载地址", f.path))?;
        tasks.push(Task { url: url.clone(), dest: game.join(rel), sha1: f.hashes.get("sha1").cloned(), size: (f.file_size > 0).then_some(f.file_size) });
    }
    stage(sink, "下载整合包文件");
    dl.fetch_all(tasks, "整合包文件", sink).await?;
    let g = game.clone();
    tokio::task::spawn_blocking(move || extract_overrides(&pack_owned, &g)).await??;

    // 版本 JSON 最后写：只有写好了，这个存档才算存在
    // 整合包存档 = 一个继承自加载器版本的空壳版本，文件都在它自己的目录里
    let shell = serde_json::json!({ "id": id, "inheritsFrom": base, "type": "release" });
    tokio::fs::write(dir.join(format!("{id}.json")), serde_json::to_vec_pretty(&shell)?).await?;
    let mut meta = instance::meta(&cfg.root, &id);
    meta.name = title;
    meta.component = false;
    instance::save_meta(&cfg.root, &id, &meta)?;
    guard.armed = false;
    // 原版和加载器留下来（下一次装同一个版本的整合包可以直接用），只清理本次失败的半成品
    Ok(id)
}

/// 没有 `armed = false` 就被丢弃（出错、或者 future 被取消）时，删掉这些目录。
struct CleanupDirs {
    dirs: Vec<PathBuf>,
    armed: bool,
}

impl Drop for CleanupDirs {
    fn drop(&mut self) {
        if self.armed {
            for d in &self.dirs {
                let _ = std::fs::remove_dir_all(d);
            }
        }
    }
}

/// 从 Modrinth 下载并安装一个整合包。
pub async fn install_modpack(cfg: &Config, dl: &Downloader, project: &str, version: Option<&str>, title: &str, sink: &Sink) -> Result<String> {
    let v = match version {
        Some(v) => version_by_id(dl, v).await?,
        None => best(versions(dl, safe_id(project)?, None, &[]).await?).ok_or_else(|| anyhow!("这个整合包还没有可下载的版本"))?,
    };
    let f = v.files.iter().find(|f| f.name.ends_with(".mrpack")).ok_or_else(|| anyhow!("这个版本没有 .mrpack 文件"))?;
    let dir = cfg.data_dir.join("modpacks");
    let task = file_task(&dir, f)?;
    let dest = task.dest.clone();
    dl.fetch_all(vec![task], "整合包", sink).await?;
    install_mrpack(cfg, dl, &dest, Some(title), sink).await
}


// ---------- 更新检查 ----------

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    /// 索引里的文件名（不带 .disabled）。
    pub file: String,
    pub project: String,
    pub title: String,
    pub current: String,
    pub latest: String,
}

/// 检查某个文件夹里通过 FLC 装的内容有没有新版本：同一个项目在这个存档的游戏版本和加载器下，是否有比已装的更新的发布。
pub async fn check_updates(cfg: &Config, dl: &Downloader, inst_id: &str, kind: Kind) -> Result<Vec<UpdateInfo>> {
    use futures::StreamExt;
    let (mc, loader, _) = instance::describe(&cfg.root, inst_id)?;
    let dir = dest_dir(cfg, inst_id, kind, None)?;
    let loaders = if kind == Kind::Mod { loader_filter(&loader) } else { vec![] };
    let index: Vec<(String, IndexEntry)> = instance::read_index(&dir).into_iter().filter(|(f, _)| dir.join(f).exists() || dir.join(format!("{f}.disabled")).exists()).collect();
    if index.is_empty() {
        return Ok(vec![]);
    }
    // 已装版本的版本号和发布时间，一次请求取回
    let ids = serde_json::to_string(&index.iter().map(|(_, e)| e.version.as_str()).collect::<Vec<_>>())?;
    let installed: Vec<RawVer> = get(dl, "/versions", &[("ids", ids)]).await.unwrap_or_default();
    let installed: HashMap<String, Ver> = installed.into_iter().map(|v| (v.id.clone(), Ver::from(v))).collect();

    let mut found: Vec<UpdateInfo> = futures::stream::iter(index)
        .map(|(file, entry)| {
            let (mc, loaders, installed) = (mc.clone(), loaders.clone(), installed.get(&entry.version).cloned());
            async move {
                let best = best(versions(dl, &entry.project, Some(&mc), &loaders).await.ok()?)?;
                if best.id == entry.version {
                    return None;
                }
                // 已装的比“最佳”还新（比如装的是测试版）就不算更新
                if installed.as_ref().is_some_and(|i| i.date >= best.date) {
                    return None;
                }
                Some(UpdateInfo { file, project: entry.project, title: entry.title, current: installed.map(|i| i.number).unwrap_or_default(), latest: best.number })
            }
        })
        .buffer_unordered(6)
        .filter_map(|x| async move { x })
        .collect()
        .await;
    found.sort_by_key(|u| u.title.to_lowercase());
    Ok(found)
}

/// 把一个已装的内容更新到最新适配版：装好新文件后删掉旧文件；旧文件如果是停用状态，新文件也保持停用。
pub async fn update_one(cfg: &Config, dl: &Downloader, inst_id: &str, kind: Kind, file: &str, sink: &Sink) -> Result<Installed> {
    let dir = dest_dir(cfg, inst_id, kind, None)?;
    let entry = instance::read_index(&dir).get(file).cloned().ok_or_else(|| anyhow!("这个文件不是通过 FLC 安装的，没法自动更新"))?;
    let was_disabled = !dir.join(file).exists() && dir.join(format!("{file}.disabled")).exists();
    let args = InstallArgs { instance: inst_id.to_string(), kind: Some(kind), project: entry.project.clone(), title: entry.title.clone(), version: None, world: None };
    let done = install(cfg, dl, &args, sink).await?;
    // 新文件和旧文件同名说明没有变化；否则删掉旧的
    let new_name = done.files.first().cloned().unwrap_or_default();
    if new_name != file {
        for old in [file.to_string(), format!("{file}.disabled")] {
            if dir.join(&old).exists() {
                let _ = std::fs::remove_file(dir.join(&old));
            }
        }
        let mut index = instance::read_index(&dir);
        index.remove(file);
        instance::write_index(&dir, &index)?;
    }
    if was_disabled && kind == Kind::Mod && !new_name.is_empty() {
        instance::set_mod_enabled(&dir, &new_name, false)?;
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn args(kind: Kind) -> SearchArgs {
        SearchArgs { kind: Some(kind), ..Default::default() }
    }

    #[test]
    fn facets_follow_kind_version_and_loader() {
        let mut a = args(Kind::Mod);
        a.mc = Some("1.21.1".into());
        a.loader = Some("quilt".into());
        a.categories = vec!["optimization".into(), "bad;drop".into()];
        let f: Vec<Vec<String>> = serde_json::from_str(&facets(&a, Kind::Mod)).unwrap();
        assert!(f.contains(&vec!["project_type:mod".to_string()]));
        assert!(f.contains(&vec!["categories!=datapack".to_string()]));
        assert!(f.contains(&vec!["versions:1.21.1".to_string()]));
        assert!(f.contains(&vec!["categories:fabric".to_string(), "categories:quilt".to_string()]));
        assert!(f.contains(&vec!["categories:optimization".to_string()]));
        assert!(!format!("{f:?}").contains("bad;drop"));

        // 多个存档：版本和加载器各自是“任一”
        let mut multi = args(Kind::Mod);
        multi.mcs = vec!["1.20.1".into(), "1.21.1".into()];
        multi.loaders = vec!["fabric".into(), "neoforge".into(), "legacyfabric".into()];
        let f: Vec<Vec<String>> = serde_json::from_str(&facets(&multi, Kind::Mod)).unwrap();
        assert!(f.contains(&vec!["versions:1.20.1".to_string(), "versions:1.21.1".to_string()]));
        assert!(f.contains(&vec!["categories:fabric".to_string(), "categories:legacy-fabric".to_string(), "categories:neoforge".to_string()]));

        // 光影、资源包不按加载器过滤；数据包用 datapack 分类
        a.loader = Some("fabric".into());
        assert!(!facets(&a, Kind::Shader).contains("fabric"));
        assert!(facets(&a, Kind::Datapack).contains("categories:datapack"));
    }

    #[test]
    fn search_hits_split_loaders_from_tags_and_drop_snapshots() {
        let raw: RawPage = serde_json::from_str(
            r#"{"hits":[{"project_id":"AANobbMI","slug":"sodium","title":"Sodium","description":"fast","author":"jelly","icon_url":null,
            "downloads":10,"follows":2,"date_modified":"2026-01-01T00:00:00Z","categories":["fabric","optimization","neoforge"],
            "versions":["1.20.1","1.21.1","24w40a","1.21-pre1","26.3"],"client_side":"required","server_side":"unsupported"}],"total_hits":1}"#,
        )
        .unwrap();
        let h = Hit::from(raw.hits.into_iter().next().unwrap());
        assert_eq!(h.loaders, ["fabric", "neoforge"]);
        assert_eq!(h.tags, ["optimization"]);
        assert_eq!(h.game_versions, ["1.20.1", "1.21.1", "26.3"]);
    }

    #[test]
    fn best_prefers_release_then_newest() {
        let v = |id: &str, kind: &str, date: &str| Ver {
            id: id.into(), project_id: "p".into(), name: id.into(), number: id.into(), kind: kind.into(), date: date.into(),
            game_versions: vec![], loaders: vec![], downloads: 0, files: vec![], deps: vec![],
        };
        let picked = best(vec![v("beta-new", "beta", "2026-03"), v("rel-old", "release", "2026-01"), v("rel-new", "release", "2026-02")]).unwrap();
        assert_eq!(picked.id, "rel-new");
        assert_eq!(best(vec![v("a", "alpha", "1"), v("b", "beta", "0")]).unwrap().id, "b");
        assert!(best(vec![]).is_none());
    }

    #[test]
    fn paths_and_urls_are_checked() {
        assert!(safe_rel("mods/a.jar").is_some());
        assert!(safe_rel("../evil.jar").is_none());
        assert!(safe_rel("/abs/evil.jar").is_none());
        assert!(safe_rel("mods/../../x").is_none());
        assert!(safe_id("AANobbMI").is_ok() && safe_id("a/b").is_err() && safe_id("").is_err());
        assert!(trusted_url("https://cdn.modrinth.com/data/x/versions/y/z.jar"));
        assert!(!trusted_url("https://evil.example/x.jar"));
        assert!(!trusted_url("https://cdn.modrinth.com.evil.example/x.jar"));
    }

    #[test]
    fn version_ids_from_pack_names_are_safe_and_unique() {
        let root = std::env::temp_dir().join(format!("flc-pack-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(new_version_id(&root, "My: Pack/1?"), "My Pack 1");
        std::fs::create_dir_all(root.join("versions/My Pack 1")).unwrap();
        assert_eq!(new_version_id(&root, "My: Pack/1?"), "My Pack 1 (2)");
        assert_eq!(new_version_id(&root, "..."), "Instance");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn mrpack_overrides_are_extracted_and_traversal_is_ignored() {
        let root = std::env::temp_dir().join(format!("flc-mrpack-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let pack = root.join("p.mrpack");
        let mut z = zip::ZipWriter::new(std::fs::File::create(&pack).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        z.start_file("modrinth.index.json", o).unwrap();
        z.write_all(br#"{"formatVersion":1,"name":"Demo","files":[{"path":"mods/a.jar","hashes":{"sha1":"x"},"downloads":["https://cdn.modrinth.com/a.jar"],"fileSize":3,"env":{"client":"required","server":"required"}}],"dependencies":{"minecraft":"1.21.1","fabric-loader":"0.16.0"}}"#).unwrap();
        z.start_file("overrides/config/a.toml", o).unwrap();
        z.write_all(b"x=1").unwrap();
        z.start_file("client-overrides/options.txt", o).unwrap();
        z.write_all(b"fov:1").unwrap();
        z.start_file("overrides/../../escape.txt", o).unwrap();
        z.write_all(b"no").unwrap();
        z.finish().unwrap();

        let idx = read_pack_index(&pack).unwrap();
        assert_eq!((idx.name.as_str(), idx.files.len(), idx.dependencies["minecraft"].as_str()), ("Demo", 1, "1.21.1"));
        let game = root.join("game");
        assert_eq!(extract_overrides(&pack, &game).unwrap(), 2);
        assert!(game.join("config/a.toml").is_file() && game.join("options.txt").is_file());
        assert!(!root.join("escape.txt").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn modpack_shell_version_resolves_through_the_loader_version() {
        let root = std::env::temp_dir().join(format!("flc-shell-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let put = |id: &str, json: &str| {
            std::fs::create_dir_all(root.join("versions").join(id)).unwrap();
            std::fs::write(root.join("versions").join(id).join(format!("{id}.json")), json).unwrap();
        };
        put("1.21.1", r#"{"id":"1.21.1","type":"release","mainClass":"vanilla","assetIndex":{"id":"17","url":"u"},"libraries":[]}"#);
        put("fabric-loader-0.16.0-1.21.1", r#"{"id":"fabric-loader-0.16.0-1.21.1","inheritsFrom":"1.21.1","mainClass":"knot"}"#);
        let shell = serde_json::json!({ "id": "Demo", "inheritsFrom": "fabric-loader-0.16.0-1.21.1", "type": "release" });
        put("Demo", &shell.to_string());
        let r = crate::version::resolve(&root, "Demo").unwrap();
        assert_eq!((r.main_class.as_str(), r.jar_id.as_str(), r.id.as_str()), ("knot", "1.21.1", "Demo"));
        let info = instance::describe(&root, "Demo").unwrap();
        assert_eq!(info, ("1.21.1".to_string(), "fabric".to_string(), "0.16.0".to_string()));
        let listed = instance::list(&root);
        let demo = listed.iter().find(|i| i.id == "Demo").unwrap();
        assert_eq!((demo.loader.as_str(), demo.mc.as_str()), ("fabric", "1.21.1"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn loader_names_map_to_modrinth_categories() {
        assert_eq!(loader_filter("quilt"), ["quilt", "fabric"]);
        assert_eq!(loader_filter("neoforge"), ["neoforge"]);
        assert_eq!(loader_filter("legacyfabric"), ["legacy-fabric"]);
        assert!(loader_filter("").is_empty());
    }
}
