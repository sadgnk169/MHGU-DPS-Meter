//! MHGU 1.4.0 本地伤害数字：采集命中 → 伤害入账 → 游戏投影 → Forge 绘制。
#![no_std]

mod data;
mod icons;
#[path = "../shared/plugin_config.rs"]
mod plugin_config;

use core::{
    arch::asm,
    ffi::c_void,
    fmt::{self, Write},
    mem::{self, MaybeUninit},
    ptr,
    sync::atomic::{AtomicU32, Ordering},
};
use data::*;
use forge::{
    imgui,
    log::prelude::{error, info, warn},
    sys::imgui::{ImDrawFlags, ImFont_GetFontBaked, ImGuiInputTextFlags, ImVec2},
};

// Forge 入口、更新和绘图。
// 字体仅在 ImGui 初始化时写入，随后由同一渲染线程读取。
static mut CUSTOM_FONT: Option<imgui::ImFont> = None;
static MENU: Shared<plugin_config::Edit<Config>> =
    Shared::new(plugin_config::Edit::new(Config::DEFAULT));
static FONT_INPUT: Shared<[u8; 168]> = Shared::new([0; 168]);

// Debug build: record every valid contact/damage event, including teammates,
// into savedata:/dps_debug.log. The display path still only shows local hits.
const DEBUG_LOG_CAP: usize = 262144;

// true for the log-collection build; false for the clean DPS panel build.
const DEBUG_LOG_ENABLED: bool = false;

// false: keep hooks/DPS but do not draw this plugin's own damage numbers.
const SHOW_LOCAL_NUMBERS: bool = false;

struct DebugLog {
    bytes: [u8; DEBUG_LOG_CAP],
    len: usize,
    truncated: bool,
    elapsed: f32,
    session_local: u32,
    session_room: u8,
    seen: [u32; 8],
    seen_len: usize,
    hits: u32,
}

impl DebugLog {
    const fn new() -> Self {
        Self {
            bytes: [0; DEBUG_LOG_CAP],
            len: 0,
            truncated: false,
            elapsed: 0.0,
            session_local: 0,
            session_room: 0,
            seen: [0; 8],
            seen_len: 0,
            hits: 0,
        }
    }

    fn push(&mut self, args: fmt::Arguments<'_>) {
        if self.truncated {
            return;
        }
        if self.write_fmt(args).is_err() {
            self.truncated = true;
        }
    }

    fn flush(&mut self) {
        if !DEBUG_LOG_ENABLED || self.len == 0 {
            return;
        }
        let text = unsafe { core::str::from_utf8_unchecked(&self.bytes[..self.len]) };
        if let Err(error) = plugin_config::DEBUG.save(text) {
            warn!("dps debug log: {error}");
        }
        self.elapsed = 0.0;
    }

    fn tick(&mut self, _dt: f32) {
        if !DEBUG_LOG_ENABLED {
            return;
        }
        // Never write periodically: save-data mount/commit stalls the game.
        // Flush only when the buffer is nearly full, or on the explicit triggers.
        if self.len >= DEBUG_LOG_CAP - 1024 {
            self.flush();
        }
    }

    fn hex(&mut self, label: &str, pointer: u32, length: usize) {
        if pointer < 0x10000 {
            return;
        }
        self.push(format_args!("[{}] ptr={:08X} data=", label, pointer));
        let words = length / 4;
        for i in 0..words {
            let value = unsafe { ptr::read_volatile((pointer + i as u32 * 4) as *const u32) };
            if self.write_fmt(format_args!("{:08X}", value)).is_err() {
                self.truncated = true;
                break;
            }
            if i + 1 < words {
                let _ = self.write_str(" ");
            }
        }
        let _ = self.write_str("
");
    }

    fn session_snapshot(&mut self, base: u32) {
        if !DEBUG_LOG_ENABLED {
            return;
        }
        let local = unsafe { local_player(base) };
        if local < 0x10000 {
            return;
        }
        let room = unsafe { read_u8(local + 0x1054) };
        if self.session_local == local && self.session_room == room {
            return;
        }
        self.session_local = local;
        self.session_room = room;
        let manager = unsafe { read_u32(base + 0x01885B58) };
        self.push(format_args!(
            "[SESSION] base={:08X} manager={:08X} local={:08X} room={:02X}
",
            base, manager, local, room
        ));
        self.hex("MANAGER", manager, 0x400);
        self.hex("LOCAL", local, 0x400);
    }

    fn owner_snapshot(&mut self, owner: u32, local: u32) {
        if !DEBUG_LOG_ENABLED || owner < 0x10000 {
            return;
        }
        if self.seen[..self.seen_len].contains(&owner) {
            return;
        }
        if self.seen_len < self.seen.len() {
            self.seen[self.seen_len] = owner;
            self.seen_len += 1;
        }
        let room = unsafe { read_u8(owner + 0x1054) };
        let is_local = if owner == local { 1 } else { 0 };
        self.push(format_args!(
            "[OWNER] ptr={:08X} local={} room={:02X}
",
            owner, is_local, room
        ));
        self.hex("OWNER", owner, 0x400);
    }

    fn hit(&mut self, contact: &Contact, damage: i32, raw: i32, mode: i32) {
        if !DEBUG_LOG_ENABLED {
            return;
        }
        self.hits += 1;
        let number = self.hits;
        let frame = FRAME.load(Ordering::Relaxed);
        let owner = contact.owner;
        let local = if contact.local { 1 } else { 0 };
        let room = contact.epoch.room;
        let attack = contact.attack;
        let monster = contact.monster;
        let slot = contact.slot;
        let part_index = contact.part_index;
        let part = contact.part;
        let world = contact.world;
        self.push(format_args!(
            "[HIT] n={} f={} owner={:08X} local={} pslot={} room={:02X} attack={:08X} monster={:08X} slot={} part={} partptr={:08X} dmg={} raw={} mode={} world={:08X},{:08X},{:08X}
",
            number, frame, owner, local, contact.player_slot, room, attack, monster, slot, part_index, part,
            damage, raw, mode,
            world[0].to_bits(), world[1].to_bits(), world[2].to_bits()
        ));
    }
}

impl fmt::Write for DebugLog {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self.len + text.len();
        if end > self.bytes.len() {
            return Err(fmt::Error);
        }
        self.bytes[self.len..end].copy_from_slice(text.as_bytes());
        self.len = end;
        Ok(())
    }
}

static DEBUG_LOG: Shared<DebugLog> = Shared::new(DebugLog::new());

// 游戏内名字是 UTF-8；manager 的玩家数据块最多保留 32 字节。
#[derive(Clone, Copy)]
struct PlayerName {
    bytes: [u8; 32],
    len: u8,
}

impl PlayerName {
    const EMPTY: Self = Self {
        bytes: [0; 32],
        len: 0,
    };

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }
}

// Minimal per-owner DPS aggregation.
#[derive(Clone, Copy)]
struct OwnerDps {
    owner: u32,
    local: bool,
    damage: u64,
    hits: u32,
    slot: u8,
    color: u8,
    weapon: u8,
    palico_type: u32,
    name: PlayerName,
}
impl OwnerDps {
    const EMPTY: Self = Self {
        owner: 0,
        local: false,
        damage: 0,
        hits: 0,
        slot: 0xFF,
        color: 0xFF,
        weapon: 0xFF,
        palico_type: 0xFFFF_FFFF,
        name: PlayerName::EMPTY,
    };
}

// 整场曲线固定保留 600 个点；满了就合并相邻点，越长的任务时间分辨率越低，
// 但每帧顶点数、状态大小和绘制成本保持不变。
const HISTORY_CAP: usize = 600;
const SAMPLE_INTERVAL: f32 = 1.0;

#[derive(Clone, Copy)]
struct HistorySample {
    time: f32,
    damage: [u32; 4],
}

impl HistorySample {
    const EMPTY: Self = Self {
        time: 0.0,
        damage: [0; 4],
    };
}

#[derive(Clone, Copy)]
struct DpsState {
    owners: [OwnerDps; 4],
    count: usize,
    total: u64,
    elapsed: f32,
    active: bool,
    visible: bool,
    in_quest: bool,
    result_frames: u32,
    history: [HistorySample; HISTORY_CAP],
    history_len: usize,
    last_sample: f32,
}

impl DpsState {
    const fn new() -> Self {
        Self {
            owners: [OwnerDps::EMPTY; 4],
            count: 0,
            total: 0,
            elapsed: 0.0,
            active: false,
            visible: true,
            in_quest: false,
            result_frames: 0,
            history: [HistorySample::EMPTY; HISTORY_CAP],
            history_len: 0,
            last_sample: 0.0,
        }
    }

    fn reset(&mut self) {
        self.owners = [OwnerDps::EMPTY; 4];
        self.count = 0;
        self.total = 0;
        self.elapsed = 0.0;
        self.active = false;
        self.history = [HistorySample::EMPTY; HISTORY_CAP];
        self.history_len = 0;
        self.last_sample = 0.0;
    }

    fn reset_now(&mut self) {
        self.reset();
    }


    // Save/result screens have no local player object. Require a short
    // debounce so area loading cannot clear an active run.
    fn on_result_screen(&mut self, result: bool) {
        if result {
            self.result_frames = self.result_frames.saturating_add(1);
            if self.result_frames >= 60 {
                self.reset();
                self.in_quest = false;
            }
        } else {
            self.result_frames = 0;
            if !self.in_quest {
                self.in_quest = true;
            }
        }
    }

    fn add(&mut self, owner: u32, local: bool, damage: i32, player_slot: u8) {
        if damage <= 0 {
            return;
        }
        if !self.in_quest {
            self.reset();
            self.in_quest = true;
        }
        let index = if player_slot < 4 {
            self.owners[..self.count]
                .iter()
                .position(|row| row.slot == player_slot)
        } else {
            self.owners[..self.count]
                .iter()
                .position(|row| row.slot == 0xFF && row.owner == owner)
        };
        let index = match index {
            Some(index) => index,
            None => {
                if self.count >= self.owners.len() {
                    return;
                }
                self.owners[self.count] = OwnerDps {
                    owner,
                    local,
                    damage: 0,
                    hits: 0,
                    slot: player_slot,
                    color: owner_color(owner, player_slot),
                    weapon: owner_weapon(owner),
                    palico_type: owner_palico_type(owner),
                    name: unsafe { owner_name(GAME_BASE.load(Ordering::Relaxed), owner) },
                };
                self.count += 1;
                self.count - 1
            }
        };
        let row = &mut self.owners[index];
        row.damage = row.damage.saturating_add(damage as u64);
        row.hits = row.hits.saturating_add(1);
        if local {
            row.local = true;
        }
        if row.slot == 0xFF {
            row.slot = player_slot;
        }
        let color = owner_color(owner, player_slot);
        if color < 4 {
            row.color = color;
        }
        let weapon = owner_weapon(owner);
        if weapon != 0xFF {
            row.weapon = weapon;
        }
        let palico_type = owner_palico_type(owner);
        if palico_type != 0xFFFF_FFFF {
            row.palico_type = palico_type;
        }
        let name = unsafe { owner_name(GAME_BASE.load(Ordering::Relaxed), owner) };
        if !name.is_empty() {
            row.name = name;
        }
        self.total = self.total.saturating_add(damage as u64);
        self.active = true;
    }

    fn decimate_history(&mut self) {
        // 保留第一点和每个偶数点，再补上最后一点；累计伤害值不会被平均，
        // 曲线末端始终对应真实的当前总伤害。
        let mut source = 0usize;
        let mut target = 0usize;
        while source < self.history_len {
            self.history[target] = self.history[source];
            target += 1;
            source += 2;
        }
        let last = self.history_len - 1;
        if target == 0 || self.history[target - 1].time != self.history[last].time {
            self.history[target] = self.history[last];
            target += 1;
        }
        self.history_len = target;
    }

    fn tick(&mut self, dt: f32) {
        if !self.active || !self.in_quest {
            return;
        }
        self.elapsed += if dt.is_finite() && dt > 0.0 { dt } else { 1.0 / 60.0 };
        if self.history_len == 0 || self.elapsed - self.last_sample >= SAMPLE_INTERVAL {
            let mut damage = [0u32; 4];
            for i in 0..self.count {
                let row = self.owners[i];
                let index = row_color(&row, i);
                damage[index.min(3)] = row.damage.min(u32::MAX as u64) as u32;
            }
            let sample = HistorySample {
                time: self.elapsed,
                damage,
            };
            if self.history_len == HISTORY_CAP {
                self.decimate_history();
            }
            if self.history_len < HISTORY_CAP {
                self.history[self.history_len] = sample;
                self.history_len += 1;
            }
            self.last_sample = self.elapsed;
        }
    }

}

fn owner_color(owner: u32, fallback: u8) -> u8 {
    if owner < 0x10000 {
        return if fallback < 4 { fallback } else { 0xFF };
    }
    let value = unsafe { read_u32(owner + 0x100) } & 0xF00;
    match value {
        0x100 => 0,
        0x200 => 1,
        0x400 => 3,
        0x800 => 2,
        _ => {
            if fallback < 4 {
                fallback
            } else {
                0xFF
            }
        }
    }
}

fn owner_weapon(owner: u32) -> u8 {
    if owner < 0x10000 {
        return 0xFF;
    }
    let pointer = unsafe { read_u32(owner + 0x13C4) };
    // Switch 的用户堆也可能落在 0x8xxxxxxx；只排除低地址和接近 4GB 的非法范围。
    if pointer < 0x10000 || pointer > 0xFFFF_0000 - 0x14C {
        return 0xFF;
    }
    let value = unsafe { read_u8(pointer + 0x14C) };
    if value <= 0x0F {
        value
    } else {
        0xFF
    }
}

fn owner_palico_type(owner: u32) -> u32 {
    if owner < 0x10000 {
        return 0xFFFF_FFFF;
    }
    unsafe { read_u32(owner + 0x1380) }
}

unsafe fn owner_name(base: u32, owner: u32) -> PlayerName {
    if owner < 0x10000 || base < 0x10000 {
        return PlayerName::EMPTY;
    }
    let manager = unsafe { read_u32(base + 0x01885B58) };
    if manager < 0x10000 {
        return PlayerName::EMPTY;
    }
    let local = unsafe { local_player(base) };
    let mut local_fallback = PlayerName::EMPTY;
    for i in 0..5u32 {
        let block = manager + 0x300 + i * 0x560;
        let name_ptr = block + 0x290;
        let mut name = PlayerName::EMPTY;
        for j in 0..32u32 {
            let byte = unsafe { read_u8(name_ptr + j) };
            if byte == 0 {
                break;
            }
            name.bytes[name.len as usize] = byte;
            name.len += 1;
        }
        if name.is_empty() || core::str::from_utf8(&name.bytes[..name.len as usize]).is_err() {
            continue;
        }
        let owner_ptr = unsafe { read_u32(block + 0x288) };
        if owner_ptr >= 0x10000 {
            if owner_ptr == owner {
                return name;
            }
        } else if owner == local && local_fallback.is_empty() {
            // 本地猎人名块（普通任务通常在这里）；若另有猫名块指向本地玩家，
            // 上面的精确匹配会优先返回。
            local_fallback = name;
        }
    }
    local_fallback
}

fn palico_icon(palico_type: u32) -> Option<usize> {
    match palico_type {
        0xAB => Some(16),
        0x88 => Some(17),
        0x6C => Some(18),
        0xBB => Some(19),
        0x80 => Some(20),
        0x68 => Some(21),
        0xD4 => Some(22),
        0x26 => Some(23),
        _ => None,
    }
}

fn row_icon(row: &OwnerDps) -> usize {
    if row.weapon == 15 {
        palico_icon(row.palico_type).unwrap_or(15)
    } else if row.weapon < 15 {
        row.weapon as usize
    } else {
        // 猎猫模式偶尔读不到武器类型；palico_type 已知时仍显示对应猫图标。
        palico_icon(row.palico_type).unwrap_or(0xFF)
    }
}

fn row_label(row: &OwnerDps, show_names: bool) -> &str {
    let name = row.name.as_str();
    if show_names && !name.is_empty() {
        return name;
    }
    if row.local {
        "You"
    } else {
        match row.slot {
            0 => "P1",
            1 => "P2",
            2 => "P3",
            3 => "P4",
            _ => "P?",
        }
    }
}

fn row_color(row: &OwnerDps, index: usize) -> usize {
    if row.color < 4 {
        row.color as usize
    } else if row.slot < 4 {
        row.slot as usize
    } else {
        index
    }
}

static DPS: Shared<DpsState> = Shared::new(DpsState::new());
const DPS_FONT_SIZE: f32 = 19.0;
static FRAME: AtomicU32 = AtomicU32::new(0);

#[unsafe(no_mangle)]
pub extern "C" fn forge_onLoad(params: *mut forge::sys::init::PluginInitParams) {
    unsafe {
        (*params).required_version = forge::REQUIRED_VERSION;
        // The C PluginInitParam has ForgeVersion (6 bytes) followed by name[64].
        let base = params as *mut u8;
        let name = b"MHGU DPS Meter";
        core::ptr::copy_nonoverlapping(name.as_ptr(), base.add(6), name.len());
        *base.add(6 + name.len()) = 0;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn forge_onInit() {
    forge::log::init().expect("Failed to initialize logger");
    main();
}

fn main() {
    let base = forge::mem::text_addr();
    GAME_BASE.store(base, Ordering::Relaxed);
    if compatible(base) {
        let config = load_config();
        MENU.with(|edit| *edit = plugin_config::Edit::new(config));
        set_font_input(config);
        STATE.with(|state| state.config = config);
        unsafe { install_hooks(base) };
        INSTALLED.store(true, Ordering::Release);
        ENABLED.store(config.enabled, Ordering::Relaxed);
        info!("mhgu-dps-meter v0.1: DPS meter, main=0x{base:08X}");
    }
}

fn load_config() -> Config {
    let mut bytes = [0; CONFIG_MAX_BYTES];
    match plugin_config::DAMAGE.read(&mut bytes) {
        Ok(text) => match Config::parse(text) {
            Ok(config) => return config,
            Err(error) => warn!("damage save INI line {}: {}", error.line, error.message),
        },
        Err(error) => warn!("damage save INI: {error:?}; using defaults"),
    }
    Config::DEFAULT
}

fn file_exists(path: &core::ffi::CStr) -> bool {
    // 直接传入已有 C 字符串，避免 SDK 的 CString 包装引入 malloc/free 导入。
    unsafe { forge::sys::fs::forge_fs_fileExists(path.as_ptr()) }
}

#[unsafe(no_mangle)]
pub extern "C" fn forge_onImGuiInit(
    _context: *mut c_void,
    _alloc: *mut c_void,
    _free: *mut c_void,
) {
    let config = STATE.with(|state| state.config);
    if let Some(file) = config.font_file {
        let path = file.path();
        let font = if file_exists(path) {
            imgui::ImFont::load(path)
        } else {
            None
        };
        match font {
            Some(font) => {
                unsafe { CUSTOM_FONT = Some(font) };
                info!(
                    "mhgu-dps-meter: loaded font {}",
                    path.to_str().unwrap()
                );
            }
            None => warn!(
                "mhgu-dps-meter: could not load font {}; using Forge font",
                path.to_str().unwrap()
            ),
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn forge_onUpdate(dt: f32) {
    // 在游戏更新结束后投影；NVN 绘制回调只读插件快照，不访问游戏对象。
    let base = GAME_BASE.load(Ordering::Relaxed);
    FRAME.fetch_add(1, Ordering::Relaxed);
    let dt = if dt.is_finite() && dt > 0.0 { dt } else { 1.0 / 60.0 };
    if DEBUG_LOG_ENABLED {
        DEBUG_LOG.with(|log| {
            log.tick(dt);
            log.session_snapshot(base);
        });
    }
    let display = unsafe { display_area(base) };
    if display.width >= 320.0 && display.height >= 240.0 {
        STATE.with(|state| state.set_display(display));
    }
    let mut snapshot = STATE.with(|state| state.snapshot());
    let player = unsafe { local_player(base) };
    DPS.with(|dps| dps.on_result_screen(player < 0x10000));
    // Quest/room transitions flush even when the last damage number has already expired.
    if player >= 0x10000
        && snapshot.epoch.player != 0
        && (player != snapshot.epoch.player || unsafe { epoch(player) } != snapshot.epoch)
    {
        if DEBUG_LOG_ENABLED {
            DEBUG_LOG.with(|log| log.flush());
        }
        STATE.with(|state| state.reset_epoch());
        return;
    }
    if !snapshot.any_active() {
        return;
    }
    if player < 0x10000 {
        return;
    }
    let projection = unsafe { Projection::current(base) };
    snapshot.display = unsafe { display_area(base) };
    for event in snapshot.events.iter_mut().filter(|event| event.active) {
        event.visible = false;
        if let Some(screen) = unsafe { projection.project(event.world) } {
            event.screen = screen;
            event.visible = true;
        }
    }
    STATE.with(|state| state.commit_projection(&snapshot));
}

#[forge::imgui_free_render]
fn draw_damage() {
    let seconds = imgui::get_io().DeltaTime;
    let seconds = if seconds.is_finite() && seconds > 0.0 {
        seconds
    } else {
        1.0 / 60.0
    };
    DPS.with(|dps| dps.tick(seconds));
    let snapshot = STATE.with(|state| state.advance(seconds));
    if !SHOW_LOCAL_NUMBERS {
        draw_dps();
        return;
    }
    let config = snapshot.config;
    let draw = imgui::DrawList::foreground();
    let font = unsafe { CUSTOM_FONT }.unwrap_or_else(imgui::ImFont::current);
    // Forge 的 DisplaySize 是整张交换纹理；掌机画面只使用其中的 1280×720。
    let scale = snapshot.display.scale();
    imgui::push_font(&font, Some(config.font_size * scale));
    let mut visible = 0;
    for event in snapshot
        .events
        .iter()
        .filter(|event| event.active && event.visible)
    {
        let size = (config.font_size + 4.0 * (1.0 - event.age / 0.15).max(0.0)) * scale;
        let text = event.digits.as_str();
        let text_size = imgui::calc_text_size(text, false, 0.0);
        let width = text_size.x * size / imgui::ImFont::current_size();
        let position = snapshot.display.position(event.screen);
        let x = position[0] - width * 0.5 + ((event.serial % 5) as f32 - 2.0) * 6.0 * scale;
        let y = position[1]
            - size
            - ((event.age / config.full_opacity).min(1.0) * 30.0 + (event.serial % 3) as f32 * 5.0)
                * scale;
        let alpha = event.alpha(&config);
        let outline = imgui::color(
            config.outline_color[0],
            config.outline_color[1],
            config.outline_color[2],
            (u16::from(alpha) * u16::from(config.outline_color[3]) / 255) as u8,
        );
        let edge = config.outline_width * scale;
        if edge > 0.0 {
            for (dx, dy) in [
                (-edge, 0.0),
                (edge, 0.0),
                (0.0, -edge),
                (0.0, edge),
                (-edge, -edge),
                (-edge, edge),
                (edge, -edge),
                (edge, edge),
            ] {
                draw.add_text_with_font(&font, size, ImVec2::new(x + dx, y + dy), outline, text);
            }
        }
        let rgb = if event.yellow {
            config.highlight_color
        } else {
            config.normal_color
        };
        let color = imgui::color(
            rgb[0],
            rgb[1],
            rgb[2],
            (u16::from(alpha) * u16::from(rgb[3]) / 255) as u8,
        );
        draw.add_text_with_font(&font, size, ImVec2::new(x, y), color, text);
        visible += 1;
    }
    imgui::pop_font();
    VISIBLE.store(visible, Ordering::Relaxed);
    draw_dps();
}

fn dps_color(index: usize, alpha: u8) -> u32 {
    const COLORS: [(u8, u8, u8); 4] = [
        (255, 107, 107),
        (120, 185, 255),
        (92, 214, 92),
        (255, 212, 92),
    ];
    let color = COLORS[index.min(3)];
    imgui::color(color.0, color.1, color.2, alpha)
}

fn font_ascent(font: &imgui::ImFont, size: f32) -> f32 {
    unsafe {
        let baked = ImFont_GetFontBaked(font.raw(), size, 1.0);
        if baked.is_null() {
            size * 0.8
        } else {
            (*baked).Ascent
        }
    }
}

fn measure_text(font: &imgui::ImFont, size: f32, text: &str) -> f32 {
    imgui::push_font(font, Some(size));
    let current = imgui::ImFont::current_size();
    let width = imgui::calc_text_size(text, false, 0.0).x;
    imgui::pop_font();
    if current > 0.0 {
        width * size / current
    } else {
        width
    }
}

fn draw_pixel_icon(
    draw: &imgui::DrawList,
    icon: &icons::Icon,
    left: f32,
    bottom: f32,
    target: f32,
    color: u32,
) {
    let logical_w = (icon.width / 2) as usize;
    let logical_h = (icon.height / 2) as usize;
    if logical_w == 0 || logical_h == 0 {
        return;
    }
    // 每个 2×2 源块使用相同的整数格尺寸：720p 为 2px，1080p 为 3px。
    let cell = (((target / logical_w as f32) + 0.5) as u32).max(1);
    let icon_left = ((left).max(0.0) + 0.5) as u32 as f32;
    let bottom_px = ((bottom).max(0.0) + 0.5) as u32 as f32;
    let icon_top = bottom_px - (logical_h as u32 * cell) as f32;
    let cell_x = cell as f32;
    let cell_y = cell as f32;
    let width = icon.width as usize;

    let tint_r = color & 0xFF;
    let tint_g = (color >> 8) & 0xFF;
    let tint_b = (color >> 16) & 0xFF;
    let tint_a = (color >> 24) & 0xFF;
    let block = |lx: usize, ly: usize| -> (u8, u8, u8, u8) {
        let sx = (lx * 2).min(width - 1);
        let sy = (ly * 2).min(icon.height as usize - 1);
        let px = (sy * width + sx) * 4;
        (
            icon.pixels[px],
            icon.pixels[px + 1],
            icon.pixels[px + 2],
            icon.pixels[px + 3],
        )
    };
    let tinted = |pixel: (u8, u8, u8, u8)| -> u32 {
        let r = pixel.0 as u32 * tint_r / 255;
        let g = pixel.1 as u32 * tint_g / 255;
        let b = pixel.2 as u32 * tint_b / 255;
        let a = pixel.3 as u32 * tint_a / 255;
        r | (g << 8) | (b << 16) | (a << 24)
    };

    for ly in 0..logical_h {
        let mut lx = 0usize;
        while lx < logical_w {
            let pixel = block(lx, ly);
            if pixel.3 == 0 {
                lx += 1;
                continue;
            }
            let start = lx;
            lx += 1;
            while lx < logical_w {
                let next = block(lx, ly);
                if next != pixel {
                    break;
                }
                lx += 1;
            }
            draw.add_rect_filled(
                ImVec2::new(
                    icon_left + start as f32 * cell_x,
                    icon_top + ly as f32 * cell_y,
                ),
                ImVec2::new(
                    icon_left + lx as f32 * cell_x,
                    icon_top + (ly + 1) as f32 * cell_y,
                ),
                tinted(pixel),
                0.0,
            );
        }
    }
}
fn draw_dps_curve(dps: &DpsState, config: &Config, display: DisplayArea) {
    let scale = display.height / 720.0;
    let font = unsafe { CUSTOM_FONT }.unwrap_or_else(imgui::ImFont::current);
    let font_size = DPS_FONT_SIZE * scale;
    let small_font = font_size * 0.72;
    let name_font = font_size * 1.15;
    let icon_size = name_font * 0.95;
    let header_h = font_size + 4.0 * scale;
    let row_text_offset = 5.0 * scale;
    let inline_stats = config.dps_inline_stats && !config.dps_show_hits;
    let two_line = !inline_stats && config.dps_show_damage && config.dps_show_dps;
    let name_bottom = row_text_offset + name_font;
    let two_line_bottom = font_size * 0.92 - 2.0 * scale + small_font;
    let content_bottom = if two_line { two_line_bottom } else { name_bottom };
    let row_h = if config.dps_show_percent_bar {
        if two_line {
            name_font + small_font + 8.0 * scale
        } else {
            name_bottom + 8.0 * scale
        }
    } else {
        content_bottom + 4.0 * scale
    };
    let pad = 7.0 * scale;
    let show_chart = config.dps_show_curve;
    let chart_height = if show_chart {
        config.dps_curve_height * scale
    } else {
        0.0
    };
    // 曲线下方与第一行的留白，等于普通玩家行之间由进度条底部到下一行
    // 顶部的空白；不显示曲线时不需要额外留白。
    let first_row_gap = if show_chart {
        name_font - font_size + 2.0 * scale
    } else {
        0.0
    };

    let mut rows = dps.owners;
    for i in 0..dps.count {
        for j in (i + 1)..dps.count {
            if rows[j].damage > rows[i].damage {
                rows.swap(i, j);
            }
        }
    }

    let icon_space = if config.dps_show_weapon {
        icon_size + 6.0 * scale
    } else {
        0.0
    };
    let stat_gap = 8.0 * scale;
    let mut labels: [plugin_config::Text<64>; 4] =
        core::array::from_fn(|_| plugin_config::Text::new());
    let mut name_widths = [0.0f32; 4];
    let mut stats_widths = [0.0f32; 4];
    let mut max_percent_w = 0.0f32;
    let mut max_total_w = 0.0f32;
    let mut max_dps_w = 0.0f32;
    let mut max_name_w = 0.0f32;
    for (index, row) in rows[..dps.count].iter().enumerate() {
        let mut label_text = plugin_config::Text::<64>::new();
        let _ = write!(&mut label_text, "{}", row_label(row, config.dps_show_names));
        name_widths[index] = measure_text(&font, name_font, label_text.as_str());
        max_name_w = max_name_w.max(name_widths[index]);
        labels[index] = label_text;
        if config.dps_show_percent {
            let percent_tenths = if dps.total > 0 {
                row.damage * 1000 / dps.total
            } else {
                0
            };
            let mut text = plugin_config::Text::<16>::new();
            let _ = write!(
                &mut text,
                "{}.{}%",
                percent_tenths / 10,
                percent_tenths % 10
            );
            let size = if inline_stats { font_size } else { font_size * 0.9 };
            max_percent_w = max_percent_w.max(measure_text(&font, size, text.as_str()));
        }
        if inline_stats {
            let percent_tenths = if dps.total > 0 {
                row.damage * 1000 / dps.total
            } else {
                0
            };
            let dps_tenths = if dps.elapsed > 0.1 {
                (row.damage as f32 * 10.0 / dps.elapsed) as u64
            } else {
                0
            };
            let mut width = 0.0f32;
            let mut count = 0usize;
            if config.dps_show_dps {
                let mut text = plugin_config::Text::<24>::new();
                let _ = write!(&mut text, "{}.{}/s", dps_tenths / 10, dps_tenths % 10);
                let measured = measure_text(&font, font_size, text.as_str());
                width += measured;
                max_dps_w = max_dps_w.max(measured);
                count += 1;
            }
            if config.dps_show_damage {
                let mut text = plugin_config::Text::<24>::new();
                let _ = write!(&mut text, "{}", row.damage);
                let measured = measure_text(&font, font_size, text.as_str());
                width += measured;
                max_total_w = max_total_w.max(measured);
                count += 1;
            }
            if config.dps_show_percent {
                let mut text = plugin_config::Text::<16>::new();
                let _ = write!(
                    &mut text,
                    "{}.{}%",
                    percent_tenths / 10,
                    percent_tenths % 10
                );
                width += measure_text(&font, font_size, text.as_str());
                count += 1;
            }
            if count > 1 {
                width += stat_gap * (count - 1) as f32;
            }
            stats_widths[index] = width;
        }
    }

    let team_dps_tenths = if dps.elapsed > 0.1 {
        (dps.total as f32 * 10.0 / dps.elapsed) as u64
    } else {
        0
    };
    let mut team_total_text = plugin_config::Text::<32>::new();
    if config.dps_show_damage {
        let _ = write!(&mut team_total_text, "{}", dps.total);
    }
    let mut team_dps_text = plugin_config::Text::<24>::new();
    if config.dps_show_dps {
        let _ = write!(
            &mut team_dps_text,
            "{}.{}/s",
            team_dps_tenths / 10,
            team_dps_tenths % 10
        );
    }
    let show_team = config.dps_show_team && (config.dps_show_damage || config.dps_show_dps) && dps.count > 1;
    let team_total_w = if config.dps_show_damage {
        measure_text(&font, font_size, team_total_text.as_str())
    } else {
        0.0
    };
    let team_label_w = if config.dps_show_damage {
        measure_text(&font, name_font, "DMG")
    } else {
        0.0
    };
    let team_dps_w = if config.dps_show_dps {
        let size = if inline_stats || !config.dps_show_damage {
            font_size
        } else {
            small_font
        };
        measure_text(&font, size, team_dps_text.as_str())
    } else {
        0.0
    };
    let total_reserve_w = measure_text(&font, font_size, "99999");
    let total_col_w = total_reserve_w.max(max_total_w).max(team_total_w);
    let percent_reserve_w = measure_text(
        &font,
        if inline_stats { font_size } else { font_size * 0.9 },
        "100.0%",
    );

    let mut width = config.dps_curve_width * scale;
    if inline_stats {
        let mut group_width = 0.0f32;
        let mut group_count = 0usize;
        if config.dps_show_dps {
            group_width += max_dps_w;
            group_count += 1;
        }
        if config.dps_show_damage {
            group_width += total_col_w;
            group_count += 1;
        }
        if config.dps_show_percent {
            group_width += percent_reserve_w;
            group_count += 1;
        }
        if group_count > 1 {
            group_width += stat_gap * (group_count - 1) as f32;
        }
        let player_required = pad * 2.0 + icon_space + max_name_w
            + if group_width > 0.0 { stat_gap + group_width } else { 0.0 };
        width = width.max(player_required);
        if show_team {
            let mut team_required = pad * 2.0 + icon_space;
            if config.dps_show_damage {
                team_required += team_label_w + stat_gap + group_width;
            } else {
                team_required += group_width;
            }
            width = width.max(team_required);
        }
    }

    let header_rows = if config.dps_show_timer { 1.0 } else { 0.0 };
    let team_top_shift = if show_team { 4.0 * scale } else { 0.0 };
    let team_row_h = if !show_team {
        0.0
    } else if two_line && config.dps_show_damage && config.dps_show_dps {
        two_line_bottom - team_top_shift
    } else {
        name_bottom - team_top_shift
    };
    let bottom_pad = if show_team { 3.0 * scale } else { pad };
    let height = pad
        + bottom_pad
        + header_rows * header_h
        + chart_height
        + first_row_gap
        + row_h * dps.count as f32
        + team_row_h;
    let x = (config.dps_offset_x * scale).clamp(0.0, (display.width - width).max(0.0));
    let y = (config.dps_offset_y * scale).clamp(0.0, (display.height - height).max(0.0));
    let draw = imgui::DrawList::foreground();
    let gold = imgui::color(255, 220, 80, 255);
    let white = imgui::color(255, 255, 255, 255);

    let alpha = config.dps_background_alpha.clamp(0.0, 255.0) as u8;
    if alpha > 0 {
        draw.add_rect_filled(
            ImVec2::new(x, y),
            ImVec2::new(x + width, y + height),
            imgui::color(0, 0, 0, alpha),
            4.0 * scale,
        );
    }

    let mut text_y = y + pad - 2.0 * scale;
    if config.dps_show_timer {
        let mut header = plugin_config::Text::<64>::new();
        let seconds = dps.elapsed as u32;
        let _ = write!(&mut header, "DPS  {:02}:{:02}", seconds / 60, seconds % 60);
        draw.add_text_with_font(
            &font,
            font_size,
            ImVec2::new(x + pad, text_y),
            gold,
            header.as_str(),
        );
        text_y += header_h;
    }

    let chart_bottom = text_y + chart_height;
    if show_chart {
    let chart_left = x + pad;
    let chart_width = width - pad * 2.0;
    let mut max_damage = 1u32;
    for index in 0..dps.history_len {
        let sample = &dps.history[(history_start(dps) + index) % HISTORY_CAP];
        for value in sample.damage {
            max_damage = max_damage.max(value);
        }
    }
    for step in 1..=3 {
        let line_y = chart_bottom - chart_height * step as f32 / 4.0;
        draw.add_line(
            ImVec2::new(chart_left, line_y),
            ImVec2::new(chart_left + chart_width, line_y),
            imgui::color(255, 255, 255, 32),
            1.0 * scale,
        );
    }

    let start = history_start(dps);
    let first_time = if dps.history_len > 0 {
        dps.history[start].time
    } else {
        0.0
    };
    let last_time = if dps.history_len > 0 {
        dps.history[(start + dps.history_len - 1) % HISTORY_CAP].time
    } else {
        0.0
    };
    let time_span = (last_time - first_time).max(1.0);
    if config.dps_curve_fill {
        for (row_index, row) in rows[..dps.count].iter().enumerate() {
            let color_index = row_color(row, row_index);
            let mut points: [ImVec2; HISTORY_CAP] =
                core::array::from_fn(|_| ImVec2::new(0.0, 0.0));
            for index in 0..dps.history_len {
                let sample = &dps.history[(start + index) % HISTORY_CAP];
                let px = if dps.history_len == 1 {
                    chart_left + chart_width
                } else {
                    chart_left
                        + ((sample.time - first_time) / time_span).clamp(0.0, 1.0)
                            * chart_width
                };
                let value = sample.damage[color_index] as f32 / max_damage as f32;
                let py = chart_bottom - value.clamp(0.0, 1.0) * chart_height;
                points[index] = ImVec2::new(px, py);
            }
            let point_count = dps.history_len;
            if point_count > 1 {
                let fill = dps_color(color_index, 55);
                let step = 1.0 * scale;
                let right = chart_left + chart_width;
                let mut column = chart_left;
                let mut segment = 1usize;
                while column < right {
                    let next = (column + step).min(right);
                    while segment < point_count && points[segment].x < column {
                        segment += 1;
                    }
                    let y = if column <= points[0].x || segment == 0 {
                        points[0].y
                    } else if segment >= point_count {
                        points[point_count - 1].y
                    } else {
                        let previous = points[segment - 1];
                        let point = points[segment];
                        if point.x > previous.x {
                            previous.y
                                + (point.y - previous.y)
                                    * ((column - previous.x) / (point.x - previous.x))
                                        .clamp(0.0, 1.0)
                        } else {
                            point.y
                        }
                    };
                    draw.add_rect_filled(
                        ImVec2::new(column, y),
                        ImVec2::new(next, chart_bottom),
                        fill,
                        0.0,
                    );
                    column = next;
                }
            }
        }
    }

    for (row_index, row) in rows[..dps.count].iter().enumerate() {
        let color_index = row_color(row, row_index);
        let mut points: [ImVec2; HISTORY_CAP] =
            core::array::from_fn(|_| ImVec2::new(0.0, 0.0));
        for index in 0..dps.history_len {
            let sample = &dps.history[(start + index) % HISTORY_CAP];
            let px = if dps.history_len == 1 {
                chart_left + chart_width
            } else {
                chart_left
                    + ((sample.time - first_time) / time_span).clamp(0.0, 1.0)
                        * chart_width
            };
            let value = sample.damage[color_index] as f32 / max_damage as f32;
            let py = chart_bottom - value.clamp(0.0, 1.0) * chart_height;
            points[index] = ImVec2::new(px, py);
        }
        let point_count = dps.history_len;
        let color = dps_color(color_index, 230);
        if point_count > 1 {
            draw.add_polyline(
                &points[..point_count],
                color,
                2.0 * scale,
                ImDrawFlags::None,
            );
        } else if point_count == 1 {
            draw.add_circle_filled(points[0], 2.0 * scale, color, 12);
        }
    }

    }

    let mut row_y = chart_bottom + first_row_gap;
    let right = x + width - pad;
    for (row_index, row) in rows[..dps.count].iter().enumerate() {
        let color_index = row_color(row, row_index);
        let percent_tenths = if dps.total > 0 {
            row.damage * 1000 / dps.total
        } else {
            0
        };
        let dps_tenths = if dps.elapsed > 0.1 {
            (row.damage as f32 * 10.0 / dps.elapsed) as u64
        } else {
            0
        };

        let icon_x = x + pad;
        let icon_bottom =
            row_y + row_text_offset + name_font - 2.0 * scale + 1.0 * scale;
        if config.dps_show_weapon {
            let icon_index = row_icon(row);
            if icon_index != 0xFF {
                let icon = &icons::ICONS[icon_index];
                if icon.width != 0 && icon.height != 0 {
                    draw_pixel_icon(
                        &draw,
                        icon,
                        icon_x,
                        icon_bottom,
                        icon_size,
                        dps_color(color_index, 210),
                    );
                }
            }
        }
        let name_color = if row.local { gold } else { white };
        let name_x = x + pad + icon_space;
        draw.add_text_with_font(
            &font,
            name_font,
            ImVec2::new(name_x, row_y + row_text_offset),
            name_color,
            labels[row_index].as_str(),
        );
        if config.dps_show_hits {
            let mut hit_text = plugin_config::Text::<16>::new();
            let _ = write!(&mut hit_text, "  {}h", row.hits);
            let name_baseline = row_y + row_text_offset + font_ascent(&font, name_font);
            let hit_y = name_baseline - font_ascent(&font, font_size);
            draw.add_text_with_font(
                &font,
                font_size,
                ImVec2::new(name_x + name_widths[row_index] + 2.0 * scale, hit_y),
                name_color,
                hit_text.as_str(),
            );
        }

        if config.dps_show_percent_bar {
            let bar_x = x + pad;
            let bar_w = width - pad * 2.0;
            let bar_y = if two_line {
                row_y + font_size + small_font - 1.0 * scale
            } else {
                row_y + name_bottom + 1.0 * scale
            };
            draw.add_rect_filled(
                ImVec2::new(bar_x, bar_y),
                ImVec2::new(bar_x + bar_w, bar_y + 3.0 * scale),
                imgui::color(255, 255, 255, 35),
                1.0 * scale,
            );
            let fill_w = bar_w * percent_tenths as f32 / 1000.0;
            if fill_w > 0.0 {
                draw.add_rect_filled(
                    ImVec2::new(bar_x, bar_y),
                    ImVec2::new(bar_x + fill_w, bar_y + 3.0 * scale),
                    dps_color(color_index, 220),
                    1.0 * scale,
                );
            }
        }

        let mut damage_text = plugin_config::Text::<24>::new();
        if config.dps_show_damage {
            let _ = write!(&mut damage_text, "{}", row.damage);
        }
        let mut dps_text = plugin_config::Text::<24>::new();
        if config.dps_show_dps {
            let _ = write!(
                &mut dps_text,
                "{}.{}/s",
                dps_tenths / 10,
                dps_tenths % 10
            );
        }
        let mut percent_text = plugin_config::Text::<16>::new();
        if config.dps_show_percent {
            let _ = write!(
                &mut percent_text,
                "{}.{}%",
                percent_tenths / 10,
                percent_tenths % 10
            );
        }
        let baseline = row_y + row_text_offset + font_ascent(&font, name_font);
        let inline_y = baseline - font_ascent(&font, font_size);
        let percent_column_w = if inline_stats {
            percent_reserve_w
        } else {
            max_percent_w
        };
        let stats_right = right
            - if config.dps_show_percent {
                percent_column_w + stat_gap
            } else {
                0.0
            };
        if inline_stats {
            let dps_w = if config.dps_show_dps {
                measure_text(&font, font_size, dps_text.as_str())
            } else {
                0.0
            };
            let percent_w = if config.dps_show_percent {
                measure_text(&font, font_size, percent_text.as_str())
            } else {
                0.0
            };
            let total_left = stats_right - total_col_w;
            let damage_x = total_left;
            let dps_right = if config.dps_show_damage {
                total_left - stat_gap
            } else {
                stats_right
            };
            let dps_x = dps_right - dps_w;
            let percent_x = right - percent_w;
            if config.dps_show_dps {
                draw.add_text_with_font(
                    &font,
                    font_size,
                    ImVec2::new(dps_x, inline_y),
                    white,
                    dps_text.as_str(),
                );
            }
            if config.dps_show_damage {
                draw.add_text_with_font(
                    &font,
                    font_size,
                    ImVec2::new(damage_x, inline_y),
                    white,
                    damage_text.as_str(),
                );
            }
            if config.dps_show_percent {
                draw.add_text_with_font(
                    &font,
                    font_size,
                    ImVec2::new(percent_x, inline_y),
                    white,
                    percent_text.as_str(),
                );
            }
        } else {
            let percent_w = if config.dps_show_percent {
                measure_text(&font, font_size * 0.9, percent_text.as_str())
            } else {
                0.0
            };
            let numbers_right = stats_right;
            let both_numbers = config.dps_show_damage && config.dps_show_dps;
            if config.dps_show_damage {
                let damage_w = measure_text(&font, font_size, damage_text.as_str());
                let damage_y = if both_numbers {
                    row_y
                } else {
                    baseline - font_ascent(&font, font_size)
                };
                draw.add_text_with_font(
                    &font,
                    font_size,
                    ImVec2::new(numbers_right - damage_w, damage_y),
                    white,
                    damage_text.as_str(),
                );
            }
            if config.dps_show_dps {
                let dps_size = if both_numbers { small_font } else { font_size };
                let dps_w = measure_text(&font, dps_size, dps_text.as_str());
                let dps_y = if both_numbers {
                    row_y + font_size * 0.92 - 2.0 * scale
                } else {
                    baseline - font_ascent(&font, dps_size)
                };
                draw.add_text_with_font(
                    &font,
                    dps_size,
                    ImVec2::new(numbers_right - dps_w, dps_y),
                    white,
                    dps_text.as_str(),
                );
            }
            if config.dps_show_percent {
                let percent_size = font_size * 0.9;
                let percent_y = baseline - font_ascent(&font, percent_size);
                draw.add_text_with_font(
                    &font,
                    percent_size,
                    ImVec2::new(right - percent_w, percent_y),
                    white,
                    percent_text.as_str(),
                );
            }
        }
        row_y += row_h;
    }

    if show_team {
        let team_y = row_y - team_top_shift;
        let team_red = imgui::color(235, 60, 60, 255);
        let team_right = right
            - if config.dps_show_percent {
                (if inline_stats {
                    percent_reserve_w
                } else {
                    max_percent_w
                }) + stat_gap
            } else {
                0.0
            };
        let inline_total_left = team_right - total_col_w;
        let inline_dps_right = if config.dps_show_damage {
            inline_total_left - stat_gap
        } else {
            team_right
        };
        let team_total_x = if inline_stats {
            inline_total_left
        } else {
            team_right - team_total_w
        };
        let team_baseline = team_y + row_text_offset + font_ascent(&font, name_font);
        let team_total_y = if inline_stats {
            team_baseline - font_ascent(&font, font_size)
        } else if config.dps_show_dps {
            team_y
        } else {
            team_baseline - font_ascent(&font, font_size)
        };
        if config.dps_show_damage {
            draw.add_text_with_font(
                &font,
                name_font,
                ImVec2::new(x + pad + icon_space, team_y + row_text_offset),
                team_red,
                "DMG",
            );
            draw.add_text_with_font(
                &font,
                font_size,
                ImVec2::new(team_total_x, team_total_y),
                team_red,
                team_total_text.as_str(),
            );
        }
        if config.dps_show_dps {
            let team_dps_size = if inline_stats || !config.dps_show_damage {
                font_size
            } else {
                small_font
            };
            let team_dps_y = if inline_stats || !config.dps_show_damage {
                team_baseline - font_ascent(&font, team_dps_size)
            } else {
                team_y + font_size * 0.92 - 2.0 * scale
            };
            let team_dps_x = if inline_stats {
                inline_dps_right - team_dps_w
            } else {
                team_right - team_dps_w
            };
            draw.add_text_with_font(
                &font,
                team_dps_size,
                ImVec2::new(team_dps_x, team_dps_y),
                team_red,
                team_dps_text.as_str(),
            );
        }
    }
}

fn history_start(_dps: &DpsState) -> usize {
    0
}

fn draw_dps() {
    let dps = DPS.with(|state| *state);
    if !dps.visible || !dps.active || dps.total == 0 || dps.count == 0 {
        return;
    }
    let state = STATE.with(|state| state.snapshot());
    let display = state.display;
    let config = state.config;
    draw_dps_curve(&dps, &config, display);
}

#[forge::imgui_render]
fn draw_settings() {
    // Forge 共用一个 Plugin UI 窗口；折叠标题不会替内部控件建立 ID 作用域。
    imgui::push_id(imgui::Id::Str(c"mhgu_dps_meter"));
    if imgui::tree_node(c"MHGU DPS Meter") {
        draw_settings_body();
        imgui::tree_pop();
    }
    imgui::pop_id();
}

fn draw_settings_body() {
    if !INSTALLED.load(Ordering::Acquire) {
        imgui::text("Hook addresses did not match. See Forge log.");
        return;
    }
    MENU.with(|edit| {
        let c = &mut edit.draft;
        imgui::checkbox(c"Enable DPS meter", &mut c.enabled);
        if imgui::button(c"Reset DPS", Default::default()) {
            DPS.with(|dps| dps.reset_now());
        }
        imgui::text("DPS panel");
        plugin_config::number(
            c"DPS background alpha",
            &mut c.dps_background_alpha,
            0.0,
            255.0,
        );
        imgui::checkbox(c"Show timer", &mut c.dps_show_timer);
        imgui::checkbox(c"Show damage", &mut c.dps_show_damage);
        imgui::checkbox(c"Show percent", &mut c.dps_show_percent);
        imgui::checkbox(c"Show DPS", &mut c.dps_show_dps);
        imgui::checkbox(c"Show hits", &mut c.dps_show_hits);
        plugin_config::number(c"Panel X", &mut c.dps_offset_x, 0.0, 1280.0);
        plugin_config::number(c"Panel Y", &mut c.dps_offset_y, 0.0, 720.0);
        imgui::checkbox(c"Show chart", &mut c.dps_show_curve);
        plugin_config::number(c"Curve width", &mut c.dps_curve_width, 220.0, 600.0);
        plugin_config::number(c"Curve height", &mut c.dps_curve_height, 55.0, 200.0);
        imgui::checkbox(c"Curve fill", &mut c.dps_curve_fill);
        imgui::checkbox(c"Percent bar", &mut c.dps_show_percent_bar);
        imgui::checkbox(c"Weapon icons", &mut c.dps_show_weapon);
        imgui::checkbox(c"Player names", &mut c.dps_show_names);
        imgui::checkbox(c"Inline stats", &mut c.dps_inline_stats);
        imgui::checkbox(c"Team summary", &mut c.dps_show_team);
        let mut input = FONT_INPUT.with(|b| *b);
        if imgui::input_text(c"DPS font TTF filename", &mut input, ImGuiInputTextFlags::None) {
            FONT_INPUT.with(|b| *b = input);
            edit.message = "";
            if let Ok(name) = plugin_config::input_text(&input) {
                if name == "default" {
                    c.font_file = None;
                } else if let Some(file) = FontFile::new(name) {
                    c.font_file = Some(file);
                }
            }
        }
        imgui::text("Menu changes preview live. Font changes need save and restart.");
        let reset = plugin_config::footer(
            edit,
            Config::DEFAULT,
            &plugin_config::DAMAGE,
            false,
            |config| {
                let name = plugin_config::input_text(&input)?;
                if name != "default" && FontFile::new(name).is_none() {
                    return Err("Font must be default or a .ttf filename without a directory.");
                }
                let mut text = plugin_config::Text::<CONFIG_MAX_BYTES>::new();
                config
                    .write_ini(&mut text)
                    .map_err(|_| "Configuration too large.")?;
                Config::parse(text.as_str()).map_err(|e| e.message)?;
                plugin_config::DAMAGE.save(text.as_str())
            },
        );
        if reset {
            set_font_input(edit.draft);
        }
        let config = edit.draft;
        let changed = ENABLED.swap(config.enabled, Ordering::Relaxed) != config.enabled;
        STATE.with(|state| {
            if changed {
                state.clear();
            }
            state.config = config;
        });
    });
}

fn set_font_input(config: Config) {
    FONT_INPUT.with(|b| {
        plugin_config::set_input(
            b,
            config
                .font_file
                .as_ref()
                .map_or("default", FontFile::filename),
        )
    });
}

// 游戏内存读取、本地玩家识别和坐标投影。
unsafe fn read_u32(address: u32) -> u32 {
    unsafe { ptr::read_volatile(address as *const u32) }
}

unsafe fn read_u8(address: u32) -> u8 {
    unsafe { ptr::read_volatile(address as *const u8) }
}

fn thread_key() -> u32 {
    let value: u32;
    // AArch32 TPIDRURO：只用 TLS 地址区分调用线程，不解引用 OS 内部结构。
    unsafe {
        asm!("mrc p15, 0, {value}, c13, c0, 3", value = out(reg) value, options(nostack, preserves_flags))
    };
    value
}

unsafe fn local_player(base: u32) -> u32 {
    let get: unsafe extern "C" fn(u32) -> u32 = unsafe { mem::transmute(base + 0x00276278) };
    unsafe { get(read_u32(base + 0x01885B58)) }
}

unsafe fn epoch(player: u32) -> Epoch {
    Epoch {
        player,
        room: unsafe { read_u8(player + 0x1054) },
    }
}

unsafe fn display_area(base: u32) -> DisplayArea {
    // 图形管理器 +B0/+B4 同时用于游戏绘图和 nvnWindowSetCrop 的宽高。
    // +00BC0874 将它们传给 +0086A24C；裁剪原点固定为 (0, 0)。
    // 每次投影读取当前值，掌机/主机切换后同步更新插件快照。
    let graphics = unsafe { read_u32(base + 0x02126998) };
    DisplayArea {
        width: unsafe { read_u32(graphics + 0xB0) } as f32,
        height: unsafe { read_u32(graphics + 0xB4) } as f32,
    }
}

unsafe fn damage_is_yellow(base: u32, contact: Contact) -> bool {
    // 在伤害入账时读取已完成设置的攻击字段，不能提前按武器或外观猜瓶爆。
    let flags = unsafe { read_u32(contact.attack + 0x30) };
    if flags & 0x18021 == 0 {
        return true;
    }
    let get_hitzones: unsafe extern "C" fn(u32, u32) -> u32 =
        unsafe { mem::transmute(base + 0x000BAB60) };
    let table = unsafe { get_hitzones(contact.monster, contact.part_index as u32) };
    let hitzones = unsafe { [read_u8(table), read_u8(table + 1), read_u8(table + 2)] };
    let weights = if flags & 0x20 != 0 {
        let player = contact.epoch.player;
        let mut weights = unsafe { [read_u8(player + 0x2B70), read_u8(player + 0x2B71)] };
        // +13CC bit2 的原结算分支只传入本次指定的斩或打权重。
        if unsafe { read_u8(player + 0x13CC) } & 4 != 0 {
            if unsafe { read_u32(contact.attack + 0x40) } & 2 != 0 {
                weights[1] = 0;
            } else {
                weights[0] = 0;
            }
        }
        weights
    } else if flags & 0x10000 != 0 {
        if unsafe { read_u32(contact.attack + 0x40) } & 2 != 0 {
            [100, 0]
        } else {
            [0, 100]
        }
    } else {
        // +30 的 0x8000 分支给斩、打都传入 100；弹分支不使用这两个值。
        [100, 100]
    };
    yellow_for_hitzone(flags, hitzones, weights)
}

impl Projection {
    pub unsafe fn current(base: u32) -> Self {
        let manager = unsafe { read_u32(base + 0x0184D750) };
        let ui = unsafe { read_u32(base + 0x0212668C) };
        let view = unsafe { read_u8(ui + 0xEC) } as u32;
        let record = manager + view * 0x1B0;
        Self {
            function: unsafe { mem::transmute(base + 0x00018A48) },
            manager,
            view,
            width: (unsafe { read_u32(record + 0x70) } as i32
                - unsafe { read_u32(record + 0x68) } as i32) as f32,
            height: (unsafe { read_u32(record + 0x74) } as i32
                - unsafe { read_u32(record + 0x6C) } as i32) as f32,
        }
    }

    pub unsafe fn project(&self, world: [f32; 3]) -> Option<[f32; 2]> {
        let world = Vector([world[0], world[1], world[2], 0.0]);
        let mut projected = Vector([0.0; 4]);
        unsafe {
            (self.function)(
                projected.0.as_mut_ptr(),
                self.manager,
                world.0.as_ptr(),
                self.view,
            )
        };
        normalized_screen(projected.0, self.width, self.height)
    }
}

fn compatible(base: u32) -> bool {
    for (offset, expected) in [
        (0x00170968, 0xE92D41F0),
        (0x0017096C, 0xE24DD010),
        (0x0009D6F8, 0xE92D4BF0),
        (0x0009D6FC, 0xED2D8B02),
        (0x00018A48, 0xE92D4FF0),
        (0x00018A4C, 0xE28DB01C),
        (0x00276278, 0xE92D4010),
        (0x0027627C, 0xE1A04000),
        (0x000BAB60, 0xE3510008),
        (0x000BAB64, 0x3A000002),
        (0x0086A24C, 0xE92D4800),
        (0x0086A250, 0xE24DD008),
    ] {
        let actual = unsafe { read_u32(base + offset) };
        if actual != expected {
            error!(
                "mhgu-dps-meter: main+0x{offset:08X}=0x{actual:08X}, expected 0x{expected:08X}; hooks skipped"
            );
            return false;
        }
    }
    true
}

// 当前加载器的 76 字节 Hook 返回类型在 data.rs 定义。
unsafe extern "C" {
    #[link_name = "forge_hook_create"]
    fn create_hook_raw(
        output: *mut RuntimeHook,
        target: u32,
        detour: u32,
        original: *mut u32,
    ) -> *mut RuntimeHook;
}

// 每次原命中调用绑定栈上下文；嵌套命中结束后恢复上一层。
// 显式清零最大 SRET 缓冲，兼容当前 24 字节与旧版 76 字节返回区。
unsafe fn create_hook(target: u32, detour: u32, original: *mut u32) -> RuntimeHook {
    let mut hook: RuntimeHook = unsafe { core::mem::zeroed() };
    unsafe { create_hook_raw(&raw mut hook, target, detour, original) };
    hook
}

unsafe fn install_hooks(base: u32) {
    // 由加载器初始化阶段安装；整个进程生命周期内保留两个 JIT 跳板。
    unsafe {
        ptr::write(
            &raw mut DAMAGE_HOOK,
            MaybeUninit::new(create_hook(
                base + 0x0009D6F8,
                damage as *const () as u32,
                &raw mut DAMAGE_ORIGINAL,
            )),
        );
        ptr::write(
            &raw mut COLLISION_HOOK,
            MaybeUninit::new(create_hook(
                base + 0x00170968,
                collision as *const () as u32,
                &raw mut COLLISION_ORIGINAL,
            )),
        );
    }
}

unsafe extern "C" fn collision(
    check: u32,
    attack: u32,
    monster: u32,
    point: u32,
    body: u32,
    attack_point: u32,
    part: u32,
    shape: u32,
) -> u32 {
    let base = GAME_BASE.load(Ordering::Relaxed);
    let mut contact = Contact {
        enabled: false,
        local: false,
        log: false,
        attack,
        monster,
        slot: 0,
        part_index: 0,
        part: 0,
        owner: 0,
        player_slot: 0xFF,
        forwarded: false,
        epoch: Epoch { player: 0, room: 0 },
        world: [0.0; 3],
    };
    if ENABLED.load(Ordering::Relaxed) && unsafe { read_u8(attack + 0x6D) } == 1 {
        let forwarded = unsafe { read_u8(attack + 0x10) } != 0;
        if !forwarded || unsafe { read_u8(attack + 0x6E) } == 0 {
            let owner = unsafe { read_u32(attack + if forwarded { 0x70 } else { 0x0C }) };
            let player = unsafe { local_player(base) };
            contact.local = owner == player && player >= 0x10000;
            contact.enabled = contact.local;
            contact.log = true;
            contact.owner = owner;
            contact.forwarded = forwarded;
            contact.part = part;
            contact.player_slot = 0xFF;
            let manager = unsafe { read_u32(base + 0x01885B58) };
            if manager >= 0x10000 {
                for i in 0..4u32 {
                    if unsafe { read_u32(manager + 0x1C + i * 4) } == owner {
                        contact.player_slot = i as u8;
                    }
                }
            }
            contact.slot = unsafe { read_u8(part + 8) } as u32;
            contact.part_index = unsafe { read_u8(part + 6) };
            contact.epoch = if player >= 0x10000 {
                unsafe { epoch(player) }
            } else {
                Epoch { player, room: 0 }
            };
            contact.world = unsafe { ptr::read_unaligned(point as *const [f32; 3]) };
            TOTAL_HITS.fetch_add(1, Ordering::Relaxed);
            DEBUG_LOG.with(|log| log.owner_snapshot(owner, player));
        }
    }
    let thread = thread_key();
    let binding = BINDINGS.with(|bindings| {
        let index = bindings
            .0
            .iter()
            .position(|entry| entry.thread == thread)
            .or_else(|| bindings.0.iter().position(|entry| entry.thread == 0))?;
        let previous = bindings.0[index];
        bindings.0[index] = Binding {
            thread,
            contact: &contact,
        };
        Some((index, previous))
    });
    let original: unsafe extern "C" fn(u32, u32, u32, u32, u32, u32, u32, u32) -> u32 =
        unsafe { mem::transmute(COLLISION_ORIGINAL) };
    let result = unsafe {
        original(
            check,
            attack,
            monster,
            point,
            body,
            attack_point,
            part,
            shape,
        )
    };
    if let Some((index, previous)) = binding {
        BINDINGS.with(|bindings| bindings.0[index] = previous);
    }
    result
}

unsafe extern "C" fn damage(monster: u32, slot: u32, raw: i32, mode: i32) -> i32 {
    let original: unsafe extern "C" fn(u32, u32, i32, i32) -> i32 =
        unsafe { mem::transmute(DAMAGE_ORIGINAL) };
    let result = unsafe { original(monster, slot, raw, mode) };
    if raw > 0 && result > 0 && mode != 2 && slot < 8 && ENABLED.load(Ordering::Relaxed) {
        let thread = thread_key();
        let contact = BINDINGS.with(|bindings| {
            bindings
                .0
                .iter()
                .find(|entry| entry.thread == thread)
                .map(|entry| unsafe { *entry.contact })
        });
        if let Some(contact) = contact {
            if contact.log && contact.monster == monster && contact.slot == slot {
                DEBUG_LOG.with(|log| log.hit(&contact, result, raw, mode));
                DPS.with(|dps| dps.add(contact.owner, contact.local, result, contact.player_slot));
                if contact.local {
                    let yellow =
                        unsafe { damage_is_yellow(GAME_BASE.load(Ordering::Relaxed), contact) };
                    STATE.with(|state| {
                        if ENABLED.load(Ordering::Relaxed)
                            && state.push(contact.epoch, contact.world, result, yellow)
                        {
                            TOTAL_EVENTS.fetch_add(1, Ordering::Relaxed);
                        }
                    });
                }
            }
        }
    }
    result
}
