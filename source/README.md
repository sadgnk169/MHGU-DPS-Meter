# MHGU DPS Meter — 源码构建

这是 `MHGU DPS Meter` 的源码目录。成品 NRO、字体和默认配置在仓库根目录的 `release/` 中。

## 构建环境

- Rust nightly（需要 `rust-src`，见 `rust-toolchain.toml`）
- devkitPro/switch-tools：`build.sh` 会设置 devkitARM 链接器路径并调用 `cargo-forge` 打包 NRO；如果只运行 `cargo forge build`，链接器按 target spec 使用 `rust-lld`
- 网络访问：`build.sh` 会从固定 Forge 修订版安装 `cargo-mhgu-forge`
- Windows 可用 Git Bash 运行；`build.ps1` 是 PowerShell 包装

## 构建

```bash
cd source
bash build.sh
```

输出：

```text
source/dist/00_mhgu_dps_meter.nro
```

构建依赖固定为：

```toml
mhgu-forge = { git = "https://github.com/Lawrence-al/forge-rs", rev = "f2af0622a2a0aa291ffbd3d2f7628c0e2d96d220", ... }
```

首次构建会自动获取 Forge 源码并生成 `Cargo.lock`。`target/`、`dist/` 和本地 NRO 不提交。

## 文件

- `src/lib.rs`：插件入口、hooks、曲线和面板绘制
- `src/data.rs`：配置、DPS 状态、投影和历史采样
- `src/icons.rs`：从内嵌图标生成的数据
- `shared/plugin_config.rs`：INI 读写、Forge 菜单辅助
- `conf/mhgu_dps_meter.ini`：编译时嵌入的默认配置
- `link/`、`.cargo/`、`armv7a-none-eabi.json`：Switch ARM 链接配置

## Research / 研究资料

中文和英文研究笔记、原始诊断日志和 NRO 检查工具见 [../research/README.md](../research/README.md)。

Chinese and English notes, the raw diagnostic log, and the NRO checker are in [../research/README.md](../research/README.md).
