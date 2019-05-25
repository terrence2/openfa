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
use failure::Fallible;
use lib::Library;
use pal::Palette;
use sh::RawShape;
use shape::{DrawSelection, ShRenderer, ShapeInstanceRef};
use std::{cell::RefCell, rc::Rc};
use window::GraphicsWindow;
use xt::TypeRef;

pub struct ObjectFactory {
    library: Rc<RefCell<Library>>,
    sh_renderer: Rc<RefCell<ShRenderer>>,
    system_palette: Rc<RefCell<Palette>>,
    window: Rc<RefCell<GraphicsWindow>>,
}

impl ObjectFactory {
    pub fn new(
        library: Rc<RefCell<Library>>,
        sh_renderer: Rc<RefCell<ShRenderer>>,
        system_palette: Rc<RefCell<Palette>>,
        window: Rc<RefCell<GraphicsWindow>>,
    ) -> Self {
        Self {
            library,
            sh_renderer,
            system_palette,
            window,
        }
    }

    pub fn build(&self, class: TypeRef, name: &str) -> Fallible<ObjectRef> {
        let mut shapes = Vec::with_capacity(if class.is_pt() { 4 } else { 2 });
        if let Some(ref file) = class.ot().shape {
            let lib = self.library.borrow();
            let data = lib.load(file)?;
            let sh = RawShape::from_bytes(&data)?;
            let shape = self.sh_renderer.borrow_mut().render_shape(
                name,
                &sh,
                DrawSelection::NormalModel,
                &self.system_palette.borrow(),
                &self.library.borrow(),
                &self.window.borrow(),
            )?;
            shapes.push(shape);
        }
        let obj = Object { class, shapes };
        Ok(ObjectRef::new(obj))
    }
}

// An "entity" in the system.
pub struct Object {
    class: TypeRef,
    shapes: Vec<ShapeInstanceRef>,
}

pub struct ObjectRef {
    object: Rc<RefCell<Object>>,
}

impl ObjectRef {
    pub fn new(object: Object) -> Self {
        Self {
            object: Rc::new(RefCell::new(object)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use camera::ArcBallCamera;
    use omnilib::OmniLib;
    use std::f32::consts::PI;
    use window::GraphicsConfigBuilder;

    #[test]
    fn it_works() -> Fallible<()> {
        let mut window = Rc::new(RefCell::new(GraphicsWindow::new(
            &GraphicsConfigBuilder::new().build(),
        )?));
        let mut camera = ArcBallCamera::new(window.borrow().aspect_ratio()?, 0.1f32, 3.4e+38f32);
        camera.set_distance(100.);
        camera.set_angle(115. * PI / 180., -135. * PI / 180.);

        let omni = OmniLib::new_for_test()?;
        for (game, lib) in omni.libraries() {
            let object_factory = ObjectFactory::new(
                lib,
                Rc::new(RefCell::new(ShRenderer::new(&window)?)),
                Rc::new(RefCell::new(Palette::from_bytes(
                    &lib.load("PALETTE.PAL")?,
                )?)),
                window,
            );
            for name in lib.find_matching("*.[JNPO]T")?.iter() {}
        }
        Ok(())
    }
}
