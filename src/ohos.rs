// Copyright 2019-2024 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

//! OHOS platform implementation for window vibrancy effects.
//!
//! Uses `openharmony-ability` as the platform SDK (analogous to `windows-sys` on Windows
//! and `objc2-app-kit` on macOS). Applies blur via ArkTS `backdropBlur(radius)` component
//! attribute on the WebView container.

use crate::{Color, Error};

/// Applies blur effect to an OHOS window.
///
/// `window_id` is the OHOS window ID (0 = main window, positive = sub-window).
/// `radius` is the blur radius in pixels (0 = no blur).
pub fn apply_ohos_blur(window_id: i64, radius: f64) -> Result<(), Error> {
    openharmony_ability::set_window_blur(window_id, radius)
        .map_err(|e| Error::OhosError(format!("{}", e)))
}

/// Clears blur effect from an OHOS window.
/// Also resets background color to transparent, so acrylic/mica tints don't persist after clearEffects.
/// Safe for blur-only: apply_ohos_blur doesn't set backgroundColor, so resetting to transparent is a no-op.
pub fn clear_ohos_blur(window_id: i64) -> Result<(), Error> {
    openharmony_ability::set_window_blur(window_id, 0.0)
        .map_err(|e| Error::OhosError(format!("{}", e)))?;
    openharmony_ability::set_window_background_color(window_id, 0x00000000)
        .map_err(|e| Error::OhosError(format!("{}", e)))
}

/// Applies acrylic-like effect to an OHOS window.
///
/// Combines blur with a semi-transparent background color.
pub fn apply_ohos_acrylic(
    window_id: i64,
    radius: f64,
    color: Option<Color>,
) -> Result<(), Error> {
    // Apply blur first
    openharmony_ability::set_window_blur(window_id, radius)
        .map_err(|e| Error::OhosError(format!("{}", e)))?;

    // Apply semi-transparent background color for acrylic effect
    let (r, g, b, a) = color.unwrap_or((0, 0, 0, 204)); // Default: 80% opaque black
    let argb = ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
    openharmony_ability::set_window_background_color(window_id, argb)
        .map_err(|e| Error::OhosError(format!("{}", e)))
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
    // Apply blur
    openharmony_ability::set_window_blur(window_id, radius)
        .map_err(|e| Error::OhosError(format!("{}", e)))?;

    // Apply background tint based on dark mode preference
    if let Some(is_dark) = dark {
        let argb = if is_dark {
            0xE6000000 // 90% opaque black
        } else {
            0xE6FFFFFF // 90% opaque white
        };
        openharmony_ability::set_window_background_color(window_id, argb)
            .map_err(|e| Error::OhosError(format!("{}", e)))?;
    }

    Ok(())
}

/// Clears mica effect from an OHOS window.
pub fn clear_ohos_mica(window_id: i64) -> Result<(), Error> {
    clear_ohos_blur(window_id)
}
