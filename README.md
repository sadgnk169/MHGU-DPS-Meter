# MHGU DPS Meter

独立的 **MHGU DPS Meter**，和 FF14 显伤共存：FF14 负责伤害数字，本插件只负责 DPS 面板。

- NRO：`00_mhgu_dps_meter.nro`
- Forge 插件列表名称：`MHGU DPS Meter`
- INI：`mhgu_dps_meter.ini`（旧的 `attack_damage_display.ini` 不再使用）
- SHA256：`80fba68f4bb4e86f6f6a96e64901c5f77300c51a090d66cc9a317dd9072b52a0`
- 大小：163840 字节

## 面板

- 位置：左侧、玩家/队伍名字下方，按 720p 基准 x = 16、y = 340；掌机/主机切换时按可见画面自动缩放；
- 行标签：默认显示 `You / P1 / P2 / P3 / P4`；打开 `dps_show_names` 后显示游戏内玩家名称；
- 内容：总伤害、占比、秒伤、可选命中次数；
- 计时：从本轮第一次有效命中开始；
- 关闭 DPS：在 Forge 菜单取消 `Enable DPS meter`，或把 INI 的 `enabled` 改为 `false`；
- 重置本轮数据：使用 Forge 菜单里的 `Reset DPS`；
- 不再使用 L/R/Plus/Minus 组合键。

## 单人/多人行为

- 单人任务也会显示；
- 换区继续累计，不会清零；
- 保存/结算画面（`local player == 0`）持续约 2 秒后自动清零；
- 任务中停手、普通换区不会清零；
- 如果 DPS 面板完全不出现，先确认：
  1. 菜单里 `Enable DPS meter` 已勾选；
  2. `enabled` 没有被设为 `false`；
  3. 旧的 `attack_damage_display.nro` 已删除。

## 配置

配置文件是 MHGU 存档根目录的 `mhgu_dps_meter.ini`。也可以在 Forge → Plugin UI → `MHGU DPS Meter` 中修改，点击 Save INI 保存。

手动修改或删除 INI 后需要**完全退出游戏再启动**；运行中的插件不会重新读取文件。Forge 菜单里的修改会实时预览。

```ini
; 配置唯一位置：MHGU 当前用户的存档根目录 /mhgu_dps_meter.ini
; Forge → Plugin UI → 本插件中编辑，点击 Save INI 保存到同一文件。
; MHGU DPS Meter v0.6
; 菜单改动实时预览；手动编辑 INI 或更换字体后重启。

[display]
; DPS 统计总开关。关闭后不统计伤害，也不会显示 DPS 面板。
enabled = true

; DPS 面板使用的字体。default = 随插件提供的 NotoSansSC 中文字体；
; 也可以填 TTF/OTF 文件名，例如 MyFont.ttf，
; 并把文件放到 romfs/nativeNX/forge/fonts/。
font_file = default

[dps]
; 黑色背景透明度，0 = 完全透明，255 = 完全不透明。
dps_background_alpha = 180.0

; 是否显示面板顶部的 DPS 计时。
dps_show_timer = true

; 是否显示每名玩家的总伤害。
dps_show_damage = true

; 是否显示每名玩家的伤害百分比。
dps_show_percent = true

; 是否显示每名玩家的每秒伤害（秒伤）。
dps_show_dps = true

; 是否在玩家名字后面显示命中次数，例如 12h。
; 开启后会强制使用上下两行样式，同行统计自动失效。
dps_show_hits = false

; true = 显示游戏内玩家名称；false = 显示 You/P1/P2/P3/P4。
dps_show_names = false

; true = 秒伤、总伤、百分比以同一字号排在右侧同一行；
; false = 保持总伤上排、秒伤下排的两行形式。
dps_inline_stats = true

; true = 在底部显示全队总伤害和全队秒伤（红色）。
dps_show_team = true

; 面板左上角 X 坐标，720p 基准像素；默认 16。
dps_offset_x = 16.0

; 面板左上角 Y 坐标，720p 基准像素；默认 340。
dps_offset_y = 340.0

; true = 显示上方累计伤害曲线图；
; false = 只关闭曲线图，下方玩家行、总伤、秒伤、百分比和进度条仍保留。
dps_show_curve = true

; 曲线面板宽度，720p 基准像素，允许 220～600；默认 270 预留约一位数字余量。
dps_curve_width = 270.0

; 上方曲线图高度，720p 基准像素，允许 55～200。
dps_curve_height = 55.0

; 是否在曲线下方画同色半透明填充；关闭后只显示曲线。
dps_curve_fill = true

; 是否在玩家名字下方显示同色百分比进度条。
dps_show_percent_bar = true

; true = 名字前显示武器图标（半透明 HUD 颜色覆盖）；
; false = 完全关闭图标，名字和 hit 占用图标位置。
dps_show_weapon = true
```


## 布局

界面固定使用曲线布局，旧的纯数字面板选项已移除。旧 INI 里的 `dps_curve` 键仍会被接受但忽略，不再切换到未优化的纯数字界面。

## 同行统计

默认 `dps_inline_stats=true`：秒伤、总伤、百分比使用同一字号排在右侧同一行，顺序为 **秒伤、总伤、百分比**。

- 总伤列固定预留 5 位 `99999` 的宽度，总伤左对齐；4→5 位向右增长，不推动秒伤；
- 秒伤列和百分比列固定右对齐；百分比预留 `100.0%` 宽度；
- 名字/hit 过长或超过 5 位时，面板、曲线和底部色条自动向右扩展；
- 上下两排格式不使用这些固定列改变。

## 全队统计

- 底部红色 `DMG` 行显示全队总伤和全队秒伤；
- 需要 `dps_show_team=true`、至少 2 名玩家；
- 关闭玩家总伤时，DMG/全队总伤隐藏；关闭玩家秒伤时，全队秒伤隐藏；
- 关闭 `Team summary` 或单人任务时整行隐藏。

## 命中次数

`dps_show_hits=true` 时：

- 命中次数使用与统计数字相同的字号，不再使用名称字号；
- 会自动强制使用上下两行格式，同行统计暂时失效，避免名称过长。

## 曲线模式

默认使用曲线布局：

- 上方是四名玩家整场累计伤害曲线，颜色读取游戏分配给玩家的 HUD 颜色；
- 曲线可以用 `dps_show_curve=false` 或菜单 `Show chart` 单独关闭，关闭后仍保留下方玩家行、右侧总伤/秒伤/百分比和百分比条；
- 曲线下方默认有同色半透明填充，可在菜单/INI 关闭；填充按 1px 列插值绘制，相邻列无缝；
- 每行底部有同色百分比进度条，100% 时延伸到面板最右侧；曲线与第一行之间的留白与普通玩家行之间一致；
- 右侧上排较大字号显示总伤害，下排较小字号显示秒伤，最右侧显示百分比；
- 曲线覆盖整场：从本轮第一次有效命中一直保留到当前，不再只显示最近 15 分钟；
- 历史固定为 600 个点：前约 10 分钟为 1 秒一个点，之后自动合并相邻点，时间分辨率逐步变为 2 秒、4 秒、8 秒；
- 每名玩家最多绘制 600 个点，比旧版的 900 点更少；填充仍按面板像素列绘制，绘制开销不随任务时长增加；
- 以最后一个采样点为右边界，最右侧不会跳动。

## 武器图标

- 武器类型读取游戏装备字段；
- 图标资源从 `图片` 文件夹内嵌到插件，猎猫会按类型切换战斗/协助/回复/收集/爆弹/防御/野兽/领袖图标；
- 图标按像素块绘制：720p 每格 2px，1080p 每格 3px；
- 保留源图阴影和高光，再乘以玩家 HUD 颜色；
- 把 `dps_show_weapon` 改为 `false` 会完全关闭图标，名字和 hit 占用图标位置。

## 玩家名称

Forge 菜单的 `Player names` 或 INI 的 `dps_show_names` 控制：

- 默认 `false`：显示 `You/P1/P2/P3/P4`；
- `true`：读取游戏内存中的 UTF-8 游戏内名称，读取不到时退回槽位标签。

显示的是游戏内名字，不是任天堂账号名。

当前猎猫模式仍显示猎人名字，猫名暂未接入。

## 玩家编号

`P1/P2/P3/P4` 是游戏内部 player manager 的槽位顺序，不是集会所名单顺序，也不是 HUD 从上到下顺序；面板行按伤害排序，所以 P2 可能显示在 P1 上面。主机屏幕通常显示本机玩家和 P2，客机屏幕显示 P1 和本机玩家。

## 安装

1. 保留 FF14 显伤，并放入 `00_mhgu_dps_meter.nro`；
2. 把 `romfs/nativeNX/forge/fonts/NotoSansSC-Medium.otf` 和 `OFL.txt` 放到对应目录；删除旧的 `mhgu_dps_meter.nro`、`attack_damage_display.nro` 和诊断版 NRO；
3. 不要同时装 damage-numbers；
4. 进任务，打一下怪，确认面板出现。

## 为什么文件名要带 `00_`

FF14 和本插件都会 hook 同一个伤害/碰撞函数。插件启动时会校验 6 组游戏函数的原始指令；如果 FF14 先加载并修改了函数开头，校验会失败，本插件就会跳过全部 hook，表现为面板完全不出现。

`00_` 前缀让本插件排在 `ff14_damage_text.nro` 之前加载，先安装 hook，然后 FF14 再挂第二个 hook。已验证这样单人和多人都会正常显示。如果以后重命名 NRO，请保持它排在 `ff14_damage_text.nro` 前面。


## 仓库结构

- `release/romfs/nativeNX/plugins/00_mhgu_dps_meter.nro`：正式 NRO；
- `release/romfs/nativeNX/forge/fonts/`：中文字体和 OFL 许可证；
- `release/conf/mhgu_dps_meter.ini`：默认配置；
- `source/`：可构建源码，依赖固定到 GitHub 上的 Forge 修订版；
- `source/README.md`：构建环境和命令。

源码构建需要 Rust nightly、devkitPro，并会从 GitHub 安装 `cargo-mhgu-forge`。构建命令：

```bash
cd source
bash build.sh
```

输出 `source/dist/00_mhgu_dps_meter.nro`。不要提交 `source/target/`、`source/dist/` 或本地日志。
