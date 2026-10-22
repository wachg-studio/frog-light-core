use anyhow::{bail, Result};
use flc_core::{install_version, java, launch, Account, Config, Downloader, Event, Mirror, Sink};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

fn sink() -> Sink {
    let last = std::sync::Mutex::new(String::new());
    Arc::new(move |e| match e {
        Event::Stage { name, ms: None } => println!("==> {name}"),
        Event::Stage { name, ms: Some(ms) } => println!("==> {name} ({ms} ms)"),
        Event::Progress { label, done, total, files_done, files_total, .. } => {
            let line = format!("    {label}: {files_done}/{files_total} 个文件, {:.1}/{:.1} MB", done as f64 / 1e6, total as f64 / 1e6);
            let mut l = last.lock().unwrap();
            if *l != line {
                println!("{line}");
                *l = line;
            }
        }
        Event::Log { line } => println!("[game] {line}"),
        Event::Started { pid } => println!("==> 游戏进程 pid={pid}"),
        Event::Ready => println!("==> 游戏资源已就绪"),
        Event::Exited { code, ready, secs, .. } => println!("==> 游戏退出, code={code:?} 就绪过={ready} 运行{secs}秒"),
    })
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut cfg = Config::new(flag(&args, "--root").map(PathBuf::from).unwrap_or_else(Config::default_root));
    cfg.mirror = match flag(&args, "--mirror").as_deref() {
        Some("official") => Mirror::Official,
        Some("bmcl") => Mirror::Bmcl,
        _ => Mirror::Auto,
    };
    cfg.java = flag(&args, "--java").map(PathBuf::from);
    let sink = sink();
    match args.first().map(String::as_str) {
        Some("list") => {
            let m = Downloader::new(cfg.mirror).manifest().await?;
            println!("latest release {}  snapshot {}", m.latest.release, m.latest.snapshot);
            let all = args.iter().any(|a| a == "--all");
            for v in m.versions.iter().filter(|v| all || v.kind == "release").take(if all { usize::MAX } else { 30 }) {
                println!("{:<14}{}", v.id, v.kind);
            }
        }
        Some("search") => {
            // flc search <mod|modpack|shader|resourcepack|datapack> <关键词> [--mc 1.21.1] [--loader fabric] [--sort downloads]
            let kind = serde_json::from_value(serde_json::Value::String(args.get(1).cloned().unwrap_or_default())).ok();
            let a = flc_core::content::SearchArgs {
                kind,
                query: args.get(2).cloned().unwrap_or_default(),
                mc: flag(&args, "--mc"),
                loader: flag(&args, "--loader"),
                sort: flag(&args, "--sort").unwrap_or_default(),
                ..Default::default()
            };
            let page = flc_core::content::search(&Downloader::new(cfg.mirror), &a).await?;
            println!("共 {} 个结果", page.total);
            for h in page.hits {
                println!("{:<10} {:<28} {:>10} 下载  {}  [{}]  {}", h.id, h.title, h.downloads, h.updated.get(..10).unwrap_or(""), h.loaders.join(","), h.summary.chars().take(40).collect::<String>());
            }
        }
        Some("get") => {
            // flc get <存档版本id> <kind> <项目id> [--world 世界名]
            let (inst, kind, project) = (args.get(1).cloned().unwrap_or_default(), args.get(2).cloned().unwrap_or_default(), args.get(3).cloned().unwrap_or_default());
            let kind = serde_json::from_value(serde_json::Value::String(kind)).ok();
            let a = flc_core::content::InstallArgs { instance: inst, kind, project, world: flag(&args, "--world"), ..Default::default() };
            let r = flc_core::content::install(&cfg, &Downloader::new(cfg.mirror), &a, &sink).await?;
            println!("==> 已安装: {}", r.files.join(", "));
            for w in r.warnings {
                println!("==> 提醒: {w}");
            }
        }
        Some("deps") => {
            // flc deps <存档版本id> <项目id>：装这个模组会带上哪些前置
            let a = flc_core::content::InstallArgs { instance: args.get(1).cloned().unwrap_or_default(), kind: Some(flc_core::content::Kind::Mod), project: args.get(2).cloned().unwrap_or_default(), ..Default::default() };
            for d in flc_core::content::dependencies(&cfg, &Downloader::new(cfg.mirror), &a).await? {
                println!("{:<10} {:<28} {}", d.status, d.title, d.version);
            }
        }
        Some("instances") => {
            for i in flc_core::instance::list(&cfg.root) {
                println!("{:<30} {}", i.id, i.title);
            }
        }
        Some("java") => {
            for j in java::scan(&[&cfg.data_dir, &cfg.root]) {
                println!("Java {:<3} {}", j.major, j.path.display());
            }
        }
        Some("loaders") => {
            // flc loaders <mc版本> <fabric|quilt|forge|neoforge>
            let (mc, kind) = (args.get(1).map(String::as_str).unwrap_or(""), args.get(2).map(String::as_str).unwrap_or(""));
            let Some(loader) = flc_core::loader::Loader::parse(kind) else { bail!("加载器须是 fabric / quilt / forge / neoforge") };
            for v in flc_core::loader::list_versions(&Downloader::new(cfg.mirror), loader, mc).await?.iter().take(15) {
                println!("{}{}", v.version, if v.stable { "  (稳定/推荐)" } else { "" });
            }
        }
        Some("install") => {
            // flc install <mc版本> [--loader fabric[@版本]]，不指定加载器版本时取最新的稳定版
            let id = args.get(1).map(String::as_str).unwrap_or("");
            if let Some(spec) = flag(&args, "--loader") {
                let (kind, ver) = spec.split_once('@').map_or((spec.as_str(), None), |(k, v)| (k, Some(v.to_string())));
                let Some(loader) = flc_core::loader::Loader::parse(kind) else { bail!("加载器须是 fabric / quilt / forge / neoforge") };
                let version = match ver {
                    Some(v) => v,
                    None => {
                        let all = flc_core::loader::list_versions(&Downloader::new(cfg.mirror), loader, id).await?;
                        all.iter().find(|v| v.stable).or(all.first()).map(|v| v.version.clone()).ok_or_else(|| anyhow::anyhow!("{} 暂不支持 {id}", loader.name()))?
                    }
                };
                let new_id = flc_core::loader::install_loader(&cfg, id, loader, &version, &sink).await?;
                println!("==> 已安装: {new_id}");
            } else {
                flc_core::install(&cfg, id, &sink).await?;
            }
        }
        Some("run") => {
            let Some(id) = args.get(1) else { bail!("用法: flc run <版本> [--name 玩家] [--kill-after 秒]") };
            let name = flag(&args, "--name").unwrap_or_else(|| "Player".into());
            install_version(&cfg, id, &sink).await?;
            let game = launch(&cfg, id, &Account::offline(&name), sink.clone()).await?;
            println!("==> 点击到进程创建: {} ms", game.prepare_ms);
            if let Some(secs) = flag(&args, "--kill-after").and_then(|s| s.parse::<u64>().ok()) {
                tokio::time::sleep(Duration::from_secs(secs)).await;
                println!("==> {secs} 秒到，结束游戏");
                game.kill().await;
            }
            game.wait().await;
        }
        _ => println!("flc list [--all] | instances | search <类型> <关键词> | get <存档> <类型> <项目> | java | install <版本> | run <版本> [--name N] [--kill-after S]\n通用: --root <目录> --mirror auto|official|bmcl --java <路径>"),
    }
    Ok(())
}
