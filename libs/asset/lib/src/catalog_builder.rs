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
use crate::{LibDrawer, Priority, GAME_INFO};
use anyhow::Result;
use catalog::{Catalog, DirectoryDrawer, FileId};
use glob::{MatchOptions, Pattern};
use log::debug;
use std::{env, fs, path::PathBuf};

// FA Engine aware lookup of asset files. This can run in several modes:
//   1) Collect all games as tags under test_data/packed and add lib drawers
//   2) Collect all games as tags under test_data/unpacked and add dir drawers
//   3) Collect all files under a single tag in the current directory.
pub struct CatalogBuilder;

impl CatalogBuilder {
    pub fn build() -> Result<Catalog> {
        let mut catalog = Catalog::empty();

        let cwd = env::current_dir()?;

        // Load any T2 and LIBs in the current directory under the default label.
        catalog.add_drawer(DirectoryDrawer::from_directory_with_extension(
            101, &cwd, "t2",
        )?)?;
        for entry in (fs::read_dir(&cwd)?).flatten() {
            if let Some(ext) = entry.path().extension() {
                if ext.to_string_lossy().to_ascii_lowercase() == "lib" {
                    catalog.add_drawer(LibDrawer::from_path(100, &entry.path())?)?;
                }
            }
        }

        // Look up the test directories and load each in a label.
        let test_data_dir = Self::find_test_data_dir(cwd);
        debug!("using test data directory: {:?}", test_data_dir);
        if let Some(test_dir) = test_data_dir {
            let base_pack_dir = test_dir.join("packed");
            let base_loose_dir = test_dir.join("unpacked");
            for &game in &GAME_INFO {
                let pack_dir = base_pack_dir.join(game.test_dir);
                for entry in fs::read_dir(&pack_dir)?.flatten() {
                    if let Some(ext) = entry.path().extension() {
                        if ext.to_string_lossy().to_ascii_lowercase() == "lib" {
                            let priority =
                                Priority::from_path(&entry.path(), 0)?.as_drawer_priority();
                            catalog.add_labeled_drawer(
                                &game.packed_label(),
                                LibDrawer::from_path(priority, &entry.path())?,
                            )?;
                        }
                    }
                    if let Some(name) = entry.path().file_name() {
                        if name.to_string_lossy() == "installdir" {
                            catalog.add_labeled_drawer(
                                &game.packed_label(),
                                DirectoryDrawer::from_directory(102, &entry.path())?,
                            )?;
                        }
                    }
                }

                let loose_dir = base_loose_dir.join(game.test_dir);
                for entry in (fs::read_dir(&loose_dir)?).flatten() {
                    if let Some(ext) = entry.path().extension() {
                        if ext.to_string_lossy().to_ascii_lowercase() == "lib" {
                            let priority =
                                Priority::from_path(&entry.path(), 0)?.as_drawer_priority();
                            catalog.add_labeled_drawer(
                                &game.unpacked_label(),
                                DirectoryDrawer::from_directory(priority, &entry.path())?,
                            )?;
                        }
                    }
                    if let Some(name) = entry.path().file_name() {
                        if name.to_string_lossy() == "installdir" {
                            catalog.add_labeled_drawer(
                                &game.unpacked_label(),
                                DirectoryDrawer::from_directory(102, &entry.path())?,
                            )?;
                        }
                    }
                }
            }
        }

        Ok(catalog)
    }

    /// Label-aware matching of diverse inputs, with globbing.
    pub fn select(catalog: &mut Catalog, inputs: &[String]) -> Result<Vec<FileId>> {
        let mut selected = Vec::new();
        let fuzzy = MatchOptions {
            case_sensitive: false,
            require_literal_leading_dot: false,
            require_literal_separator: true,
        };
        for input in inputs {
            // Expand input into a game match and a name match.
            let (game_input, name_input) = if input.contains(':') {
                let parts = input.split(':').collect::<Vec<_>>();
                (parts[parts.len() - 2], parts[parts.len() - 1].to_owned())
            } else {
                (catalog.default_label(), input.to_owned())
            };

            // Match against all games.
            let game_pattern = Pattern::new(game_input)?;
            for game in &GAME_INFO {
                let game_label = game.label();
                catalog.set_default_label(&game_label);
                if game_pattern.matches_with(game.test_dir, &fuzzy) {
                    let matching = catalog.find_labeled_matching(&game_label, &name_input, None)?;
                    selected.extend_from_slice(&matching);
                }
            }
        }
        Ok(selected)
    }

    pub fn build_and_select(inputs: &[String]) -> Result<(Catalog, Vec<FileId>)> {
        let mut catalog = Self::build()?;
        let selected = Self::select(&mut catalog, inputs)?;
        Ok((catalog, selected))
    }

    fn find_test_data_dir(mut cwd: PathBuf) -> Option<PathBuf> {
        loop {
            if cwd.join("test_data").exists() {
                return Some(cwd.join("test_data"));
            }
            if let Some(next) = cwd.parent() {
                cwd = next.to_owned();
            } else {
                break;
            }
        }
        None
    }
}
