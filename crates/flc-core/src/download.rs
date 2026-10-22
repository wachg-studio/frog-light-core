use crate::event::{Event, Sink};
use crate::model::Manifest;
use anyhow::{anyhow, bail, Context, Result};
use futures::{future::select_ok, stream, FutureExt, StreamExt, TryStreamExt};
use sha1::{Digest, Sha1};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;

const MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const CONCURRENCY: usize = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mirror {
    /// 同时探测官方与 BMCLAPI，谁先响应就优先用谁，失败时自动换源。
    Auto,
    Official,
    Bmcl,
}

#[derive(Clone, Debug)]
pub struct Task {
    pub url: String,
    pub dest: PathBuf,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

#[derive(Clone)]
pub struct Downloader {
    client: reqwest::Client,
    mirror: Mirror,
    prefer_bmcl: Arc<AtomicBool>,
}

fn to_bmcl(u: &str) -> String {
    const MAP: [(&str, &str); 6] = [
        ("https://piston-data.mojang.com", "https://bmclapi2.bangbang93.com"),
        ("https://piston-meta.mojang.com", "https://bmclapi2.bangbang93.com"),
        ("https://launchermeta.mojang.com", "https://bmclapi2.bangbang93.com"),
        ("https://launcher.mojang.com", "https://bmclapi2.bangbang93.com"),
        ("https://libraries.minecraft.net", "https://bmclapi2.bangbang93.com/maven"),
        ("https://resources.download.minecraft.net", "https://bmclapi2.bangbang93.com/assets"),
    ];
    for (from, to) in MAP {
        if let Some(rest) = u.strip_prefix(from) {
            return format!("{to}{rest}");
        }
    }
    u.to_string()
}

struct Progress {
    done: AtomicU64,
    files: AtomicU32,
    last_emit_ms: AtomicU64,
    start: Instant,
    total: u64,
    files_total: u32,
    label: String,
    /// 正在下载的文件名。
    active: Mutex<Vec<String>>,
}

/// 下载被取消（future 被丢弃）时，把没下完的 .part 文件删掉。
struct PartGuard(Option<PathBuf>);

impl Drop for PartGuard {
    fn drop(&mut self) {
        if let Some(p) = self.0.take() {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// 文件下完（或者任务被取消、future 被丢弃）时，把它从“正在下载”里去掉。
struct ActiveGuard<'a> {
    prog: &'a Progress,
    name: String,
}

impl<'a> ActiveGuard<'a> {
    fn new(prog: &'a Progress, name: String) -> Self {
        prog.active.lock().unwrap().push(name.clone());
        Self { prog, name }
    }
}

impl Drop for ActiveGuard<'_> {
    fn drop(&mut self) {
        let mut a = self.prog.active.lock().unwrap();
        if let Some(i) = a.iter().position(|n| *n == self.name) {
            a.remove(i);
        }
    }
}

impl Progress {
    fn emit(&self, sink: &Sink, force: bool) {
        let now = self.start.elapsed().as_millis() as u64;
        let last = self.last_emit_ms.load(Ordering::Relaxed);
        if !force && (now < last + 80 || self.last_emit_ms.compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed).is_err()) {
            return;
        }
        sink(Event::Progress {
            label: self.label.clone(),
            done: self.done.load(Ordering::Relaxed),
            total: self.total,
            files_done: self.files.load(Ordering::Relaxed),
            files_total: self.files_total,
            active: self.active.lock().unwrap().iter().take(3).cloned().collect(),
        });
    }
}

/// 只比较文件是否存在与大小，避免每次启动都重算哈希。
pub fn is_fresh(t: &Task) -> bool {
    match std::fs::metadata(&t.dest) {
        Ok(m) => m.is_file() && t.size.map_or(true, |s| s == m.len()),
        Err(_) => false,
    }
}

impl Downloader {
    pub fn new(mirror: Mirror) -> Self {
        let client = reqwest::Client::builder()
            .user_agent(concat!("FLC/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(8))
            .read_timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(CONCURRENCY)
            .build()
            .expect("http client");
        Self { client, mirror, prefer_bmcl: Arc::new(AtomicBool::new(mirror == Mirror::Bmcl)) }
    }

    fn candidates(&self, url: &str) -> Vec<String> {
        let b = to_bmcl(url);
        if b == url {
            return vec![url.to_string()];
        }
        match self.mirror {
            Mirror::Official => vec![url.to_string()],
            Mirror::Bmcl => vec![b, url.to_string()],
            Mirror::Auto if self.prefer_bmcl.load(Ordering::Relaxed) => vec![b, url.to_string()],
            Mirror::Auto => vec![url.to_string(), b],
        }
    }

    pub async fn get_text(&self, url: &str) -> Result<String> {
        let mut errs = Vec::new();
        for u in self.candidates(url).into_iter().flat_map(|u| [u.clone(), u]) {
            match self.client.get(&u).send().await.and_then(|r| r.error_for_status()) {
                Ok(r) => match r.text().await {
                    Ok(t) => return Ok(t),
                    Err(e) => errs.push(format!("{u}: {e:?}")),
                },
                Err(e) => errs.push(format!("{u}: {e:?}")),
            }
        }
        bail!("请求失败: {}", errs.join(" | "))
    }

    pub async fn manifest(&self) -> Result<Manifest> {
        let text = if self.mirror == Mirror::Auto {
            // 官方与镜像竞速，赢家成为本次会话的首选源。
            let go = |bmcl: bool| {
                let u = if bmcl { to_bmcl(MANIFEST) } else { MANIFEST.to_string() };
                let c = self.client.clone();
                async move {
                    let t = c.get(u).send().await?.error_for_status()?.text().await?;
                    Ok::<_, anyhow::Error>((bmcl, t))
                }
                .boxed()
            };
            let ((bmcl, t), _) = select_ok([go(false), go(true)]).await?;
            self.prefer_bmcl.store(bmcl, Ordering::Relaxed);
            t
        } else {
            self.get_text(MANIFEST).await?
        };
        Ok(serde_json::from_str(&text)?)
    }

    pub async fn fetch_all(&self, tasks: Vec<Task>, label: &str, sink: &Sink) -> Result<()> {
        let mut seen = std::collections::HashSet::new();
        let pending: Vec<Task> = tasks.into_iter().filter(|t| !is_fresh(t) && seen.insert(t.dest.clone())).collect();
        if pending.is_empty() {
            return Ok(());
        }
        let prog = Progress {
            done: AtomicU64::new(0),
            files: AtomicU32::new(0),
            last_emit_ms: AtomicU64::new(0),
            start: Instant::now(),
            total: pending.iter().map(|t| t.size.unwrap_or(0)).sum(),
            files_total: pending.len() as u32,
            label: label.to_string(),
            active: Mutex::new(Vec::new()),
        };
        prog.emit(sink, true);
        // 闭包持有所有权（而不是借用），这样整个 future 才满足 Tauri 命令要求的 Send。
        let prog = Arc::new(prog);
        stream::iter(pending)
            .map(|t| {
                let (this, prog, sink) = (self.clone(), prog.clone(), sink.clone());
                async move { this.fetch_one(&t, &prog, &sink).await }
            })
            .buffer_unordered(CONCURRENCY)
            .try_collect::<Vec<_>>()
            .await?;
        prog.emit(sink, true);
        Ok(())
    }

    async fn fetch_one(&self, t: &Task, prog: &Progress, sink: &Sink) -> Result<()> {
        let _active = ActiveGuard::new(prog, t.dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
        let mut last = None;
        for url in self.candidates(&t.url) {
            for _ in 0..2 {
                match self.get_to_file(&url, t, prog, sink).await {
                    Ok(()) => {
                        prog.files.fetch_add(1, Ordering::Relaxed);
                        prog.emit(sink, false);
                        return Ok(());
                    }
                    Err(e) => last = Some(e),
                }
            }
        }
        Err(last.unwrap_or_else(|| anyhow!("no url"))).with_context(|| format!("下载失败: {}", t.url))
    }

    async fn get_to_file(&self, url: &str, t: &Task, prog: &Progress, sink: &Sink) -> Result<()> {
        if let Some(dir) = t.dest.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }
        let mut part = t.dest.clone().into_os_string();
        part.push(".part");
        let part = PathBuf::from(part);
        let mut guard = PartGuard(Some(part.clone()));
        let mut got = 0u64;
        let res: Result<()> = async {
            let resp = self.client.get(url).send().await?.error_for_status()?;
            let mut file = tokio::fs::File::create(&part).await?;
            let mut hasher = Sha1::new();
            let mut body = resp.bytes_stream();
            while let Some(chunk) = body.next().await {
                let chunk = chunk?;
                hasher.update(&chunk);
                file.write_all(&chunk).await?;
                got += chunk.len() as u64;
                prog.done.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                prog.emit(sink, false);
            }
            file.flush().await?;
            drop(file);
            if let Some(want) = &t.sha1 {
                if !hex::encode(hasher.finalize()).eq_ignore_ascii_case(want) {
                    bail!("SHA1 校验不匹配");
                }
            }
            if let Some(size) = t.size {
                if size != got {
                    bail!("文件大小不匹配: 期望 {size}, 实际 {got}");
                }
            }
            // 在 EFS 加密的目录（如被加密的 %APPDATA%）里，改名可能报“不同的磁盘驱动器”，退回复制再删除。
            if let Err(e) = tokio::fs::rename(&part, &t.dest).await {
                tokio::fs::copy(&part, &t.dest).await.map_err(|_| e)?;
                let _ = tokio::fs::remove_file(&part).await;
            }
            Ok(())
        }
        .await;
        if res.is_err() {
            prog.done.fetch_sub(got, Ordering::Relaxed);
            let _ = tokio::fs::remove_file(&part).await;
        }
        if res.is_ok() {
            guard.0 = None;
        }
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::{AbortHandle, Abortable};

    /// 本机起一个很慢的 HTTP 服务器：先给一部分，然后一直不给了。
    async fn slow_server() -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else { return };
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let _ = tokio::io::AsyncReadExt::read(&mut sock, &mut buf).await;
                    let _ = sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100000000\r\n\r\n").await;
                    for _ in 0..1000 {
                        if sock.write_all(&[0u8; 1024]).await.is_err() {
                            return;
                        }
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                });
            }
        });
        format!("http://{addr}/big.bin")
    }

    #[tokio::test]
    async fn cancelling_a_download_stops_it_promptly_and_leaves_no_partial_file() {
        let url = slow_server().await;
        let dir = std::env::temp_dir().join(format!("flc-cancel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let dest = dir.join("big.bin");
        let seen: Arc<Mutex<Vec<Vec<String>>>> = Arc::default();
        let sink: Sink = {
            let seen = seen.clone();
            Arc::new(move |e| {
                if let Event::Progress { active, .. } = e {
                    seen.lock().unwrap().push(active);
                }
            })
        };
        let dl = Downloader::new(Mirror::Official);
        let (handle, reg) = AbortHandle::new_pair();
        let task = Task { url, dest: dest.clone(), sha1: None, size: None };
        let job = tokio::spawn(async move { Abortable::new(dl.fetch_all(vec![task], "测试", &sink), reg).await });
        tokio::time::sleep(Duration::from_millis(600)).await;
        let started = Instant::now();
        handle.abort();
        let res = job.await.unwrap();
        assert!(res.is_err(), "应该是被中止的");
        assert!(started.elapsed() < Duration::from_secs(2));
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!dest.exists(), "不应该留下成品");
        let mut part = dest.clone().into_os_string();
        part.push(".part");
        assert!(!std::path::Path::new(&part).exists(), "不应该留下 .part");
        assert!(seen.lock().unwrap().iter().any(|a| a == &["big.bin".to_string()]), "进度事件里应该带着正在下载的文件名");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
