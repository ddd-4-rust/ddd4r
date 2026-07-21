# 安全模型

- 自有核心代码禁止 `unsafe`。
- Auth 适配器默认 fail-closed；缓存后端默认 fail-open。
- task-local 上下文在 Future 完成、取消或 panic unwind 后由 Tokio scope 回收。
- Cache key 必须包含 tenant、数据源、驱动和 schema generation。
- 二级缓存保存数据库/加密态结果，不保存解密后的敏感字段。
- 加密密钥只能通过 `KeyProvider` 获取，禁止出现在日志、错误、metrics 和 trace 中。
- rbatis-plus 字段信封使用带版本/Key ID 的 AES-256-GCM 与随机 96-bit nonce，并绑定表字段上下文。
- Blind index 使用独立 HMAC key；行签名使用 canonical JSON，篡改和未知 key 均 fail-closed。
- 结果管线固定 `RESULT_VERIFY` 先于 `RESULT_TRANSFORM`，保证先验签再解密。
- `REJECT_PARTIAL` 拒绝不可验签投影；`DEFERRED_RESIGN` 返回显式延迟补签状态。
- `RbatisMapper::with_interceptors` 已执行完整前后管线；当前 SM4/SM3 provider 和默认安全装配尚未完成，不能宣称国密兼容或默认启用。
- 依赖发布执行 `cargo audit`、`cargo deny`、SBOM 和 provenance 检查。
- 安全问题通过 GitHub Security Advisory 私下报告，不在公开 Issue 中携带利用细节。
