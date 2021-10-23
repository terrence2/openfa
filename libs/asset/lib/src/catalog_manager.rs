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
use crate::{
    game_info::{GameInfo, GAME_INFO},
    LibDrawer, Priority,
};
use anyhow::{bail, ensure, Result};
use catalog::{Catalog, DirectoryDrawer};
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    path::{Path, PathBuf},
};

/// Search for artifacts and create catalogs for any and every game we
/// can get our hands on.
pub struct CatalogManager {
    catalogs: HashMap<String, Catalog>,
}

impl CatalogManager {
    /// Find out what we have to work with.
    pub fn bootstrap(
        game_path: Option<PathBuf>,
        cd_path: Option<PathBuf>,
        cd2_path: Option<PathBuf>,
        lib_paths: &[PathBuf],
    ) -> Result<Self> {
        let mut catalogs = HashMap::new();

        // If we didn't specify a path, use cwd.
        let game_path = if let Some(path) = game_path {
            path
        } else {
            env::current_dir()?
        };
        let game_files = Self::list_directory_canonical(&game_path)?;

        // Search for the game so we can figure out what is required to be loaded.
        if let Some(game) = Self::detect_game_from_files(&game_files) {
            // Load libs from the installdir
            let mut catalog = Catalog::empty();
            Self::populate_catalog(game, &game_path, 0, &mut catalog)?;

            // If the user has not copied over the CD libs, we need to search for them.
            if !game.cd_libs.iter().all(|&name| game_files.contains(name)) {
                // Accumulate all CD files we can find so we check if we have everything.
                let mut all_cd_files = HashSet::new();
                if let Some(cd_path) = &cd_path {
                    for name in Self::list_directory_canonical(cd_path)?.drain() {
                        all_cd_files.insert(name);
                    }
                }
                if let Some(cd2_path) = &cd2_path {
                    for name in Self::list_directory_canonical(cd2_path)?.drain() {
                        all_cd_files.insert(name);
                    }
                }

                if game.cd_libs.iter().all(|&path| all_cd_files.contains(path)) {
                    if let Some(cd_path) = &cd_path {
                        Self::populate_catalog(game, cd_path, -10, &mut catalog)?;
                    }
                    if let Some(cd2_path) = &cd2_path {
                        Self::populate_catalog(game, cd2_path, -20, &mut catalog)?;
                    }
                } else {
                    match (&cd_path, &cd2_path) {
                        (Some(p1), Some(p2)) => bail!(
                            "Did not find all expected CD LIBs in {} and {} for {}",
                            p1.to_string_lossy(),
                            p2.to_string_lossy(),
                            game.name,
                        ),
                        (Some(p1), None) => bail!(
                            "Did not find all expected CD LIBs in {} for {}",
                            p1.to_string_lossy(),
                            game.name,
                        ),
                        (None, None) => bail!(
                            "Did not find CD LIBs for {}; please indicate the CD path(s)!",
                            game.name
                        ),
                        (None, Some(_)) => bail!("You must provide a CD1 path before a CD2 path!"),
                    }
                }
                println!("Detected {} in game and CD paths...", game.name);
            } else {
                println!("Detected {} in game path...", game.name);
            }

            // Load any additional libdirs into the catalog
            for (i, lib_path) in lib_paths.iter().enumerate() {
                catalog.add_drawer(DirectoryDrawer::from_directory(i as i64 + 1, lib_path)?)?;
            }

            catalogs.insert(game.test_dir.to_owned(), catalog);
        } else {
            println!(
                "Did not detect any games in {}",
                game_path.to_string_lossy()
            );

            // Look for test directories
            let mut test_path = env::current_dir()?;
            test_path.push("disk_dumps");
            for game in GAME_INFO {
                let mut game_path = test_path.clone();
                game_path.push(game.test_dir);
                let mut installdir = game_path.clone();
                installdir.push("installdir");
                let mut cdrom1dir = game_path.clone();
                cdrom1dir.push("cdrom1");
                let mut cdrom2dir = game_path.clone();
                cdrom2dir.push("cdrom2");
                if let Ok(game_files) = Self::list_directory_canonical(&installdir) {
                    if let Some(detected_game) = Self::detect_game_from_files(&game_files) {
                        ensure!(
                            detected_game.name == game.name,
                            "unexpected game in game's test_dir"
                        );
                        let mut game_catalog = Catalog::empty();
                        Self::populate_catalog(game, &installdir, 1, &mut game_catalog)?;
                        Self::populate_catalog(game, &cdrom1dir, 0, &mut game_catalog)?;
                        if cdrom2dir.exists() {
                            Self::populate_catalog(game, &cdrom2dir, 0, &mut game_catalog)?;
                        }
                        catalogs.insert(game.test_dir.to_owned(), game_catalog);
                    }
                }
            }
        }

        return Ok(Self { catalogs });
    }

    fn populate_catalog(
        game: &'static GameInfo,
        path: &Path,
        priority_adjust: i64,
        catalog: &mut Catalog,
    ) -> Result<()> {
        if !game.allow_packed_t2 {
            catalog.add_drawer(DirectoryDrawer::from_directory_with_extension(
                Priority::from_path(path, priority_adjust)?.as_drawer_priority(),
                &path,
                "t2",
            )?)?;
        }
        for entry in (fs::read_dir(&path)?).flatten() {
            if let Some(ext) = entry.path().extension() {
                if ext.to_string_lossy().to_ascii_lowercase() == "lib" {
                    catalog.add_drawer(LibDrawer::from_path(
                        Priority::from_path(&entry.path(), priority_adjust)?.as_drawer_priority(),
                        &entry.path(),
                    )?)?;
                } else if ext.to_string_lossy().to_ascii_lowercase() == "l_b" {
                    catalog.add_drawer(DirectoryDrawer::from_directory(
                        Priority::from_path(&entry.path(), priority_adjust + 1)?
                            .as_drawer_priority(),
                        &entry.path(),
                    )?)?;
                }
            }
        }
        Ok(())
    }

    fn detect_game_from_files(game_files: &HashSet<String>) -> Option<&'static GameInfo> {
        for game in GAME_INFO {
            if game
                .unique_files
                .iter()
                .all(|&name| game_files.contains(name))
            {
                return Some(game);
            }
        }
        None
    }

    fn list_directory_canonical(path: &Path) -> Result<HashSet<String>> {
        // Filename capitalization is :shrug: so list and canonicalize everything all instead of
        // trying to rely on sane or predicable behavior.
        let mut game_files = HashSet::new();
        for p in fs::read_dir(path)? {
            if let Ok(p) = p {
                game_files.insert(
                    p.file_name()
                        .to_ascii_uppercase()
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
        Ok(game_files)
    }
}
