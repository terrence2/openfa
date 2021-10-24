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
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
};
use structopt::StructOpt;

/// Include this with #[structopt(flatten)] to provide cli arguments to CatalogManager for game discovery.
#[derive(Debug, StructOpt)]
pub struct CatalogOpts {
    /// The path to look in for game files (default: pwd)
    #[structopt(short, long)]
    game_path: Option<PathBuf>,

    /// If not all required libs are found in the game path, look here. If the CD's LIB files have
    /// been copied into the game directory, this is unused.
    #[structopt(short, long)]
    cd_path: Option<PathBuf>,

    /// For Fighter's Anthology, if the second disk's LIB files have not been copied into the game
    /// directory, and you want to use the reference materials, also provide this path. There is no
    /// ability to switch the disk, currently. (Note: reference still WIP, so not much point yet.)
    #[structopt(long)]
    cd2_path: Option<PathBuf>,

    /// Extra directories to treat as libraries
    #[structopt(short, long)]
    lib_paths: Vec<PathBuf>,

    /// Select the game, if there is more than one available (e.g. in test mode)
    #[structopt(short = "S", long)]
    select_game: Option<String>,
}

/// Search for artifacts and create catalogs for any and every game we
/// can get our hands on.
pub struct CatalogManager {
    selected_game: Option<usize>,
    catalogs: Vec<(&'static GameInfo, Catalog)>,
}

impl CatalogManager {
    /// Find out what we have to work with.
    pub fn bootstrap(
        opts: &CatalogOpts, // game_path: Option<PathBuf>,
                            // cd_path: Option<PathBuf>,
                            // cd2_path: Option<PathBuf>,
                            // lib_paths: &[PathBuf]
    ) -> Result<Self> {
        // If we didn't specify a path, use cwd.
        let game_path = if let Some(path) = &opts.game_path {
            path.to_owned()
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
                if let Some(cd_path) = &opts.cd_path {
                    for name in Self::list_directory_canonical(cd_path)?.drain() {
                        all_cd_files.insert(name);
                    }
                }
                if let Some(cd2_path) = &opts.cd2_path {
                    for name in Self::list_directory_canonical(cd2_path)?.drain() {
                        all_cd_files.insert(name);
                    }
                }

                if game.cd_libs.iter().all(|&path| all_cd_files.contains(path)) {
                    if let Some(cd_path) = &opts.cd_path {
                        Self::populate_catalog(game, cd_path, -10, &mut catalog)?;
                    }
                    if let Some(cd2_path) = &opts.cd2_path {
                        Self::populate_catalog(game, cd2_path, -20, &mut catalog)?;
                    }
                } else {
                    match (&opts.cd_path, &opts.cd2_path) {
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
            for (i, lib_path) in opts.lib_paths.iter().enumerate() {
                catalog.add_drawer(DirectoryDrawer::from_directory(i as i64 + 1, lib_path)?)?;
            }

            Ok(Self {
                selected_game: Some(0),
                catalogs: vec![(game, catalog)],
            })
        } else {
            println!(
                "Did not detect any games in {}, falling back to test mode...",
                game_path.to_string_lossy()
            );
            Self::for_testing(opts)
        }
    }

    /// Build a new catalog manager with whatever test data we can scrounge up.
    pub fn for_testing(opts: &CatalogOpts) -> Result<Self> {
        // Look up until we find the disk_dumps directory or run out of up.
        let mut test_path = env::current_dir()?;
        test_path.push("disk_dumps");
        while !test_path.exists() && test_path.to_string_lossy() != "/disk_dumps" {
            test_path.pop();
            test_path.pop();
            test_path.push("disk_dumps");
        }
        ensure!(test_path.to_string_lossy() != "/disk_dumps");

        let mut catalogs = vec![];

        // Find games under disk_dumps
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
                    catalogs.push((game, game_catalog));
                }
            }
        }

        let mut selected_game = None;
        if let Some(selected) = &opts.select_game {
            for (i, (game, _)) in catalogs.iter().enumerate() {
                if game.test_dir == selected {
                    selected_game = Some(i);
                    break;
                }
            }
        }

        Ok(Self {
            selected_game,
            catalogs,
        })
    }

    pub fn all(&self) -> impl Iterator<Item = &(&'static GameInfo, Catalog)> + '_ {
        self.catalogs.iter()
    }

    pub fn selected(&self) -> Box<dyn Iterator<Item = &(&'static GameInfo, Catalog)> + '_> {
        if let Some(selected) = self.selected_game {
            Box::new(self.catalogs.iter().skip(selected).take(1))
        } else {
            Box::new(self.all())
        }
    }

    pub fn best(&self) -> &Catalog {
        &self.catalogs[self.selected_game.unwrap_or(0)].1
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
