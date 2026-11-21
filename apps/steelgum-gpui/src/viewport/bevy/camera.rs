use bevy::camera::ScalingMode;
use bevy::prelude::*;
use bevy::shape::{Aabb3d, RayCast3d};
use gpui_interop::SurfacePoint;

use super::{ReviewCamera, View};

#[derive(Clone, Copy, PartialEq)]
pub(super) struct Orbit {
    yaw: f32,
    pitch: f32,
    zoom: f32,
    pan: Vec3,
    map_pan: Vec3,
}

impl Default for Orbit {
    fn default() -> Self {
        Self {
            yaw: 0.57,
            pitch: 0.78,
            zoom: 1.0,
            pan: Vec3::ZERO,
            map_pan: Vec3::ZERO,
        }
    }
}

fn camera_distance(view: &View, orbit: Orbit, size: [u32; 2]) -> f32 {
    let scale =
        focused_object(view).map_or(17., |object| Vec3::from_array(object.size).length() * 1.75);
    let aspect = size[0] as f32 / size[1] as f32;
    scale * orbit.zoom / aspect.min(1.)
}

fn focused_object(view: &View) -> Option<&steelgum::SceneObject> {
    view.selection
        .as_deref()
        .filter(|_| view.focus)
        .and_then(|id| {
            // Keep the same framing when switching revisions, even if this
            // object moved or changed dimensions.
            view.review
                .after()
                .object(id)
                .or_else(|| view.review.before().object(id))
        })
}

impl Orbit {
    pub(super) fn reset_pan(&mut self) {
        self.pan = Vec3::ZERO;
        self.map_pan = Vec3::ZERO;
    }

    pub(super) fn rotate(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * 0.007;
        self.pitch = (self.pitch + dy * 0.007).clamp(0.12, 1.48);
    }

    pub(super) fn zoom_by(&mut self, amount: f32) {
        self.zoom = (self.zoom / 1.12_f32.powf(amount)).clamp(0.45, 4.);
    }

    fn direction(self) -> Vec3 {
        Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        )
    }

    pub(super) fn pan_by(&mut self, view: &View, size: [u32; 2], dx: f32, dy: f32) {
        let distance = camera_distance(view, *self, size);
        if view.top_down {
            self.map_pan += (-Vec3::X * dx - Vec3::Z * dy) * (distance / size[1] as f32);
        } else {
            let direction = self.direction();
            let right = Vec3::Y.cross(direction).normalize();
            let up = direction.cross(right);
            let frustum_height = 2. * distance * (PerspectiveProjection::default().fov * 0.5).tan();
            self.pan += (-right * dx + up * dy) * (frustum_height / size[1] as f32);
        }
    }
}

pub(super) fn update_camera(world: &mut World, view: &View, orbit: Orbit, size: [u32; 2]) {
    let [width, height] = size;
    let (transform, camera_projection) = camera_pose(view, orbit, size);
    let mut cameras =
        world.query_filtered::<(&mut Transform, &mut Projection), With<ReviewCamera>>();
    for (mut camera, mut projection) in cameras.iter_mut(world) {
        *camera = transform;
        *projection = camera_projection.clone();
        projection.update(width as f32, height as f32);
    }
}

fn camera_pose(view: &View, orbit: Orbit, size: [u32; 2]) -> (Transform, Projection) {
    let center = focused_object(view).map_or(Vec3::ZERO, |object| Vec3::from_array(object.center));
    let distance = camera_distance(view, orbit, size);
    if view.top_down {
        let target = center + orbit.map_pan;
        (
            Transform::from_translation(target + Vec3::Y * 20.).looking_at(target, Vec3::NEG_Z),
            OrthographicProjection {
                scaling_mode: ScalingMode::FixedVertical {
                    viewport_height: distance,
                },
                ..OrthographicProjection::default_3d()
            }
            .into(),
        )
    } else {
        let target = center + orbit.pan;
        (
            Transform::from_translation(target + orbit.direction() * distance)
                .looking_at(target, Vec3::Y),
            Projection::Perspective(PerspectiveProjection::default()),
        )
    }
}

pub(super) fn pick_object(
    view: &View,
    orbit: Orbit,
    size: [u32; 2],
    position: SurfacePoint,
) -> Option<&str> {
    let [width, height] = size.map(|value| value as f32);
    if !(0. ..width).contains(&position.x) || !(0. ..height).contains(&position.y) {
        return None;
    }
    let (camera, mut projection) = camera_pose(view, orbit, size);
    projection.update(width, height);
    let ndc = Vec2::new(2. * position.x / width - 1., 1. - 2. * position.y / height);
    let unproject = projection.get_clip_from_view().inverse();
    let point_at = |depth| camera.transform_point(unproject.project_point3(ndc.extend(depth)));
    // Bevy's reverse-Z projection maps the near plane to 1, with 0 at infinity.
    let near = point_at(1.);
    let farther = point_at(0.5);
    let direction = Dir3::new(farther - near).ok()?;
    let ray = RayCast3d::from_ray(Ray3d::new(near, direction), f32::MAX);
    view.scene()
        .objects()
        .iter()
        .filter(|object| !view.focus || view.selection.as_deref() == Some(object.id.as_str()))
        .filter_map(|object| {
            let bounds = Aabb3d::new(
                Vec3::from_array(object.center),
                Vec3::from_array(object.size) * 0.5,
            );
            ray.aabb_intersection_at(&bounds)
                .map(|distance| (distance, object.id.as_str()))
        })
        .min_by(|(left, _), (right, _)| left.total_cmp(right))
        .map(|(_, id)| id)
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::*;

    #[test]
    fn top_down_pick_hits_the_plate_above_the_floor() {
        let view = View {
            review: Rc::new(crate::demo().unwrap()),
            before: false,
            selection: None,
            focus: false,
            top_down: true,
        };
        let orbit = Orbit::default();
        let size = [500, 500];
        let plate = SurfacePoint {
            x: 250.,
            y: 250. + 1.48 * 500. / camera_distance(&view, orbit, size),
        };
        assert_eq!(pick_object(&view, orbit, size, plate), Some("plate"));
        assert_eq!(
            pick_object(&view, orbit, size, SurfacePoint { x: 0., y: 0. }),
            None
        );
    }
}
