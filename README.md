# Addon Wrench

使用 Rust + GPUI 开发的 Windows 桌面模组管理器，面向 Valve Source 引擎游戏。项目处于早期开发阶段。

## 功能

- 浏览本地 VPK，按名称、作者、标签搜索与筛选，支持列宽调整。
- 自定义分组、批量启停与本地 VPK 导入。
- 从 Steam 创意工坊补全缺失名称，在详情中阅读文字简介，支持离线缓存。

目前完整适配 **Left 4 Dead 2**；其他已识别的 Source 游戏提供只读浏览。工坊订阅、下载和更新仍由 Steam 管理。

## 构建与运行

需要 Rust stable、MSVC C++ Build Tools 和 Windows SDK（含 `fxc.exe`）。

```powershell
cargo build --release --locked
.\target\release\addon-wrench.exe
```

启动后选择游戏目录。游戏运行时自动保护模组列表，退出后刷新启用状态。

详细用法、数据目录与构建说明见 [使用手册](docs/manual.md)。

## 参与开发

欢迎通过 Issue 或 Pull Request 反馈问题、完善游戏适配与改进体验。提交前运行：

```powershell
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked -- --include-ignored
```

测试使用独立临时目录，默认全部启用；更多验证方式见 [使用手册](docs/manual.md#验证)。

## 许可

[MIT](LICENSE)。第三方许可见 [licenses](licenses/GPUI-APACHE-2.0.txt)。
