use crate::model::*;
use anyhow::{bail, Context, Result};
use std::path::Path;

/// 处理完 inheritsFrom 之后的完整版本描述。Fabric/Forge 等加载器版本靠这一层复用原版 JSON。
#[derive(Debug, Clone)]
pub struct Resolved {
    pub id: String,
    pub main_class: String,
    pub game_args: Vec<Arg>,
    pub jvm_args: Vec<Arg>,
    pub legacy_args: Option<String>,
    pub libraries: Vec<Library>,
    pub asset_index: AssetIndexRef,
    pub client: Option<Artifact>,
    /// 客户端 jar 所在的版本目录名（继承时指向父版本）。
    pub jar_id: String,
    pub java: Option<JavaVersion>,
    pub kind: String,
}

pub fn read_json(root: &Path, id: &str) -> Result<VersionJson> {
    let p = root.join("versions").join(id).join(format!("{id}.json"));
    let text = std::fs::read_to_string(&p).with_context(|| format!("读取版本 JSON 失败: {}", p.display()))?;
    serde_json::from_str(&text).with_context(|| format!("解析版本 JSON 失败: {}", p.display()))
}

pub fn resolve(root: &Path, id: &str) -> Result<Resolved> {
    resolve_depth(root, id, 0)
}

fn resolve_depth(root: &Path, id: &str, depth: u8) -> Result<Resolved> {
    if depth > 8 {
        bail!("版本继承链过深: {id}");
    }
    let v = read_json(root, id)?;
    let parent = match &v.inherits_from {
        Some(p) => Some(resolve_depth(root, p, depth + 1)?),
        None => None,
    };

    let args = v.arguments.clone().unwrap_or_default();
    let mut game_args = parent.as_ref().map(|p| p.game_args.clone()).unwrap_or_default();
    let mut jvm_args = parent.as_ref().map(|p| p.jvm_args.clone()).unwrap_or_default();
    game_args.extend(args.game);
    jvm_args.extend(args.jvm);

    // 子库在前，同 group:artifact:classifier 只保留第一个，这样加载器可以覆盖原版依赖。
    let mut libraries = v.libraries.clone();
    if let Some(p) = &parent {
        libraries.extend(p.libraries.iter().cloned());
    }
    let mut seen = std::collections::HashSet::new();
    libraries.retain(|l| seen.insert(lib_key(&l.name)));

    let main_class = v
        .main_class
        .clone()
        .or_else(|| parent.as_ref().map(|p| p.main_class.clone()))
        .with_context(|| format!("{id} 缺少 mainClass"))?;
    let asset_index = v
        .asset_index
        .clone()
        .or_else(|| parent.as_ref().map(|p| p.asset_index.clone()))
        .with_context(|| format!("{id} 缺少 assetIndex"))?;
    let client = v
        .downloads
        .as_ref()
        .and_then(|d| d.client.clone())
        .or_else(|| parent.as_ref().and_then(|p| p.client.clone()));
    let jar_id = v
        .jar
        .clone()
        .or_else(|| parent.as_ref().map(|p| p.jar_id.clone()))
        .unwrap_or_else(|| id.to_string());

    Ok(Resolved {
        id: id.to_string(),
        main_class,
        game_args,
        jvm_args,
        legacy_args: v.minecraft_arguments.clone().or_else(|| parent.as_ref().and_then(|p| p.legacy_args.clone())),
        libraries,
        asset_index,
        client,
        jar_id,
        java: v.java_version.clone().or_else(|| parent.as_ref().and_then(|p| p.java.clone())),
        kind: v.kind.clone().or_else(|| parent.as_ref().map(|p| p.kind.clone())).unwrap_or_else(|| "release".into()),
    })
}

fn lib_key(name: &str) -> String {
    let mut it = name.split(':');
    let g = it.next().unwrap_or("");
    let a = it.next().unwrap_or("");
    let _version = it.next();
    let classifier = it.next().unwrap_or("");
    format!("{g}:{a}:{classifier}")
}

/// 把 maven 坐标 `g:a:v[:classifier][@ext]` 转成仓库相对路径。
pub fn maven_path(name: &str) -> String {
    let (coord, ext) = name.split_once('@').unwrap_or((name, "jar"));
    let mut it = coord.split(':');
    let g = it.next().unwrap_or("").replace('.', "/");
    let a = it.next().unwrap_or("");
    let v = it.next().unwrap_or("");
    match it.next() {
        Some(c) => format!("{g}/{a}/{v}/{a}-{v}-{c}.{ext}"),
        None => format!("{g}/{a}/{v}/{a}-{v}.{ext}"),
    }
}

/// 版本 id 会直接拼进路径，必须是单层目录名。
pub fn check_id(id: &str) -> Result<()> {
    if id.is_empty() || id == "." || id == ".." || id.contains(['/', '\\']) {
        bail!("无效的版本名: {id}");
    }
    Ok(())
}

/// 删除一个已安装的版本（整个 versions/<id> 目录）。
/// 注意版本隔离时这个目录里也有该版本的存档、模组和配置；依赖库与资源是共享的，不会删。
/// 如果还有别的版本继承自它（比如装了加载器），拒绝删除并列出它们。
pub fn delete(root: &Path, id: &str) -> Result<()> {
    check_id(id)?;
    let dir = root.join("versions").join(id);
    if !dir.join(format!("{id}.json")).is_file() {
        bail!("版本 {id} 不存在");
    }
    let dependents: Vec<String> = installed(root)
        .into_iter()
        .filter(|o| o != id && read_json(root, o).ok().and_then(|j| j.inherits_from).as_deref() == Some(id))
        .collect();
    if !dependents.is_empty() {
        bail!("这些版本依赖它，请先删除它们: {}", dependents.join("、"));
    }
    std::fs::remove_dir_all(&dir).with_context(|| format!("删除 {} 失败", dir.display()))
}

/// 扫描 versions 目录，返回已安装版本 id。
pub fn installed(root: &Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(root.join("versions")) else { return vec![] };
    let mut out: Vec<String> = rd
        .flatten()
        .filter(|e| e.path().join(format!("{}.json", e.file_name().to_string_lossy())).is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, id: &str, json: &str) {
        let d = root.join("versions").join(id);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(format!("{id}.json")), json).unwrap();
    }

    #[test]
    fn loader_version_inherits_vanilla() {
        let root = std::env::temp_dir().join(format!("flc-test-{}", std::process::id()));
        write(&root, "1.20.1", r#"{"id":"1.20.1","type":"release","mainClass":"net.minecraft.client.main.Main",
            "assetIndex":{"id":"5","url":"u"},"downloads":{"client":{"url":"c","sha1":"s","size":1}},
            "arguments":{"game":["--a"],"jvm":["-Dx=1"]},
            "libraries":[{"name":"org.ow2.asm:asm:9.3"},{"name":"com.google:guava:31"}]}"#);
        write(&root, "fabric-1.20.1", r#"{"id":"fabric-1.20.1","inheritsFrom":"1.20.1","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient",
            "arguments":{"game":["--b"]},
            "libraries":[{"name":"org.ow2.asm:asm:9.6","url":"https://maven.fabricmc.net/"}]}"#);
        let r = resolve(&root, "fabric-1.20.1").unwrap();
        assert_eq!(r.main_class, "net.fabricmc.loader.impl.launch.knot.KnotClient");
        assert_eq!(r.jar_id, "1.20.1");
        assert_eq!(r.game_args.len(), 2);
        assert_eq!(r.asset_index.id, "5");
        let asm: Vec<_> = r.libraries.iter().filter(|l| l.name.contains(":asm:")).collect();
        assert_eq!(asm.len(), 1);
        assert_eq!(asm[0].name, "org.ow2.asm:asm:9.6");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn maven_coordinates() {
        assert_eq!(maven_path("net.fabricmc:fabric-loader:0.15.0"), "net/fabricmc/fabric-loader/0.15.0/fabric-loader-0.15.0.jar");
        assert_eq!(maven_path("org.lwjgl:lwjgl:3.3.3:natives-windows"), "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar");
        assert_eq!(maven_path("a.b:c:1@zip"), "a/b/c/1/c-1.zip");
    }
}
