#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use base64::Engine;
use flc_core::content::{self, InstallArgs, Installed, Kind, Page, SearchArgs, Ver};
use flc_core::instance::{self, FileInfo, Instance, Meta, ModInfo};
use flc_core::msa::{self, AccountInfo, DeviceCode};
use flc_core::{install_version, launch, Account, Config, Downloader, Event, Mirror, RunningGame, Sink};
use futures::future::{AbortHandle, Abortable};
use std::future::Future;
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Default)]
struct AppState {
    game: Mutex<Option<RunningGame>>,
    /// 当前正在进行的下载 / 安装 / 登录任务，用来取消。
    task: Mutex<Option<AbortHandle>>,
}

const CANCELLED: &str = "已取消";

/// 运行一个可以被 `cancel_task` 中止的任务。被取消时 future 直接丢弃，下载到一半的文件下次会被覆盖，
/// 整合包安装之类带清理逻辑的操作靠 Drop 收拾残局。
async fn cancellable<T>(state: &AppState, fut: impl Future<Output = Result<T, String>>) -> Result<T, String> {
    let (handle, reg) = AbortHandle::new_pair();
    *state.task.lock().unwrap() = Some(handle);
    let r = Abortable::new(fut, reg).await;
    *state.task.lock().unwrap() = None;
    r.unwrap_or_else(|_| Err(CANCELLED.to_string()))
}

#[tauri::command]
fn cancel_task(state: State<'_, AppState>) {
    if let Some(h) = state.task.lock().unwrap().take() {
        h.abort();
    }
}

/// 界面里的设置，每次调用时带过来；后端不自己存设置。
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Settings {
    root: String,
    data_dir: String,
    mirror: String,
    isolate: Option<bool>,
    mem_mb: Option<u32>,
    java: String,
    lang: String,
    ms_client_id: String,
}

fn config(s: &Settings) -> Config {
    let root = if s.root.trim().is_empty() { Config::default_root() } else { PathBuf::from(s.root.trim()) };
    let mut cfg = Config::new(root);
    if !s.data_dir.trim().is_empty() {
        cfg.data_dir = PathBuf::from(s.data_dir.trim());
    }
    cfg.mirror = match s.mirror.as_str() {
        "official" => Mirror::Official,
        "bmcl" => Mirror::Bmcl,
        _ => Mirror::Auto,
    };
    cfg.isolate = s.isolate.unwrap_or(true);
    cfg.max_mem_mb = s.mem_mb;
    if !s.java.trim().is_empty() {
        cfg.java = Some(PathBuf::from(s.java.trim()));
    }
    if !s.lang.trim().is_empty() {
        cfg.lang = Some(s.lang.trim().to_string());
    }
    cfg
}

fn sink_for(app: &AppHandle) -> Sink {
    let handle = app.clone();
    Arc::new(move |e: Event| {
        if matches!(e, Event::Exited { .. }) {
            *handle.state::<AppState>().game.lock().unwrap() = None;
        }
        let _ = handle.emit("flc://event", e);
    })
}

#[tauri::command]
fn default_root() -> String {
    Config::default_root().to_string_lossy().into_owned()
}

fn io_err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

/// 文件系统和压缩包都是同步 IO，丢到阻塞线程池里做，免得卡住界面。
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(io_err)?
}

#[tauri::command]
async fn list_instances(settings: Settings) -> Result<Vec<Instance>, String> {
    let root = config(&settings).root;
    blocking(move || Ok(instance::list(&root))).await
}

#[tauri::command]
fn delete_instance(id: String, settings: Settings) -> Result<(), String> {
    instance::delete(&config(&settings).root, &id).map_err(io_err)
}

#[tauri::command]
fn save_instance(id: String, meta: Meta, settings: Settings) -> Result<(), String> {
    instance::save_meta(&config(&settings).root, &id, &meta).map_err(io_err)
}

/// `data` 是界面缩放好的 PNG，形如 `data:image/png;base64,...`。
#[tauri::command]
fn set_instance_icon(id: String, data: String, settings: Settings) -> Result<(), String> {
    let png = decode_data_url(&data)?;
    instance::set_icon(&config(&settings).root, &id, &png).map_err(io_err)
}

fn decode_data_url(data: &str) -> Result<Vec<u8>, String> {
    let b64 = data.split_once(',').map_or(data, |(_, b)| b);
    base64::engine::general_purpose::STANDARD.decode(b64.trim()).map_err(|_| "图片数据无效".to_string())
}

fn mods_dir(settings: &Settings, id: &str) -> Result<PathBuf, String> {
    flc_core::version::check_id(id).map_err(io_err)?;
    Ok(instance::game_dir(&config(settings), id).join("mods"))
}

#[tauri::command]
async fn list_mods(id: String, settings: Settings) -> Result<Vec<ModInfo>, String> {
    let dir = mods_dir(&settings, &id)?;
    blocking(move || Ok(instance::list_mods(&dir))).await
}

#[tauri::command]
fn set_mod_enabled(id: String, file: String, enabled: bool, settings: Settings) -> Result<(), String> {
    instance::set_mod_enabled(&mods_dir(&settings, &id)?, &file, enabled).map_err(io_err)
}

#[tauri::command]
fn remove_mod(id: String, file: String, settings: Settings) -> Result<(), String> {
    instance::remove_mod(&mods_dir(&settings, &id)?, &file).map_err(io_err)
}

#[tauri::command]
async fn add_mods(id: String, paths: Vec<String>, settings: Settings) -> Result<usize, String> {
    let dir = mods_dir(&settings, &id)?;
    blocking(move || instance::add_mods(&dir, &paths.into_iter().map(PathBuf::from).collect::<Vec<_>>()).map_err(io_err)).await
}

/// 在资源管理器里打开实例的某个文件夹：game（游戏目录）| mods | saves | resourcepacks。
#[tauri::command]
fn open_instance_dir(id: String, which: String, settings: Settings) -> Result<(), String> {
    flc_core::version::check_id(&id).map_err(io_err)?;
    let base = instance::game_dir(&config(&settings), &id);
    let dir = match which.as_str() {
        "game" => base,
        "mods" | "saves" | "resourcepacks" | "shaderpacks" => base.join(&which),
        _ => return Err("未知的文件夹".into()),
    };
    std::fs::create_dir_all(&dir).map_err(io_err)?;
    #[cfg(target_os = "windows")]
    let opener = "explorer";
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(all(unix, not(target_os = "macos")))]
    let opener = "xdg-open";
    std::process::Command::new(opener).arg(&dir).spawn().map(|_| ()).map_err(io_err)
}

// ---------- 内容库（Modrinth） ----------

#[tauri::command]
async fn content_search(args: SearchArgs, settings: Settings) -> Result<Page, String> {
    content::search(&Downloader::new(config(&settings).mirror), &args).await.map_err(io_err)
}

#[tauri::command]
async fn content_project(id: String, settings: Settings) -> Result<content::Detail, String> {
    content::project(&Downloader::new(config(&settings).mirror), &id).await.map_err(io_err)
}

/// 一个项目的版本；`loader` 是存档的加载器，只对模组按它过滤。
#[tauri::command]
async fn content_versions(project: String, kind: Kind, mc: Option<String>, loader: Option<String>, settings: Settings) -> Result<Vec<Ver>, String> {
    let loaders = if kind == Kind::Mod { content::loader_filter(loader.as_deref().unwrap_or("")) } else { vec![] };
    content::versions(&Downloader::new(config(&settings).mirror), &project, mc.as_deref(), &loaders).await.map_err(io_err)
}

#[tauri::command]
async fn content_install(app: AppHandle, state: State<'_, AppState>, args: InstallArgs, settings: Settings) -> Result<Installed, String> {
    let cfg = config(&settings);
    cancellable(&state, async { content::install(&cfg, &Downloader::new(cfg.mirror), &args, &sink_for(&app)).await.map_err(io_err) }).await
}

/// 装某个模组会带上哪些前置（安装之前给界面展示）。
#[tauri::command]
async fn content_dependencies(args: InstallArgs, settings: Settings) -> Result<Vec<content::DepInfo>, String> {
    let cfg = config(&settings);
    content::dependencies(&cfg, &Downloader::new(cfg.mirror), &args).await.map_err(io_err)
}

#[tauri::command]
async fn content_check_updates(instance: String, kind: Kind, settings: Settings) -> Result<Vec<content::UpdateInfo>, String> {
    let cfg = config(&settings);
    content::check_updates(&cfg, &Downloader::new(cfg.mirror), &instance, kind).await.map_err(io_err)
}

#[tauri::command]
async fn content_update(app: AppHandle, state: State<'_, AppState>, instance: String, kind: Kind, file: String, settings: Settings) -> Result<Installed, String> {
    let cfg = config(&settings);
    cancellable(&state, async { content::update_one(&cfg, &Downloader::new(cfg.mirror), &instance, kind, &file, &sink_for(&app)).await.map_err(io_err) }).await
}

/// 复制一个存档（连同世界、模组、配置），返回新存档的版本 id。
#[tauri::command]
async fn duplicate_instance(id: String, name: String, settings: Settings) -> Result<String, String> {
    let root = config(&settings).root;
    blocking(move || instance::duplicate(&root, &id, &name).map_err(io_err)).await
}

#[tauri::command]
async fn modpack_install(app: AppHandle, state: State<'_, AppState>, project: String, version: Option<String>, title: String, settings: Settings) -> Result<String, String> {
    let cfg = config(&settings);
    cancellable(&state, async { content::install_modpack(&cfg, &Downloader::new(cfg.mirror), &project, version.as_deref(), &title, &sink_for(&app)).await.map_err(io_err) }).await
}

#[tauri::command]
async fn modpack_import(app: AppHandle, state: State<'_, AppState>, path: String, settings: Settings) -> Result<String, String> {
    let cfg = config(&settings);
    cancellable(&state, async { content::install_mrpack(&cfg, &Downloader::new(cfg.mirror), &PathBuf::from(path), None, &sink_for(&app)).await.map_err(io_err) }).await
}

/// 目标文件夹里已经装过的项目（项目 id → 文件名），用来在列表里标“已安装”。
#[tauri::command]
async fn content_installed(instance: String, kind: Kind, world: Option<String>, settings: Settings) -> Result<std::collections::HashMap<String, String>, String> {
    flc_core::version::check_id(&instance).map_err(io_err)?;
    let cfg = config(&settings);
    let Ok(dir) = content::dest_dir(&cfg, &instance, kind, world.as_deref()) else { return Ok(Default::default()) };
    blocking(move || Ok(instance::installed_projects(&dir))).await
}

#[tauri::command]
fn list_worlds(id: String, settings: Settings) -> Result<Vec<String>, String> {
    flc_core::version::check_id(&id).map_err(io_err)?;
    Ok(instance::list_worlds(&instance::game_dir(&config(&settings), &id)))
}

#[tauri::command]
async fn list_content(id: String, kind: Kind, world: Option<String>, settings: Settings) -> Result<Vec<FileInfo>, String> {
    flc_core::version::check_id(&id).map_err(io_err)?;
    let Ok(dir) = content::dest_dir(&config(&settings), &id, kind, world.as_deref()) else { return Ok(vec![]) };
    blocking(move || Ok(instance::list_files(&dir))).await
}

#[tauri::command]
fn remove_content(id: String, kind: Kind, file: String, world: Option<String>, settings: Settings) -> Result<(), String> {
    flc_core::version::check_id(&id).map_err(io_err)?;
    let dir = content::dest_dir(&config(&settings), &id, kind, world.as_deref()).map_err(io_err)?;
    instance::remove_entry(&dir, &file).map_err(io_err)
}

/// 在系统浏览器里打开网页，只允许 https。
#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with("https://") || url.contains(char::is_whitespace) {
        return Err("只能打开 https 网址".into());
    }
    #[cfg(target_os = "windows")]
    let opener = "explorer";
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(all(unix, not(target_os = "macos")))]
    let opener = "xdg-open";
    std::process::Command::new(opener).arg(&url).spawn().map(|_| ()).map_err(io_err)
}

#[tauri::command]
async fn list_remote(settings: Settings) -> Result<Vec<flc_core::model::VersionEntry>, String> {
    let m = Downloader::new(config(&settings).mirror).manifest().await.map_err(|e| format!("{e:#}"))?;
    Ok(m.versions)
}

#[tauri::command]
fn default_data_dir() -> String {
    Config::default_data_dir().to_string_lossy().into_owned()
}

#[derive(Deserialize)]
struct LoaderSel {
    kind: String,
    version: String,
}

/// 某个 Minecraft 版本可用的加载器版本；该加载器不支持这个版本时返回空列表。
#[tauri::command]
async fn list_loader_versions(mc: String, loader: String, settings: Settings) -> Result<Vec<flc_core::loader::LoaderVersion>, String> {
    let loader = flc_core::loader::Loader::parse(&loader).ok_or("未知的加载器")?;
    flc_core::loader::list_versions(&Downloader::new(config(&settings).mirror), loader, &mc).await.map_err(|e| format!("{e:#}"))
}

/// 只下载不启动，进度通过同一个事件通道推给界面；返回安装好的版本 id（带加载器时是加载器版本的 id）。
#[tauri::command]
async fn install_game(app: AppHandle, state: State<'_, AppState>, id: String, loader: Option<LoaderSel>, settings: Settings) -> Result<String, String> {
    let cfg = config(&settings);
    let sink = sink_for(&app);
    cancellable(&state, async {
        match loader {
            None => {
                flc_core::install(&cfg, &id, &sink).await.map_err(io_err)?;
                instance::ensure(&cfg.root, &id).map_err(io_err)?;
                Ok(id)
            }
            Some(l) => {
                let kind = flc_core::loader::Loader::parse(&l.kind).ok_or("未知的加载器")?;
                flc_core::loader::install_loader(&cfg, &id, kind, &l.version, &sink).await.map_err(io_err)
            }
        }
    })
    .await
}

/// 离线账号的皮肤，PNG 为 64×64，base64 编码。
#[derive(Deserialize)]
struct SkinSel {
    png: String,
}

#[tauri::command]
async fn launch_game(app: AppHandle, state: State<'_, AppState>, id: String, name: String, account: Option<String>, skin: Option<SkinSel>, settings: Settings) -> Result<u32, String> {
    let mut cfg = config(&settings);
    // 这个实例自己的设置优先于全局设置。
    let own = instance::meta(&cfg.root, &id);
    instance::apply_overrides(&mut cfg, &own);
    // 正版账号用自己在 Mojang 那边的皮肤，不再用 FLC 的皮肤包（没传 skin 时旧的皮肤包会被清掉）
    if let (Some(s), None) = (skin, &account) {
        cfg.skin = Some(flc_core::skin::Skin { png: decode_data_url(&s.png)? });
    }
    let sink = sink_for(&app);
    // 准备阶段（下载文件、刷新登录）可以取消；游戏进程一旦创建就不归这里管了
    let game = cancellable(&state, async {
        let acct = match &account {
            Some(uuid) => msa::account_for_launch(&cfg.data_dir, &settings.ms_client_id, uuid).await.map_err(io_err)?,
            None => Account::offline(if name.trim().is_empty() { "Player" } else { name.trim() }),
        };
        install_version(&cfg, &id, &sink).await.map_err(io_err)?;
        launch(&cfg, &id, &acct, sink.clone()).await.map_err(io_err)
    })
    .await?;
    let _ = instance::touch_played(&cfg.root, &id);
    let pid = game.pid;
    *state.game.lock().unwrap() = Some(game);
    Ok(pid)
}

// ---------- 微软账号 ----------

#[tauri::command]
async fn ms_login_start(settings: Settings) -> Result<DeviceCode, String> {
    msa::start(&msa::client_id(&settings.ms_client_id).map_err(io_err)?).await.map_err(io_err)
}

/// 等用户在浏览器里完成登录，然后保存账号。可以用 `cancel_task` 取消。
#[tauri::command]
async fn ms_login_finish(state: State<'_, AppState>, device: DeviceCode, settings: Settings) -> Result<AccountInfo, String> {
    let cfg = config(&settings);
    cancellable(&state, async {
        let s = msa::finish(&msa::client_id(&settings.ms_client_id).map_err(io_err)?, &device).await.map_err(io_err)?;
        let info = AccountInfo { id: s.uuid.clone(), name: s.name.clone(), kind: "msa" };
        msa::upsert(&cfg.data_dir, s).map_err(io_err)?;
        Ok(info)
    })
    .await
}

#[tauri::command]
fn list_accounts(settings: Settings) -> Result<Vec<AccountInfo>, String> {
    msa::list(&config(&settings).data_dir).map_err(io_err)
}

#[tauri::command]
fn remove_account(id: String, settings: Settings) -> Result<(), String> {
    msa::remove(&config(&settings).data_dir, &id).map_err(io_err)
}

/// 账号在 Mojang 那边的当前皮肤，返回 PNG 的 data URL 和手臂类型；没有皮肤返回 null。
#[tauri::command]
async fn account_skin(id: String, settings: Settings) -> Result<Option<serde_json::Value>, String> {
    let Some((png, slim)) = msa::skin_of(&config(&settings).data_dir, &id).await.map_err(io_err)? else { return Ok(None) };
    Ok(Some(serde_json::json!({ "url": format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png)), "slim": slim })))
}

#[tauri::command]
async fn kill_game(state: State<'_, AppState>) -> Result<(), String> {
    // RunningGame::kill 是 async，先把句柄取出再 await，避免持锁跨 await。
    let game = state.game.lock().unwrap().take();
    if let Some(g) = game {
        g.kill().await;
    }
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .setup(|app| {
            #[cfg(target_os = "windows")]
            if let Some(win) = app.get_webview_window("main") {
                use window_vibrancy::{apply_acrylic, apply_mica};
                // Win11 用 Mica，旧系统退回 Acrylic。
                if apply_mica(&win, None).is_err() {
                    let _ = apply_acrylic(&win, Some((20, 24, 22, 120)));
                }
            }
            let _ = app;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![default_root, default_data_dir, list_instances, delete_instance, save_instance, set_instance_icon, list_mods, set_mod_enabled, remove_mod, add_mods, open_instance_dir, content_search, content_project, content_versions, content_install, modpack_install, modpack_import, content_installed, content_dependencies, content_check_updates, content_update, duplicate_instance, list_worlds, list_content, remove_content, open_url, cancel_task, ms_login_start, ms_login_finish, list_accounts, remove_account, account_skin, list_remote, list_loader_versions, install_game, launch_game, kill_game])
        .run(tauri::generate_context!())
        .expect("启动 FLC 失败");
}
