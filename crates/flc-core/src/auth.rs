use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Account {
    pub name: String,
    pub uuid: String,
    pub access_token: String,
    pub user_type: String,
}

impl Account {
    /// 离线账号：UUID 与原版服务端的离线 UUID 算法一致（MD5 of "OfflinePlayer:<name>"，v3）。
    pub fn offline(name: &str) -> Self {
        let mut h = md5::compute(format!("OfflinePlayer:{name}")).0;
        h[6] = (h[6] & 0x0f) | 0x30;
        h[8] = (h[8] & 0x3f) | 0x80;
        let s = hex::encode(h);
        let uuid = format!("{}-{}-{}-{}-{}", &s[0..8], &s[8..12], &s[12..16], &s[16..20], &s[20..32]);
        Self { name: name.into(), uuid, access_token: "0".into(), user_type: "legacy".into() }
    }

    /// 微软（正版）账号；`uuid` 是不带横线的玩家 ID。
    pub fn msa(name: &str, uuid: &str, token: &str) -> Self {
        Self { name: name.into(), uuid: uuid.into(), access_token: token.into(), user_type: "msa".into() }
    }

    /// 老版本（1.6 以前）的 `--session` 参数要求 `token:<令牌>:<uuid>` 的形式。
    pub fn session(&self) -> String {
        if self.user_type == "msa" {
            format!("token:{}:{}", self.access_token, self.uuid)
        } else {
            self.access_token.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn offline_uuid_matches_vanilla_server() {
        // 已知值：原版服务端对离线玩家 "Notch" 生成的 UUID。
        assert_eq!(super::Account::offline("Notch").uuid, "b50ad385-829d-3141-a216-7e7d7539ba7f");
    }
}
