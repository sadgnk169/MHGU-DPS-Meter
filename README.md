# MHGU DPS Meter

[中文](README.md) | [English](README.en.md)

《怪物猎人 XX / Generations Ultimate》的 Forge DPS 面板插件，显示队伍四名玩家的整场累计伤害、总伤害、秒伤、百分比和累计伤害曲线。

- 前置：需要 **Forge 1.5.1**：[https://github.com/Lawrence-al/forge](https://github.com/Lawrence-al/forge)
- NRO：`00_mhgu_dps_meter.nro`
- Forge 插件名称：`MHGU DPS Meter`
- INI：`mhgu_dps_meter.ini`
- SHA256：`80fba68f4bb4e86f6f6a96e64901c5f77300c51a090d66cc9a317dd9072b52a0`
- 大小：163840 字节

## 功能

- 显示四名玩家的整场累计伤害曲线；
- 显示每名玩家的总伤害、百分比、秒伤和累计伤害；
- 默认使用秒伤、总伤、百分比同行布局，也可以切换为上下两排；
- 支持武器图标、游戏内玩家名称和全队统计；
- 面板颜色跟随玩家的 HUD 颜色；
- 支持掌机和主机模式自动适配。

## 安装

1. 安装 **Forge 1.5.1** 前置：[https://github.com/Lawrence-al/forge](https://github.com/Lawrence-al/forge)；
2. 把 `romfs` 目录放到 MHGU 对应的 mod 目录：
   - `romfs/nativeNX/plugins/00_mhgu_dps_meter.nro`
   - `romfs/nativeNX/forge/fonts/NotoSansSC-Medium.otf`
   - `romfs/nativeNX/forge/fonts/OFL.txt`
3. 把 `conf/mhgu_dps_meter.ini` 放到 MHGU 当前用户的存档根目录，或首次运行时让插件自动生成；
4. 完全退出并重启游戏；
5. 进入 Forge 菜单，确认 `Enable DPS meter` 已开启。

## 使用

- 面板默认在画面左侧、玩家名字下方，位置由 `dps_offset_x` 和 `dps_offset_y` 控制；
- 默认行标签为 `You / P1 / P2 / P3 / P4`；
- 打开 `dps_show_names` 后显示游戏内玩家名称，读取不到时自动退回槽位标签；
- 面板按伤害从高到低排列；
- 关闭统计：在 Forge 菜单取消 `Enable DPS meter`，或把 INI 的 `enabled` 改为 `false`；
- 重置本轮数据：使用 Forge 菜单中的 `Reset DPS`；
- 换区会保留累计数据；结算或保存画面出现约 2 秒后自动清零；
- 单人任务也会显示，但底部全队统计只在两名及以上玩家时出现。

## 配置

配置文件是存档根目录的 `mhgu_dps_meter.ini`。也可以在 Forge → Plugin UI → `MHGU DPS Meter` 中修改并点击 Save INI。

手动修改或删除 INI 后，需要完全退出游戏再启动；Forge 菜单中的修改会实时预览。

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

; 曲线面板宽度，720p 基准像素，允许 220～600；默认 270。
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

## 显示布局

默认使用同行统计：秒伤、总伤、百分比使用同一字号排在右侧，顺序为秒伤、总伤、百分比。

- 总伤使用固定的 5 位宽度，4 位变成 5 位时不会推动秒伤；
- 秒伤和百分比右对齐；
- 名称或命中次数过长、总伤超过 5 位时，面板、曲线和底部色条会自动向右扩展；
- 关闭总伤后，全队行中的 `DMG` 和全队总伤隐藏；关闭秒伤后，全队秒伤隐藏；
- 关闭 `Team summary` 或只有一名玩家时，底部全队行隐藏；
- 开启命中次数后会自动使用上下两排布局，命中次数使用与统计数字相同的字号。

## 武器图标

- 武器图标已内嵌在插件中，不需要额外安装图片；
- 图标按像素块绘制，并保留阴影和高光；
- 图标会使用玩家的 HUD 颜色进行着色；
- 把 `dps_show_weapon` 改为 `false` 会完全关闭图标，名字和命中次数占用图标位置。

## 玩家名称

- `dps_show_names = false` 时显示 `You / P1 / P2 / P3 / P4`；
- 改为 `true` 后显示游戏内玩家名称；
- 猎猫模式目前显示猎人名字，猫名暂未接入；

## 字体

默认使用随插件提供的 NotoSansSC 中文字体：

- `romfs/nativeNX/forge/fonts/NotoSansSC-Medium.otf`
- `romfs/nativeNX/forge/fonts/OFL.txt`

也可以使用其他 TTF/OTF 字体，把文件放入同一目录，并在 INI 的 `font_file` 中填写文件名。字体使用 OFL 1.1 许可证，可随插件分发。
