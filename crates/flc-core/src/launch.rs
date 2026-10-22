use crate::auth::Account;
use crate::download::{Downloader, Mirror, Task};
use crate::event::{stage, Event, Sink};
use crate::java;
use crate::model::*;
use crate::rules::{self, OS_NAME};
use crate::version::{self, maven_path, Resolved};
use anyhow::{bail, Context, Result};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

pub const LAUNCHER_NAME: &str = "FLC";
const NATIVES_MARKER: &str = ".flc-ok";

#[derive(Clone, Debug)]
pub struct Config {
    /// 游戏下载位置：versions / libraries / assets 所在的 .minecraft 目录。
    pub root: PathBuf,
    /// FLC 自己的数据位置：下载的 Java 运行时、加载器安装器缓存等。
    pub data_dir: PathBuf,
    pub mirror: Mirror,
    /// 版本隔离：每个版本使用自己的 saves/mods/config 目录。
    pub isolate: bool,
    pub max_mem_mb: Option<u32>,
    pub java: Option<PathBuf>,
    pub extra_jvm: Vec<String>,
    pub window: Option<(u32, u32)>,
    /// 游戏内语言，如 `zh_cn`、`en_us`；每次启动前写入该版本的 options.txt。None 表示不干预。
    pub lang: Option<String>,
    /// 离线账号的皮肤；None 表示不替换游戏默认皮肤（并清掉之前生成的皮肤包）。
    pub skin: Option<crate::skin::Skin>,
}

impl Config {
    pub fn new(root: PathBuf) -> Self {
        Self { root, data_dir: Config::default_data_dir(), mirror: Mirror::Auto, isolate: true, max_mem_mb: None, java: None, extra_jvm: vec![], window: None, lang: None, skin: None }
    }

    pub fn default_data_dir() -> PathBuf {
        if let Some(a) = std::env::var_os("APPDATA") {
            return PathBuf::from(a).join("FLC");
        }
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".flc")).unwrap_or_else(|| PathBuf::from(".flc"))
    }

    pub fn default_root() -> PathBuf {
        if let Some(a) = std::env::var_os("APPDATA") {
            return PathBuf::from(a).join(".minecraft");
        }
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".minecraft")).unwrap_or_else(|| PathBuf::from(".minecraft"))
    }
}

pub struct RunningGame {
    pub pid: u32,
    /// 点击启动到进程创建的耗时（毫秒）。
    pub prepare_ms: u64,
    kill_tx: mpsc::Sender<()>,
    exit: JoinHandle<Option<i32>>,
}

impl RunningGame {
    pub async fn kill(&self) {
        let _ = self.kill_tx.send(()).await;
    }
    pub async fn wait(self) -> Option<i32> {
        self.exit.await.ok().flatten()
    }
}

/// 下载版本 JSON。已存在则直接返回，不访问网络。
pub async fn install_version(cfg: &Config, id: &str, sink: &Sink) -> Result<()> {
    let dest = cfg.root.join("versions").join(id).join(format!("{id}.json"));
    if dest.is_file() {
        return Ok(());
    }
    let dl = Downloader::new(cfg.mirror);
    stage(sink, "获取版本清单");
    let manifest = dl.manifest().await?;
    let entry = manifest.versions.iter().find(|v| v.id == id).with_context(|| format!("官方清单里没有版本 {id}"))?;
    dl.fetch_all(vec![Task { url: entry.url.clone(), dest, sha1: entry.sha1.clone(), size: None }], "版本信息", sink).await
}

struct Plan {
    tasks: Vec<Task>,
    classpath: Vec<PathBuf>,
    natives: Vec<(PathBuf, Vec<String>)>,
}

fn plan_files(root: &Path, r: &Resolved, features: &HashMap<&str, bool>) -> Plan {
    let libs = root.join("libraries");
    let mut plan = Plan { tasks: vec![], classpath: vec![], natives: vec![] };
    let arch = if cfg!(target_pointer_width = "64") { "64" } else { "32" };

    for lib in &r.libraries {
        if !rules::allowed(&lib.rules, features) {
            continue;
        }
        let dls = lib.downloads.as_ref();
        let art = dls.and_then(|d| d.artifact.as_ref());
        // 只有 natives 分类包、没有主 jar 的库不进 classpath。
        if art.is_some() || lib.natives.is_none() {
            let path = art.and_then(|a| a.path.clone()).unwrap_or_else(|| maven_path(&lib.name));
            let url = match art {
                Some(a) if !a.url.is_empty() => a.url.clone(),
                _ => match (&lib.url, dls) {
                    (Some(base), _) => format!("{}{}", if base.ends_with('/') { base.clone() } else { format!("{base}/") }, path),
                    (None, None) => format!("https://libraries.minecraft.net/{path}"),
                    _ => String::new(),
                },
            };
            let dest = libs.join(&path);
            if !url.is_empty() {
                plan.tasks.push(Task { url, dest: dest.clone(), sha1: art.and_then(|a| a.sha1.clone()), size: art.and_then(|a| a.size) });
            }
            plan.classpath.push(dest);
        }
        if let Some(cls) = lib.natives.as_ref().and_then(|n| n.get(OS_NAME)).map(|c| c.replace("${arch}", arch)) {
            if let Some(a) = dls.and_then(|d| d.classifiers.get(&cls)) {
                let path = a.path.clone().unwrap_or_else(|| maven_path(&format!("{}:{cls}", lib.name)));
                let dest = libs.join(path);
                plan.tasks.push(Task { url: a.url.clone(), dest: dest.clone(), sha1: a.sha1.clone(), size: a.size });
                plan.natives.push((dest, lib.extract.as_ref().map(|e| e.exclude.clone()).unwrap_or_default()));
            }
        }
    }

    let jar = root.join("versions").join(&r.jar_id).join(format!("{}.jar", r.jar_id));
    if let Some(c) = &r.client {
        plan.tasks.push(Task { url: c.url.clone(), dest: jar.clone(), sha1: c.sha1.clone(), size: c.size });
    }
    plan.classpath.push(jar);
    plan
}

fn extract_natives(jars: &[(PathBuf, Vec<String>)], dir: &Path) -> Result<()> {
    let marker = dir.join(NATIVES_MARKER);
    if marker.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(dir)?;
    for (jar, exclude) in jars {
        let mut z = zip::ZipArchive::new(std::fs::File::open(jar).with_context(|| format!("打开 {}", jar.display()))?)?;
        for i in 0..z.len() {
            let mut f = z.by_index(i)?;
            let name = f.name().to_string();
            if f.is_dir() || name.starts_with("META-INF") || exclude.iter().any(|e| name.starts_with(e.as_str())) {
                continue;
            }
            let Some(base) = Path::new(&name).file_name() else { continue };
            std::io::copy(&mut f, &mut std::fs::File::create(dir.join(base))?)?;
        }
    }
    std::fs::write(marker, b"")?;
    Ok(())
}

/// 旧版本（virtual / map_to_resources）需要把哈希命名的资源还原成真实文件名。
fn restore_legacy_assets(index: &AssetIndex, assets: &Path, target: &Path) -> Result<()> {
    for (name, obj) in &index.objects {
        let to = target.join(name);
        if to.exists() {
            continue;
        }
        let from = assets.join("objects").join(&obj.hash[..2]).join(&obj.hash);
        if let Some(p) = to.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::copy(from, to)?;
    }
    Ok(())
}

fn subst(s: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = s.to_string();
    for (k, v) in vars {
        out = out.replace(&format!("${{{k}}}"), v);
    }
    out
}

fn expand(args: &[Arg], features: &HashMap<&str, bool>, vars: &HashMap<&str, String>, out: &mut Vec<String>) {
    for a in args {
        match a {
            Arg::Plain(s) => out.push(subst(s, vars)),
            Arg::Ruled { rules, value } => {
                if !rules::allowed(rules, features) {
                    continue;
                }
                match value {
                    ArgValue::One(s) => out.push(subst(s, vars)),
                    ArgValue::Many(v) => out.extend(v.iter().map(|s| subst(s, vars))),
                }
            }
        }
    }
}

fn auto_memory_mb() -> u32 {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let total = (sys.total_memory() / 1024 / 1024) as u32;
    ((total / 3).clamp(1024, 6144) / 256) * 256
}

/// 下载、校验并展开运行所需的一切（版本 JSON 之外的 jar、依赖库、资源、Java、natives），但不启动游戏。
struct Prepared {
    r: Resolved,
    classpath: Vec<PathBuf>,
    natives_dir: PathBuf,
    game_dir: PathBuf,
    java_path: PathBuf,
    assets: PathBuf,
    game_assets: PathBuf,
}

async fn prepare(cfg: &Config, id: &str, sink: &Sink, t0: Instant) -> Result<Prepared> {
    let root = &cfg.root;
    let dl = Downloader::new(cfg.mirror);

    sink(Event::Stage { name: "解析版本".into(), ms: Some(t0.elapsed().as_millis() as u64) });
    let r = version::resolve(root, id)?;
    let mut features: HashMap<&str, bool> = HashMap::new();
    features.insert("has_custom_resolution", cfg.window.is_some());

    let game_dir = if cfg.isolate { root.join("versions").join(id) } else { root.clone() };
    std::fs::create_dir_all(&game_dir)?;
    let assets = root.join("assets");

    // 资源索引必须先到位，后面才知道要下哪些资源文件。
    let idx = &r.asset_index;
    let idx_path = assets.join("indexes").join(format!("{}.json", idx.id));
    dl.fetch_all(vec![Task { url: idx.url.clone(), dest: idx_path.clone(), sha1: idx.sha1.clone(), size: idx.size }], "资源索引", &sink).await?;
    let index: AssetIndex = serde_json::from_slice(&std::fs::read(&idx_path)?).context("解析资源索引")?;

    let mut plan = plan_files(root, &r, &features);
    // 资源有几千个小文件，逐个 stat 在 Windows 上要一秒多。完整校验通过一次后留下标记，之后启动直接跳过。
    let assets_ok = assets.join("indexes").join(format!("{}.ok", idx.id));
    let skip_assets = assets_ok.exists();
    for o in index.objects.values().filter(|_| !skip_assets) {
        let p = &o.hash[..2];
        plan.tasks.push(Task {
            url: format!("https://resources.download.minecraft.net/{p}/{}", o.hash),
            dest: assets.join("objects").join(p).join(&o.hash),
            sha1: Some(o.hash.clone()),
            size: Some(o.size),
        });
    }

    sink(Event::Stage { name: "检查文件".into(), ms: Some(t0.elapsed().as_millis() as u64) });
    let tasks = std::mem::take(&mut plan.tasks);
    let java_dirs = [cfg.data_dir.as_path(), root.as_path()];
    let (files, java_path) = tokio::join!(
        dl.fetch_all(tasks, "游戏文件", sink),
        java::ensure(&dl, &java_dirs, r.java.as_ref(), cfg.java.as_deref(), sink)
    );
    files?;
    let java_path = java_path?;
    // 继承自原版的版本（Fabric、Forge 等）要以“自己的名字”把客户端 jar 放进 classpath：
    // 现代 Forge/NeoForge 的 -DignoreList 里正是按 ${version_name}.jar 忽略它，
    // 否则原版 jar 会和补丁后的客户端模块冲突（"Modules _1._21._1 and minecraft export package ..."）。
    if r.jar_id != id {
        let own = root.join("versions").join(id).join(format!("{id}.jar"));
        let parent = root.join("versions").join(&r.jar_id).join(format!("{}.jar", r.jar_id));
        if !own.exists() && parent.exists() {
            std::fs::copy(&parent, &own).with_context(|| format!("复制客户端 jar 到 {}", own.display()))?;
        }
        if own.exists() {
            if let Some(last) = plan.classpath.last_mut() {
                *last = own;
            }
        }
    }
    if !skip_assets {
        std::fs::write(&assets_ok, b"")?;
    }

    sink(Event::Stage { name: "准备运行环境".into(), ms: Some(t0.elapsed().as_millis() as u64) });
    let natives_dir = root.join("versions").join(id).join("natives");
    {
        let (jars, dir) = (plan.natives.clone(), natives_dir.clone());
        tokio::task::spawn_blocking(move || extract_natives(&jars, &dir)).await??;
    }
    let game_assets = if index.is_virtual || index.map_to_resources {
        let target = if index.map_to_resources { game_dir.join("resources") } else { assets.join("virtual").join("legacy") };
        let (ix, a, t) = (index.clone(), assets.clone(), target.clone());
        tokio::task::spawn_blocking(move || restore_legacy_assets(&ix, &a, &t)).await??;
        target
    } else {
        assets.clone()
    };
    Ok(Prepared { r, classpath: plan.classpath, natives_dir, game_dir, java_path, assets, game_assets })
}

/// 只下载不启动：版本 JSON、客户端、依赖库、资源与 Java 一次备齐。
pub async fn install(cfg: &Config, id: &str, sink: &Sink) -> Result<()> {
    let t0 = Instant::now();
    install_version(cfg, id, sink).await?;
    prepare(cfg, id, sink, t0).await?;
    Ok(())
}

/// 1.11 之前的版本语言代码写作 `zh_CN`，之后写作 `zh_cn`。资源索引 id 在这些老版本里形如 `1.8`、`legacy`。
fn legacy_lang_format(asset_index_id: &str) -> bool {
    if asset_index_id == "legacy" || asset_index_id.starts_with("pre-") {
        return true;
    }
    asset_index_id
        .strip_prefix("1.")
        .and_then(|rest| rest.split('.').next())
        .and_then(|minor| minor.parse::<u32>().ok())
        .is_some_and(|minor| minor < 11)
}

/// 把 `lang:xx_xx` 写进 options.txt，其余设置原样保留；文件不存在就新建。
fn apply_lang(game_dir: &Path, lang: &str, legacy: bool) -> Result<()> {
    let code = match (legacy, lang.split_once('_')) {
        (true, Some((l, r))) => format!("{}_{}", l.to_lowercase(), r.to_uppercase()),
        _ => lang.to_lowercase(),
    };
    let path = game_dir.join("options.txt");
    let old = std::fs::read_to_string(&path).unwrap_or_default();
    let line = format!("lang:{code}");
    let mut found = false;
    let mut lines: Vec<String> = old
        .lines()
        .map(|l| {
            if l.starts_with("lang:") {
                found = true;
                line.clone()
            } else {
                l.to_string()
            }
        })
        .collect();
    if !found {
        lines.push(line);
    }
    let new = lines.join("\n") + "\n";
    if new != old {
        std::fs::write(&path, new)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lang_code_format_by_era() {
        assert!(legacy_lang_format("1.8"));
        assert!(legacy_lang_format("legacy"));
        assert!(!legacy_lang_format("1.12"));
        assert!(!legacy_lang_format("17"));
    }

    #[test]
    fn options_txt_lang_is_replaced_and_rest_kept() {
        let dir = std::env::temp_dir().join(format!("flc-lang-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("options.txt"), "fov:0.5\nlang:en_us\nmusic:1.0\n").unwrap();
        apply_lang(&dir, "zh_cn", false).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("options.txt")).unwrap(), "fov:0.5\nlang:zh_cn\nmusic:1.0\n");
        apply_lang(&dir, "zh_cn", true).unwrap();
        assert!(std::fs::read_to_string(dir.join("options.txt")).unwrap().contains("lang:zh_CN"));
        let fresh = dir.join("new");
        std::fs::create_dir_all(&fresh).unwrap();
        apply_lang(&fresh, "en_us", false).unwrap();
        assert_eq!(std::fs::read_to_string(fresh.join("options.txt")).unwrap(), "lang:en_us\n");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// 输出里出现这些行时，说明游戏已经走到加载资源之后，窗口基本出现了。
fn looks_ready(line: &str) -> bool {
    line.contains("Sound engine started")
}

pub async fn launch(cfg: &Config, id: &str, account: &Account, sink: Sink) -> Result<RunningGame> {
    let t0 = Instant::now();
    let root = &cfg.root;
    let Prepared { r, classpath: cp, natives_dir, game_dir, java_path, assets, game_assets } = prepare(cfg, id, &sink, t0).await?;
    let mut features: HashMap<&str, bool> = HashMap::new();
    features.insert("has_custom_resolution", cfg.window.is_some());
    let idx_name = r.asset_index.id.clone();
    if let Err(e) = crate::skin::apply(&game_dir, &r.jar_id, &r.kind, cfg.skin.as_ref()) {
        sink(Event::Log { line: format!("[FLC] 应用皮肤失败: {e}") });
    }
    if let Some(lang) = &cfg.lang {
        if let Err(e) = apply_lang(&game_dir, lang, legacy_lang_format(&idx_name)) {
            sink(Event::Log { line: format!("[FLC] 写入游戏语言失败: {e}") });
        }
    }

    let sep = if cfg!(windows) { ";" } else { ":" };
    let classpath = cp.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>().join(sep);
    let (w, h) = cfg.window.unwrap_or((854, 480));
    let vars: HashMap<&str, String> = HashMap::from([
        ("auth_player_name", account.name.clone()),
        ("auth_uuid", account.uuid.clone()),
        ("auth_access_token", account.access_token.clone()),
        ("auth_session", account.session()),
        ("auth_xuid", String::new()),
        ("clientid", String::new()),
        ("user_type", account.user_type.clone()),
        ("user_properties", "{}".into()),
        ("version_name", id.to_string()),
        ("version_type", LAUNCHER_NAME.to_string()),
        ("game_directory", game_dir.to_string_lossy().into_owned()),
        ("assets_root", assets.to_string_lossy().into_owned()),
        ("game_assets", game_assets.to_string_lossy().into_owned()),
        ("assets_index_name", idx_name),
        ("natives_directory", natives_dir.to_string_lossy().into_owned()),
        ("library_directory", root.join("libraries").to_string_lossy().into_owned()),
        ("classpath_separator", sep.to_string()),
        ("classpath", classpath),
        ("launcher_name", LAUNCHER_NAME.to_string()),
        ("launcher_version", env!("CARGO_PKG_VERSION").to_string()),
        ("resolution_width", w.to_string()),
        ("resolution_height", h.to_string()),
    ]);

    let mem = cfg.max_mem_mb.unwrap_or_else(auto_memory_mb);
    let mut args: Vec<String> = vec![
        format!("-Xmx{mem}M"),
        "-XX:+UseG1GC".into(),
        "-XX:+UnlockExperimentalVMOptions".into(),
        "-XX:G1NewSizePercent=20".into(),
        "-XX:G1ReservePercent=20".into(),
        "-XX:MaxGCPauseMillis=50".into(),
        "-XX:G1HeapRegionSize=32M".into(),
        "-Dfile.encoding=UTF-8".into(),
        "-Dlog4j2.formatMsgNoLookups=true".into(),
    ];
    args.extend(cfg.extra_jvm.iter().cloned());
    if r.jvm_args.is_empty() {
        args.push(format!("-Djava.library.path={}", natives_dir.display()));
    } else {
        expand(&r.jvm_args, &features, &vars, &mut args);
    }
    if !args.iter().any(|a| a == "-cp" || a == "-classpath") {
        args.push("-cp".into());
        args.push(vars["classpath"].clone());
    }
    args.push(r.main_class.clone());
    if let Some(legacy) = &r.legacy_args {
        args.extend(legacy.split_whitespace().map(|s| subst(s, &vars)));
        if cfg.window.is_some() {
            args.extend(["--width".into(), w.to_string(), "--height".into(), h.to_string()]);
        }
    }
    expand(&r.game_args, &features, &vars, &mut args);
    if args.iter().any(|a| a.contains("${")) {
        sink(Event::Log { line: "[FLC] 警告: 启动参数里有未替换的占位符".into() });
    }

    sink(Event::Stage { name: "启动游戏".into(), ms: Some(t0.elapsed().as_millis() as u64) });
    sink(Event::Log { line: format!("[FLC] java = {}  内存 = {mem}M", java_path.display()) });
    let mut cmd = tokio::process::Command::new(&java_path);
    cmd.args(&args).current_dir(&game_dir).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let mut child = cmd.spawn().with_context(|| format!("无法启动 {}", java_path.display()))?;
    let Some(pid) = child.id() else { bail!("进程已退出") };
    let prepare_ms = t0.elapsed().as_millis() as u64;
    sink(Event::Stage { name: "进程已创建".into(), ms: Some(prepare_ms) });
    sink(Event::Started { pid });

    // 最近的输出始终保留一小段，游戏异常退出时用来告诉用户原因，不依赖界面是否开启日志显示。
    let tail: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
    let ready = Arc::new(AtomicBool::new(false));
    for pipe in [child.stdout.take().map(|s| Box::new(s) as Box<dyn tokio::io::AsyncRead + Send + Unpin>), child.stderr.take().map(|s| Box::new(s) as _)].into_iter().flatten() {
        let (sink, tail, ready) = (sink.clone(), tail.clone(), ready.clone());
        tokio::spawn(async move {
            let mut lines = BufReader::new(pipe).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if !ready.load(Ordering::Relaxed) && looks_ready(&line) {
                    ready.store(true, Ordering::Relaxed);
                    sink(Event::Ready);
                }
                {
                    let mut t = tail.lock().unwrap();
                    if t.len() >= 80 {
                        t.pop_front();
                    }
                    t.push_back(line.clone());
                }
                sink(Event::Log { line });
            }
        });
    }

    let started = Instant::now();
    let (kill_tx, mut kill_rx) = mpsc::channel::<()>(1);
    let exit = tokio::spawn(async move {
        let status = tokio::select! {
            s = child.wait() => s.ok(),
            _ = kill_rx.recv() => {
                let _ = child.kill().await;
                child.wait().await.ok()
            }
        };
        let code = status.and_then(|s| s.code());
        tokio::time::sleep(Duration::from_millis(150)).await; // 让读管道的任务把最后几行吃完
        let tail = tail.lock().unwrap().iter().cloned().collect();
        sink(Event::Exited { code, ready: ready.load(Ordering::Relaxed), secs: started.elapsed().as_secs(), tail });
        code
    });
    Ok(RunningGame { pid, prepare_ms, kill_tx, exit })
}

