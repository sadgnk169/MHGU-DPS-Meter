//! 伤害数字的数据结构、固定队列、共享状态和数据操作。
#![cfg_attr(test, allow(dead_code))]

use core::{
    cell::UnsafeCell,
    ffi::CStr,
    mem::MaybeUninit,
    ptr,
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
};

// 事件不持有游戏指针；只保留命中点、数字、颜色和用于清理的玩家/区域编号。
pub const CAPACITY: usize = 64;

pub const CONFIG_MAX_BYTES: usize = 4096;

// 外观配置和解析留在数据文件，不增加第三个 Rust 文件或 INI 依赖。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    pub enabled: bool,
    pub font_size: f32,
    pub font_file: Option<FontFile>,
    pub highlight_color: [u8; 4],
    pub normal_color: [u8; 4],
    pub outline_color: [u8; 4],
    pub outline_width: f32,
    pub full_opacity: f32,
    pub lifetime: f32,
    pub dps_background_alpha: f32,
    pub dps_show_timer: bool,
    pub dps_show_damage: bool,
    pub dps_show_percent: bool,
    pub dps_show_dps: bool,
    pub dps_show_hits: bool,
    pub dps_offset_x: f32,
    pub dps_offset_y: f32,
    pub dps_show_curve: bool,
    pub dps_curve_width: f32,
    pub dps_curve_height: f32,
    pub dps_curve_fill: bool,
    pub dps_show_percent_bar: bool,
    pub dps_show_weapon: bool,
    pub dps_show_names: bool,
    pub dps_inline_stats: bool,
    pub dps_show_team: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontFile([u8; 192]);

pub const DEFAULT_FONT: FontFile = FontFile::from_static(b"NotoSansSC-Medium.otf");

impl FontFile {
    const fn from_static(filename: &[u8]) -> Self {
        let mut bytes = [0u8; 192];
        let prefix = b"app:/nativeNX/forge/fonts/";
        let mut i = 0usize;
        while i < prefix.len() {
            bytes[i] = prefix[i];
            i += 1;
        }
        let mut j = 0usize;
        while j < filename.len() {
            bytes[i + j] = filename[j];
            j += 1;
        }
        Self(bytes)
    }

    pub fn new(filename: &str) -> Option<Self> {
        let prefix = b"app:/nativeNX/forge/fonts/";
        let bytes = filename.as_bytes();
        if !(bytes.ends_with(b".ttf") || bytes.ends_with(b".otf"))
            || filename
                .bytes()
                .any(|c| c.is_ascii_control() || b"/\\:;#".contains(&c))
            || prefix.len() + filename.len() >= 192
        {
            return None;
        }
        let mut bytes = [0; 192];
        bytes[..prefix.len()].copy_from_slice(prefix);
        bytes[prefix.len()..prefix.len() + filename.len()].copy_from_slice(filename.as_bytes());
        Some(Self(bytes))
    }

    pub fn path(&self) -> &CStr {
        // 构造时排除了内嵌 NUL，并保留至少一个末尾 NUL。
        CStr::from_bytes_until_nul(&self.0).unwrap()
    }

    pub fn filename(&self) -> &str {
        self.path()
            .to_str()
            .unwrap()
            .strip_prefix("app:/nativeNX/forge/fonts/")
            .unwrap()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ConfigError {
    pub line: usize,
    pub message: &'static str,
}

impl Config {
    pub fn write_ini(&self, out: &mut impl core::fmt::Write) -> core::fmt::Result {
        writeln!(
            out,
            "; MHGU save: /mhgu_dps_meter.ini\n[display]\nenabled = {}\nfont_file = {}",
            self.enabled,
            self.font_file
                .as_ref()
                .map_or("default", FontFile::filename)
        )?;
        writeln!(
            out,
            "[dps]\ndps_background_alpha = {}\ndps_show_timer = {}\ndps_show_damage = {}\ndps_show_percent = {}\ndps_show_dps = {}\ndps_show_hits = {}\ndps_offset_x = {}\ndps_offset_y = {}\ndps_show_curve = {}\ndps_curve_width = {}\ndps_curve_height = {}\ndps_curve_fill = {}\ndps_show_percent_bar = {}\ndps_show_weapon = {}\ndps_show_names = {}\ndps_inline_stats = {}\ndps_show_team = {}",
            self.dps_background_alpha,
            self.dps_show_timer,
            self.dps_show_damage,
            self.dps_show_percent,
            self.dps_show_dps,
            self.dps_show_hits,
            self.dps_offset_x,
            self.dps_offset_y,
            self.dps_show_curve,
            self.dps_curve_width,
            self.dps_curve_height,
            self.dps_curve_fill,
            self.dps_show_percent_bar,
            self.dps_show_weapon,
            self.dps_show_names,
            self.dps_inline_stats,
            self.dps_show_team
        )
    }

    pub const DEFAULT: Self = Self {
        enabled: true,
        font_size: 34.0,
        font_file: Some(DEFAULT_FONT),
        highlight_color: [255, 220, 80, 255],
        normal_color: [255, 255, 255, 255],
        outline_color: [10, 10, 10, 255],
        outline_width: 2.0,
        full_opacity: 1.10,
        lifetime: 1.40,
        dps_background_alpha: 180.0,
        dps_show_timer: true,
        dps_show_damage: true,
        dps_show_percent: true,
        dps_show_dps: true,
        dps_show_hits: false,
        dps_offset_x: 16.0,
        dps_offset_y: 340.0,
        dps_show_curve: true,
        dps_curve_width: 270.0,
        dps_curve_height: 55.0,
        dps_curve_fill: true,
        dps_show_percent_bar: true,
        dps_show_weapon: true,
        dps_show_names: false,
        dps_inline_stats: true,
        dps_show_team: true,
    };

    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        let mut config = Self::DEFAULT;
        let mut section = 0u8;
        let mut timing_line = 0;
        for (index, line) in text.trim_start_matches('\u{FEFF}').lines().enumerate() {
            let line = line.split(';').next().unwrap().trim();
            let error = |message| ConfigError {
                line: index + 1,
                message,
            };
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line == "[display]" {
                section = 1;
                continue;
            }
            if line == "[dps]" {
                section = 2;
                continue;
            }
            if section == 0 {
                return Err(error("settings must follow [display] or [dps]"));
            }
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| error("expected key = value"))?;
            let value = if value.trim_start().starts_with('#') {
                value.split_ascii_whitespace().next().unwrap()
            } else {
                value.split('#').next().unwrap().trim()
            };
            let number = |min: f32, max: f32, message| {
                value
                    .parse::<f32>()
                    .ok()
                    .filter(|v| v.is_finite() && (min..=max).contains(v))
                    .ok_or_else(|| error(message))
            };
            let color =
                || parse_color(value).ok_or_else(|| error("color must be #RRGGBB or #RRGGBBAA"));
            let boolean = || {
                value
                    .parse::<bool>()
                    .map_err(|_| error("must be true or false"))
            };
            match key.trim() {
                "enabled" => {
                    config.enabled = value
                        .parse()
                        .map_err(|_| error("enabled must be true or false"))?
                }
                "font_size" => {
                    config.font_size = number(8.0, 128.0, "font_size must be in 8..128")?
                }
                "font_file" => {
                    config.font_file = if value == "default" {
                        Some(DEFAULT_FONT)
                    } else {
                        Some(FontFile::new(value).ok_or_else(|| {
                            error("font_file must be a .ttf/.otf filename, without a directory")
                        })?)
                    }
                }
                "highlight_color" => config.highlight_color = color()?,
                "normal_color" => config.normal_color = color()?,
                "outline_color" => config.outline_color = color()?,
                "outline_width" => {
                    config.outline_width = number(0.0, 10.0, "outline_width must be in 0..10")?
                }
                "full_opacity" => {
                    config.full_opacity = number(0.0, 60.0, "full_opacity must be in 0..60")?;
                    timing_line = index + 1;
                }
                "lifetime" => {
                    config.lifetime = number(0.0, 60.0, "lifetime must be in 0..60")?;
                    timing_line = index + 1;
                }
                // Kept as an accepted legacy key; font size is fixed now.
                "dps_font_size" => {}
                "dps_background_alpha" => {
                    config.dps_background_alpha =
                        number(0.0, 255.0, "dps_background_alpha must be in 0..255")?
                }
                "dps_show_timer" => config.dps_show_timer = boolean()?,
                "dps_show_damage" => config.dps_show_damage = boolean()?,
                "dps_show_percent" => config.dps_show_percent = boolean()?,
                "dps_show_dps" => config.dps_show_dps = boolean()?,
                "dps_show_hits" => config.dps_show_hits = boolean()?,
                "dps_offset_x" => {
                    let value = number(0.0, 1280.0, "dps_offset_x must be in 0..1280")?;
                    // 键名保留 offset，但数值是面板左上角坐标；旧配置的 0 仍落到默认位置。
                    config.dps_offset_x = if value == 0.0 { 16.0 } else { value };
                }
                "dps_offset_y" => {
                    let value = number(0.0, 720.0, "dps_offset_y must be in 0..720")?;
                    config.dps_offset_y = if value == 0.0 { 340.0 } else { value };
                }
                "dps_curve" => {} // 旧键：固定曲线布局，忽略
                "dps_show_curve" => config.dps_show_curve = boolean()?,
                "dps_curve_width" => {
                    config.dps_curve_width =
                        number(220.0, 600.0, "dps_curve_width must be in 220..600")?
                }
                "dps_curve_height" => {
                    config.dps_curve_height =
                        number(55.0, 200.0, "dps_curve_height must be in 55..200")?
                }
                "dps_curve_fill" => config.dps_curve_fill = boolean()?,
                "dps_show_percent_bar" => config.dps_show_percent_bar = boolean()?,
                "dps_show_weapon" => config.dps_show_weapon = boolean()?,
                "dps_show_names" => config.dps_show_names = boolean()?,
                "dps_inline_stats" => config.dps_inline_stats = boolean()?,
                "dps_show_team" => config.dps_show_team = boolean()?,
                _ => return Err(error("unknown setting")),
            }
        }
        if config.full_opacity <= 0.0 || config.lifetime <= config.full_opacity {
            return Err(ConfigError {
                line: timing_line,
                message: "keep 0 < full_opacity < lifetime <= 60",
            });
        }
        Ok(config)
    }
}

fn parse_color(value: &str) -> Option<[u8; 4]> {
    if let Some(hex) = value.strip_prefix('#') {
        if !matches!(hex.len(), 6 | 8) || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let mut rgba = [0, 0, 0, 255];
        for (i, out) in rgba.iter_mut().take(hex.len() / 2).enumerate() {
            *out = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
        }
        return Some(rgba);
    }
    let mut parts = value.split(',');
    let rgba = [
        parts.next()?.trim().parse().ok()?,
        parts.next()?.trim().parse().ok()?,
        parts.next()?.trim().parse().ok()?,
        parts.next().map_or(Some(255), |v| v.trim().parse().ok())?,
    ];
    parts.next().is_none().then_some(rgba)
}

// 对应原结算的物理肉质分支；weights 已按本次攻击选好斩/打权重。
pub fn yellow_for_hitzone(flags: u32, hitzones: [u8; 3], weights: [u8; 2]) -> bool {
    let hitzone = if flags & 0x18020 != 0 {
        if hitzones[0] as u16 * weights[0] as u16 >= hitzones[1] as u16 * weights[1] as u16 {
            hitzones[0]
        } else {
            hitzones[1]
        }
    } else if flags & 1 != 0 {
        hitzones[2]
    } else {
        // 原函数的物理倍率为 1，不乘斩/打/弹肉质；榴弹瓶等直接黄字。
        return true;
    };
    hitzone >= 45
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Epoch {
    pub player: u32,
    pub room: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct Digits {
    bytes: [u8; 10],
    start: u8,
}

impl Digits {
    pub fn new(mut value: u32) -> Self {
        let mut digits = Self {
            bytes: [0; 10],
            start: 10,
        };
        loop {
            digits.start -= 1;
            digits.bytes[digits.start as usize] = b'0' + (value % 10) as u8;
            value /= 10;
            if value == 0 {
                break;
            }
        }
        digits
    }

    pub fn as_str(&self) -> &str {
        // 构造过程只会写 ASCII 数字。
        unsafe { core::str::from_utf8_unchecked(&self.bytes[self.start as usize..]) }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Event {
    pub serial: u32,
    pub world: [f32; 3],
    pub screen: [f32; 2],
    pub age: f32,
    pub active: bool,
    pub visible: bool,
    pub yellow: bool,
    pub digits: Digits,
}

impl Event {
    const EMPTY: Self = Self {
        serial: 0,
        world: [0.0; 3],
        screen: [0.0; 2],
        age: 0.0,
        active: false,
        visible: false,
        yellow: false,
        digits: Digits {
            bytes: [0; 10],
            start: 10,
        },
    };

    pub fn alpha(&self, config: &Config) -> u8 {
        if self.age <= config.full_opacity {
            255
        } else {
            (255.0
                * ((config.lifetime - self.age) / (config.lifetime - config.full_opacity))
                    .clamp(0.0, 1.0)) as u8
        }
    }
}

#[derive(Clone, Copy)]
pub struct DisplayArea {
    pub width: f32,
    pub height: f32,
}

impl DisplayArea {
    // 游戏 NVN Window 的可见裁剪范围从 (0, 0) 开始，未投影时没有可见数字。
    const INITIAL: Self = Self {
        width: 1280.0,
        height: 720.0,
    };

    pub fn position(self, screen: [f32; 2]) -> [f32; 2] {
        [screen[0] * self.width, screen[1] * self.height]
    }

    pub fn scale(self) -> f32 {
        self.height / 720.0
    }
}

#[derive(Clone, Copy)]
pub struct Snapshot {
    pub config: Config,
    pub epoch: Epoch,
    pub display: DisplayArea,
    pub events: [Event; CAPACITY],
}

impl Snapshot {
    pub fn any_active(&self) -> bool {
        self.events.iter().any(|event| event.active)
    }
}

pub struct State {
    pub config: Config,
    epoch: Epoch,
    display: DisplayArea,
    events: [Event; CAPACITY],
    next: usize,
    serial: u32,
}

impl State {
    pub const fn new() -> Self {
        Self {
            config: Config::DEFAULT,
            epoch: Epoch { player: 0, room: 0 },
            display: DisplayArea::INITIAL,
            events: [Event::EMPTY; CAPACITY],
            next: 0,
            serial: 0,
        }
    }

    pub fn clear(&mut self) {
        self.events.fill(Event::EMPTY);
        self.next = 0;
    }

    pub fn clear_epoch(&mut self, epoch: Epoch) {
        if self.epoch == epoch {
            self.clear();
        }
    }

    // Debug logging: forget the live epoch so the next quest starts clean and
    // the auto-flush trigger cannot fire on every update after a transition.
    pub fn reset_epoch(&mut self) {
        self.epoch = Epoch { player: 0, room: 0 };
        self.clear();
    }

    pub fn push(&mut self, epoch: Epoch, world: [f32; 3], damage: i32, yellow: bool) -> bool {
        if !world.iter().all(|value| value.is_finite()) || !(1..=32767).contains(&damage) {
            return false;
        }
        if self.epoch != epoch {
            self.clear();
            self.epoch = epoch;
        }
        self.serial = self.serial.wrapping_add(1);
        self.events[self.next] = Event {
            serial: self.serial,
            world,
            screen: [0.0; 2],
            age: 0.0,
            active: true,
            visible: false,
            yellow,
            digits: Digits::new(damage as u32),
        };
        self.next = (self.next + 1) % CAPACITY;
        true
    }

    pub fn set_display(&mut self, display: DisplayArea) {
        self.display = display;
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            config: self.config,
            epoch: self.epoch,
            display: self.display,
            events: self.events,
        }
    }

    pub fn advance(&mut self, seconds: f32) -> Snapshot {
        for event in &mut self.events {
            if event.active {
                event.age += seconds;
                if event.age >= self.config.lifetime {
                    event.active = false;
                }
            }
        }
        self.snapshot()
    }

    pub fn commit_projection(&mut self, projected: &Snapshot) {
        if self.epoch != projected.epoch {
            return;
        }
        self.display = projected.display;
        for (current, sample) in self.events.iter_mut().zip(projected.events.iter()) {
            // 投影期间新命中可以覆盖槽位，不能把旧坐标写给新数字。
            if current.active && current.serial == sample.serial {
                current.screen = sample.screen;
                current.visible = sample.visible;
            }
        }
    }
}

pub fn normalized_screen(projected: [f32; 4], width: f32, height: f32) -> Option<[f32; 2]> {
    if width <= 0.0
        || height <= 0.0
        || !projected.iter().all(|value| value.is_finite())
        || projected[3] <= 0.0
        || projected[2] <= 0.0
    {
        return None;
    }
    let screen = [projected[0] / width, projected[1] / height];
    if screen.iter().all(|value| (0.0..=1.0).contains(value)) {
        Some(screen)
    } else {
        None
    }
}

// 只在复制或修改插件自身数据时持锁；不在锁内调用游戏、Forge 或 ImGui。

pub struct Shared<T> {
    locked: AtomicBool,
    value: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for Shared<T> {}

impl<T> Shared<T> {
    pub const fn new(value: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            value: UnsafeCell::new(value),
        }
    }

    pub fn with<R>(&self, operation: impl FnOnce(&mut T) -> R) -> R {
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        let guard = Guard(&self.locked);
        let result = operation(unsafe { &mut *self.value.get() });
        drop(guard);
        result
    }
}

struct Guard<'a>(&'a AtomicBool);

impl Drop for Guard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

// 命中上下文、相机投影参数与原钩子存储。
#[derive(Clone, Copy)]
pub(crate) struct Contact {
    pub(crate) enabled: bool,
    pub(crate) local: bool,
    pub(crate) log: bool,
    pub(crate) attack: u32,
    pub(crate) monster: u32,
    pub(crate) slot: u32,
    pub(crate) part_index: u8,
    pub(crate) part: u32,
    pub(crate) owner: u32,
    pub(crate) player_slot: u8,
    pub(crate) forwarded: bool,
    pub(crate) epoch: Epoch,
    pub(crate) world: [f32; 3],
}

#[derive(Clone, Copy)]
pub(crate) struct Binding {
    pub(crate) thread: u32,
    pub(crate) contact: *const Contact,
}

pub(crate) struct Bindings(pub(crate) [Binding; 8]);
// 栈指针仅在原命中函数尚未返回时有效；所有读写都经过同一把锁。
unsafe impl Send for Bindings {}

#[repr(C, align(16))]
pub(crate) struct Vector(pub(crate) [f32; 4]);

pub(crate) struct Projection {
    pub(crate) function: unsafe extern "C" fn(*mut f32, u32, *const f32, u32) -> u32,
    pub(crate) manager: u32,
    pub(crate) view: u32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

// 已安装 subsdk9 的 forge_hook_create 返回 76 字节（原 ARM memcpy 长度 0x4C）。
// SDK 声明缺 context，Jit 长度也不同；用核对后的不透明 C 返回类型保存原结果。
#[repr(C)]
pub(crate) struct RuntimeHook([u32; 19]);
const _: () = assert!(core::mem::size_of::<RuntimeHook>() == 76);

pub(crate) static GAME_BASE: AtomicU32 = AtomicU32::new(0);
pub(crate) static ENABLED: AtomicBool = AtomicBool::new(false);
pub(crate) static INSTALLED: AtomicBool = AtomicBool::new(false);
pub(crate) static TOTAL_HITS: AtomicU32 = AtomicU32::new(0);
pub(crate) static TOTAL_EVENTS: AtomicU32 = AtomicU32::new(0);
pub(crate) static VISIBLE: AtomicU32 = AtomicU32::new(0);
pub(crate) static STATE: Shared<State> = Shared::new(State::new());
pub(crate) static BINDINGS: Shared<Bindings> = Shared::new(Bindings(
    [Binding {
        thread: 0,
        contact: ptr::null(),
    }; 8],
));
pub(crate) static mut COLLISION_HOOK: MaybeUninit<RuntimeHook> = MaybeUninit::uninit();
pub(crate) static mut DAMAGE_HOOK: MaybeUninit<RuntimeHook> = MaybeUninit::uninit();
pub(crate) static mut COLLISION_ORIGINAL: u32 = 0;
pub(crate) static mut DAMAGE_ORIGINAL: u32 = 0;

// 主机测试直接编译本文件，不加载 Forge 或执行游戏函数。
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hex_rgba_and_legacy_colors_preserve_alpha_on_save() {
        let config = Config::parse("# comment\n[display]\nnormal_color = #12aBcD80 ; comment\nhighlight_color = #FFDC50\noutline_color = 1,2,3,4 # old format\n").unwrap();
        assert_eq!(config.normal_color, [0x12, 0xAB, 0xCD, 0x80]);
        assert_eq!(config.highlight_color, [255, 220, 80, 255]);
        assert_eq!(config.outline_color, [1, 2, 3, 4]);
        let mut text = std::string::String::new();
        config.write_ini(&mut text).unwrap();
        assert!(!text.contains("normal_color"));
        let saved = Config::parse(&text).unwrap();
        assert_eq!(saved.enabled, config.enabled);
        for color in ["#12345", "#1234567", "#123456789", "#12GG5680", "#中文"] {
            assert!(Config::parse(&std::format!("[display]\nnormal_color={color}")).is_err());
        }
    }
    const QUEST: Epoch = Epoch {
        player: 0x12340000,
        room: 3,
    };

    #[test]
    fn slash_blunt_and_shot_use_their_own_44_45_boundary() {
        for value in [44, 45] {
            let yellow = value >= 45;
            assert_eq!(yellow_for_hitzone(0x20, [value, 80, 80], [100, 0]), yellow);
            assert_eq!(yellow_for_hitzone(0x20, [80, value, 80], [0, 100]), yellow);
            assert_eq!(yellow_for_hitzone(1, [80, 80, value], [100, 100]), yellow);
        }
    }

    #[test]
    fn hybrid_melee_uses_weighted_channel_instead_of_largest_hitzone() {
        assert!(!yellow_for_hitzone(0x20, [40, 45, 90], [100, 72]));
        assert!(yellow_for_hitzone(0x20, [44, 70, 0], [100, 72]));
        // 原物理肉质函数在权重乘积相等时选斩肉质。
        assert!(!yellow_for_hitzone(0x20, [40, 80, 0], [100, 50]));
    }

    #[test]
    fn special_physical_branches_still_obey_their_selected_hitzone() {
        assert!(!yellow_for_hitzone(0x10000, [44, 80, 80], [100, 0]));
        assert!(yellow_for_hitzone(0x10000, [80, 45, 0], [0, 100]));
        assert!(!yellow_for_hitzone(0x8000, [40, 44, 80], [100, 100]));
        assert!(yellow_for_hitzone(0x8000, [40, 45, 0], [100, 100]));
    }

    #[test]
    fn fixed_phial_damage_is_yellow_even_on_zero_hitzones() {
        for flags in [0, 4, 0x400, 0x200000] {
            assert!(yellow_for_hitzone(flags, [0; 3], [0; 2]));
            assert!(yellow_for_hitzone(flags, [10, 20, 30], [100, 0]));
        }
    }

    #[test]
    fn ten_phials_at_different_points_remain_ten_events() {
        let mut state = State::new();
        for phial in 0..10 {
            state.push(QUEST, [10.0, 20.0, phial as f32 * 100.0], 42, true);
        }
        let snapshot = state.snapshot();
        assert_eq!(
            snapshot.events.iter().filter(|event| event.active).count(),
            10
        );
        for phial in 0..10 {
            assert_eq!(snapshot.events[phial].world[2], phial as f32 * 100.0);
            assert_eq!(snapshot.events[phial].digits.as_str(), "42");
            assert!(snapshot.events[phial].yellow);
        }
    }

    #[test]
    fn dense_multihit_keeps_newest_64_without_deduplication() {
        let mut state = State::new();
        for hit in 1..=80 {
            state.push(QUEST, [0.0, 0.0, 0.0], hit, hit % 2 == 0);
        }
        let snapshot = state.snapshot();
        assert_eq!(
            snapshot.events.iter().filter(|event| event.active).count(),
            CAPACITY
        );
        let mut values: Vec<_> = snapshot
            .events
            .iter()
            .map(|event| event.digits.as_str().parse::<i32>().unwrap())
            .collect();
        values.sort();
        assert_eq!(values, (17..=80).collect::<Vec<_>>());
        for event in snapshot.events {
            let damage = event.digits.as_str().parse::<i32>().unwrap();
            assert_eq!(event.yellow, damage % 2 == 0);
        }
    }

    #[test]
    fn a_projection_finishing_after_slot_overwrite_cannot_move_new_hit() {
        let mut state = State::new();
        state.push(QUEST, [0.0; 3], 30, false);
        let mut old = state.snapshot();
        old.events[0].visible = true;
        old.events[0].screen = [0.99, 0.01];
        for _ in 0..CAPACITY {
            state.push(QUEST, [5.0; 3], 80, true);
        }
        state.commit_projection(&old);
        assert_eq!(state.snapshot().events[0].digits.as_str(), "80");
        assert!(state.snapshot().events[0].yellow);
        assert!(!state.snapshot().events[0].visible);
    }

    #[test]
    fn room_change_and_quest_end_discard_previous_numbers() {
        let mut state = State::new();
        state.push(QUEST, [0.0; 3], 8, false);
        let next = Epoch { room: 4, ..QUEST };
        state.push(next, [1.0; 3], 9, true);
        state.clear_epoch(QUEST); // 上一区域的更新不能清掉新命中。
        assert_eq!(
            state
                .snapshot()
                .events
                .iter()
                .filter(|event| event.active)
                .count(),
            1
        );
        state.clear_epoch(next);
        assert!(!state.snapshot().any_active());
    }

    #[test]
    fn expiry_and_full_pause_do_not_leave_stale_numbers() {
        let mut state = State::new();
        let config = Config::DEFAULT;
        state.push(QUEST, [0.0; 3], 1, false);
        let mid =
            state.advance(config.full_opacity + (config.lifetime - config.full_opacity) * 0.5);
        assert!(mid.events[0].alpha(&config) > 0 && mid.events[0].alpha(&config) < 255);
        assert!(!state.advance(2.0).any_active());
        state.push(QUEST, [0.0; 3], 1, true);
        assert!(!state.advance(config.lifetime).any_active());
    }

    #[test]
    fn numbers_stay_fully_visible_until_the_last_0_3_seconds() {
        let mut event = Event::EMPTY;
        let config = Config::DEFAULT;
        for age in [0.0, 0.55, 0.85, config.full_opacity] {
            event.age = age;
            assert_eq!(event.alpha(&config), 255);
        }
        let mut previous = 255;
        for step in 1..=6 {
            event.age =
                config.full_opacity + (config.lifetime - config.full_opacity) * step as f32 / 6.0;
            let alpha = event.alpha(&config);
            assert!(alpha < previous);
            previous = alpha;
        }
        assert_eq!(previous, 0);
    }

    #[test]
    fn shipped_ini_matches_defaults() {
        let text = include_str!("../conf/mhgu_dps_meter.ini");
        assert!(text.len() <= CONFIG_MAX_BYTES);
        assert_eq!(Config::parse(text).unwrap(), Config::DEFAULT);
    }

    #[test]
    fn panel_position_is_absolute_and_legacy_zero_uses_default() {
        let config =
            Config::parse("[dps]
dps_offset_x = 300
dps_offset_y = 300
").unwrap();
        assert_eq!(config.dps_offset_x, 300.0);
        assert_eq!(config.dps_offset_y, 300.0);
        let legacy = Config::parse("[dps]
dps_offset_x = 0
dps_offset_y = 0
").unwrap();
        assert_eq!(legacy.dps_offset_x, 16.0);
        assert_eq!(legacy.dps_offset_y, 340.0);
        assert!(Config::parse("[dps]
dps_offset_y = 800
").is_err());
        assert!(Config::parse("[dps]
dps_show_names = false
").unwrap().dps_show_names == false);
    }

    #[test]
    fn ini_accepts_bom_crlf_comments_and_custom_font() {
        let config = Config::parse(concat!(
            "\u{FEFF}; 用户配置\r\n[display]\r\n",
            "enabled = false\r\nfont_size = 42.0 ; 放大\r\n",
            "font_file = My Font.ttf\r\n",
            "highlight_color = 80, 255, 120 # 绿色\r\n",
            "normal_color = 230, 240, 255\r\n",
            "outline_color = 0, 0, 0\r\noutline_width = 0\r\n",
            "lifetime = 2.0\r\nfull_opacity = 1.5\r\n",
        ))
        .unwrap();
        assert!(!config.enabled);
        assert_eq!(config.font_size, 42.0);
        assert_eq!(config.highlight_color, [80, 255, 120, 255]);
        assert_eq!(config.normal_color, [230, 240, 255, 255]);
        assert_eq!(config.outline_color, [0, 0, 0, 255]);
        assert_eq!(config.outline_width, 0.0);
        assert_eq!(config.full_opacity, 1.5);
        assert_eq!(config.lifetime, 2.0);
        assert_eq!(
            config.font_file.unwrap().path().to_str().unwrap(),
            "app:/nativeNX/forge/fonts/My Font.ttf"
        );
    }

    #[test]
    fn omitted_settings_keep_defaults_and_typos_report_the_line() {
        let config = Config::parse("[display]\nfont_size = 48\n").unwrap();
        assert_eq!(config.font_size, 48.0);
        assert_eq!(config.normal_color, Config::DEFAULT.normal_color);
        assert_eq!(config.lifetime, Config::DEFAULT.lifetime);
        assert_eq!(config.font_file, Config::DEFAULT.font_file);
        let error = Config::parse("[display]\nfont_szie = 48\n").unwrap_err();
        assert_eq!(error.line, 2);
        for text in [
            "font_size = 48",
            "[display]\nfont_size",
            "[other]\nfont_size = 48",
            "[display]\nenabled = 1",
        ] {
            assert!(Config::parse(text).is_err(), "{text}");
        }
    }

    #[test]
    fn invalid_numbers_and_color_components_are_rejected() {
        for setting in [
            "font_size = NaN",
            "font_size = inf",
            "font_size = 0",
            "font_size = 129",
            "outline_width = -1",
            "outline_width = 11",
            "full_opacity = 0",
            "lifetime = 1",
            "lifetime = 61",
            "lifetime = NaN",
            "normal_color = 0, 0, 256",
            "normal_color = -1, 0, 0",
            "normal_color = 0, 0",
            "normal_color = 0, 0, 0, 0, 0",
            "normal_color = red",
        ] {
            assert!(
                Config::parse(&format!("[display]\n{setting}\n")).is_err(),
                "{setting}"
            );
        }
        let config =
            Config::parse("[display]\nnormal_color = 0, 128, 255\nfont_size = 128\n").unwrap();
        assert_eq!(config.normal_color, [0, 128, 255, 255]);
        assert_eq!(config.font_size, 128.0);
    }

    #[test]
    fn custom_font_must_be_a_filename_inside_the_fonts_directory() {
        for value in [
            "../Other.ttf",
            "app:/Other.ttf",
            "C:\\Other.ttf",
            "bad\0.ttf",
            "font.txt",
        ] {
            assert!(
                Config::parse(&format!("[display]\nfont_file = {value}\n")).is_err(),
                "{value}"
            );
        }
        let long = format!("{}.ttf", "a".repeat(192));
        assert!(FontFile::new(&long).is_none());
        assert_eq!(
            Config::parse("[display]\nfont_file = default\n")
                .unwrap()
                .font_file,
            Some(DEFAULT_FONT)
        );
        assert!(FontFile::new("NotoSansSC-Medium.otf").is_some());
    }

    #[test]
    fn configured_times_control_visibility_fade_and_expiry() {
        let mut state = State::new();
        state.config = Config::parse("[display]\nfull_opacity = 1.5\nlifetime = 2.0\n").unwrap();
        state.push(QUEST, [0.0; 3], 50, true);
        let full = state.advance(1.5);
        assert!(full.events[0].active);
        assert_eq!(full.events[0].alpha(&full.config), 255);
        let fading = state.advance(0.25);
        assert!(fading.events[0].active);
        assert_eq!(fading.events[0].alpha(&fading.config), 127);
        assert!(!state.advance(0.25).any_active());
    }

    #[test]
    fn projection_uses_viewport_size_and_rejects_behind_camera() {
        assert_eq!(
            normalized_screen([640.0, 360.0, 100.0, 1.0], 1280.0, 720.0),
            Some([0.5, 0.5])
        );
        assert_eq!(
            normalized_screen([960.0, 540.0, 100.0, 1.0], 1920.0, 1080.0),
            Some([0.5, 0.5])
        );
        for projected in [
            [640.0, 360.0, -10.0, -1.0],
            [1300.0, 0.0, 10.0, 1.0],
            [f32::NAN, 0.0, 10.0, 1.0],
        ] {
            assert_eq!(normalized_screen(projected, 1280.0, 720.0), None);
        }
        assert_eq!(normalized_screen([0.0, 0.0, 1.0, 1.0], 0.0, 720.0), None);
    }

    #[test]
    fn handheld_numbers_use_the_720p_crop_inside_a_1080p_texture() {
        let area = DisplayArea {
            width: 1280.0,
            height: 720.0,
        };
        // 游戏视口可以是逻辑 720p 或物理 1080p；显示范围都应取当前裁剪。
        for (width, height) in [(1280.0, 720.0), (1920.0, 1080.0)] {
            let center =
                normalized_screen([width / 2.0, height / 2.0, 100.0, 1.0], width, height).unwrap();
            assert_eq!(area.position(center), [640.0, 360.0]);
            let lower_right =
                normalized_screen([width * 0.9375, height * 0.9375, 100.0, 1.0], width, height)
                    .unwrap();
            assert_eq!(area.position(lower_right), [1200.0, 675.0]);
        }
        assert_eq!(Config::DEFAULT.font_size * area.scale(), 34.0);
        assert_eq!(Config::DEFAULT.outline_width * area.scale(), 2.0);
    }

    #[test]
    fn docked_numbers_keep_the_full_1080p_area_and_size() {
        let area = DisplayArea {
            width: 1920.0,
            height: 1080.0,
        };
        let screen = normalized_screen([960.0, 540.0, 100.0, 1.0], 1280.0, 720.0).unwrap();
        assert_eq!(area.position(screen), [1440.0, 810.0]);
        assert_eq!(area.position([0.5, 0.5]), [960.0, 540.0]);
        assert_eq!(Config::DEFAULT.font_size * area.scale(), 51.0);
        assert_eq!(Config::DEFAULT.outline_width * area.scale(), 3.0);
    }

    #[test]
    fn mode_changes_remap_existing_numbers_without_resetting_events() {
        let mut state = State::new();
        state.push(QUEST, [10.0, 20.0, 30.0], 42, true);
        state.advance(0.5);
        let serial = state.snapshot().events[0].serial;
        for (width, height, expected) in [
            (1280.0, 720.0, [640.0, 360.0]),
            (1920.0, 1080.0, [960.0, 540.0]),
            (1280.0, 720.0, [640.0, 360.0]),
        ] {
            let mut projected = state.snapshot();
            projected.display = DisplayArea { width, height };
            projected.events[0].screen = [0.5, 0.5];
            projected.events[0].visible = true;
            state.commit_projection(&projected);
            let frame = state.snapshot();
            let event = frame.events[0];
            assert_eq!(frame.display.position(event.screen), expected);
            assert_eq!(event.serial, serial);
            assert_eq!(event.age, 0.5);
            assert_eq!(event.world, [10.0, 20.0, 30.0]);
            assert_eq!(event.digits.as_str(), "42");
            assert!(event.yellow && event.active && event.visible);
        }
    }

    #[test]
    fn negative_sentinel_and_invalid_contact_never_become_damage_numbers() {
        let mut state = State::new();
        for value in [-100, 0, 32768] {
            state.push(QUEST, [0.0; 3], value, true);
        }
        state.push(QUEST, [f32::INFINITY, 0.0, 0.0], 10, true);
        assert!(!state.snapshot().any_active());
        assert_eq!(Digits::new(32767).as_str(), "32767");
        assert_eq!(Digits::new(u32::MAX).as_str(), "4294967295");
    }

    #[test]
    fn concurrent_event_producers_do_not_corrupt_digits_or_coordinates() {
        let state = std::sync::Arc::new(Shared::new(State::new()));
        let workers: Vec<_> = (0..4)
            .map(|thread| {
                let state = state.clone();
                std::thread::spawn(move || {
                    for hit in 1..=2000 {
                        let damage = thread * 2000 + hit;
                        state.with(|state| {
                            state.push(QUEST, [damage as f32, 0.0, 0.0], damage, damage % 2 == 0)
                        });
                    }
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        let snapshot = state.with(|state| state.snapshot());
        for event in snapshot.events {
            assert!(event.active);
            assert_eq!(
                event.digits.as_str().parse::<i32>().unwrap(),
                event.world[0] as i32
            );
            assert_eq!(event.yellow, event.world[0] as i32 % 2 == 0);
        }
    }
}
