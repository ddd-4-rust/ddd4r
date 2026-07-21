# Contributing

1. 使用 `rust-toolchain.toml` 固定的工具链。
2. 架构变化必须同时修改 C4 文档和 ADR。
3. 移植模块必须在 `port-manifest.toml` 增加 API 与测试证据后才能标记为 `complete`。
4. 提交前运行：

```bash
python3 tools/verify_port_manifest.py
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
```

禁止在 PR 中提交密钥、数据库密码、真实 Token、生产数据或未审计的生成文件。
