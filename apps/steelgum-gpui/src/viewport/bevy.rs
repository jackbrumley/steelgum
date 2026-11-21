//! A same-device Bevy render target sampled directly by the GPUI canvas.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use bevy::app::{App as BevyApp, SubApps, TerminalCtrlCHandlerPlugin};
use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::ecs as bevy_ecs;
use bevy::input::InputPlugin;
use bevy::prelude::*;
use bevy::render::pipelined_rendering::PipelinedRenderingPlugin;
use bevy::render::render_asset::RenderAssets;
use bevy::render::renderer::{
    RenderAdapter, RenderAdapterInfo, RenderDevice, RenderInstance, RenderQueue,
};
use bevy::render::settings::RenderCreation;
use bevy::render::texture::GpuImage;
use bevy::render::view::DebandDither;
use bevy::render::{Render, RenderApp, RenderPlugin, RenderSystems};
use bevy::window::ExitCondition;
use gpui::prelude::*;
use gpui::{AnyElement, Corners, GpuCanvasContext, div, gpu_canvas, rgb};
use gpui_interop::{
    SurfaceButton, SurfaceEvent, SurfaceInput, SurfacePoint, SurfaceScroll, surface_input,
};
use gpui_wgpu::{ImportedTextureExt as _, WgpuRenderer};

use super::View;

#[derive(Clone, Default, Resource)]
struct RenderedFrame(Arc<Mutex<Option<wgpu::TextureView>>>);

#[derive(Clone, Resource)]
struct Target(Handle<Image>);

#[derive(Component)]
struct SceneMesh;

#[derive(Component)]
struct ReviewCamera;

#[derive(Resource)]
struct Cube(Handle<Mesh>);

#[derive(Clone, Copy, PartialEq)]
struct Orbit {
    yaw: f32,
    pitch: f32,
    zoom: f32,
}

impl Default for Orbit {
    fn default() -> Self {
        Self {
            yaw: 0.57,
            pitch: 0.78,
            zoom: 1.0,
        }
    }
}

struct Renderer {
    apps: SubApps,
    frame: RenderedFrame,
    size: [u32; 2],
    displayed: Option<View>,
    orbit: Orbit,
}

impl Renderer {
    fn new(gpu: &mut GpuCanvasContext, size: [u32; 2]) -> Result<Self> {
        let device = gpu.try_device::<WgpuRenderer>();
        #[cfg(all(test, target_os = "linux"))]
        let device = device.or_else(|| gpu.try_device::<gpui_wgpu::WgpuHeadlessRenderer>());
        let (device, queue) =
            device.ok_or_else(|| anyhow!("GPUI is not using the wgpu renderer"))?;
        let creation = render_resources(&device, &queue)?;

        let mut app = BevyApp::new();
        app.add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: ExitCondition::DontExit,
                    ..default()
                })
                .set(RenderPlugin {
                    render_creation: creation,
                    synchronous_pipeline_compilation: true,
                    ..default()
                })
                .disable::<PipelinedRenderingPlugin>()
                .disable::<TerminalCtrlCHandlerPlugin>()
                .disable::<InputPlugin>(),
        );

        let mut image = Image::new_uninit(
            extent(size),
            wgpu::TextureDimension::D2,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        );
        image.texture_descriptor.usage |= wgpu::TextureUsages::RENDER_ATTACHMENT;
        let target = app.world_mut().resource_mut::<Assets<Image>>().add(image);
        let cube = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Cuboid::new(1., 1., 1.));
        app.insert_resource(ClearColor(Color::srgb_u8(14, 22, 30)));
        app.insert_resource(Cube(cube));
        app.world_mut().spawn((
            Camera3d::default(),
            RenderTarget::from(target.clone()),
            Tonemapping::None,
            DebandDither::Disabled,
            ReviewCamera,
            Transform::default(),
        ));
        app.world_mut().spawn((
            DirectionalLight {
                illuminance: 9_000.,
                shadow_maps_enabled: true,
                ..default()
            },
            Transform::from_xyz(-4., 9., 5.).looking_at(Vec3::ZERO, Vec3::Y),
        ));
        app.insert_resource(GlobalAmbientLight {
            brightness: 120.,
            ..default()
        });
        app.insert_resource(Target(target.clone()));
        let frame = RenderedFrame::default();
        app.sub_app_mut(RenderApp)
            .insert_resource(Target(target))
            .insert_resource(frame.clone())
            .add_systems(Render, publish.in_set(RenderSystems::Render));

        app.finish();
        app.cleanup();
        let apps = std::mem::take(app.sub_apps_mut());
        Ok(Self {
            apps,
            frame,
            size,
            displayed: None,
            orbit: Orbit::default(),
        })
    }

    fn draw(
        &mut self,
        gpu: &mut GpuCanvasContext,
        view: &View,
        size: [u32; 2],
        orbit: Orbit,
    ) -> Result<()> {
        let first_frame = self.displayed.is_none();
        let changed = self.displayed.as_ref().is_none_or(|old| {
            !Rc::ptr_eq(&view.review, &old.review)
                || view.before != old.before
                || view.selection != old.selection
                || view.focus != old.focus
        });
        if changed || size != self.size || self.orbit != orbit {
            let world = self.apps.main.world_mut();
            if size != self.size {
                let handle = world.resource::<Target>().0.clone();
                world
                    .resource_mut::<Assets<Image>>()
                    .get_mut(&handle)
                    .ok_or_else(|| anyhow!("Bevy lost the viewport texture"))?
                    .texture_descriptor
                    .size = extent(size);
            }
            if changed {
                rebuild_room(world, view);
            }
            update_camera(world, view, orbit, size);
            self.displayed = Some(view.clone());
            self.orbit = orbit;
            self.size = size;
            self.apps.update();
            if first_frame {
                // Startup introduces render assets. Run the next schedule before
                // the first demand-driven paint so a parked window has a scene.
                self.apps.update();
            }
        }

        let frame = self
            .frame
            .0
            .lock()
            .map_err(|_| anyhow!("Bevy's render texture lock was poisoned"))?
            .clone();
        if let Some(frame) = frame {
            gpu.paint_texture(frame.to_imported_handle()?, Corners::default(), 1.0, false);
        }
        Ok(())
    }
}

fn extent([width, height]: [u32; 2]) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    }
}

fn render_resources(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<RenderCreation> {
    let info = device.adapter_info();
    let backends = wgpu::Backends::from(info.backend);
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = backends;
    let instance = wgpu::Instance::new(descriptor);

    // wgpu gives us the existing device but Bevy also needs an adapter for capability queries.
    let adapter = bevy::tasks::block_on(instance.enumerate_adapters(backends))
        .into_iter()
        .find(|candidate| {
            let candidate = candidate.get_info();
            candidate.name == info.name
                && candidate.vendor == info.vendor
                && candidate.device == info.device
                && candidate.backend == info.backend
        })
        .ok_or_else(|| anyhow!("Could not match GPUI's GPU adapter: {}", info.name))?;
    Ok(RenderCreation::manual(
        RenderDevice::from(device.clone()),
        RenderQueue::new(queue.clone()),
        RenderAdapterInfo::new(info),
        RenderAdapter::new(adapter),
        RenderInstance::new(instance),
    ))
}

fn publish(target: Res<Target>, images: Res<RenderAssets<GpuImage>>, frame: Res<RenderedFrame>) {
    if let Some(image) = images.get(&target.0)
        && let Ok(mut slot) = frame.0.lock()
    {
        *slot = Some((*image.texture_view).clone());
    }
}

fn rebuild_room(world: &mut World, view: &View) {
    let previous = world
        .query_filtered::<Entity, With<SceneMesh>>()
        .iter(world)
        .collect::<Vec<_>>();
    for entity in previous {
        world.despawn(entity);
    }
    let mesh = world.resource::<Cube>().0.clone();
    for object in view.scene().objects() {
        let selected = view.selection.as_deref() == Some(object.id.as_str());
        if view.focus && !selected {
            continue;
        }
        let [r, g, b] = object.color;
        let material = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial {
                base_color: Color::srgb_u8(r, g, b),
                perceptual_roughness: 0.9,
                ..default()
            });
        world.spawn((
            SceneMesh,
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material),
            Transform::from_translation(Vec3::from_array(object.center))
                .with_scale(Vec3::from_array(object.size)),
        ));
    }
}

fn update_camera(world: &mut World, view: &View, orbit: Orbit, size: [u32; 2]) {
    let focused = view
        .selection
        .as_deref()
        .filter(|_| view.focus)
        .and_then(|id| view.scene().object(id));
    let target = focused.map_or(Vec3::ZERO, |object| Vec3::from_array(object.center));
    let scale = focused.map_or(14., |object| Vec3::from_array(object.size).length() * 1.75);
    let aspect = size[0] as f32 / size[1] as f32;
    let distance = scale * orbit.zoom / aspect.min(1.);
    let direction = Vec3::new(
        orbit.yaw.sin() * orbit.pitch.cos(),
        orbit.pitch.sin(),
        orbit.yaw.cos() * orbit.pitch.cos(),
    );
    let mut cameras = world.query_filtered::<&mut Transform, With<ReviewCamera>>();
    for mut camera in cameras.iter_mut(world) {
        *camera =
            Transform::from_translation(target + direction * distance).looking_at(target, Vec3::Y);
    }
}

#[derive(Default)]
pub(super) struct Surface {
    renderer: Rc<RefCell<Option<Renderer>>>,
    orbit: Rc<Cell<Orbit>>,
    previous: Rc<Cell<Option<SurfacePoint>>>,
    error: Rc<RefCell<Option<String>>>,
}

impl Surface {
    pub fn element(&self, view: View) -> AnyElement {
        let (renderer, error, orbit) = (
            self.renderer.clone(),
            self.error.clone(),
            self.orbit.clone(),
        );
        let canvas = gpu_canvas(move |gpu| {
            if error.borrow().is_some() {
                return;
            }
            let pixels = gpu.bounds().size;
            let scale = gpu.scale_factor();
            let size = [
                (pixels.width.0 * scale).round().max(1.) as u32,
                (pixels.height.0 * scale).round().max(1.) as u32,
            ];
            let mut renderer = renderer.borrow_mut();
            let result = (|| -> Result<()> {
                if renderer.is_none() {
                    *renderer = Some(Renderer::new(gpu, size)?);
                }
                renderer
                    .as_mut()
                    .expect("renderer initialized")
                    .draw(gpu, &view, size, orbit.get())
            })();
            if let Err(failure) = result {
                let message = format!("Viewport could not start: {failure:#}");
                eprintln!("{message}");
                *error.borrow_mut() = Some(message);
            }
        })
        .size_full();
        let previous = self.previous.clone();
        let orbit = self.orbit.clone();
        let interactive = surface_input(canvas, move |input: SurfaceInput| match input.event {
            SurfaceEvent::PointerDown {
                position,
                button: SurfaceButton::Left,
                ..
            } => {
                previous.set(Some(position));
            }
            SurfaceEvent::PointerMove {
                position,
                pressed: Some(SurfaceButton::Left),
                ..
            } => {
                if let Some(last) = previous.get() {
                    previous.set(Some(position));
                    let mut camera = orbit.get();
                    camera.yaw -= (position.x - last.x) * 0.007;
                    camera.pitch = (camera.pitch + (position.y - last.y) * 0.007).clamp(0.12, 1.48);
                    orbit.set(camera);
                }
            }
            SurfaceEvent::PointerUp {
                button: SurfaceButton::Left,
                ..
            } => {
                previous.set(None);
            }
            SurfaceEvent::Scroll { delta, .. } => {
                let amount = match delta {
                    SurfaceScroll::Lines { y, .. } => y,
                    SurfaceScroll::Pixels { y, .. } => y / 40.,
                };
                let mut camera = orbit.get();
                camera.zoom = (camera.zoom / 1.12_f32.powf(amount)).clamp(0.45, 4.);
                orbit.set(camera);
            }
            _ => {}
        });
        if let Some(message) = self.error.borrow().clone() {
            return div()
                .size_full()
                .p_4()
                .text_color(rgb(0xffa6a6))
                .child(message)
                .into_any_element();
        }
        div()
            .size_full()
            .bg(rgb(0x0e161e))
            .child(interactive)
            .into_any_element()
    }
}
