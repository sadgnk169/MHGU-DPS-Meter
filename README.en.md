# MHGU DPS Meter

[English](README.en.md) | [中文](README.md)

A Forge DPS overlay for **Monster Hunter Generations Ultimate (MHGU) 1.4.0**. It shows the whole-party cumulative damage chart, total damage, DPS, percentage, and a team summary for up to four players.

- Prerequisite: **Forge 1.5.1** — [https://github.com/Lawrence-al/forge](https://github.com/Lawrence-al/forge)
- NRO: `00_mhgu_dps_meter.nro`
- Forge plugin name: `MHGU DPS Meter`
- INI: `mhgu_dps_meter.ini`
- SHA256: `80fba68f4bb4e86f6f6a96e64901c5f77300c51a090d66cc9a317dd9072b52a0`
- Size: 163840 bytes

## Features

- Whole-quest cumulative damage chart for all four players;
- Per-player total damage, percentage, DPS, and cumulative damage;
- Inline stats by default, with an optional two-line layout;
- Weapon icons, optional in-game player names, and a team summary;
- Player colors follow the in-game HUD colors;
- Automatic layout scaling for handheld and docked modes.

## Installation

1. Install **Forge 1.5.1**: [https://github.com/Lawrence-al/forge](https://github.com/Lawrence-al/forge)
2. Copy the `romfs` directory into the MHGU mod directory:
   - `romfs/nativeNX/plugins/00_mhgu_dps_meter.nro`
   - `romfs/nativeNX/forge/fonts/NotoSansSC-Medium.otf`
   - `romfs/nativeNX/forge/fonts/OFL.txt`
3. Copy `conf/mhgu_dps_meter.ini` to the MHGU save root, or let the plugin create it on first launch.
4. Fully exit and restart the game.
5. Open the Forge menu and make sure `Enable DPS meter` is on.

## Usage

- The panel appears on the left, below the player/party names. Position is controlled by `dps_offset_x` and `dps_offset_y`.
- Default row labels are `You / P1 / P2 / P3 / P4`.
- Enable `dps_show_names` to show in-game player names; unavailable names fall back to slot labels.
- Rows are sorted by damage, highest first.
- To turn the overlay off, disable `Enable DPS meter` in the Forge menu or set `enabled = false` in the INI.
- To reset the current run, use `Reset DPS` in the Forge menu.
- Area changes keep the accumulated data. The data clears about two seconds after the save/result screen appears.
- Single-player quests are supported. The team summary appears only with two or more players.

## Configuration

The INI file is `mhgu_dps_meter.ini` in the MHGU save root. It can also be edited from Forge → Plugin UI → `MHGU DPS Meter`.

After manually editing or deleting the INI, fully restart the game. Changes made in the Forge menu preview immediately.

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `true` | Enable damage tracking and the overlay |
| `font_file` | `default` | Bundled Noto Sans SC, or a TTF/OTF filename in `romfs/nativeNX/forge/fonts/` |
| `dps_background_alpha` | `180.0` | Black background opacity, 0–255 |
| `dps_show_timer` | `true` | Show the elapsed timer |
| `dps_show_damage` | `true` | Show each player's total damage |
| `dps_show_percent` | `true` | Show each player's percentage |
| `dps_show_dps` | `true` | Show each player's DPS |
| `dps_show_hits` | `false` | Show hit count; forces the two-line layout |
| `dps_show_names` | `false` | Show in-game names instead of `You/P1...P4` |
| `dps_inline_stats` | `true` | DPS/total/percentage on one line |
| `dps_show_team` | `true` | Show the team summary row |
| `dps_offset_x` | `16.0` | Panel X position, 720p reference |
| `dps_offset_y` | `340.0` | Panel Y position, 720p reference |
| `dps_show_curve` | `true` | Show the cumulative damage chart |
| `dps_curve_width` | `270.0` | Panel width, 720p reference |
| `dps_curve_height` | `55.0` | Chart height, 720p reference |
| `dps_curve_fill` | `true` | Fill below the chart |
| `dps_show_percent_bar` | `true` | Show each player's percentage bar |
| `dps_show_weapon` | `true` | Show weapon icons; `false` hides them completely |

## Layout

Inline stats are the default: DPS, total damage, and percentage use the same font size in one right-aligned row.

- The total damage column reserves space for five digits, so changing from four to five digits does not move DPS.
- DPS and percentage columns are right-aligned.
- Long names/hit counts or totals longer than five digits expand the panel, chart, and bottom bar to the right.
- Disabling total damage hides `DMG` and team total; disabling DPS hides team DPS.
- The team row is hidden when `dps_show_team = false` or when there is only one player.
- Enabling hit counts forces the two-line layout; hit counts use the same font size as the other stats.

## Weapon icons

- Weapon icons are embedded in the plugin; no separate image folder is required.
- Icons are rendered as pixel blocks with their original shading and highlights.
- Icons are tinted with the player's HUD color.
- Set `dps_show_weapon = false` to hide icons completely; names and hit counts take the icon position.

## Player names

- `dps_show_names = false` shows `You / P1 / P2 / P3 / P4`.
- `true` shows in-game player names, with slot labels as fallback.
- In Prowler mode the hunter name is currently shown; Palico names are not supported yet.

## Font

The bundled font is Noto Sans SC Medium:

- `romfs/nativeNX/forge/fonts/NotoSansSC-Medium.otf`
- `romfs/nativeNX/forge/fonts/OFL.txt`

You can use another TTF/OTF font by placing it in the same directory and setting `font_file` to its filename. The bundled font is licensed under the SIL Open Font License 1.1 and may be redistributed with the plugin.
