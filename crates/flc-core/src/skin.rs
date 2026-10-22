//! 离线账号的皮肤：把选中的皮肤打成一个资源包，覆盖游戏内置的默认皮肤，并在 options.txt 里启用它。
//!
//! 离线账号没有 Mojang 的皮肤服务可查，游戏只会显示按 UUID 挑出来的默认皮肤，所以这里替换的就是默认皮肤贴图。
//! 贴图本身一定是用户选的；手臂粗细（经典 / 纤细）由游戏按 UUID 决定，FLC 管不到。
use anyhow::{bail, Context, Result};
use std::io::Write;
use std::path::Path;

pub const PACK_FILE: &str = "flc-skin.zip";

#[derive(Clone, Debug)]
pub struct Skin {
    /// 64×64 的 PNG。
    pub png: Vec<u8>,
}

/// 只接受标准的 64×64 皮肤。64×32 的老格式在界面导入时已经转换过。
pub fn validate(png: &[u8]) -> Result<()> {
    if png.len() < 24 || !png.starts_with(b"\x89PNG\r\n\x1a\n") || &png[12..16] != b"IHDR" {
        bail!("皮肤必须是 PNG 图片");
    }
    let (w, h) = (u32::from_be_bytes(png[16..20].try_into()?), u32::from_be_bytes(png[20..24].try_into()?));
    if (w, h) != (64, 64) {
        bail!("皮肤尺寸必须是 64×64（当前 {w}×{h}）");
    }
    Ok(())
}

/// 版本对应的资源包格式号，以及 options.txt 是否用 1.13 之后的写法（`file/` 前缀、带 `vanilla`）。
/// 格式号只是尽力而为：包里同时声明了很宽的 supported_formats，游戏不会因此拒绝加载。
fn era(mc: &str, kind: &str) -> Option<(u32, bool)> {
    if kind.starts_with("old_") {
        return None; // 远古版本没有资源包功能
    }
    if let Some((yy, ww)) = mc.split_once('w').and_then(|(y, w)| Some((y.parse::<u32>().ok()?, w.get(..2)?.parse::<u32>().ok()?))) {
        let format = match yy {
            ..=16 => 3,
            17 => 4,
            18 => 4,
            19 => 5,
            20 => 6,
            21 => 7,
            22 => 9,
            23 => 15,
            24 => 34,
            _ => 46,
        };
        return Some((format, yy > 17 || (yy == 17 && ww >= 43)));
    }
    let Some(rest) = mc.strip_prefix("1.") else {
        // 26.3 这类按年份编号的新版本：当作最新的格式。
        return mc.starts_with(|c: char| c.is_ascii_digit()).then_some((55, true));
    };
    let mut it = rest.split(|c: char| !c.is_ascii_digit());
    let minor: u32 = it.next()?.parse().ok()?;
    let patch: u32 = it.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let format = match (minor, patch) {
        (..=8, _) => 1,
        (9..=10, _) => 2,
        (11..=12, _) => 3,
        (13..=14, _) => 4,
        (15, _) | (16, 0..=1) => 5,
        (16, _) => 6,
        (17, _) => 7,
        (18, _) => 8,
        (19, 0..=2) => 9,
        (19, 3) => 12,
        (19, _) => 13,
        (20, 0..=1) => 15,
        (20, 2) => 18,
        (20, 3..=4) => 22,
        (20, _) => 32,
        (21, 0..=1) => 34,
        (21, 2..=3) => 42,
        (21, 4) => 46,
        _ => 55,
    };
    Some((format, minor >= 13))
}

const CHARACTERS: [&str; 9] = ["steve", "alex", "ari", "efe", "kai", "makena", "noor", "sunny", "zuri"];

fn build_pack(png: &[u8], pack_format: u32) -> Result<Vec<u8>> {
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    z.start_file("pack.mcmeta", opts)?;
    write!(
        z,
        r#"{{"pack":{{"pack_format":{pack_format},"supported_formats":{{"min_inclusive":1,"max_inclusive":999}},"description":"FLC skin"}}}}"#
    )?;
    // 1.19.3 之前默认皮肤只有 steve / alex 两张；之后每个角色各有 wide / slim 两个版本。路径全写上，多余的文件不会有影响。
    let mut paths: Vec<String> = ["steve", "alex"].iter().map(|n| format!("assets/minecraft/textures/entity/{n}.png")).collect();
    for kind in ["wide", "slim"] {
        paths.extend(CHARACTERS.iter().map(|n| format!("assets/minecraft/textures/entity/player/{kind}/{n}.png")));
    }
    for p in paths {
        z.start_file(p, opts)?;
        z.write_all(png)?;
    }
    Ok(z.finish()?.into_inner())
}

/// 把 `resourcePacks:[...]` 里的 FLC 皮肤包加上或去掉；其余设置原样保留。
fn set_in_options(game_dir: &Path, flat: bool, on: bool) -> Result<()> {
    let path = game_dir.join("options.txt");
    let old = std::fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<String> = old.lines().map(str::to_string).collect();
    let idx = lines.iter().position(|l| l.starts_with("resourcePacks:"));
    let mut packs: Vec<String> = match idx {
        Some(i) => serde_json::from_str(&lines[i]["resourcePacks:".len()..]).unwrap_or_default(),
        None if flat => vec!["vanilla".into()],
        None => vec![],
    };
    let entry = if flat { format!("file/{PACK_FILE}") } else { PACK_FILE.to_string() };
    packs.retain(|p| *p != entry && *p != PACK_FILE);
    if on {
        packs.push(entry); // 列表里越靠后优先级越高
    }
    if idx.is_none() && !on {
        return Ok(());
    }
    let line = format!("resourcePacks:{}", serde_json::to_string(&packs)?);
    match idx {
        Some(i) => lines[i] = line,
        None => lines.push(line),
    }
    let new = lines.join("\n") + "\n";
    if new != old {
        std::fs::write(&path, new).context("写入 options.txt 失败")?;
    }
    Ok(())
}

/// 启动前调用：有皮肤就生成并启用资源包，没有就清理掉之前留下的。
pub fn apply(game_dir: &Path, mc: &str, kind: &str, skin: Option<&Skin>) -> Result<()> {
    let Some((format, flat)) = era(mc, kind) else { return Ok(()) };
    let pack = game_dir.join("resourcepacks").join(PACK_FILE);
    match skin {
        Some(s) => {
            validate(&s.png)?;
            std::fs::create_dir_all(pack.parent().unwrap())?;
            let data = build_pack(&s.png, format)?;
            if std::fs::read(&pack).ok().as_deref() != Some(&data) {
                std::fs::write(&pack, data).context("写入皮肤资源包失败")?;
            }
            set_in_options(game_dir, flat, true)
        }
        None => {
            if pack.exists() {
                let _ = std::fs::remove_file(&pack);
                set_in_options(game_dir, flat, false)?;
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        v.extend(w.to_be_bytes());
        v.extend(h.to_be_bytes());
        v.extend([8, 6, 0, 0, 0]);
        v
    }

    fn temp(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("flc-skin-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn only_64x64_png_is_accepted() {
        assert!(validate(&png(64, 64)).is_ok());
        assert!(validate(&png(64, 32)).is_err());
        assert!(validate(b"not a png at all, really not").is_err());
    }

    #[test]
    fn pack_format_follows_version() {
        assert_eq!(era("1.8.9", "release"), Some((1, false)));
        assert_eq!(era("1.12.2", "release"), Some((3, false)));
        assert_eq!(era("1.16.5", "release"), Some((6, true)));
        assert_eq!(era("1.21.1", "release"), Some((34, true)));
        assert_eq!(era("1.21.2-rc1", "snapshot"), Some((42, true)));
        assert_eq!(era("24w40a", "snapshot"), Some((34, true)));
        assert_eq!(era("13w36a", "snapshot"), Some((3, false)));
        assert_eq!(era("b1.7.3", "old_beta"), None);
        assert_eq!(era("26.3", "release"), Some((55, true)));
    }

    #[test]
    fn options_line_is_added_replaced_and_removed() {
        let d = temp("opts");
        std::fs::write(d.join("options.txt"), "fov:0.5\nresourcePacks:[\"vanilla\",\"file/other.zip\"]\n").unwrap();
        set_in_options(&d, true, true).unwrap();
        assert_eq!(std::fs::read_to_string(d.join("options.txt")).unwrap(), "fov:0.5\nresourcePacks:[\"vanilla\",\"file/other.zip\",\"file/flc-skin.zip\"]\n");
        set_in_options(&d, true, true).unwrap(); // 重复启用不会产生两份
        assert_eq!(std::fs::read_to_string(d.join("options.txt")).unwrap().matches("flc-skin").count(), 1);
        set_in_options(&d, true, false).unwrap();
        assert_eq!(std::fs::read_to_string(d.join("options.txt")).unwrap(), "fov:0.5\nresourcePacks:[\"vanilla\",\"file/other.zip\"]\n");

        let fresh = temp("fresh");
        set_in_options(&fresh, true, true).unwrap();
        assert_eq!(std::fs::read_to_string(fresh.join("options.txt")).unwrap(), "resourcePacks:[\"vanilla\",\"file/flc-skin.zip\"]\n");
        let old = temp("legacy");
        set_in_options(&old, false, true).unwrap();
        assert_eq!(std::fs::read_to_string(old.join("options.txt")).unwrap(), "resourcePacks:[\"flc-skin.zip\"]\n");
        let none = temp("none");
        set_in_options(&none, true, false).unwrap();
        assert!(!none.join("options.txt").exists());
    }

    #[test]
    fn apply_writes_pack_and_cleans_up() {
        let d = temp("apply");
        apply(&d, "1.21.1", "release", Some(&Skin { png: png(64, 64) })).unwrap();
        let mut z = zip::ZipArchive::new(std::fs::File::open(d.join("resourcepacks").join(PACK_FILE)).unwrap()).unwrap();
        assert!(z.by_name("pack.mcmeta").is_ok());
        assert!(z.by_name("assets/minecraft/textures/entity/player/slim/alex.png").is_ok());
        assert!(z.by_name("assets/minecraft/textures/entity/steve.png").is_ok());

        apply(&d, "1.21.1", "release", None).unwrap();
        assert!(!d.join("resourcepacks").join(PACK_FILE).exists());
        assert!(!std::fs::read_to_string(d.join("options.txt")).unwrap().contains("flc-skin"));
        assert!(apply(&d, "1.21.1", "release", Some(&Skin { png: png(10, 10) })).is_err());
    }
}
