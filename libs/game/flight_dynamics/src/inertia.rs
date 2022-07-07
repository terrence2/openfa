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
use absolute_unit::{kilograms_meter2, meters, scalar, Kilograms, Meters, RotationalInertia};
use bevy_ecs::prelude::*;
use geometry::Cylinder;
use nalgebra::{Point3, Vector3};
use nitrous::{inject_nitrous_component, NitrousComponent};
use shape::ShapeExtent;

// Each FlightDynamics gets its own Inertia reference. The base plane
// bits are shared between all instances of that plane, but are must created
// and used here directly as they are small.
#[derive(Component, NitrousComponent, Debug, Clone)]
#[Name = "inertia"]
pub struct Inertia {
    // The following shape impostors act as our inertia model.
    fuselage: Cylinder<Meters>,
    // TODO: wing: Aabb<f64, 3>,
    // TODO: fuel tanks
    // TODO: munitions

    // Re-computing the inertia tensor results in the following tracked components.
    i_xx: RotationalInertia<Kilograms, Meters>,
    i_yy: RotationalInertia<Kilograms, Meters>,
    i_zz: RotationalInertia<Kilograms, Meters>,
}

#[inject_nitrous_component]
impl Inertia {
    /// Create the Inertia from the plane data.
    /// TODO: fuel and munitions
    pub fn from_extent(extent: &ShapeExtent) -> Self {
        let nose = Point3::new(
            // *extent.aabb().low(0) + (extent.aabb().span(0) / scalar!(2_f64)),
            // *extent.aabb().low(1) + (extent.aabb().span(1) / scalar!(2_f64)),
            meters!(0_f64),
            meters!(0_f64),
            *extent.aabb().low(2),
        );
        let centerline = Vector3::new(meters!(0_f64), meters!(0_f64), extent.aabb().span(2));
        println!("x: {}, y: {}", extent.aabb().span(0), extent.aabb().span(1));
        let fuselage = Cylinder::new(
            nose,
            centerline,
            extent.aabb().span(0).min(extent.aabb().span(1)) * scalar!(0.25),
        );
        // let wing = extent.aabb.clone();
        Self {
            fuselage,
            // wing
            // tanks
            // munitions
            i_xx: kilograms_meter2!(100_f64),
            i_yy: kilograms_meter2!(100_f64),
            i_zz: kilograms_meter2!(100_f64),
        }
    }

    pub fn fuselage(&self) -> &Cylinder<Meters> {
        &self.fuselage
    }

    /// Recalculate the tensor from change shapes.
    pub fn recompute_tensor(&mut self) {}
}
