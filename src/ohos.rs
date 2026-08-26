// Copyright 2019-2024 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

//! OHOS platform implementation for window vibrancy effects.
//!
//! Uses `openharmony-ability-plugin-window` facade (`WindowClient`) as the platform SDK.
//! Applies blur via the bridge plugin model (action `set-blur`) and background color
//! via action `set-background-color`.
//!
//! The `WindowClient` is initialized once via [`set_ohos_app`] and stored in a global
//! `OnceCell`, mirroring the tray-icon initialization pattern.

use crate::{Color, Error};

use openharmony_ability_plugin_window::WindowClient;
use std::sync::OnceLock;

static WINDOW_CLIENT: OnceLock<WindowClient> = OnceLock::new();

/// Initialize the vibrancy facade with the application's `OpenHarmonyApp`.
///
/// Must be called once during app setup (before any vibrancy function is invoked).
/// The `WindowClient` is created from the app's bridge runtime and stored globally.
pub fn set_ohos_app(app: &openharmony_ability::OpenHarmonyApp) {
    let client = WindowClient::new(app)
        .expect("Failed to create WindowClient for vibrancy");
    eprintln!("[vibrancy] set_ohos_app: WindowClient created OK");
    if WINDOW_CLIENT.set(client).is_err() {
        eprintln!("[vibrancy] WINDOW_CLIENT already initialized; ignoring duplicate call");
    } else {
        eprintln!("[vibrancy] WINDOW_CLIENT initialized (first time)");
    }
}

fn client() -> Result<&'static WindowClient, Error> {
    WINDOW_CLIENT
        .get()
        .ok_or_else(|| Error::OhosError("vibrancy WindowClient not initialized (call set_ohos_app first)".to_string()))
}

/// Runs an async bridge call on a background thread **without waiting for the
/// result** (fire-and-forget).
///
/// ## Why fire-and-forget?
///
/// The bridge infrastructure (`call_raw` in `bridge/mod.rs`) sends a TSFN request
/// to ArkTS and then `receiver.await`s the response. The TSFN callback is
/// dispatched by the ArkTS runtime on the **main thread's JS event loop**.
///
/// If the main thread blocks (via `block_on` OR `recv()`), the callback can never
/// fire → THREAD_BLOCK deadlock (appfreeze THREAD_BLOCK_6S). The bridge source
/// explicitly documents this constraint:
/// > "必须从 worker 线程调用 —— N-API 主线程在等待其自身的 TSFN 队列时会死锁"
///
/// Previous attempt: moved `block_on` to a worker thread and `recv()`'d the
/// result on the main thread. This still deadlocked because `recv()` blocks the
/// main thread, preventing the TSFN callback from being dispatched.
///
/// **Solution**: spawn the bridge call on a background thread and do NOT wait.
/// Vibrancy effects (blur, background color) are visual-only — they don't need
/// synchronous error reporting. The background thread runs `block_on` while the
/// main thread's event loop stays free to dispatch the TSFN callback. The future
/// completes normally on the background thread; the result is silently discarded.
fn block_bridge<F, T, E>(fut: F) -> Result<(), Error>
where
    F: std::future::Future<Output = std::result::Result<T, E>> + Send + 'static,
    T: Send + 'static,
    E: std::fmt::Display + Send + 'static,
{
    std::thread::spawn(move || {
        let result = futures_executor::block_on(fut);
        match &result {
            Ok(_) => eprintln!("[vibrancy] bridge call OK (fire-and-forget)"),
            Err(e) => eprintln!("[vibrancy] bridge call failed (fire-and-forget): {}", e),
        }
    });
    Ok(())
}

/// Applies blur effect to an OHOS window.
///
/// `window_id` is the OHOS window ID (0 = main window, positive = sub-window).
/// `radius` is the blur radius in pixels (0 = no blur).
pub fn apply_ohos_blur(window_id: i64, radius: f64) -> Result<(), Error> {
    eprintln!("[vibrancy] apply_ohos_blur: window_id={} radius={}", window_id, radius);
    block_bridge(client()?.set_window_blur(window_id, radius))
}

/// Clears blur effect from an OHOS window.
/// Also resets background color to transparent, so acrylic/mica tints don't persist after clearEffects.
/// Safe for blur-only: apply_ohos_blur doesn't set backgroundColor, so resetting to transparent is a no-op.
pub fn clear_ohos_blur(window_id: i64) -> Result<(), Error> {
    let c = client()?;
    std::thread::spawn(move || {
        if let Err(e) = futures_executor::block_on(c.set_window_blur(window_id, 0.0)) {
            eprintln!("[vibrancy] clear blur failed: {}", e);
            return;
        }
        if let Err(e) = futures_executor::block_on(c.set_window_background_color(window_id, 0x00000000)) {
            eprintln!("[vibrancy] clear background failed: {}", e);
        }
    });
    Ok(())
}

/// Applies acrylic-like effect to an OHOS window.
///
/// Combines blur with a semi-transparent background color.
pub fn apply_ohos_acrylic(
    window_id: i64,
    radius: f64,
    color: Option<Color>,
) -> Result<(), Error> {
    let c = client()?;
    let argb = acrylic_argb(color);
    std::thread::spawn(move || {
        // Sequential: blur first, then background color
        if let Err(e) = futures_executor::block_on(c.set_window_blur(window_id, radius)) {
            eprintln!("[vibrancy] acrylic blur failed: {}", e);
            return;
        }
        if let Err(e) = futures_executor::block_on(c.set_window_background_color(window_id, argb)) {
            eprintln!("[vibrancy] acrylic background failed: {}", e);
        }
    });
    Ok(())
}

/// Computes the acrylic background ARGB color from an optional user-provided RGBA.
/// Falls back to the default semi-transparent black (0, 0, 0, 204) when `color` is `None`.
fn acrylic_argb(color: Option<Color>) -> u32 {
    let (r, g, b, a) = color.unwrap_or((0, 0, 0, 204));
    to_argb(r, g, b, a)
}

/// Computes the optional mica tint ARGB color. `None` means no tint is applied.
/// `Some(true)` → dark tint (0xE6000000), `Some(false)` → light tint (0xE6FFFFFF).
fn mica_tint_argb(dark: Option<bool>) -> Option<u32> {
    dark.map(|is_dark| if is_dark { 0xE6000000u32 } else { 0xE6FFFFFFu32 })
}

/// Pack RGBA color components into a single u32 in ARGB format (alpha in high byte).
fn to_argb(r: u8, g: u8, b: u8, a: u8) -> u32 {
    ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

/// Clears acrylic effect from an OHOS window.
pub fn clear_ohos_acrylic(window_id: i64) -> Result<(), Error> {
    clear_ohos_blur(window_id)
}

/// Applies mica-like effect to an OHOS window.
///
/// Combines blur with an optional dark/light background tint.
pub fn apply_ohos_mica(
    window_id: i64,
    radius: f64,
    dark: Option<bool>,
) -> Result<(), Error> {
    let c = client()?;
    let tint_argb = mica_tint_argb(dark);
    std::thread::spawn(move || {
        // Sequential: blur first, then tint
        if let Err(e) = futures_executor::block_on(c.set_window_blur(window_id, radius)) {
            eprintln!("[vibrancy] mica blur failed: {}", e);
            return;
        }
        if let Some(argb) = tint_argb {
            if let Err(e) = futures_executor::block_on(c.set_window_background_color(window_id, argb)) {
                eprintln!("[vibrancy] mica tint failed: {}", e);
            }
        }
    });
    Ok(())
}

/// Clears mica effect from an OHOS window.
pub fn clear_ohos_mica(window_id: i64) -> Result<(), Error> {
    clear_ohos_blur(window_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_argb_packs_components_correctly() {
        // Standard case: r=255, g=128, b=0, a=200
        let result = to_argb(255, 128, 0, 200);
        assert_eq!(result, 0xC8FF8000);
    }

    #[test]
    fn to_argb_max_values() {
        let result = to_argb(255, 255, 255, 255);
        assert_eq!(result, 0xFFFFFFFF);
    }

    #[test]
    fn to_argb_zero_values() {
        let result = to_argb(0, 0, 0, 0);
        assert_eq!(result, 0x00000000);
    }

    #[test]
    fn to_argb_alpha_in_high_byte() {
        // Alpha should be the most significant byte
        let result = to_argb(0, 0, 0, 1);
        assert_eq!(result, 0x01000000);
    }

    #[test]
    fn to_argb_red_in_second_byte() {
        let result = to_argb(1, 0, 0, 0);
        assert_eq!(result, 0x00010000);
    }

    #[test]
    fn to_argb_green_in_third_byte() {
        let result = to_argb(0, 1, 0, 0);
        assert_eq!(result, 0x00000100);
    }

    #[test]
    fn to_argb_blue_in_low_byte() {
        let result = to_argb(0, 0, 1, 0);
        assert_eq!(result, 0x00000001);
    }

    #[test]
    fn to_argb_acrylic_default_color() {
        // Default acrylic color: (0, 0, 0, 204) = 0xCC000000
        let result = to_argb(0, 0, 0, 204);
        assert_eq!(result, 0xCC000000);
    }

    #[test]
    fn to_argb_mica_dark_tint() {
        // Mica dark tint: 0xE6000000
        let result = to_argb(0, 0, 0, 0xE6);
        assert_eq!(result, 0xE6000000);
    }

    #[test]
    fn to_argb_mica_light_tint() {
        // Mica light tint: 0xE6FFFFFF
        let result = to_argb(255, 255, 255, 0xE6);
        assert_eq!(result, 0xE6FFFFFF);
    }

    // ── acrylic_argb ──────────────────────────────────────────────────────

    #[test]
    fn acrylic_argb_default_color() {
        // Default: (0, 0, 0, 204) → 0xCC000000
        assert_eq!(acrylic_argb(None), 0xCC000000);
    }

    #[test]
    fn acrylic_argb_custom_color() {
        // Custom: (255, 128, 0, 200)
        assert_eq!(acrylic_argb(Some((255, 128, 0, 200))), 0xC8FF8000);
    }

    #[test]
    fn acrylic_argb_transparent() {
        assert_eq!(acrylic_argb(Some((0, 0, 0, 0))), 0x00000000);
    }

    #[test]
    fn acrylic_argb_opaque_white() {
        assert_eq!(acrylic_argb(Some((255, 255, 255, 255))), 0xFFFFFFFF);
    }

    // ── mica_tint_argb ────────────────────────────────────────────────────

    #[test]
    fn mica_tint_none_yields_none() {
        assert_eq!(mica_tint_argb(None), None);
    }

    #[test]
    fn mica_tint_dark() {
        assert_eq!(mica_tint_argb(Some(true)), Some(0xE6000000));
    }

    #[test]
    fn mica_tint_light() {
        assert_eq!(mica_tint_argb(Some(false)), Some(0xE6FFFFFF));
    }
}
