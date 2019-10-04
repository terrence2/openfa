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
use shaderc::{
    CompileOptions, Compiler, Error, IncludeType, OptimizationLevel, ResolvedInclude, ShaderKind,
};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

/**
 * Scripts should be put in files like: <project>/shaders/<name>.<type>.glsl
 * Outputs get put in the project target dir: <project>/target/<name>.<type>.spir
 *
 * Compiler Options:
 *     DUMP_SPIRV=1   Dump disassembled code next to bytecode.
 *     DEBUG=1        Compile with debug settings.
 */
pub struct BuildShaders;

impl BuildShaders {
    fn type_for_filename(name: &str) -> ShaderKind {
        if name.ends_with(".vert.glsl") {
            ShaderKind::Vertex
        } else if name.ends_with(".frag.glsl") {
            ShaderKind::Fragment
        } else if name.ends_with(".comp.glsl") {
            ShaderKind::Compute
        } else if name.ends_with(".tess.ctrl.glsl") {
            ShaderKind::TessControl
        } else if name.ends_with(".tess.eval.glsl") {
            ShaderKind::TessEvaluation
        } else {
            ShaderKind::InferFromSource
        }
    }

    fn output_for_name(name: &str) -> String {
        assert!(name.ends_with(".glsl"));
        assert!(name.len() > 5);
        let file_name = format!("{}.spirv", &name[..name.len() - 5]);

        let project_cargo_root = env::var("CARGO_MANIFEST_DIR").unwrap();

        let target_dir = Path::new(&project_cargo_root).join("target");
        println!("creating directory: {:?}", target_dir);
        fs::create_dir_all(&target_dir).expect("a directory");

        let target = target_dir.join(file_name);
        println!("generating: {:?}", target);
        target.to_str().expect("a file").to_owned()
    }

    fn decorate_error(msg: &str) -> String {
        msg.replace(" error: ", " \x1B[91merror\x1B[0m: ")
    }

    fn find_included_file(
        name: &str,
        _include_type: IncludeType,
        _source_file: &str,
        _include_depth: usize,
    ) -> Result<ResolvedInclude, String> {
        let project_cargo_root = env::var("CARGO_MANIFEST_DIR").unwrap();
        let libs_dir = Path::new(&project_cargo_root)
            .parent()
            .expect("non-root")
            .parent()
            .expect("non-root")
            .parent()
            .expect("non-root");
        assert_eq!(libs_dir.file_stem().expect("non-root"), "libs");
        let include_dirs = vec![libs_dir.join("render-wgpu")];
        let input_path: PathBuf = name.split('/').collect();
        println!("Using include dirs: {:?}", include_dirs);
        for path in &include_dirs {
            let attempt = path.join(&input_path);
            println!("Checking: {:?}", attempt);
            if attempt.is_file() {
                return Ok(ResolvedInclude {
                    resolved_name: attempt.to_str().expect("a path").to_owned(),
                    content: fs::read_to_string(attempt).expect("file content"),
                });
            }
        }
        Err("NOT_FOUND".to_owned())
    }

    pub fn build() -> Fallible<()> {
        println!("cargo:rerun-if-changed=shaders");
        println!("cargo:rerun-if-env-changed=DUMP_SPIRV");
        println!("cargo:rerun-if-env-changed=DEBUG");

        let shader_dir = Path::new("shaders/");
        if !shader_dir.is_dir() {
            return Ok(());
        }

        for entry in fs::read_dir(shader_dir)? {
            let entry = entry?;
            let pathbuf = entry.path();
            let path = pathbuf.to_str().expect("a filename");
            if !pathbuf.is_file() {
                continue;
            }

            let shader_content = fs::read_to_string(&pathbuf)?;
            let shader_type = Self::type_for_filename(&path);

            let mut options = CompileOptions::new().expect("some options");
            options.set_warnings_as_errors();
            let opt_level = if env::var("DEBUG").unwrap_or("0".to_owned()) == "1" {
                options.set_generate_debug_info();
                OptimizationLevel::Zero
            } else {
                OptimizationLevel::Performance
            };
            options.set_optimization_level(opt_level);
            options.set_include_callback(Self::find_included_file);

            let mut compiler = Compiler::new().expect("a compiler");
            let result = compiler.compile_into_spirv(
                &shader_content,
                shader_type,
                path,
                "main",
                Some(&options),
            );
            match &result {
                Err(Error::CompilationError(_, msg)) => println!("{}", Self::decorate_error(msg)),
                _ => {}
            };
            let spirv = result?;
            let target_path = Self::output_for_name(
                pathbuf
                    .file_name()
                    .expect("a file name")
                    .to_str()
                    .expect("a string"),
            );
            fs::write(&target_path, spirv.as_binary_u8())?;

            if env::var("DUMP_SPIRV").unwrap_or("0".to_owned()) == "1" {
                let spirv_assembly = compiler.compile_into_spirv_assembly(
                    &shader_content,
                    shader_type,
                    path,
                    "main",
                    Some(&options),
                )?;
                fs::write("out.spirv.s", spirv_assembly.as_text())?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() -> Fallible<()> {
        BuildShaders::build()
    }
}
