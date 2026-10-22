//! 微软账号（正版）登录：设备码流程 → Xbox Live → XSTS → Minecraft 服务。
//!
//! 设备码流程不需要内置浏览器：界面显示一个登录码，用户在系统浏览器里打开微软的页面输入即可。
//! 需要一个在 Azure 注册、并通过 Minecraft 服务审核的应用 Client ID（构建时用环境变量 `FLC_MS_CLIENT_ID` 内置，
//! 或者在设置里填写）。令牌用 DPAPI 加密保存在 FLC 数据目录里。
use crate::auth::Account;
use crate::vault;
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DEVICE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const SCOPE: &str = "XboxLive.signin offline_access";
/// 发布版在构建时内置的 Client ID。
pub const DEFAULT_CLIENT_ID: Option<&str> = option_env!("FLC_MS_CLIENT_ID");
/// 令牌快过期（5 分钟内）就先刷新。
const EXPIRY_MARGIN: u64 = 300;

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn http() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder().user_agent(concat!("FLC/", env!("CARGO_PKG_VERSION"))).timeout(Duration::from_secs(25)).build()?)
}

/// 用户填的优先，其次是构建时内置的。
pub fn client_id(user: &str) -> Result<String> {
    let user = user.trim();
    if !user.is_empty() {
        return Ok(user.to_string());
    }
    DEFAULT_CLIENT_ID.map(str::to_string).ok_or_else(|| anyhow!("还没有配置微软登录的应用 Client ID。请在 设置 → 账号 里填写。"))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCode {
    pub user_code: String,
    pub verification_uri: String,
    pub device_code: String,
    pub interval: u64,
    pub expires_in: u64,
}

/// 这个账号信息会发给界面，不含任何令牌。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    pub id: String,
    pub name: String,
    pub kind: &'static str,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Stored {
    pub uuid: String,
    pub name: String,
    pub mc_token: String,
    pub mc_expires: u64,
    pub refresh_token: String,
    pub skin_url: Option<String>,
    pub skin_slim: bool,
}

// ---------- 第一步：设备码 ----------

fn error_text(v: &Value) -> String {
    let d = v["error_description"].as_str().or_else(|| v["error"].as_str()).unwrap_or("未知错误");
    // 微软的描述后面跟着 Trace ID、时间戳这些，对用户没用
    d.split("Trace ID").next().unwrap_or(d).lines().next().unwrap_or(d).trim().to_string()
}

pub async fn start(client_id: &str) -> Result<DeviceCode> {
    let v: Value = http()?
        .post(DEVICE_URL)
        .form(&[("client_id", client_id), ("scope", SCOPE)])
        .send()
        .await
        .context("连不上微软登录服务")?
        .json()
        .await
        .context("微软登录服务返回了无法理解的内容")?;
    if v.get("error").is_some() {
        bail!("微软登录服务拒绝了请求：{}", error_text(&v));
    }
    Ok(DeviceCode {
        user_code: v["user_code"].as_str().ok_or_else(|| anyhow!("没有拿到登录码"))?.to_string(),
        verification_uri: v["verification_uri"].as_str().unwrap_or("https://www.microsoft.com/link").to_string(),
        device_code: v["device_code"].as_str().ok_or_else(|| anyhow!("没有拿到设备码"))?.to_string(),
        interval: v["interval"].as_u64().unwrap_or(5).max(1),
        expires_in: v["expires_in"].as_u64().unwrap_or(900),
    })
}

#[derive(Debug, PartialEq)]
enum Poll {
    Pending,
    SlowDown,
    Done { access: String, refresh: Option<String> },
    Fail(String),
}

fn classify(v: &Value) -> Poll {
    if let Some(access) = v["access_token"].as_str() {
        return Poll::Done { access: access.to_string(), refresh: v["refresh_token"].as_str().map(str::to_string) };
    }
    match v["error"].as_str() {
        Some("authorization_pending") => Poll::Pending,
        Some("slow_down") => Poll::SlowDown,
        Some("expired_token") | Some("code_expired") => Poll::Fail("登录码已过期，请重新开始登录".into()),
        Some("authorization_declined") | Some("access_denied") => Poll::Fail("你在微软页面上拒绝了登录".into()),
        _ => Poll::Fail(format!("登录失败：{}", error_text(v))),
    }
}

/// 轮询直到用户在浏览器里完成登录，然后走完整条认证链。可以被取消（丢弃 future 即可）。
pub async fn finish(client_id: &str, dc: &DeviceCode) -> Result<Stored> {
    let http = http()?;
    let deadline = now() + dc.expires_in;
    let mut interval = dc.interval.max(1);
    loop {
        tokio::time::sleep(Duration::from_secs(interval)).await;
        if now() > deadline {
            bail!("登录码已过期，请重新开始登录");
        }
        let v: Value = http
            .post(TOKEN_URL)
            .form(&[("grant_type", "urn:ietf:params:oauth:grant-type:device_code"), ("client_id", client_id), ("device_code", dc.device_code.as_str())])
            .send()
            .await
            .context("连不上微软登录服务")?
            .json()
            .await
            .context("微软登录服务返回了无法理解的内容")?;
        match classify(&v) {
            Poll::Pending => {}
            Poll::SlowDown => interval += 5,
            Poll::Fail(m) => bail!(m),
            Poll::Done { access, refresh } => return login_from_ms(&http, &access, refresh.unwrap_or_default()).await,
        }
    }
}

// ---------- 后面几步：Xbox → XSTS → Minecraft ----------

/// XSTS 拒绝时的错误码，翻译成用户能照着做的话。
fn xerr_message(code: u64) -> String {
    match code {
        2148916233 => "这个微软账号还没有 Xbox 账号。请先到 xbox.com 登录一次完成创建，再回来。".into(),
        2148916235 => "Xbox 服务在你所在的国家/地区不可用。".into(),
        2148916236 | 2148916237 => "这个账号需要先在 Xbox 页面完成成人身份验证。".into(),
        2148916238 => "这是未成年人账号，需要家长在 Microsoft 家庭组里把它加入后才能登录。".into(),
        c => format!("Xbox 认证被拒绝（错误码 {c}）"),
    }
}

async fn xbox_chain(http: &reqwest::Client, ms_access: &str) -> Result<(String, u64)> {
    let json_post = |url: &str, body: Value| http.post(url.to_string()).header("Accept", "application/json").json(&body);

    let user: Value = json_post(
        "https://user.auth.xboxlive.com/user/authenticate",
        json!({ "Properties": { "AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com", "RpsTicket": format!("d={ms_access}") }, "RelyingParty": "http://auth.xboxlive.com", "TokenType": "JWT" }),
    )
    .send()
    .await
    .context("连不上 Xbox 服务")?
    .error_for_status()
    .context("Xbox 登录失败")?
    .json()
    .await?;
    let xbl = user["Token"].as_str().ok_or_else(|| anyhow!("Xbox 没有返回令牌"))?;

    let resp = json_post(
        "https://xsts.auth.xboxlive.com/xsts/authorize",
        json!({ "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl] }, "RelyingParty": "rp://api.minecraftservices.com/", "TokenType": "JWT" }),
    )
    .send()
    .await
    .context("连不上 Xbox 服务")?;
    let status = resp.status();
    let xsts: Value = resp.json().await.unwrap_or_default();
    if !status.is_success() {
        bail!(xerr_message(xsts["XErr"].as_u64().unwrap_or(0)));
    }
    let token = xsts["Token"].as_str().ok_or_else(|| anyhow!("XSTS 没有返回令牌"))?;
    let uhs = xsts["DisplayClaims"]["xui"][0]["uhs"].as_str().ok_or_else(|| anyhow!("XSTS 没有返回用户哈希"))?;

    let resp = json_post("https://api.minecraftservices.com/authentication/login_with_xbox", json!({ "identityToken": format!("XBL3.0 x={uhs};{token}") }))
        .send()
        .await
        .context("连不上 Minecraft 服务")?;
    let status = resp.status();
    let mc: Value = resp.json().await.unwrap_or_default();
    if status == reqwest::StatusCode::FORBIDDEN {
        bail!("Minecraft 服务拒绝了这个登录应用（Client ID 还没有通过 Minecraft 的审核，或账号被限制）");
    }
    if !status.is_success() {
        bail!("Minecraft 登录失败（{status}）");
    }
    let access = mc["access_token"].as_str().ok_or_else(|| anyhow!("Minecraft 没有返回令牌"))?.to_string();
    Ok((access, mc["expires_in"].as_u64().unwrap_or(86400)))
}

struct Profile {
    id: String,
    name: String,
    skin_url: Option<String>,
    slim: bool,
}

fn parse_profile(v: &Value) -> Result<Profile> {
    let skin = v["skins"].as_array().and_then(|s| s.iter().find(|x| x["state"] == "ACTIVE").or_else(|| s.first()));
    Ok(Profile {
        id: v["id"].as_str().ok_or_else(|| anyhow!("没有拿到玩家 ID"))?.to_string(),
        name: v["name"].as_str().ok_or_else(|| anyhow!("没有拿到玩家名"))?.to_string(),
        skin_url: skin.and_then(|s| s["url"].as_str()).map(str::to_string),
        slim: skin.is_some_and(|s| s["variant"] == "SLIM"),
    })
}

async fn profile(http: &reqwest::Client, mc_token: &str) -> Result<Profile> {
    let resp = http.get("https://api.minecraftservices.com/minecraft/profile").bearer_auth(mc_token).send().await.context("连不上 Minecraft 服务")?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        bail!("这个微软账号没有购买 Minecraft Java 版（Game Pass 用户请先在官方启动器里激活一次）");
    }
    let v: Value = resp.error_for_status().context("获取玩家信息失败")?.json().await?;
    parse_profile(&v)
}

async fn login_from_ms(http: &reqwest::Client, ms_access: &str, refresh: String) -> Result<Stored> {
    let (mc_token, expires_in) = xbox_chain(http, ms_access).await?;
    let p = profile(http, &mc_token).await?;
    Ok(Stored { uuid: p.id, name: p.name, mc_token, mc_expires: now() + expires_in, refresh_token: refresh, skin_url: p.skin_url, skin_slim: p.slim })
}

/// 用刷新令牌换新的登录状态。
pub async fn refresh(client_id: &str, old: &Stored) -> Result<Stored> {
    let http = http()?;
    let v: Value = http
        .post(TOKEN_URL)
        .form(&[("grant_type", "refresh_token"), ("client_id", client_id), ("refresh_token", old.refresh_token.as_str()), ("scope", SCOPE)])
        .send()
        .await
        .context("连不上微软登录服务")?
        .json()
        .await?;
    match classify(&v) {
        Poll::Done { access, refresh } => {
            let mut s = login_from_ms(&http, &access, refresh.unwrap_or_else(|| old.refresh_token.clone())).await?;
            if s.uuid != old.uuid {
                bail!("刷新后得到了另一个账号，请重新登录");
            }
            s.refresh_token = if s.refresh_token.is_empty() { old.refresh_token.clone() } else { s.refresh_token };
            Ok(s)
        }
        _ => bail!("登录已失效，请重新登录这个账号"),
    }
}

// ---------- 账号保存 ----------

fn store_path(data_dir: &Path) -> PathBuf {
    data_dir.join("accounts.bin")
}

pub fn load(data_dir: &Path) -> Result<Vec<Stored>> {
    match vault::read(&store_path(data_dir))? {
        Some(bytes) => serde_json::from_slice(&bytes).context("账号文件已损坏，请重新登录"),
        None => Ok(vec![]),
    }
}

fn save(data_dir: &Path, list: &[Stored]) -> Result<()> {
    vault::write(&store_path(data_dir), &serde_json::to_vec(list)?)
}

pub fn upsert(data_dir: &Path, s: Stored) -> Result<()> {
    let mut list = load(data_dir).unwrap_or_default();
    match list.iter_mut().find(|a| a.uuid == s.uuid) {
        Some(slot) => *slot = s,
        None => list.push(s),
    }
    save(data_dir, &list)
}

pub fn remove(data_dir: &Path, uuid: &str) -> Result<()> {
    let mut list = load(data_dir)?;
    list.retain(|a| a.uuid != uuid);
    save(data_dir, &list)
}

pub fn list(data_dir: &Path) -> Result<Vec<AccountInfo>> {
    Ok(load(data_dir)?.into_iter().map(|a| AccountInfo { id: a.uuid, name: a.name, kind: "msa" }).collect())
}

/// 启动游戏用：取出账号，令牌快过期就先刷新并保存。
pub async fn account_for_launch(data_dir: &Path, user_client_id: &str, uuid: &str) -> Result<Account> {
    let mut s = load(data_dir)?.into_iter().find(|a| a.uuid == uuid).ok_or_else(|| anyhow!("找不到这个账号，请重新登录"))?;
    if s.mc_expires <= now() + EXPIRY_MARGIN {
        s = refresh(&client_id(user_client_id)?, &s).await?;
        upsert(data_dir, s.clone())?;
    }
    Ok(Account::msa(&s.name, &s.uuid, &s.mc_token))
}

/// 账号当前的皮肤（PNG 字节和是否纤细手臂）；只从 Mojang 的贴图服务器下载。
pub async fn skin_of(data_dir: &Path, uuid: &str) -> Result<Option<(Vec<u8>, bool)>> {
    let Some(s) = load(data_dir)?.into_iter().find(|a| a.uuid == uuid) else { return Ok(None) };
    let Some(url) = s.skin_url.filter(|u| u.starts_with("https://textures.minecraft.net/")) else { return Ok(None) };
    let bytes = http()?.get(url).send().await?.error_for_status()?.bytes().await?;
    if bytes.len() > 256 * 1024 {
        bail!("皮肤文件异常大");
    }
    Ok(Some((bytes.to_vec(), s.skin_slim)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poll_states_are_classified() {
        assert_eq!(classify(&json!({"error": "authorization_pending"})), Poll::Pending);
        assert_eq!(classify(&json!({"error": "slow_down"})), Poll::SlowDown);
        assert_eq!(classify(&json!({"access_token": "a", "refresh_token": "r"})), Poll::Done { access: "a".into(), refresh: Some("r".into()) });
        assert!(matches!(classify(&json!({"error": "expired_token"})), Poll::Fail(m) if m.contains("过期")));
        assert!(matches!(classify(&json!({"error": "authorization_declined"})), Poll::Fail(m) if m.contains("拒绝")));
        assert!(matches!(classify(&json!({"error": "invalid_client", "error_description": "AADSTS700016: App not found\r\nTrace ID: x"})), Poll::Fail(m) if m.contains("AADSTS700016") && !m.contains("Trace")));
    }

    #[test]
    fn xbox_errors_are_explained() {
        assert!(xerr_message(2148916233).contains("Xbox 账号"));
        assert!(xerr_message(2148916238).contains("家庭"));
        assert!(xerr_message(1).contains("错误码 1"));
    }

    #[test]
    fn profile_picks_the_active_skin_and_variant() {
        let v = json!({"id": "abc123", "name": "Steve", "skins": [
            {"state": "INACTIVE", "url": "https://textures.minecraft.net/old", "variant": "CLASSIC"},
            {"state": "ACTIVE", "url": "https://textures.minecraft.net/new", "variant": "SLIM"}]});
        let p = parse_profile(&v).unwrap();
        assert_eq!((p.id.as_str(), p.name.as_str(), p.skin_url.as_deref(), p.slim), ("abc123", "Steve", Some("https://textures.minecraft.net/new"), true));
        assert!(parse_profile(&json!({"name": "x"})).is_err());
    }

    #[test]
    fn accounts_are_stored_encrypted_and_listed_without_tokens() {
        let dir = std::env::temp_dir().join(format!("flc-msa-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = Stored { uuid: "u1".into(), name: "Alex".into(), mc_token: "SECRET-TOKEN".into(), mc_expires: now() + 3600, refresh_token: "SECRET-REFRESH".into(), ..Default::default() };
        upsert(&dir, a.clone()).unwrap();
        upsert(&dir, Stored { name: "Alex2".into(), ..a }).unwrap();
        let list = list(&dir).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Alex2");
        assert!(!serde_json::to_string(&list).unwrap().contains("SECRET"));
        #[cfg(windows)]
        assert!(!String::from_utf8_lossy(&std::fs::read(store_path(&dir)).unwrap()).contains("SECRET"));
        remove(&dir, "u1").unwrap();
        assert!(self::list(&dir).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn token_that_is_still_valid_is_used_without_network() {
        let dir = std::env::temp_dir().join(format!("flc-msa-launch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        upsert(&dir, Stored { uuid: "u2".into(), name: "Steve".into(), mc_token: "tok".into(), mc_expires: now() + 7200, refresh_token: "r".into(), ..Default::default() }).unwrap();
        let acc = account_for_launch(&dir, "", "u2").await.unwrap();
        assert_eq!((acc.name.as_str(), acc.access_token.as_str(), acc.user_type.as_str()), ("Steve", "tok", "msa"));
        assert!(account_for_launch(&dir, "", "nobody").await.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
