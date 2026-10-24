# MHGU DPS Meter — Research Notes

适用版本：**MHGU 1.4.0 + Forge 1.5.1**。这些地址都是 `main` 模块的相对偏移，换游戏版本后必须先做字节校验。

本文记录 DPS 面板用到的最小逆向结论、构建方法、失败实验和验证方式。它不是游戏内存的完整文档，只覆盖本项目实际验证过的部分。英文完整版见 [MHGU-Forge-DPS-Research-Notes.md](MHGU-Forge-DPS-Research-Notes.md)。

---

## 1. 结论摘要

- 插件是 Forge NRO，放在 `romfs/nativeNX/plugins/`。
- 通过两个游戏函数 Hook 采集命中：碰撞函数和伤害函数。
- 插件统计本机游戏能够处理到的命中；自己的伤害最准确。队友伤害能否完整出现取决于本机是否处理到该次命中，客机端更依赖同步。
- 玩家管理器和玩家对象的偏移在本文第 3 节。
- `player + 0x1380` **不是猎猫类型**，也不是一个稳定的类型值；它可能是每只猫的实例、外观、语音或其他 ID，具体语义尚未确认。1.1.0 已取消类型检测，所有猎猫统一使用 15 号图标。
- 图标源多数为 20×20 RGBA，旧野兽图是 22×22；当前统一图标是 20×20。绘制时按 2×2 源像素合并，720p 通常每个逻辑格 2px，1080p 通常为 3px。
- 实测猎猫图标视觉上比武器图标低约 2px；1.1.0 用 `PALICO_ICON_LIFT = 2.0` 对猎猫图标槽整体上移。这是经验修正，不是已确认的引擎槽位规则。
- 1.2.0 只有一个通用 NRO `mhgu_dps_meter.nro`，已分别测试兼容 FF14-style Damage Text v0.1.1、Damage Numbers r30、MHGU Attack Damage Display 和单独使用。
- 生成的 NRO 必须检查动态重定位不能写入只读段。本项目的 NRO 没有 `TEXTREL`。

## 2. 构建与验证

### 2.1 工具链

- Rust nightly，需要 `rust-src`（见 `source/rust-toolchain.toml`）。
- `cargo-mhgu-forge`，固定 Forge 修订版：
  `f2af0622a2a0aa291ffbd3d2f7628c0e2d96d220`
- 自定义 target：`source/armv7a-none-eabi.json`
  - `linker = "rust-lld"`
  - `-Tlink/switch32.ld`
  - `-C link-arg=--build-id=sha1`
- devkitPro/switch-tools：`build.sh` 会设置 devkitARM 链接器路径并调用 `cargo-forge` 打包 NRO；target spec 同时声明 `rust-lld`。使用 `build.sh` 时请确认 devkitARM 路径存在；仅运行 `cargo forge build` 时，链接器按 target spec 使用 `rust-lld`。
- 官方构建方式：

```bash
cd source
bash build.sh
```

`build.sh` 会把 `cargo-mhgu-forge` 安装到项目自己的 `target/forge-tools`，不污染全局环境。

### 2.2 本地离线构建

发布源码的 `Cargo.toml` 使用 GitHub 上的 Forge。网络受限时，可以把依赖临时指向本地 Forge 源码目录（路径按自己的目录替换）：

```toml
mhgu-forge = { path = "<本地 Forge 源码目录>/forge", default-features = false, features = ["imgui", "mt"] }
```

本地路径仅用于本机构建。

### 2.3 NRO 重定位检查

`tools/verify_nro.py` 会模拟加载器，映射 text/rodata/data/bss，解析动态符号和重定位，检查是否有重定位写入 text 或 rodata。放行条件：

```text
DT_TEXTREL present: False
reloc writes by segment: {'data': ...}
OK: every relocation targets writable memory
```

真实 Switch 上出现只读段写入会导致插件加载后崩溃，所以每次发布前都跑一次。

## 3. 运行时内存

`base` 是 MHGU main 模块的运行时基址，`GAME_BASE` 在 `forge_onLoad` 后保存。

| 内容 | 地址/偏移 | 说明 |
| --- | --- | --- |
| 玩家管理器 | `main + 0x01885B58` | 取到 `manager` 指针 |
| 本地玩家 | `manager` 传入 `main + 0x00276278` | 函数返回本地 player 对象 |
| 玩家槽位 | `manager + 0x1C + i*4`，`i=0..3` | 4 个 owner 指针，用于去重和标签；显示行按伤害排序，P1–P4 不是队伍顺序或主机顺序 |
| HUD 颜色编号 | `player + 0x100` | 掩码 `0xF00`；映射为 `0x100→0, 0x200→1, 0x400→3, 0x800→2`。这是颜色编号，不是玩家身份或槽位 |
| 武器类型指针 | `player + 0x13C4` | 指针目标 `+0x14C` 取 u8；`0..14` 武器，`15` 猎猫 |
| 房间字节/会话代际 | `player + 0x1054` | 与 player 指针组成 `Epoch`，用于判断浮动伤害数字是否属于当前任务实例；换区会清空旧数字事件，**不会清空 DPS 累计** |
| 玩家/猫名字 | `manager + 0x300 + i*0x560` | owner 在 `+0x288`，UTF-8 名字在 `+0x290`，最多 32 字节；名称块在普通任务和猎猫模式下的语义可能不同 |
| 猎猫 ID | `player + 0x1380` | 不是类型，也不是稳定的类型值；具体语义未确认，只用于实验 |

指针读取前至少排除低地址和接近 4GB 的范围；武器指针还要确认 `+0x14C` 在范围内：

```rust
if pointer < 0x10000 || pointer > 0xFFFF_0000 - 0x14C {
    return 0xFF;
}
```

名字块没有 owner 指针时，本地玩家可以使用 `i=0` 的本地名字回退。普通任务和猎猫模式的名字块布局不完全一致，所以代码同时保留精确 owner 匹配和本地回退。

### 3.1 `Epoch` 的准确含义

```rust
pub struct Epoch {
    pub player: u32, // 当前玩家对象指针
    pub room: u8,    // player + 0x1054 的房间字节
}
```

- 它表示“当前任务实例/会话代际”，不是时间单位。
- 命中时记录当时的 `Epoch`；伤害入账和投影时用它对号入座，避免上一区域的浮动数字出现在新区域。
- `forge_onUpdate` 发现 player 或 room 变化时调用 `STATE.reset_epoch()`，清理的是浮动数字/投影状态。
- DPS 累计是独立状态，换区不会清零；只在结算/保存画面出现后，或新任务第一次命中时重置。

## 4. 采集流程

### 4.1 Hook

| 目标 | 偏移 | 作用 |
| --- | --- | --- |
| 碰撞判定 | `main + 0x00170968` | 在原始碰撞调用前绑定 attack/owner/部位/世界坐标 |
| 伤害结算 | `main + 0x0009D6F8` | 原始伤害函数返回后确认伤害并入账 |

碰撞回调读取：

- `attack + 0x6D` 是否为 1；
- `attack + 0x10` 是否 forwarded；
- owner 在 forwarded 时取 `attack + 0x70`，否则取 `attack + 0x0C`；
- 部位对象 `part + 8` 是 slot，`part + 6` 是 part index；
- `point` 指向 3 个 f32 的世界坐标。

伤害回调只有在 `raw > 0`、`result > 0`、`mode != 2`、`slot < 8` 时才入账，避免异常和重复结算。

### 4.2 线程绑定

碰撞和伤害不是同一个调用栈，代码用 `thread_key()`（AArch32 TPIDRURO）区分线程，并用一个 `BINDINGS` 表保存当前线程的 contact。嵌套命中时恢复上一层绑定。

### 4.3 归属和显示

- 只有 `owner == local_player` 的伤害标记为本地。
- DPS 面板可以显示最多 4 个 owner；玩家槽位从 manager 槽位读取，但显示行会按伤害排序。
- 本地伤害用于自己的浮动数字和黄色/白色判定。DPS 曲线按所有已入账 owner 的累计伤害绘制，队友曲线仍受同步影响。
- 房间变化或玩家对象变化会让 `Epoch` 失效，清空旧浮动数字/投影；DPS 累计不会因换区清零，只在结算/保存画面或新任务第一次命中时重置。

## 5. DPS 状态和绘制

- `HISTORY_CAP = 600`：整场曲线最多 600 个点。
- `SAMPLE_INTERVAL = 1.0`：正常每秒采样一次。
- 点数满后合并相邻点，保留首点、偶数点和最后一点；累计伤害不平均，曲线末端仍是当前真实总伤害。
- 面板默认字号 `DPS_FONT_SIZE = 19.0`，以 720p 为基准，按 `display.height / 720` 缩放。
- 曲线宽度、高度、填充、名字、hits、inline stats、队伍合计等都在 INI 中配置。
- 配置路径：`savedata:/mhgu_dps_meter.ini`；默认模板在 `source/conf/mhgu_dps_meter.ini`，编译时嵌入。
- 中文字体：`romfs/nativeNX/forge/fonts/NotoSansSC-Medium.otf`，OFL 授权，许可证在发布包根目录的 `OFL.txt`。

### 5.1 图标

多数图标源文件是 20×20 RGBA；旧野兽图是 22×22，但 1.1.0 不再使用类型图标，当前统一猎猫图是 20×20。

`draw_pixel_icon` 的逻辑：

1. `logical_w = icon.width / 2`，`logical_h = icon.height / 2`；
2. 每个 2×2 源像素块取左上角像素；
3. 720p 通常 `cell = 2px`，1080p 通常 `cell = 3px`；其他分辨率由 `round(target / logical_w)` 计算；
4. 使用 HUD 颜色对 RGB 做乘法，保留 alpha；
5. 同一行连续相同颜色合并成一个矩形，减少 draw call。

猎猫统一图标是 `ICONS[15]`。实测猎猫图标视觉上比武器图标低约 2px；绘制时对 `icon_index >= PALICO_ICON` 减去：

```rust
const PALICO_ICON: usize = 15;
const PALICO_ICON_LIFT: f32 = 2.0;
let icon_bottom = icon_bottom - PALICO_ICON_LIFT * scale;
```

这是**经验修正**，不能证明引擎里存在“猎猫槽固定低 2px”的规则。旧猎猫图标在实机上也表现为同样的视觉偏移；新图顶部两行透明使偏移更明显。若未来更换图标，应重新确认偏移量。

## 6. 猎猫类型调查（失败实验）

### 6.1 最初假设

旧版本把 `player + 0x1380` 当作类型，并映射到 8 个图标：

```text
0xAB 战斗, 0x88 协助, 0x6C 回复, 0xBB 采集,
0x80 爆弹, 0x68 防御, 0xD4 野兽, 0x26 领袖
```

### 6.2 实测结果

用本地猎猫逐只测试并写 `[TYPE]` 日志：

| 测试类型 | `+0x1380` |
| --- | --- |
| 领袖 | `0x26, 0x1F, 0xBA, 0xCE` |
| 回复 | `0x6C` |
| 爆弹 | `0x80` |
| 采集 | `0xBB` |
| 防御 | `0x68` |
| 战斗 | `0xAB` |
| 协助 | `0x88` |
| 野兽 | `0xD4` |

按测试记录，4 只领袖得到 4 个不同的值。这只能证明 `+0x1380` 不是稳定的类型值；它可能是实例、外观、语音、模型变体或其他 ID，具体语义尚未确认。旧映射只是把当时测试到的某一只猫的 ID 当成了类型，不能用于其他猫。

### 6.3 搜索过但没有结果的区域

- v8 导出的 player 区域 `0x0000..0x4000`。这是导出范围，不能证明 player 对象只有 0x4000 字节。
- v10 导出的 `manager` 区域 `0x0000..0x1C00`。这不是完整 manager，也不覆盖完整的 5 个管理块。
- v9 完整导出的 5 个管理块 `manager + 0x300 + i*0x560`，每块 `0x560`。v10 原始日志只保留到 `0x1C00`，第 5 块末尾约 `0x1E0` 字节被截断。
- 最多 13 个候选玩家对象指针目标；部分快照因指针校验被跳过。
- 11 个候选管理块指针目标。
- 32 位字段、1/2 字节字段、1–8 位位域，按当时的测试分组搜索“4 只领袖相同、7 种类型互不相同”。

所有搜索都没有候选。因此**没有继续用内存 dump 找类型**。1.1.0 取消类型检测，统一使用猎猫图标。

原始 v10 诊断日志在 `research/data/palico-v10.log`。它包含 11 次快照和 v10 导出的 `0x1C00` 管理区（第 5 块末尾截断），可作为后续重新调查的起点，但不能当作完整管理器快照。

## 7. 诊断方法

- 组合键：**L + R + Minus**。
- 日志路径：`savedata:/dps_debug.log`。
- 诊断版不记录每次 `[HIT]`，否则会很快占满缓冲区。
- v8 导出玩家对象 `0x4000` 区域；v9 导出管理块和指针目标；v10 导出 `0x1C00` 管理区域。
- 日志在内存中累计，每次按键覆盖保存完整累计内容；测试 11 只猫约 370KB。
- 读指针目标前先检查地址范围和对齐；失效指针可能导致崩溃，所以只在任务内、玩家对象有效时按组合键。
- 联机测试优先本地无线，不要在未稳定版本上随机匹配，避免影响其他玩家。

## 8. 发布结构

发布包只包含运行所需文件：

```text
MHGU-DPS-Meter-v1.2.0.zip
  README.md
  conf/mhgu_dps_meter.ini
  docs/demo.jpg
  romfs/nativeNX/forge/fonts/NotoSansSC-Medium.otf
  OFL.txt
  romfs/nativeNX/plugins/mhgu_dps_meter.nro
```

发布前检查：

1. `cargo +nightly forge build --release` 成功；
2. NRO `verify_nro.py` 没有只读段重定位；
3. NRO 启动字符串版本与 `Cargo.toml` 一致；
4. INI 头部版本一致；
5. NRO 的 SHA256 与 `成品` 目录一致；
6. NRO 跑一次 `verify_nro.py`。

## 9. 已知限制

- 只适用于 MHGU 1.4.0；游戏更新后偏移可能变化。
- 队友伤害能否出现取决于本机是否处理到该次命中；客机端更依赖同步，可能延迟、缺失或与结算不一致。
- 猎猫类型不自动识别；所有猎猫使用统一图标。
- 武器类型读取失败时（返回 `0xFF`）不显示图标，避免把猎人误判成猎猫。
- 没有在线上随机匹配中验证；正式测试应使用本地无线或可信队伍。


## 10. 伤害显示模组兼容性

### 10.1 Hook 路径

| 插件 | 主要 Hook/补丁地址 |
| --- | --- |
| DPS 通用版 | `0x0009D6F8`、`0x00170968` |
| [Damage Numbers r30](https://gamebanana.com/mods/725408) | `0x0009D6F8`、`0x00170968`、`0x00276278` |
| [MHGU Attack Damage Display](https://github.com/Lawrence-al/mhgu-attack-damage-display) | `0x0009D6F8`、`0x00170968` |
| [FF14-style Damage Text v0.1.1](https://gamebanana.com/mods/727751) | 不使用上述入口，走另一条显示路径 |

DPS、Damage Numbers 和 Attack Damage Display 使用同一组命中入口；FF14 显伤使用另一条路径。

### 10.2 如何兼容

通用版 `mhgu_dps_meter.nro` 做了两件事：

1. Hook 安装延迟到第一次 `forge_onUpdate`，此时所有插件的 `forge_onInit` 已执行完，Damage Numbers / Attack Damage Display 已先安装；
2. `compatible()` 允许 `0x170968`、`0x9D6F8`、`0x276278` 已被其他模组修改的入口继续安装。

Forge 的 `forge_hook_create` 会把当前入口复制到跳板，因此形成：

```text
目标入口 -> DPS detour -> 其他显伤 detour -> 游戏原函数
```

两个 detour 都会调用自己的 original，所以两边都能工作。文件名前缀和加载顺序不再是兼容条件。通用版已分别实测可与 Damage Numbers r30、Attack Damage Display 和 FF14-style Damage Text 共存。

通用版对这三个入口采用“允许已被修改”的策略；更换游戏版本后必须重新验证。

### 10.3 使用与冲突排查

- 只安装一个 DPS NRO：`mhgu_dps_meter.nro`。
- FF14-style Damage Text 可以和 Damage Numbers 或 Attack Damage Display 同时存在。
- Damage Numbers 和 Attack Damage Display 彼此占用同一入口，**不能同时启用**；DPS 无法替它们解决互相冲突。
- 不要同时保留旧版本或其他 DPS NRO。
- 如果 DPS 不计数或伤害显示无效果，检查 Forge 日志：
  - `invalid or overlapping hook recipe` / `Forge hook installation failed`：可能还装着另一个 DPS NRO，或同时启用了 Damage Numbers 和 Attack Damage Display。
  - `already patched ... chaining`：正常日志，说明 DPS 接在已有 Hook 后面。
  - `hooks skipped` / `hook installation failed`：签名校验失败，检查游戏版本和 NRO。
- 如果崩溃或数字异常，删除 NRO 并恢复旧版本；不要继续在随机联机中测试。
