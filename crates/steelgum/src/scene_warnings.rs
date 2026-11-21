use crate::{Scene, SceneObject};

impl Scene {
    /// Pairs of authored objects with outward-facing box faces that share an area
    /// on the same plane, within the precision of the authored coordinates.
    /// These pairs can cause z-fighting. Face-to-face and edge-to-edge joins
    /// are valid and do not appear in the list.
    pub fn coplanar_overlaps(&self) -> impl ExactSizeIterator<Item = [&SceneObject; 2]> {
        self.coplanar_overlaps
            .get_or_init(|| {
                let bounds: Vec<_> = self.objects().iter().map(box_bounds).collect();
                let mut overlaps = Vec::new();
                for (index, left) in bounds.iter().enumerate() {
                    for (right_index, right) in bounds.iter().enumerate().skip(index + 1) {
                        if shares_face_area(left, right) {
                            overlaps.push([index, right_index]);
                        }
                    }
                }
                overlaps
            })
            .iter()
            .map(|pair| pair.map(|index| &self.objects()[index]))
    }
}

struct AxisBounds {
    low: f64,
    high: f64,
    rounding: f64,
}

fn box_bounds(object: &SceneObject) -> [AxisBounds; 3] {
    std::array::from_fn(|axis| {
        let center = f64::from(object.center[axis]);
        let half = f64::from(object.size[axis]) / 2.;
        AxisBounds {
            low: center - half,
            high: center + half,
            // The two authored inputs were rounded independently to f32.
            rounding: (center.abs() + half) * f64::from(f32::EPSILON) / 2.,
        }
    })
}

fn shares_face_area(left: &[AxisBounds; 3], right: &[AxisBounds; 3]) -> bool {
    let mut aligned_face = false;
    for (a, b) in left.iter().zip(right) {
        let tolerance = a.rounding + b.rounding;
        if a.high.min(b.high) - a.low.max(b.low) <= tolerance {
            return false;
        }
        // Compare matching outward normals. Opposing faces can meet at an
        // internal join without competing for the same rendered pixels.
        aligned_face |= (a.low - b.low).abs().min((a.high - b.high).abs()) <= tolerance;
    }
    aligned_face
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(center: [f32; 3], size: [f32; 3]) -> Scene {
        let source = serde_json::json!({
            "version": 1,
            "objects": [
                {"id": "north", "center": [0, 1.6, -6], "size": [12.3, 3.2, 0.3], "color": [10, 20, 30], "solid": true},
                {"id": "west", "center": center, "size": size, "color": [40, 50, 60], "solid": false}
            ]
        });
        Scene::from_json(&source.to_string()).unwrap()
    }

    #[test]
    fn flags_overlapping_room_corners_and_identical_boxes_once_per_pair() {
        for scene in [
            pair([-6., 1.6, 0.], [0.3, 3.2, 12.]),
            pair([0., 1.6, -6.], [12.3, 3.2, 0.3]),
        ] {
            let mut overlaps = scene.coplanar_overlaps();
            assert_eq!(overlaps.len(), 1);
            assert_eq!(
                overlaps.next().unwrap().map(|object| object.id.as_str()),
                ["north", "west"]
            );
        }
    }

    #[test]
    fn allows_flush_joins_and_intersections_without_coplanar_faces() {
        for scene in [
            pair([-6., 1.6, 0.], [0.3, 3.2, 11.7]),
            pair([0., 1.6, -5.7], [12.3, 3.2, 0.3]),
            pair([0., 1.5, -6.1], [8., 3.2, 0.3]),
            pair([0., 1.6, 0.], [12.3, 3.2, 0.3]),
        ] {
            assert!(scene.coplanar_overlaps().next().is_none());
        }
    }
}
