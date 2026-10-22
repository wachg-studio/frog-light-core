//! 登录令牌的加密存储。
//!
//! Windows 上用系统的 DPAPI 加密（只有当前 Windows 用户能解开，别的用户或把文件拷到别的电脑上都解不开）。
//! 其他系统暂时没有接入系统钥匙串，只能写成普通文件，权限收紧到仅当前用户可读。
use anyhow::{bail, Context, Result};
use std::path::Path;

#[cfg(windows)]
mod sys {
    use super::*;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB};

    fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 }
    }

    fn take(out: CRYPT_INTEGER_BLOB) -> Vec<u8> {
        // SAFETY: 系统保证 pbData 指向 cbData 个有效字节，用完要用 LocalFree 释放。
        unsafe {
            let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
            LocalFree(out.pbData as _);
            v
        }
    }

    pub fn protect(data: &[u8]) -> Result<Vec<u8>> {
        let input = blob(data);
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
        // SAFETY: 所有指针要么有效，要么是文档允许的空指针。
        let ok = unsafe { CryptProtectData(&input, std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0, &mut out) };
        if ok == 0 {
            bail!("加密登录信息失败（{}）", std::io::Error::last_os_error());
        }
        Ok(take(out))
    }

    pub fn unprotect(data: &[u8]) -> Result<Vec<u8>> {
        let input = blob(data);
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
        // SAFETY: 同上。
        let ok = unsafe { CryptUnprotectData(&input, std::ptr::null_mut(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0, &mut out) };
        if ok == 0 {
            bail!("无法解密登录信息（可能是换了 Windows 用户或电脑），请重新登录");
        }
        Ok(take(out))
    }
}

#[cfg(not(windows))]
mod sys {
    use super::*;
    pub fn protect(data: &[u8]) -> Result<Vec<u8>> {
        Ok(data.to_vec())
    }
    pub fn unprotect(data: &[u8]) -> Result<Vec<u8>> {
        Ok(data.to_vec())
    }
}

pub use sys::{protect, unprotect};

/// 加密后写入文件。
pub fn write(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let sealed = protect(data)?;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    std::fs::write(&tmp, &sealed).context("保存登录信息失败")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, path).context("保存登录信息失败")
}

/// 读取并解密；文件不存在返回 None。
pub fn read(path: &Path) -> Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(sealed) => Ok(Some(unprotect(&sealed)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).context("读取登录信息失败"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealed_data_round_trips_and_is_not_plaintext() {
        let dir = std::env::temp_dir().join(format!("flc-vault-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("a.bin");
        assert!(read(&path).unwrap().is_none());
        write(&path, b"refresh-token-123").unwrap();
        assert_eq!(read(&path).unwrap().as_deref(), Some(&b"refresh-token-123"[..]));
        #[cfg(windows)]
        assert!(!std::fs::read(&path).unwrap().windows(10).any(|w| w == b"refresh-to"), "文件里不应该有明文");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
