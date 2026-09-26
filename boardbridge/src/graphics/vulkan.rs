// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! Vulkan backend — **interface only, not implemented**.
//!
//! ## Why the seam exists now
//!
//! Minecraft Java Edition is migrating its renderer from OpenGL to Vulkan
//! (Mojang announced the switch as part of the "Vibrant Visuals" work), and
//! Android is the one platform where the launcher gets to choose the swapchain
//! instead of inheriting one from a desktop driver. Putting the seam in place
//! now means the migration is a new `impl GraphicsBackend` rather than a rewrite
//! of the surface lifecycle, the input layer and the JNI shell.
//!
//! ## What is *not* claimed
//!
//! Nothing in this file works. Every entry point returns
//! [`Error::BackendUnavailable`] and [`GraphicsBackend::status`] reports
//! [`BackendStatus::InterfaceOnly`], so the launcher UI and logcat can say
//! "Vulkan: not implemented" honestly. There is no fake success path.
//!
//! ## Exact remaining work
//!
//! 1. **Loader** — `dlopen("libvulkan.so.1")` with a fallback to
//!    `libvulkan.so`, resolve `vkGetInstanceProcAddr`, and build the dispatch
//!    table. Android's loader is not the same as a desktop one: the ICD is
//!    loaded by the platform, and `VK_LOADER_LAYERS_*` behavior differs.
//! 2. **Instance/device** — `VkApplicationInfo` with `apiVersion` negotiated
//!    from `vkEnumerateInstanceVersion`, one graphics+present queue family,
//!    `VK_KHR_swapchain` (or `VK_ANDROID_external_memory_android_hardware_buffer`
//!    if the game renders externally).
//! 3. **Surface** — `vkCreateAndroidSurfaceKHR` from the same
//!    `ANativeWindow` the EGL path already owns; swapchain creation must be
//!    redone on every `bind_window` (rotation invalidates it) with
//!    `oldSwapchain` where supported, and `VK_ERROR_OUT_OF_DATE_KHR` /
//!    `VK_SUBOPTIMAL_KHR` handled on present.
//! 4. **Threading** — one queue-owning thread, or the same owner/fence protocol
//!    this module's sibling implements ([`crate::graphics::gles`]): a Vulkan
//!    queue is externally synchronized, so the "one owner at a time" rule
//!    becomes "one submitting thread at a time".
//! 5. **LWJGL compatibility** — `VK_KHR_*` extensions Minecraft asks for must
//!    be advertised as available; the bridge does not implement Vulkan for the
//!    game, it provides the surface/instance environment (the same division as
//!    EGL, where the game brings its own GL calls).
//! 6. **Diagnostics** — a `None` diagnostic mode for Vulkan (no clear-colour
//!    renderer) or a tiny compute-to-image path for the self-test.
//!
//! Until (1)–(3) exist, `RendererKind::Vulkan` deliberately stays selectable but
//! fails fast at initialization with that list referenced in the log line.

use crate::android::surface::{OwnedNativeWindow, SurfaceSize};
use crate::bb_debug;
use crate::error::{Error, Result};
use crate::graphics::{
    BackendStatus, GraphicsBackend, GraphicsConfig, GraphicsStats, RendererInfo, RendererKind,
    ThreadRole,
};

/// What remains to be implemented, quoted in diagnostics output.
pub const REMAINING_WORK: &str = "Vulkan: instance/device/swapchain and the libvulkan loader are not implemented (see docs/GRAPHICS.md)";

/// Vulkan backend placeholder.
#[derive(Debug)]
pub struct VulkanBackend {
    config: GraphicsConfig,
}

impl VulkanBackend {
    /// Creates the placeholder; no Vulkan call is made until `initialize`.
    pub fn new(config: GraphicsConfig) -> VulkanBackend {
        VulkanBackend { config }
    }

    /// The configuration this backend would use.
    pub fn config(&self) -> &GraphicsConfig {
        &self.config
    }
}

impl GraphicsBackend for VulkanBackend {
    fn name(&self) -> &'static str {
        "Vulkan"
    }

    fn kind(&self) -> RendererKind {
        RendererKind::Vulkan
    }

    fn status(&self) -> BackendStatus {
        BackendStatus::InterfaceOnly {
            remaining: REMAINING_WORK,
        }
    }

    fn describe(&self) -> String {
        REMAINING_WORK.to_string()
    }

    fn initialize(&self) -> Result<()> {
        bb_debug!("{REMAINING_WORK}");
        Err(Error::BackendUnavailable(REMAINING_WORK))
    }

    fn bind_window(
        &self,
        window: OwnedNativeWindow,
        _requested: SurfaceSize,
    ) -> Result<SurfaceSize> {
        // `window` is moved in and dropped here: the ANativeWindow reference is
        // released rather than leaked by an unimplemented backend.
        drop(window);
        Err(Error::BackendUnavailable(REMAINING_WORK))
    }

    fn unbind_window(&self) -> Result<()> {
        // Nothing was ever bound, so unbinding is trivially satisfied.
        Ok(())
    }

    fn has_window(&self) -> bool {
        false
    }

    fn binding_generation(&self) -> u64 {
        0
    }

    fn make_current(&self, _role: ThreadRole) -> Result<()> {
        Err(Error::BackendUnavailable(REMAINING_WORK))
    }

    fn release_current(&self) -> Result<()> {
        Ok(())
    }

    fn present(&self) -> Result<()> {
        Err(Error::BackendUnavailable(REMAINING_WORK))
    }

    fn window_size(&self) -> SurfaceSize {
        SurfaceSize::default()
    }

    fn refresh_window_size(&self) -> SurfaceSize {
        SurfaceSize::default()
    }

    fn renderer_info(&self) -> Result<RendererInfo> {
        Err(Error::BackendUnavailable(REMAINING_WORK))
    }

    fn stats(&self) -> GraphicsStats {
        GraphicsStats::default()
    }

    fn set_swap_interval(&self, _interval: i32) -> Result<()> {
        // Present modes (`VK_PRESENT_MODE_FIFO_KHR` vs `MAILBOX_KHR`) will play
        // the role of the swap interval here.
        Err(Error::BackendUnavailable(REMAINING_WORK))
    }

    fn shutdown(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vulkan_reports_itself_as_interface_only() {
        let backend = VulkanBackend::new(GraphicsConfig::default());
        assert_eq!(backend.kind(), RendererKind::Vulkan);
        assert!(!backend.status().is_implemented());
        assert!(backend.status().remaining().unwrap().contains("libvulkan"));
        assert!(backend.initialize().is_err());
        assert!(!backend.has_window());
        assert_eq!(backend.stats(), GraphicsStats::default());
    }
}
