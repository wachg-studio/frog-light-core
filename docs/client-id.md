# 获取微软登录的 Client ID

FLC 的微软登录走设备码流程，需要一个在 Azure 注册的应用 Client ID。拿到它分几种情况。

## 先确认你有一个 Entra 目录

个人 Microsoft 账户如果从来没碰过 Azure，默认不属于任何 Entra 目录。微软已经停止“在目录外创建应用”，这种情况下打开“应用注册”会提示：

> 这些应用程序与帐户 xxx 关联，但不包含在任何目录中。在目录外部创建应用程序的功能已被弃用。

解决办法是给账户建一个目录，二选一：

- 注册免费 Azure：<https://azure.microsoft.com/free>。需要手机验证和一张银行卡做身份验证，不会扣费（有支出保护）。
- 加入 Microsoft 365 开发者计划：<https://developer.microsoft.com/microsoft-365/dev-program>，会得到一个独立的租户，也可能要求银行卡。

如果账户已经有工作/学校目录，或者之前注册过 Azure，就可以直接跳到下一步。

## 注册应用

1. 打开 <https://portal.azure.com>，搜索“应用注册”（App registrations），点“新注册”。
2. 名称随便填，比如 `FLC`。
3. **受支持的帐户类型**选“仅个人 Microsoft 账户”（对应微软的 `consumers` 端点，FLC 用的就是它）。
4. 点“注册”。
5. 在应用概览页复制“应用程序(客户端) ID”，形如 `12345678-1234-1234-1234-123456789abc`。这就是要填到 FLC 的 Client ID。

## 打开公共客户端流

1. 在应用左侧点“身份验证”（Authentication）。
2. 拉到最下面的“高级设置”，把“允许公共客户端流”（Allow public client flows）设为“是”。
3. 保存。

设备码流程依赖这个开关，不开的话换取令牌会失败。

## 填到 FLC

设置 → 账号 → 微软登录 Client ID，粘贴刚才复制的 ID 即可。

想让所有用户都不用自己填，可以在构建时内置：

```bash
FLC_MS_CLIENT_ID=你的ID cargo build --release -p flc-app --features tauri/custom-protocol
```

发布到 GitHub Releases 的构建，则在仓库 Secrets 里配置 `FLC_MS_CLIENT_ID`。

## 还需要通过 Minecraft 服务审核

新注册的应用默认没有 Minecraft 的访问权限。登录最后一步会返回 403（`Invalid app registration`），界面会提示“Client ID 还没有通过 Minecraft 的审核”。Azure 应用注册和 Minecraft 的 AppID 审核是两件独立的事，后者必须单独提交：

- 官方说明：[Minecraft Java Edition API Application Approval Process](https://help.minecraft.net/hc/en-us/articles/16254801392141)
- 提交表单：[AppID Review form](https://forms.cloud.microsoft/Pages/ResponsePage.aspx?id=v4j5cvGGr0GRqy180BHbR-ajEQ1td1ROpz00KtS8Gd5UNVpPTkVLNFVROVQxNkdRMEtXVjNQQjdXVC4u)

表单要填：联系人邮箱（要和 Azure 账户一致）、应用名称（不能包含 Mojang、Minecraft、Microsoft、Live、Xbox、Discord、Hypixel）、应用程序(客户端) ID、目录(租户) ID、网站，以及对 API 用途的说明，并确认已阅读 EULA 和使用指南。审核大致每周一批，重复提交不会加快。

审核通过前，应用只能自己测试；通过后所有用户都能直接用内置的 Client ID 登录，不需要任何配置。

## 常见错误

- `AADSTS900144: The request body must contain ... 'client_id'`：没有填 Client ID，或构建时的环境变量是空的。0.1.2 之后的版本会直接提示“还没有配置”。
- `AADSTS16000` / “所选用户帐户在租户 Microsoft Services 中不存在”：你的账户没有 Entra 目录，或者把微软自家的应用 ID 填了进来。注意 `c44b4083-3bb0-49c1-b47d-974e53cbdf3c` 是 Azure 门户自己的 ID，不是你的。
- `AADSTS700016: Application with identifier ... was not found`：ID 复制错了，或者应用注册在了另一个目录里。
- 登录最后一步 403：应用还没通过 Minecraft 审核。

## 参考

- 微软身份平台设备码流程：<https://learn.microsoft.com/entra/identity-platform/v2-oauth2-device-code>
- 本项目的构建说明：[构建与发布.md](构建与发布.md)
