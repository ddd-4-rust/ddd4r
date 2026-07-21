# 安全模型

- 自有核心代码禁止 `unsafe`。
- Auth 适配器默认 fail-closed；缓存后端默认 fail-open。
- task-local 上下文在 Future 完成、取消或 panic unwind 后由 Tokio scope 回收。
- Cache key 必须包含 tenant、数据源、驱动和 schema generation。
- 二级缓存保存数据库/加密态结果，不保存解密后的敏感字段。
- 加密密钥只能通过 `KeyProvider` 获取，禁止出现在日志、错误、metrics 和 trace 中。
- 依赖发布执行 `cargo audit`、`cargo deny`、SBOM 和 provenance 检查。
- 安全问题通过 GitHub Security Advisory 私下报告，不在公开 Issue 中携带利用细节。
