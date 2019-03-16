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
use failure::{err_msg, Fallible};
use lib::Library;
use log::trace;
use std::{collections::HashMap, path::Path, sync::Arc};

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
                short = "t",
                long = "from-test",
                help = "Treat the given path as a test reference."
            )]
            omni_from_test: bool,

            #[structopt(
                short = "g",
                long = "game-dir",
                help = "The location of the game directory if not pwd."
            )]
            omni_game_dir: Option<::std::path::PathBuf>,

            #[structopt(help = "The component to load either from the libs in the current directory.")]
            omni_input: String,

            $(
                #[$structopt_options]
                $opt_name: $opt_type
            ),*
        }

        impl $opt_struct_name {
            pub fn find_inputs(&self) -> Fallible<(OmniLib, Vec<(String, String)>)> {
                // Load relevant libraries.
                ::failure::ensure!(
                    !self.omni_from_test || self.omni_game_dir.is_none(),
                    "only one of -t or -g is allowed"
                );
                let omni = if self.omni_from_test {
                    OmniLib::new_for_test()
                } else {
                    if let Some(ref game_dir) = self.omni_game_dir {
                        OmniLib::new_for_game_directory(&game_dir)
                    } else {
                        OmniLib::new_for_game_directory(&::std::env::current_dir()?)
                    }
                }?;

                // If the name is in a : form, it is a game:name pair.
                let inputs = if self.omni_input.contains(':') {
                    let parts = self.omni_input.splitn(2, ':').collect::<Vec<_>>();
                    ::failure::ensure!(
                        parts.len() == 2,
                        "expected two parts in file spec with a colon"
                    );
                    omni.library(&parts[0].to_uppercase())
                        .find_matching(parts[1])?
                        .drain(..)
                        .map(|s| (parts[0].to_owned(), s))
                        .collect::<Vec<_>>()
                } else {
                    omni.find_matching(&self.omni_input)?
                };

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
        let mut libraries = HashMap::new();
        for &dir in dirs {
            trace!("adding libraries for {}", dir);

            let libs = if USE_LIB {
                let path = Path::new("../../test_data/packed/").join(dir);
                Library::from_file_search(&path)?
            } else {
                let path = Path::new("../../test_data/unpacked/").join(dir);
                Library::from_dir_search(&path)?
            };
            libraries.insert(dir.to_owned(), Arc::new(Box::new(libs)));
        }
        Ok(Self { libraries })
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
        return Ok(Self { libraries });
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

    pub fn libraries(&self) -> Vec<Arc<Box<Library>>> {
        self.libraries.values().cloned().collect()
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
