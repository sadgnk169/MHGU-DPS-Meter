# Research / 研究资料

这里放 MHGU DPS Meter 的逆向记录、失败实验、伤害显示模组兼容说明和可复用工具，面向维护者，以及想用 Forge 做类似 MHGU 插件的人。

1.2.0 只有一个通用 NRO `mhgu_dps_meter.nro`，已分别测试兼容 FF14-style Damage Text、Damage Numbers r30 和 MHGU Attack Damage Display。兼容矩阵、冲突排查和链式 Hook 原理见研究笔记第 10 节。

This folder contains reverse-engineering notes, failed experiments, damage-display compatibility notes, and reusable tools for maintainers and for anyone building similar MHGU plugins with Forge.

Version 1.2.0 ships one universal NRO, `mhgu_dps_meter.nro`, tested with FF14-style Damage Text, Damage Numbers r30, and MHGU Attack Damage Display. See section 10 of the research notes for the compatibility matrix, conflict checks, and chain-hook mechanism.

## Notes / 笔记

- [MHGU-Forge-DPS-研究笔记.md](MHGU-Forge-DPS-研究笔记.md) — 中文完整版
  - 关键内存偏移、Hook 流程、DPS/曲线结构、图标绘制、猎猫类型调查和发布检查。
- [MHGU-Forge-DPS-Research-Notes.md](MHGU-Forge-DPS-Research-Notes.md) — full English version
  - Memory offsets, hook flow, DPS/curve structure, icon rendering, Palico investigation, and release checks.

## Data / 数据

- `data/palico-v10.log`
  - v10 诊断版的原始日志：11 次快照、导出的 `0x1C00` 管理区（第 5 个管理块末尾被截断）和管理块指针目标。不是完整管理器快照。
  - Raw v10 diagnostic log: eleven snapshots, the exported `0x1C00` manager region (the end of block 5 is truncated), and manager-block pointer targets. This is not a complete manager snapshot.

## Tools / 工具

- `tools/verify_nro.py`
  - 离线检查 NRO 动态重定位，确认没有写入只读段。
  - Offline NRO relocation checker; verifies that no relocation writes into read-only segments.

## Caveats / 注意

- 所有地址只适用于 **MHGU 1.4.0 + Forge 1.5.1**。
- 诊断日志只用于研究，不包含游戏本体或存档文件。
- 重新测试前先备份存档；不要从未验证的测试版直接进入随机联机。
- The offsets apply only to **MHGU 1.4.0 + Forge 1.5.1**.
- The diagnostic log is research data only; it contains no game files or save data.
- Back up saves before experiments, and avoid random online testing with unverified builds.
