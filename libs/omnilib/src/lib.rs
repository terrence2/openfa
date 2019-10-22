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
use failure::{bail, err_msg, Fallible};
use lib::Library;
use log::trace;
use std::{
    collections::HashMap,
    env,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Create a StructOpt that contains the default path loader and whatever else is given.
#[macro_export]
macro_rules! make_opt_struct {
    (#[$opt_struct_options:meta]
     $opt_struct_name:ident {
        $(
            #[$structopt_options:meta]
            $opt_name:ident => $opt_type:ty
        ),*
    }) => {
        #[derive(Debug, StructOpt)]
        #[$opt_struct_options]
        struct $opt_struct_name {
            #[structopt(
                short = "g",
                long = "game-dir",
                help = "The location of the game directory if not pwd."
            )]
            omni_game_dir: Option<::std::path::PathBuf>,

            $(
                #[$structopt_options]
                $opt_name: $opt_type
            ),*
        }

        impl $opt_struct_name {
            // Return true if the input is of the form {game}:{name}.
            fn omni_from_test(&self, omni_input: &str) -> bool {
                omni_input.contains(':')
            }

            fn load_omni_for_inputs(&self, omni_input: &str) -> Fallible<OmniLib> {
                // FIXME: if in test most, load all libs references, rather than just the first
                Ok(if self.omni_from_test(omni_input) {
                    OmniLib::new_for_test()
                } else {
                    if let Some(ref game_dir) = self.omni_game_dir {
                        OmniLib::new_for_game_directory(&game_dir)
                    } else {
                        OmniLib::new_for_game_directory(&::std::env::current_dir()?)
                    }
                }?)
            }

            fn expand_inputs(&self, omni_input: &str, omni: &OmniLib) -> Fallible<Vec<(String, String)>> {
                // If the name is in a : form, it is a game:name pair.
                Ok(if omni_input.contains(':') {
                    let parts = omni_input.splitn(2, ':').collect::<Vec<_>>();
                    ::failure::ensure!(
                        parts.len() == 2,
                        "expected two parts in file spec with a colon"
                    );
                    if parts[0] == "*" {
                        omni.find_matching(parts[1])?
                    } else {
                        omni.library(&parts[0].to_uppercase())
                            .find_matching(parts[1])?
                            .drain(..)
                            .map(|s| (parts[0].to_owned(), s))
                            .collect::<Vec<_>>()
                    }
                } else {
                    omni.find_matching(&omni_input)?
                })
            }

            pub fn find_input(&self, omni_input: &str) -> Fallible<(OmniLib, String, String)> {
                let omni = self.load_omni_for_inputs(omni_input)?;
                let inputs = self.expand_inputs(omni_input, &omni)?;
                ::failure::ensure!(inputs.len() > 0, "expected a single input, but expanded to none");
                ::failure::ensure!(inputs.len() == 1, "expected a single input, but expanded to many");
                Ok((omni, inputs[0].0.to_owned(), inputs[0].1.to_owned()))
            }

            pub fn find_inputs(&self, omni_inputs: &[String]) -> Fallible<(OmniLib, Vec<(String, String)>)> {
                let omni = self.load_omni_for_inputs(&omni_inputs[0])?;
                let mut inputs = Vec::new();
                for omni_input in omni_inputs {
                    let mut expanded_inputs = self.expand_inputs(omni_input, &omni)?;
                    inputs.append(&mut expanded_inputs);
                }
                Ok((omni, inputs))
            }
        }
    }
}

/// Hold multiple LibStacks at once: e.g. for visiting resources from multiple games at once.
pub struct OmniLib {
    libraries: HashMap<String, Arc<Box<Library>>>,
}

// Tests run dramatically slower when using the libs because we cannot force sub-libraries to
// be built with optimizations if we are building the using library without them.
const USE_LIB: bool = false;

impl OmniLib {
    pub fn new_for_test() -> Fallible<Self> {
        Self::new_for_test_in_games(&["FA", "USNF97", "ATFGOLD", "ATFNATO", "ATF", "MF", "USNF"])
    }

    pub fn new_for_test_in_games(dirs: &[&str]) -> Fallible<Self> {
        let test_data_dir = Self::find_test_data_dir()?;
        let mut libraries = HashMap::new();
        for &dir in dirs {
            trace!("adding libraries for {}", dir);

            let libs = if USE_LIB {
                let path = test_data_dir.join("packed").join(dir);
                Library::from_file_search(&path)?
            } else {
                let path = test_data_dir.join("unpacked").join(dir);
                Library::from_dir_search(&path)?
            };
            libraries.insert(dir.to_owned(), Arc::new(Box::new(libs)));
        }
        Ok(Self { libraries })
    }

    fn find_test_data_dir() -> Fallible<PathBuf> {
        let mut cwd = env::current_dir()?;
        loop {
            if cwd.join("test_data").exists() {
                return Ok(cwd.join("test_data"));
            }
            if let Some(next) = cwd.parent() {
                cwd = next.to_owned();
            } else {
                break;
            }
        }
        bail!("did not find test_data directory")
    }

    pub fn new_for_game_directory(path: &Path) -> Fallible<Self> {
        let game = path
            .file_name()
            .ok_or_else(|| err_msg("omnilib: no file name"))?
            .to_str()
            .ok_or_else(|| err_msg("omnilib: file name not utf8"))?
            .to_uppercase()
            .to_owned();

        let mut libraries = HashMap::new();
        if let Ok(libs) = Library::from_dir_search(path) {
            if libs.num_libs() > 0 {
                trace!("loaded {} libdirs in game: {}", libs.num_libs(), game);
                libraries.insert(game, Arc::new(Box::new(libs)));
                return Ok(Self { libraries });
            }
        }
        let libs = Library::from_file_search(path)?;
        trace!("loaded {} libfiles in game: {}", libs.num_libs(), game);
        libraries.insert(game, Arc::new(Box::new(libs)));
        Ok(Self { libraries })
    }

    pub fn find_matching(&self, glob: &str) -> Fallible<Vec<(String, String)>> {
        let mut out = Vec::new();
        for (libname, libs) in self.libraries.iter() {
            let names = libs.find_matching(glob)?;
            for name in names {
                out.push((libname.to_owned(), name));
            }
        }
        out.sort();
        Ok(out)
    }

    pub fn libraries(&self) -> Vec<(String, Arc<Box<Library>>)> {
        self.libraries
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    pub fn library(&self, libname: &str) -> Arc<Box<Library>> {
        self.libraries[libname].clone()
    }

    pub fn path(&self, libname: &str, name: &str) -> Fallible<String> {
        Ok(self
            .library(libname)
            .stat(name)?
            .path
            .ok_or_else(|| err_msg("no path for name"))?
            .to_str()
            .ok_or_else(|| err_msg("path with invalid characters"))?
            .to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_omnilib_from_dir() -> Fallible<()> {
        let omni = OmniLib::new_for_test()?;
        let palettes = omni.find_matching("PALETTE.PAL")?;
        assert!(!palettes.is_empty());
        Ok(())
    }

    #[test]
    fn test_omnilib_from_game_libdirs() -> Fallible<()> {
        let omni = OmniLib::new_for_game_directory(Path::new("../../test_data/unpacked/FA"))?;
        let tests = omni.find_matching("PALETTE.PAL")?;
        assert!(!tests.is_empty());
        let (game, name) = tests.first().unwrap();
        assert!(game == "FA");
        assert!(name == "PALETTE.PAL");
        Ok(())
    }

    #[test]
    fn test_omnilib_from_game_libfiles() -> Fallible<()> {
        let omni = OmniLib::new_for_game_directory(Path::new("../../test_data/packed/FA"))?;
        let tests = omni.find_matching("PALETTE.PAL")?;
        assert!(!tests.is_empty());
        let (game, name) = tests.first().unwrap();
        assert!(game == "FA");
        assert!(name == "PALETTE.PAL");
        Ok(())
    }
}
