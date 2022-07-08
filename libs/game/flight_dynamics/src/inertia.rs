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
    kilograms_meter2, meters, scalar, Kilograms, Mass, Meters, RotationalInertia, Scalar,
};
use bevy_ecs::prelude::*;
use geometry::{Aabb3, Cylinder};
use nalgebra::{Point3, Vector3};
use nitrous::{inject_nitrous_component, NitrousComponent};
use pt::PlaneType;
use shape::ShapeExtent;

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

    // Constant masses.
    fuselage_front_mass: Mass<Kilograms>,
    fuselage_back_mass: Mass<Kilograms>,
    wing_mass: Mass<Kilograms>,

    // Re-computing the inertia tensor results in the following tracked components.
    i_xx: RotationalInertia<Kilograms, Meters>,
    i_yy: RotationalInertia<Kilograms, Meters>,
    i_zz: RotationalInertia<Kilograms, Meters>,
}

#[inject_nitrous_component]
impl Inertia {
    /// Create the Inertia from the plane data.
    /// TODO: fuel and munitions
    pub fn from_extent(pt: &PlaneType, extent: &ShapeExtent) -> Self {
        // Assumption: fuselage is 1/4 the total elevator to gear height
        // Assumption: the wing chord is 1/5 the total plane length
        let length = extent.aabb().span(2);
        let height = extent.aabb().span(0).min(extent.aabb().span(1));
        let fuselage_top = height * scalar!(0.25);
        let wingspan = extent.aabb().span(0);
        let chord = length * scalar!(0.2);

        // The CG is the center of mass (duh!); what we're trying to model here
        // is the distribution of weight about the CG.
        //
        // We break the fuselage into front and back parts and compute the mass
        // as if each is the same weight (which it should be, cf cg).
        let nose = Point3::new(meters!(0_f64), meters!(0_f64), *extent.aabb().low(2));
        let tail = Point3::new(meters!(0_f64), meters!(0_f64), *extent.aabb().high(2));
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

        // Assumption: constant density between wings and fuselage
        let empty_mass = pt.nt.ot.empty_weight.mass::<Kilograms>();
        let fuselage_volume = fuselage_front.volume() + fuselage_back.volume();
        let wing_volume = wing.volume();
        let total_volume = fuselage_volume + wing_volume;
        let wing_fraction: Scalar = (wing_volume.as_dyn() / total_volume.as_dyn()).into();
        let fuselage_fraction: Scalar = (fuselage_volume.as_dyn() / total_volume.as_dyn()).into();
        let wing_mass = wing_fraction * empty_mass;
        let fuselage_mass = fuselage_fraction * empty_mass;
        let fuselage_back_mass = fuselage_mass / scalar!(2_f64);
        let fuselage_front_mass = fuselage_mass / scalar!(2_f64);

        // TODO: recompute every step
        // Inertia at the end of a cylinder = 1/4*m*r**2 + 1/3*m*l**2
        let i_xx = Self::cylinder_i_xx(&fuselage_back, fuselage_back_mass)
            + Self::cylinder_i_xx(&fuselage_front, fuselage_front_mass)
            + Self::aabb_i_xx(&wing, wing_mass);
        println!("computed i_xx: {}", i_xx);

        Self {
            // Impostors
            fuselage_front,
            fuselage_back,
            wing,
            // tanks
            // munitions

            // Pre-computed factors
            fuselage_back_mass,
            fuselage_front_mass,
            wing_mass,

            i_xx,
            i_yy: kilograms_meter2!(100_f64),
            i_zz: kilograms_meter2!(100_f64),
        }
    }

    fn cylinder_i_xx(
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

    pub fn i_xx(&self) -> RotationalInertia<Kilograms, Meters> {
        self.i_xx
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
    /// TODO: take current munitions and fuel levels.
    pub fn recompute_tensor(&mut self) {}
}
