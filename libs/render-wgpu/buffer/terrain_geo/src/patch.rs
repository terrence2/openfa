// This file is part of OpenFA.
//
// OpenFA is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// OpenFA is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with OpenFA.  If not, see <http://www.gnu.org/licenses/>.
use crate::patch_tree::TreeIndex;

use geometry::{
    algorithm::{compute_normal, solid_angle_tri},
    intersect,
    intersect::{CirclePlaneIntersection, PlaneSide, SpherePlaneIntersection},
    Plane, Sphere,
};
use nalgebra::{Point3, Vector3};
use physical_constants::{EARTH_RADIUS_KM, EVEREST_HEIGHT_KM};

// We introduce a substantial amount of error in our intersection computations below
// with all the dot products and re-normalizations. This is fine, as long as we use a
// large enough offset when comparing near zero to get stable results and that pad
// extends the collisions in the right direction.
const SIDEDNESS_OFFSET: f64 = -1f64;

#[derive(Debug, Copy, Clone)]
pub(crate) struct Patch {
    // Normal at center of patch.
    normal: Vector3<f64>,

    // In geocentric, cartesian kilometers
    pts: [Point3<f64>; 3],

    // Planes
    planes: [Plane<f64>; 3],

    // The leaf node that owns this patch, or None if a tombstone.
    owner: TreeIndex,

    // Current view state.
    cached_solid_angle: f64,
    cached_in_view: bool,
}

impl Patch {
    pub(crate) fn empty() -> Self {
        Self {
            normal: Vector3::new(1f64, 0f64, 0f64),
            planes: [Plane::from_normal_and_distance(Vector3::new(1f64, 0f64, 0f64), 0f64); 3],
            pts: [Point3::new(0f64, 0f64, 0f64); 3],
            owner: TreeIndex(usize::MAX),
            cached_solid_angle: 0f64,
            cached_in_view: false,
        }
    }

    pub(crate) fn new(owner: TreeIndex, pts: [Point3<f64>; 3]) -> Self {
        let origin = Point3::new(0f64, 0f64, 0f64);
        let patch = Self {
            normal: compute_normal(&pts[0], &pts[1], &pts[2]),
            planes: [
                Plane::from_point_and_normal(&pts[0], &compute_normal(&pts[1], &origin, &pts[0])),
                Plane::from_point_and_normal(&pts[1], &compute_normal(&pts[2], &origin, &pts[1])),
                Plane::from_point_and_normal(&pts[2], &compute_normal(&pts[0], &origin, &pts[2])),
            ],
            pts,
            owner,
            cached_solid_angle: 0f64,
            cached_in_view: false,
        };
        assert!(patch.planes[0].point_is_in_front(&pts[2]));
        assert!(patch.planes[1].point_is_in_front(&pts[0]));
        assert!(patch.planes[2].point_is_in_front(&pts[1]));
        patch
    }

    pub(crate) fn retarget(&mut self, owner: TreeIndex, pts: [Point3<f64>; 3]) {
        *self = Self::new(owner, pts);
    }

    pub(crate) fn update_for_view(
        &mut self,
        viewable_area: &[Plane<f64>; 6],
        eye_position: &Point3<f64>,
        eye_direction: &Vector3<f64>,
    ) {
        self.cached_solid_angle = f64::MIN;
        if !self.keep(viewable_area) {
            self.cached_in_view = false;
            return;
        }
        self.cached_in_view = true;

        // Cross north and eye_direction to get a right vector for the polygon.
        let sa = solid_angle_tri(&eye_position, &eye_direction, &self.pts);
        if sa > self.cached_solid_angle {
            self.cached_solid_angle = sa;
        }
        assert!(!self.cached_solid_angle.is_nan());
    }

    pub(crate) fn solid_angle(&self) -> f64 {
        self.cached_solid_angle
    }

    pub(crate) fn in_view(&self) -> bool {
        self.cached_in_view
    }

    pub(crate) fn owner(&self) -> TreeIndex {
        self.owner
    }

    pub(crate) fn points(&self) -> &[Point3<f64>; 3] {
        &self.pts
    }

    pub(crate) fn edge(&self, i: u8) -> (Point3<f64>, Point3<f64>) {
        match i {
            0 => (self.pts[0], self.pts[1]),
            1 => (self.pts[1], self.pts[2]),
            2 => (self.pts[2], self.pts[0]),
            _ => unreachable!(),
        }
    }

    // Might be useful later?
    #[allow(unused)]
    pub(crate) fn distance_squared_to(&self, point: &Point3<f64>) -> f64 {
        if self.point_is_in_cone(point) {
            let m = point.coords.magnitude();
            if m < EARTH_RADIUS_KM + EVEREST_HEIGHT_KM {
                return 0.0;
            }
            return m - (EARTH_RADIUS_KM + EVEREST_HEIGHT_KM);
        }

        let mut minimum = 999_999_999f64;

        // bottom points
        for p in &self.pts {
            let v = p - point;
            let d = v.dot(&v);
            if d < minimum {
                minimum = d;
            }
        }
        // top points
        for p in &self.pts {
            let top_point = p + (p.coords.normalize() * EARTH_RADIUS_KM);
            let v = top_point - point;
            let d = v.dot(&v);
            if d < minimum {
                minimum = d;
            }
        }

        minimum
    }

    fn is_behind_plane(&self, plane: &Plane<f64>, show_msgs: bool) -> bool {
        // Patch Extent:
        //   outer: the three planes cutting from geocenter through each pair of points in vertices.
        //   bottom: radius of the planet
        //   top: radius of planet from height of everest

        // Two phases:
        //   1) Convex hull over points
        //   2) Plane-sphere for convex top area

        // bottom points
        for p in &self.pts {
            if plane.point_is_in_front_with_offset(&p, SIDEDNESS_OFFSET) {
                return false;
            }
        }
        // top points
        for p in &self.pts {
            let top_point = p + (p.coords.normalize() * EARTH_RADIUS_KM);
            if plane.point_is_in_front_with_offset(&top_point, SIDEDNESS_OFFSET) {
                return false;
            }
        }

        // plane vs top sphere
        let top_sphere = Sphere::from_center_and_radius(
            &Point3::new(0f64, 0f64, 0f64),
            EVEREST_HEIGHT_KM + EVEREST_HEIGHT_KM,
        );
        let intersection = intersect::sphere_vs_plane(&top_sphere, &plane);
        match intersection {
            SpherePlaneIntersection::NoIntersection { side, .. } => side == PlaneSide::Above,
            SpherePlaneIntersection::Intersection(ref circle) => {
                for (i, plane) in self.planes.iter().enumerate() {
                    let intersect = intersect::circle_vs_plane(circle, plane, SIDEDNESS_OFFSET);
                    match intersect {
                        CirclePlaneIntersection::Parallel => {
                            if show_msgs {
                                println!("  parallel {}", i);
                            }
                        }
                        CirclePlaneIntersection::BehindPlane => {
                            if show_msgs {
                                println!("  outside {}", i);
                            }
                        }
                        CirclePlaneIntersection::Tangent(ref p) => {
                            if self.point_is_in_cone(p) {
                                if show_msgs {
                                    println!("  tangent {} in cone: {}", i, p);
                                }
                                return false;
                            }
                            if show_msgs {
                                println!("  tangent {} NOT in cone: {}", i, p);
                            }
                        }
                        CirclePlaneIntersection::Intersection(ref p0, ref p1) => {
                            if self.point_is_in_cone(p0) || self.point_is_in_cone(p1) {
                                if show_msgs {
                                    println!("  intersection {} in cone: {}, {}", i, p0, p1);
                                }
                                return false;
                            }
                            if show_msgs {
                                println!("  intersection {} NOT in cone: {}, {}", i, p0, p1);
                            }
                        }
                        CirclePlaneIntersection::InFrontOfPlane => {
                            if self.point_is_in_cone(circle.center()) {
                                if show_msgs {
                                    println!("  circle {} in cone: {}", i, circle.center());
                                }
                                return false;
                            }
                            if show_msgs {
                                println!("  circle {} NOT in cone: {}", i, circle.center());
                            }
                        }
                    }
                }

                if show_msgs {
                    println!("  fell out of all planes");
                }
                // No test was in front of the plane, so we are fully behind it.
                true
            }
        }
    }

    fn point_is_in_cone(&self, point: &Point3<f64>) -> bool {
        for plane in &self.planes {
            if !plane.point_is_in_front_with_offset(point, SIDEDNESS_OFFSET) {
                return false;
            }
        }
        true
    }

    fn keep(&self, viewable_area: &[Plane<f64>; 6]) -> bool {
        for plane in viewable_area {
            if self.is_behind_plane(plane, false) {
                return false;
            }
        }

        true
    }
}
