use std::cell::RefCell;
use std::rc::Rc;

use anyhow::{Result, anyhow, bail};
use gpui::{FnRendererFactory, GpuPreference, PlatformRenderer, RendererTarget, WindowOptions};
use gpui_wgpu::{WgpuRenderer, WgpuSurfaceConfig};
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
    RawWindowHandle, Win32WindowHandle, WindowHandle,
};

#[derive(Clone, Copy, Debug)]
struct WindowHandles(Win32WindowHandle);

impl HasWindowHandle for WindowHandles {
    #[allow(unsafe_code, reason = "Reborrow the HWND from GPUI's renderer factory")]
    fn window_handle(&self) -> std::result::Result<WindowHandle<'_>, HandleError> {
        // SAFETY: GPUI holds the HWND throughout the window renderer's lifetime. This
        // wrapper is only created inside that window's renderer factory and passed to wgpu.
        Ok(unsafe { WindowHandle::borrow_raw(self.0.into()) })
    }
}

impl HasDisplayHandle for WindowHandles {
    fn display_handle(&self) -> std::result::Result<DisplayHandle<'_>, HandleError> {
        Ok(DisplayHandle::windows())
    }
}

pub(crate) fn with_wgpu(options: WindowOptions) -> WindowOptions {
    options.with_renderer_factory(FnRendererFactory(create_renderer))
}

fn create_renderer(target: RendererTarget<'_>) -> Result<Box<dyn PlatformRenderer>> {
    let Some(RawWindowHandle::Win32(window)) = target.window_handle else {
        bail!("GPUI did not provide a Win32 HWND");
    };
    if !matches!(target.display_handle, Some(RawDisplayHandle::Windows(_))) {
        return Err(anyhow!("GPUI did not provide a Windows display handle"));
    }
    let renderer = WgpuRenderer::new(
        Rc::new(RefCell::new(None)),
        &WindowHandles(window),
        WgpuSurfaceConfig {
            size: target.size.to_device_pixels(target.scale_factor),
            transparent: false,
            preferred_present_mode: None,
        },
        None,
        GpuPreference::default(),
    )?;
    Ok(Box::new(renderer))
}
