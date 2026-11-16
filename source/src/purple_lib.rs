//! Purple-lib 可选设置页：Options > Mod Options。
//!
//! Purple-lib 不链接本插件，它在自己的第一次 `forge_onUpdate` 里按名字调用本模块导出的
//! `purplelib_on_register`。没装 Purple-lib 时这个函数不会被调用，插件行为不变。
//!
//! `get` / `set` 运行在游戏更新里，只读写原子变量；真正的配置同步、DPS 重置和保存由
//! `forge_onUpdate` 完成，避免在回调里加锁或访问文件。

use core::{
    ffi::{c_char, c_void},
    ptr,
    sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering},
};

use crate::data::Config;
use forge::log::prelude::{info, warn};

pub const API_VERSION: u32 = 4;

const ROW_LABELS: u32 = 0;
const ROW_NUMBERS: u32 = 1;
const ROW_ACTION: u32 = 3;
/// 数字行不在首尾回绕。
const ROW_NO_WRAP: u32 = 1;

/// 本页总行数（3 页：8 + 8 + 4）。
const ROW_COUNT: u32 = 20;
const RESET_DPS_ROW: u32 = 18;
const RESET_SETTINGS_ROW: u32 = 19;

#[repr(C)]
pub struct OptionRow {
    pub name: *const c_char,
    pub help: *const c_char,
    pub labels: *const *const c_char,
    pub label_count: u32,
    pub kind: u32,
    pub value_count: u32,
    pub first: i32,
    pub step: i32,
    pub suffix: *const c_char,
    pub flags: u32,
}

#[repr(C)]
pub struct Options {
    pub size: u32,
    pub context: *mut c_void,
    pub title: Option<extern "C" fn(*mut c_void, u32) -> *const c_char>,
    pub row_count: u32,
    pub row: Option<extern "C" fn(*mut c_void, u32, u32, *mut OptionRow) -> bool>,
    pub get: Option<extern "C" fn(*mut c_void, u32) -> u32>,
    pub set: Option<extern "C" fn(*mut c_void, u32, u32)>,
    pub closed: Option<extern "C" fn(*mut c_void)>,
    pub value_text: Option<extern "C" fn(*mut c_void, u32, u32, u32) -> *const c_char>,
    pub action: Option<extern "C" fn(*mut c_void, u32)>,
}

#[repr(C)]
#[allow(dead_code)]
pub struct Api {
    pub version: u32,
    pub add_options: extern "C" fn(*const Options) -> bool,
    pub add_help: extern "C" fn(*const c_void) -> bool,
    pub hold_input: extern "C" fn(),
    pub options_shown: extern "C" fn(*const Options) -> bool,
    pub options_row: extern "C" fn(*const Options) -> i32,
    pub add_set_help: extern "C" fn(*const Options, *const c_void) -> bool,
    pub help_shown: extern "C" fn(*const Options) -> bool,
}

// 与 Purple-lib 头文件一致；32 位下分别是 40 / 40 / 32 字节。
const _: () = assert!(core::mem::size_of::<Options>() == 40);
const _: () = assert!(core::mem::size_of::<OptionRow>() == 40);
const _: () = assert!(core::mem::size_of::<Api>() == 32);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lang {
    En,
    Hans,
    Hant,
}

fn lang(value: u32) -> Lang {
    match value {
        8 => Lang::Hans,
        7 => Lang::Hant,
        _ => Lang::En,
    }
}

struct Labels([*const c_char; 2]);
// SAFETY: 全部指向静态字符串，永不写入。
unsafe impl Sync for Labels {}

static OFF_ON: Labels = Labels([c"Off".as_ptr(), c"On".as_ptr()]);
static OFF_ON_HANS: Labels = Labels([c"关闭".as_ptr(), c"开启".as_ptr()]);
static OFF_ON_HANT: Labels = Labels([c"關閉".as_ptr(), c"開啟".as_ptr()]);

struct ActionLabel([*const c_char; 1]);
// SAFETY: 同上。
unsafe impl Sync for ActionLabel {}

static RESET_DPS_EN: ActionLabel = ActionLabel([c"Reset".as_ptr()]);
static RESET_DPS_HANS: ActionLabel = ActionLabel([c"重置 DPS".as_ptr()]);
static RESET_DPS_HANT: ActionLabel = ActionLabel([c"重設 DPS".as_ptr()]);
static RESET_ALL_EN: ActionLabel = ActionLabel([c"Reset".as_ptr()]);
static RESET_ALL_HANS: ActionLabel = ActionLabel([c"恢复默认".as_ptr()]);
static RESET_ALL_HANT: ActionLabel = ActionLabel([c"恢復預設".as_ptr()]);

static NAMES_EN: [&core::ffi::CStr; ROW_COUNT as usize] = [
    c"DPS meter",
    c"Show timer",
    c"Show damage",
    c"Show percent",
    c"Show DPS",
    c"Show hits",
    c"Team summary",
    c"Inline stats",
    c"Overlay size",
    c"Black background",
    c"Background",
    c"Show chart",
    c"Curve fill",
    c"Percent bar",
    c"Weapon icons",
    c"Player names",
    c"Panel X",
    c"Panel Y",
    c"Reset DPS",
    c"Reset settings",
];
static NAMES_HANS: [&core::ffi::CStr; ROW_COUNT as usize] = [
    c"DPS 计量",
    c"显示计时",
    c"显示伤害",
    c"显示百分比",
    c"显示 DPS",
    c"显示命中数",
    c"队伍合计",
    c"内联数据",
    c"面板大小",
    c"黑色背景",
    c"背景透明度",
    c"显示图表",
    c"曲线填充",
    c"百分比条",
    c"武器图标",
    c"玩家名字",
    c"面板 X",
    c"面板 Y",
    c"重置 DPS",
    c"恢复默认设置",
];
static NAMES_HANT: [&core::ffi::CStr; ROW_COUNT as usize] = [
    c"DPS 計量",
    c"顯示計時",
    c"顯示傷害",
    c"顯示百分比",
    c"顯示 DPS",
    c"顯示命中數",
    c"隊伍合計",
    c"內聯數據",
    c"面板大小",
    c"黑色背景",
    c"背景透明度",
    c"顯示圖表",
    c"曲線填充",
    c"百分比條",
    c"武器圖示",
    c"玩家名稱",
    c"面板 X",
    c"面板 Y",
    c"重設 DPS",
    c"恢復預設設定",
];

static HELP_EN: [Option<&core::ffi::CStr>; ROW_COUNT as usize] = [
    Some(c"Turn the damage meter on or off."),
    Some(c"Show the quest time in the panel."),
    Some(c"Show each hunter's total damage."),
    Some(c"Show each hunter's share of the damage."),
    Some(c"Show damage per second."),
    Some(c"Show how many hits each hunter landed."),
    Some(c"Show the team's combined damage."),
    Some(c"Show each row's stats on one line."),
    Some(c"Panel size, 50% to 300%, in steps of 5%."),
    Some(c"Show or hide the panel's black background."),
    Some(c"Panel background opacity, 0 to 255."),
    Some(c"Draw the damage-over-time chart."),
    Some(c"Fill the area under the chart."),
    Some(c"Show the percent bar under the panel."),
    Some(c"Show the weapon icon of each hunter."),
    Some(c"Show hunter names instead of colours only."),
    Some(c"Moves the panel right in steps of 5 pixels."),
    Some(c"Moves the panel down in steps of 5 pixels."),
    Some(c"Clear the current DPS totals and chart."),
    Some(c"Reset every setting to its default and save."),
];
static HELP_HANS: [Option<&core::ffi::CStr>; ROW_COUNT as usize] = [
    Some(c"开启或关闭伤害计量。"),
    Some(c"在面板中显示任务计时。"),
    Some(c"显示每位猎人的总伤害。"),
    Some(c"显示每位猎人的伤害占比。"),
    Some(c"显示每秒伤害。"),
    Some(c"显示每位猎人的命中次数。"),
    Some(c"显示队伍的总伤害。"),
    Some(c"把每行的统计数据排在同一行。"),
    Some(c"面板大小，50% 到 300%，每档 5%。"),
    Some(c"显示或隐藏面板的黑色背景。"),
    Some(c"面板背景不透明度，0 到 255。"),
    Some(c"绘制伤害随时间变化的图表。"),
    Some(c"填充图表下方的区域。"),
    Some(c"在面板下方显示百分比条。"),
    Some(c"显示每位猎人的武器图标。"),
    Some(c"显示猎人名字，而不只是颜色。"),
    Some(c"以 5 像素为一步右移面板。"),
    Some(c"以 5 像素为一步下移面板。"),
    Some(c"清空当前 DPS 累计和图表。"),
    Some(c"恢复所有设置为默认值并保存。"),
];
static HELP_HANT: [Option<&core::ffi::CStr>; ROW_COUNT as usize] = [
    Some(c"開啟或關閉傷害計量。"),
    Some(c"在面板中顯示任務計時。"),
    Some(c"顯示每位獵人的總傷害。"),
    Some(c"顯示每位獵人的傷害佔比。"),
    Some(c"顯示每秒傷害。"),
    Some(c"顯示每位獵人的命中次數。"),
    Some(c"顯示隊伍的總傷害。"),
    Some(c"把每行的統計數據排在同一行。"),
    Some(c"面板大小，50% 到 300%，每檔 5%。"),
    Some(c"顯示或隱藏面板的黑色背景。"),
    Some(c"面板背景不透明度，0 到 255。"),
    Some(c"繪製傷害隨時間變化的圖表。"),
    Some(c"填充圖表下方的區域。"),
    Some(c"在面板下方顯示百分比條。"),
    Some(c"顯示每位獵人的武器圖示。"),
    Some(c"顯示獵人名稱，而不只是顏色。"),
    Some(c"以 5 像素為一步右移面板。"),
    Some(c"以 5 像素為一步下移面板。"),
    Some(c"清空目前 DPS 累計和圖表。"),
    Some(c"恢復所有設定為預設值並保存。"),
];

static ENABLED: AtomicU32 = AtomicU32::new(1);
static TIMER: AtomicU32 = AtomicU32::new(1);
static DAMAGE: AtomicU32 = AtomicU32::new(1);
static PERCENT: AtomicU32 = AtomicU32::new(1);
static SHOW_DPS: AtomicU32 = AtomicU32::new(1);
static HITS: AtomicU32 = AtomicU32::new(1);
static TEAM: AtomicU32 = AtomicU32::new(1);
static INLINE: AtomicU32 = AtomicU32::new(1);
static SCALE: AtomicU32 = AtomicU32::new(10);
static BG_ENABLED: AtomicU32 = AtomicU32::new(1);
static BACKGROUND: AtomicU32 = AtomicU32::new(19);
static CHART: AtomicU32 = AtomicU32::new(1);
static CURVE_FILL: AtomicU32 = AtomicU32::new(1);
static PERCENT_BAR: AtomicU32 = AtomicU32::new(1);
static WEAPON: AtomicU32 = AtomicU32::new(1);
static NAMES: AtomicU32 = AtomicU32::new(0);
static PANEL_X: AtomicU32 = AtomicU32::new(0);
static PANEL_Y: AtomicU32 = AtomicU32::new(68);
static DUMMY: AtomicU32 = AtomicU32::new(0);

static SETTINGS: [&AtomicU32; ROW_COUNT as usize] = [
    &ENABLED,
    &TIMER,
    &DAMAGE,
    &PERCENT,
    &SHOW_DPS,
    &HITS,
    &TEAM,
    &INLINE,
    &SCALE,
    &BG_ENABLED,
    &BACKGROUND,
    &CHART,
    &CURVE_FILL,
    &PERCENT_BAR,
    &WEAPON,
    &NAMES,
    &PANEL_X,
    &PANEL_Y,
    &DUMMY,
    &DUMMY,
];

static CHANGED: AtomicU32 = AtomicU32::new(0);
static SAVE: AtomicBool = AtomicBool::new(false);
static RESET: AtomicBool = AtomicBool::new(false);
static RESET_SETTINGS: AtomicBool = AtomicBool::new(false);
static API: AtomicPtr<Api> = AtomicPtr::new(ptr::null_mut());

extern "C" fn title(_: *mut c_void, _: u32) -> *const c_char {
    c"MHGU DPS Meter".as_ptr()
}

fn number_row(out: &mut OptionRow, name: *const c_char, count: u32, first: i32, step: i32) {
    out.name = name;
    out.kind = ROW_NUMBERS;
    out.value_count = count;
    out.first = first;
    out.step = step;
    out.flags = ROW_NO_WRAP;
}

extern "C" fn row(_: *mut c_void, language: u32, index: u32, out: *mut OptionRow) -> bool {
    if out.is_null() || index >= ROW_COUNT {
        return false;
    }
    // SAFETY: Purple-lib 传入可写且已清零的 OptionRow。
    let out = unsafe { &mut *out };
    let lang = lang(language);
    let names = match lang {
        Lang::Hans => &NAMES_HANS,
        Lang::Hant => &NAMES_HANT,
        Lang::En => &NAMES_EN,
    };
    let helps = match lang {
        Lang::Hans => &HELP_HANS,
        Lang::Hant => &HELP_HANT,
        Lang::En => &HELP_EN,
    };
    let name = names[index as usize];
    out.name = name.as_ptr();
    if let Some(help) = helps[index as usize] {
        out.help = help.as_ptr();
    }
    match index {
        0..=7 | 9 | 11..=15 => {
            let labels = match lang {
                Lang::Hans => &OFF_ON_HANS,
                Lang::Hant => &OFF_ON_HANT,
                Lang::En => &OFF_ON,
            };
            out.kind = ROW_LABELS;
            out.labels = labels.0.as_ptr();
            out.label_count = 2;
        }
        // 50%..=300%，每档 5%。
        8 => number_row(out, name.as_ptr(), 51, 50, 5),
        // 0..=255，每档 5。
        10 => number_row(out, name.as_ptr(), 52, 0, 5),
        // 0..=1280，每档 5px。
        16 => number_row(out, name.as_ptr(), 257, 0, 5),
        // 0..=720，每档 5px。
        17 => number_row(out, name.as_ptr(), 145, 0, 5),
        18 => {
            let label = match lang {
                Lang::Hans => &RESET_DPS_HANS,
                Lang::Hant => &RESET_DPS_HANT,
                Lang::En => &RESET_DPS_EN,
            };
            out.kind = ROW_ACTION;
            out.labels = label.0.as_ptr();
            out.label_count = 1;
        }
        19 => {
            let label = match lang {
                Lang::Hans => &RESET_ALL_HANS,
                Lang::Hant => &RESET_ALL_HANT,
                Lang::En => &RESET_ALL_EN,
            };
            out.kind = ROW_ACTION;
            out.labels = label.0.as_ptr();
            out.label_count = 1;
        }
        _ => return false,
    }
    true
}

extern "C" fn get(_: *mut c_void, index: u32) -> u32 {
    SETTINGS
        .get(index as usize)
        .map_or(0, |setting| setting.load(Ordering::Relaxed))
}

extern "C" fn set(_: *mut c_void, index: u32, value: u32) {
    if let Some(setting) = SETTINGS.get(index as usize) {
        setting.store(value, Ordering::Relaxed);
        CHANGED.fetch_or(1 << index, Ordering::Release);
    }
}

extern "C" fn action(_: *mut c_void, index: u32) {
    if index == RESET_DPS_ROW {
        RESET.store(true, Ordering::Release);
    } else if index == RESET_SETTINGS_ROW {
        RESET_SETTINGS.store(true, Ordering::Release);
    }
}

extern "C" fn closed(_: *mut c_void) {
    SAVE.store(true, Ordering::Release);
}

/// 只读记录；context 为空，不会被解引用。
pub struct Record<T>(pub T);
// SAFETY: 记录只包含静态数据和函数指针。
unsafe impl<T> Sync for Record<T> {}

pub static OPTIONS: Record<Options> = Record(Options {
    size: core::mem::size_of::<Options>() as u32,
    context: ptr::null_mut(),
    title: Some(title),
    row_count: ROW_COUNT,
    row: Some(row),
    get: Some(get),
    set: Some(set),
    closed: Some(closed),
    value_text: None,
    action: Some(action),
});

fn index_of(value: f32, step: f32, max: u32) -> u32 {
    let scaled = value / step;
    if !scaled.is_finite() {
        return 0;
    }
    // no_std 没有 f32::round，手动四舍五入。
    let index = if scaled >= 0.0 {
        (scaled + 0.5) as i64
    } else {
        (scaled - 0.5) as i64
    };
    index.clamp(0, max as i64) as u32
}

fn store(config: &Config) {
    let flags = |value: bool| if value { 1 } else { 0 };
    ENABLED.store(flags(config.enabled), Ordering::Relaxed);
    TIMER.store(flags(config.dps_show_timer), Ordering::Relaxed);
    DAMAGE.store(flags(config.dps_show_damage), Ordering::Relaxed);
    PERCENT.store(flags(config.dps_show_percent), Ordering::Relaxed);
    SHOW_DPS.store(flags(config.dps_show_dps), Ordering::Relaxed);
    HITS.store(flags(config.dps_show_hits), Ordering::Relaxed);
    TEAM.store(flags(config.dps_show_team), Ordering::Relaxed);
    INLINE.store(flags(config.dps_inline_stats), Ordering::Relaxed);
    // 50% 对应档位 0，300% 对应档位 50。
    SCALE.store(
        index_of(config.dps_scale * 100.0 - 50.0, 5.0, 50),
        Ordering::Relaxed,
    );
    BG_ENABLED.store(flags(config.dps_show_background), Ordering::Relaxed);
    BACKGROUND.store(
        index_of(config.dps_background_alpha, 5.0, 51),
        Ordering::Relaxed,
    );
    CHART.store(flags(config.dps_show_curve), Ordering::Relaxed);
    CURVE_FILL.store(flags(config.dps_curve_fill), Ordering::Relaxed);
    PERCENT_BAR.store(flags(config.dps_show_percent_bar), Ordering::Relaxed);
    WEAPON.store(flags(config.dps_show_weapon), Ordering::Relaxed);
    NAMES.store(flags(config.dps_show_names), Ordering::Relaxed);
    PANEL_X.store(index_of(config.dps_offset_x, 5.0, 256), Ordering::Relaxed);
    PANEL_Y.store(index_of(config.dps_offset_y, 5.0, 144), Ordering::Relaxed);
}

/// 初始化时把当前配置写进原子变量。
pub fn init(config: &Config) {
    store(config);
}

/// ImGui 设置页改动后保持原子变量与界面一致；有未应用的 Purple-lib 改动时跳过。
pub fn sync(config: &Config) {
    if CHANGED.load(Ordering::Acquire) == 0 {
        store(config);
    }
}

/// 把 Purple-lib 页面上改动过的行写回配置；未改动的行保留原值，避免取整影响。
pub fn apply(config: &mut Config, changed: u32) {
    let value = |setting: &AtomicU32| setting.load(Ordering::Relaxed);
    if changed & (1 << 0) != 0 {
        config.enabled = value(&ENABLED) != 0;
    }
    if changed & (1 << 1) != 0 {
        config.dps_show_timer = value(&TIMER) != 0;
    }
    if changed & (1 << 2) != 0 {
        config.dps_show_damage = value(&DAMAGE) != 0;
    }
    if changed & (1 << 3) != 0 {
        config.dps_show_percent = value(&PERCENT) != 0;
    }
    if changed & (1 << 4) != 0 {
        config.dps_show_dps = value(&SHOW_DPS) != 0;
    }
    if changed & (1 << 5) != 0 {
        config.dps_show_hits = value(&HITS) != 0;
    }
    if changed & (1 << 6) != 0 {
        config.dps_show_team = value(&TEAM) != 0;
    }
    if changed & (1 << 7) != 0 {
        config.dps_inline_stats = value(&INLINE) != 0;
    }
    if changed & (1 << 8) != 0 {
        config.dps_scale = (50 + value(&SCALE) * 5) as f32 / 100.0;
    }
    if changed & (1 << 9) != 0 {
        config.dps_show_background = value(&BG_ENABLED) != 0;
    }
    if changed & (1 << 10) != 0 {
        config.dps_background_alpha = value(&BACKGROUND) as f32 * 5.0;
    }
    if changed & (1 << 11) != 0 {
        config.dps_show_curve = value(&CHART) != 0;
    }
    if changed & (1 << 12) != 0 {
        config.dps_curve_fill = value(&CURVE_FILL) != 0;
    }
    if changed & (1 << 13) != 0 {
        config.dps_show_percent_bar = value(&PERCENT_BAR) != 0;
    }
    if changed & (1 << 14) != 0 {
        config.dps_show_weapon = value(&WEAPON) != 0;
    }
    if changed & (1 << 15) != 0 {
        config.dps_show_names = value(&NAMES) != 0;
    }
    if changed & (1 << 16) != 0 {
        config.dps_offset_x = value(&PANEL_X) as f32 * 5.0;
    }
    if changed & (1 << 17) != 0 {
        config.dps_offset_y = value(&PANEL_Y) as f32 * 5.0;
    }
}

/// 返回待应用行的位图并清除标记；0 表示没有改动。
pub fn take_dirty() -> u32 {
    CHANGED.swap(0, Ordering::AcqRel)
}

/// 页面关闭且改动过，返回 true 并清除标记。
pub fn take_save() -> bool {
    SAVE.swap(false, Ordering::AcqRel)
}

/// 动作行请求重置 DPS，返回 true 并清除标记。
pub fn take_reset() -> bool {
    RESET.swap(false, Ordering::AcqRel)
}

/// 动作行请求恢复默认设置，返回 true 并清除标记。
pub fn take_reset_settings() -> bool {
    RESET_SETTINGS.swap(false, Ordering::AcqRel)
}

/// 当前是否正显示本插件的 Mod Options 页。
pub fn page_shown() -> bool {
    // SAFETY: 空指针，或 Purple-lib 提供的、整局有效的函数表。
    unsafe { API.load(Ordering::Acquire).as_ref() }
        .is_some_and(|api| (api.options_shown)(&OPTIONS.0))
}

/// Purple-lib 在第一次 `forge_onUpdate` 里按名字调用；这里只注册设置页。
///
/// # Safety
/// `api` 为空或 Purple-lib 的函数表，整局有效。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn purplelib_on_register(api: *const Api) {
    // SAFETY: 调用方契约；表的第一个字段在所有版本里都是 version。
    let Some(table) = (unsafe { api.as_ref() }) else {
        return;
    };
    if table.version < API_VERSION {
        return;
    }
    API.store(api.cast_mut(), Ordering::Release);
    let added = (table.add_options)(&OPTIONS.0);
    if added {
        info!(
            "mhgu-dps-meter: purple-lib page registered (API v{})",
            table.version
        );
    } else {
        warn!("mhgu-dps-meter: refused the purple-lib page");
    }
}
