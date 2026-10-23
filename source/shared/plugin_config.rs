//! 独立插件共用的存档根目录配置与菜单控件；各插件保留自己的数据和游戏逻辑。
#![allow(dead_code)]

use core::{
    cell::UnsafeCell,
    ffi::CStr,
    fmt::{self, Write},
    sync::atomic::{AtomicBool, Ordering},
};
use forge::{
    imgui,
    sys::imgui::{ImGuiColorEditFlags, ImGuiInputTextFlags},
};

#[path = "config_storage.rs"]
mod storage;
pub use storage::*;

pub struct Text<const N: usize> {
    bytes: [u8; N],
    len: usize,
}
impl<const N: usize> Text<N> {
    pub const fn new() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
        }
    }
    pub fn as_str(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(&self.bytes[..self.len]) }
    }
}
impl<const N: usize> Write for Text<N> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self.len + text.len();
        if end > N {
            return Err(fmt::Error);
        }
        self.bytes[self.len..end].copy_from_slice(text.as_bytes());
        self.len = end;
        Ok(())
    }
}

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
    pub fn with<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        struct Guard<'a>(&'a AtomicBool);
        impl Drop for Guard<'_> {
            fn drop(&mut self) {
                self.0.store(false, Ordering::Release);
            }
        }
        let _guard = Guard(&self.locked);
        f(unsafe { &mut *self.value.get() })
    }
}
pub struct Edit<C> {
    pub draft: C,
    pub saved: C,
    pub message: &'static str,
}
impl<C: Copy> Edit<C> {
    pub const fn new(config: C) -> Self {
        Self {
            draft: config,
            saved: config,
            message: "",
        }
    }
}
// 返回 true 表示恢复／默认值改变了草稿，调用者据此更新字体名等输入缓冲。
pub fn footer<C: Copy + PartialEq>(
    edit: &mut Edit<C>,
    defaults: C,
    location: &Location,
    restart: bool,
    save: impl FnOnce(&C) -> Result<(), &'static str>,
) -> bool {
    imgui::separator();
    imgui::text(location.display);
    if edit.draft != edit.saved {
        imgui::text("Unsaved changes.");
    }
    if imgui::button(c"Save INI", Default::default()) {
        edit.message = match save(&edit.draft) {
            Ok(()) => {
                edit.saved = edit.draft;
                if restart {
                    "Saved. Restart the game to apply."
                } else {
                    "Saved to MHGU save INI."
                }
            }
            Err(message) => message,
        };
    }
    let mut reset = false;
    imgui::same_line(0.0, -1.0);
    if imgui::button(c"Revert changes", Default::default()) {
        edit.draft = edit.saved;
        edit.message = "Edits reverted.";
        reset = true;
    }
    imgui::same_line(0.0, -1.0);
    if imgui::button(c"Defaults", Default::default()) {
        edit.draft = defaults;
        edit.message = "Defaults selected. Save INI to keep them.";
        reset = true;
    }
    imgui::text(edit.message);
    reset
}
pub fn number(label: &CStr, value: &mut f32, min: f32, max: f32) {
    let old = *value;
    if imgui::input_float(label, value, 1.0, 10.0, ImGuiInputTextFlags::None) {
        *value = if value.is_finite() {
            value.clamp(min, max)
        } else {
            old
        };
    }
}
pub fn color4(label: &CStr, color: &mut [u8; 4]) {
    let mut rgba = color.map(|v| v as f32 / 255.0);
    let flags = ImGuiColorEditFlags::DisplayHex
        | ImGuiColorEditFlags::Uint8
        | ImGuiColorEditFlags::InputRGB
        | ImGuiColorEditFlags::NoOptions;
    if imgui::color_edit4(label, &mut rgba, flags) {
        *color = rgba.map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
    }
}
pub fn set_input(buffer: &mut [u8], text: &str) {
    buffer.fill(0);
    buffer[..text.len()].copy_from_slice(text.as_bytes());
}
pub fn input_text(buffer: &[u8]) -> Result<&str, &'static str> {
    CStr::from_bytes_until_nul(buffer)
        .map_err(|_| "Input exceeds the buffer.")?
        .to_str()
        .map_err(|_| "Input must be UTF-8.")
}
