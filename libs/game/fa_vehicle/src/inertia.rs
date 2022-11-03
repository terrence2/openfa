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
use absolute_unit::{
    kilograms, meters, scalar, Kilograms, Mass, Meters, RotationalInertia, Scalar,
};
use bevy_ecs::prelude::*;
use geometry::{Aabb3, Cylinder};
use nalgebra::Point3;
use nitrous::{inject_nitrous_component, NitrousComponent};
use ot::ObjectType;
use shape::ShapeExtent;

pub struct InertiaTensor {
    pub i_xx: RotationalInertia<Kilograms, Meters>,
    pub i_yy: RotationalInertia<Kilograms, Meters>,
    pub i_zz: RotationalInertia<Kilograms, Meters>,
}

// Each FlightDynamics gets its own Inertia reference. The base plane
// bits are shared between all instances of that plane, but are must created
// and used here directly as they are small.
#[derive(Component, NitrousComponent, Debug, Clone)]
#[Name = "inertia"]
pub struct Inertia {
    // The following shape impostors act as our inertia model.
    fuselage_front: Cylinder<Meters>,
    fuselage_back: Cylinder<Meters>,
    wing: Aabb3<Meters>,
    // TODO: fuel tanks
    // TODO: munitions

    // Constants
    empty_mass: Mass<Kilograms>,
}

#[inject_nitrous_component]
impl Inertia {
    /// Create the Inertia from the plane data.
    /// TODO: fuel and munitions
    pub fn from_extent(ot: &ObjectType, extent: &ShapeExtent) -> Self {
        // Assumption: fuselage is 1/4 the total elevator to gear height
        // Assumption: the wing chord is 1/5 the total plane length
        let length = extent.aabb_body().span(2);
        let height = extent.aabb_body().span(0).min(extent.aabb_body().span(1));
        let fuselage_top = height * scalar!(0.25);
        let wingspan = extent.aabb_body().span(0);
        let chord = length * scalar!(0.2);

        // The CG is the center of mass (duh!); what we're trying to model here
        // is the distribution of weight about the CG.
        //
        // We break the fuselage into front and back parts and compute the mass
        // as if each is the same weight (which it should be, cf cg).
        let nose = Point3::new(meters!(0_f64), meters!(0_f64), *extent.aabb_body().low(2));
        let tail = Point3::new(meters!(0_f64), meters!(0_f64), *extent.aabb_body().high(2));
        let fuselage_front = Cylinder::new(Point3::origin(), nose.coords, fuselage_top);
        let fuselage_back = Cylinder::new(Point3::origin(), tail.coords, fuselage_top);

        // TODO: for fighters, the wing is generally on the centerline, so no need to reflect
        //       the distribution of mass here, but we should follow up for bigger aircraft.
        //       If possible: bombers like the b-1&2 mean we can't hinge e.g. on engine count.
        let wing = Aabb3::from_bounds(
            Point3::new(
                -wingspan / scalar!(2_f64),
                meters!(-0.05_f64),
                -chord / scalar!(2_f64),
            ),
            Point3::new(
                wingspan / scalar!(2_f64),
                meters!(0.05_f64),
                chord / scalar!(2_f64),
            ),
        );

        Self {
            // Impostors
            fuselage_front,
            fuselage_back,
            wing,
            // tanks
            // munitions

            // Pre-computed factors
            empty_mass: kilograms!(ot.empty_weight),
        }
    }

    fn cylinder_i_xx(
        c: &Cylinder<Meters>,
        m: Mass<Kilograms>,
    ) -> RotationalInertia<Kilograms, Meters> {
        let r = c.radius_bottom();
        scalar!(1. / 2_f64) * m * (r * r)
    }

    fn cylinder_i_yy_zz_end(
        c: &Cylinder<Meters>,
        m: Mass<Kilograms>,
    ) -> RotationalInertia<Kilograms, Meters> {
        let r = c.radius_bottom();
        let l = c.length();
        scalar!(1. / 4_f64) * m * (r * r) + scalar!(1. / 3_f64) * m * (l * l)
    }

    fn aabb_i_xx(b: &Aabb3<Meters>, m: Mass<Kilograms>) -> RotationalInertia<Kilograms, Meters> {
        // Note: i_xx in the Allerton frame is actually -z in the model frame
        let ly = b.span(0);
        let lz = b.span(1);
        scalar!(1. / 12_f64) * m * (ly * ly + lz * lz)
    }

    fn aabb_i_yy(b: &Aabb3<Meters>, m: Mass<Kilograms>) -> RotationalInertia<Kilograms, Meters> {
        // Note: i_xx in the Allerton frame is actually -z in the model frame
        let lx = b.span(2);
        let lz = b.span(1);
        scalar!(1. / 12_f64) * m * (lx * lx + lz * lz)
    }

    fn aabb_i_zz(b: &Aabb3<Meters>, m: Mass<Kilograms>) -> RotationalInertia<Kilograms, Meters> {
        // Note: i_xx in the Allerton frame is actually -z in the model frame
        let lx = b.span(2);
        let ly = b.span(0);
        scalar!(1. / 12_f64) * m * (lx * lx + ly * ly)
    }

    pub fn fuselage_front(&self) -> &Cylinder<Meters> {
        &self.fuselage_front
    }

    pub fn fuselage_back(&self) -> &Cylinder<Meters> {
        &self.fuselage_back
    }

    pub fn wing(&self) -> &Aabb3<Meters> {
        &self.wing
    }

    /// Recalculate the tensor from change shapes.
    /// TODO: take current hardpoints, munitions, and fuel levels.
    pub fn recompute_tensor(&self, internal_fuel: Mass<Kilograms>) -> InertiaTensor {
        // Assumption: constant density between wings and fuselage
        // Assumption: uniform distribution of fuel in wings and fuselage
        let current_mass = self.empty_mass + internal_fuel;
        let fuselage_volume = self.fuselage_front.volume() + self.fuselage_back.volume();
        let wing_volume = self.wing.volume();
        let total_volume = fuselage_volume + wing_volume;
        let wing_fraction: Scalar = (wing_volume.as_dyn() / total_volume.as_dyn()).into();
        let fuselage_fraction: Scalar = (fuselage_volume.as_dyn() / total_volume.as_dyn()).into();
        let wing_mass = wing_fraction * current_mass;
        let fuselage_mass = fuselage_fraction * current_mass;
        let fuselage_back_mass = fuselage_mass / scalar!(2_f64);
        let fuselage_front_mass = fuselage_mass / scalar!(2_f64);

        let i_xx = Self::cylinder_i_xx(&self.fuselage_back, fuselage_back_mass)
            + Self::cylinder_i_xx(&self.fuselage_front, fuselage_front_mass)
            + Self::aabb_i_xx(&self.wing, wing_mass);
        let i_yy = Self::cylinder_i_yy_zz_end(&self.fuselage_back, fuselage_back_mass)
            + Self::cylinder_i_yy_zz_end(&self.fuselage_front, fuselage_front_mass)
            + Self::aabb_i_yy(&self.wing, wing_mass);
        let i_zz = Self::cylinder_i_yy_zz_end(&self.fuselage_back, fuselage_back_mass)
            + Self::cylinder_i_yy_zz_end(&self.fuselage_front, fuselage_front_mass)
            + Self::aabb_i_zz(&self.wing, wing_mass);

        InertiaTensor { i_xx, i_yy, i_zz }
    }
}
