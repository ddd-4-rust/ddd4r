# 安全模型

- 自有核心代码禁止 `unsafe`。
- Auth 适配器默认 fail-closed；缓存后端默认 fail-open。
- Auth 默认签发 realm 前缀的 UUIDv7 opaque token，不使用 login ID 作为凭证；Bearer token 只有在
  `verify` 成功后才能写入 task-local 上下文。
- `SessionStore` 在一个写锁临界区内执行 token 唯一性、共享/互斥登录、设备范围替换和最大会话数策略；
  禁用账号会立即撤销已有会话，空权限、空角色、未知 token 和失效 token 全部 fail-closed。
- 临时 token 与 API Key 存储只保留 keyed BLAKE3 digest；API Key pepper、Security credential hash
  和 Shiro credential 使用 `Zeroizing` 在最终释放时清零，并从 Debug/序列化输出中剔除。
- task-local 上下文在 Future 完成、取消或 panic unwind 后由 Tokio scope 回收。
- Cache key 必须包含 tenant、数据源、驱动和 schema generation。
- 二级缓存保存数据库/加密态结果，不保存解密后的敏感字段。
- 加密密钥只能通过 `KeyProvider` 获取，禁止出现在日志、错误、metrics 和 trace 中。
- rbatis-plus 字段信封支持带版本/Key ID 的 AES-256-GCM，以及 SM4-CBC/PKCS#7 + HMAC-SM3 Encrypt-then-MAC；两者均使用随机 IV/nonce 并绑定表字段上下文。
- Blind index 使用独立 HMAC key；行签名使用 canonical JSON，篡改和未知 key 均 fail-closed。
- 结果管线固定 `RESULT_VERIFY` 先于 `RESULT_TRANSFORM`，保证先验签再解密。
- `REJECT_PARTIAL` 拒绝不可验签投影；`DEFERRED_RESIGN` 返回显式延迟补签状态。
- `SecurePipelineBuilder` 保留加密、验签和解密阶段，拒绝缺失策略及覆盖关键阶段的 fail-open 装配。
- SM4/HMAC-SM3 使用 `gm1.key-id.iv.ciphertext.tag` 信封并先验 MAC 后解密；ddd4j 默认适配路径因空 mode/padding/key/IV 无法形成稳定金标，迁移不宣称原始密文字节兼容。
- 每次 push/PR 使用固定版本执行 `cargo audit --deny warnings` 与 `cargo deny check`；未知依赖来源、未许可许可证、通配版本和 RustSec advisory 均阻断构建。
- 唯一临时 advisory 例外由 `tools/verify_supply_chain_policy.py` 同时校验 ID、`0.x` 版本线、CI 命令和 2026-09-30 截止日期；超期或进入 `1.x` 自动失败。
- tag 发布使用 `cargo-cyclonedx 0.5.9` 聚合全部 84 个 package，生成无本机路径且可重复的 CycloneDX 1.5 SBOM，并由固定 SHA-256 的官方 CycloneDX CLI 0.32.0 校验 schema；源码包、SHA-256、SBOM 和 GitHub Sigstore provenance/SBOM attestation 一并发布。
- 安全问题通过 GitHub Security Advisory 私下报告，不在公开 Issue 中携带利用细节。
