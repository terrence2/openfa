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
use anyhow::{anyhow, bail, ensure, Result};
use catalog::{Catalog, CatalogOpts, DirectoryDrawer, FileId};
use glob::{glob_with, MatchOptions};
use log::{error, info, trace, warn};
use nitrous::{inject_nitrous_resource, method, NitrousResource};
use pal::Palette;
use runtime::{Extension, Runtime};
use std::{
    borrow::Cow,
    collections::HashSet,
    env,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};
use structopt::StructOpt;

#[derive(Clone, Debug, Default, StructOpt)]
pub struct LibsOpts {
    /// The path to look in for game files (default: pwd)
    #[structopt(short, long)]
    game_path: Option<PathBuf>,

    /// If not all required libs are found in the game path, look here. If the
    /// CD's LIB files have been copied into the game directory, this is unused.
    #[structopt(short, long)]
    cd_path: Option<PathBuf>,

    /// For Fighter's Anthology, if the second disk's LIB files have not been
    /// copied into the game directory, and you want to use the reference
    /// materials, also provide this path. There is no ability to switch the
    /// disk, currently. (Note: reference still WIP, so not much point yet.)
    #[structopt(long)]
    cd2_path: Option<PathBuf>,

    /// Extra directories to treat as libraries
    #[structopt(short = "-l", long)]
    lib_paths: Vec<PathBuf>,

    /// Select the game, if there is more than one available (e.g. in test mode)
    #[structopt(short = "S", long)]
    select_game: Option<String>,
}

/// Search for artifacts and create catalogs for any and every game we
/// can get our hands on.
#[derive(NitrousResource)]
pub struct Libs {
    selected_game: Option<usize>,
    catalogs: Vec<(&'static GameInfo, Palette, Catalog)>,
}

impl Extension for Libs {
    type Opts = LibsOpts;

    fn init(runtime: &mut Runtime, opts: LibsOpts) -> Result<()> {
        let extra_paths = opts.lib_paths.clone();
        let libs = Libs::bootstrap(&opts)?;
        runtime.insert_named_resource("libs", libs);

        runtime.load_extension_with::<Catalog>(CatalogOpts::from_extra_paths(extra_paths))?;

        Ok(())
    }
}

#[inject_nitrous_resource]
impl Libs {
    /// For some tools, the input list may be a empty (in which case glob), or a set of files,
    /// in which case return the set of paths, or a list of names of lib entries.
    /// If no files match, this returns None, otherwise Some and the set of matching files.
    pub fn input_files(inputs: &[String], glob_pattern: &str) -> Result<Vec<PathBuf>> {
        Ok(if inputs.is_empty() {
            glob_with(
                glob_pattern,
                MatchOptions {
                    case_sensitive: false,
                    require_literal_separator: false,
                    require_literal_leading_dot: false,
                },
            )?
            .filter_map(|v| v.ok())
            .collect()
        } else {
            inputs
                .iter()
                .map(|v| Path::new(v).to_owned())
                .filter(|v| v.exists())
                .collect()
        })
    }

    /// Find out what we have to work with.
    pub fn bootstrap(opts: &LibsOpts) -> Result<Self> {
        let mut libs = Self::bootstrap_inner(opts)?;

        // Load any additional libdirs into the catalog
        for (_, _, catalog) in libs.catalogs.iter_mut() {
            for (i, lib_path) in opts.lib_paths.iter().enumerate() {
                catalog.add_drawer(DirectoryDrawer::from_directory(i as i64 + 1, lib_path)?)?;
            }
        }

        Ok(libs)
    }

    fn bootstrap_inner(opts: &LibsOpts) -> Result<Self> {
        // If we specified a path manually, expect to find a game there and fail otherwise.
        if let Some(path) = &opts.game_path {
            info!("Requiring game to be in specified path: {:?}", path);
            let game = Self::detect_game_in_path(path)?
                .ok_or_else(|| anyhow!("no game in command line game path"))?;
            return Self::bootstrap_game_in_path(opts, game, path);
        }

        // If we didn't specify a path, try cwd first, in case we run from some other directory.
        let current_dir = env::current_dir()?;
        info!("Looking for game in CWD: {:?}", current_dir);
        if let Some(game) = Self::detect_game_in_path(&current_dir)? {
            info!("Loading game files in CWD: {:?}", current_dir);
            if let Ok(v) = Self::bootstrap_game_in_path(opts, game, &current_dir) {
                return Ok(v);
            }
        }

        // If we failed to find a game in cwd, try the exe directory
        let exe_dir = env::current_exe()?
            .parent()
            .map(|v| v.to_owned())
            .ok_or_else(|| anyhow!("OpenFA should not be run in the root directory"))?;
        info!("Looking for game in exe path: {:?}", exe_dir);
        if let Some(game) = Self::detect_game_in_path(&exe_dir)? {
            info!("Loading game files in exe path: {:?}", exe_dir);
            if let Ok(v) = Self::bootstrap_game_in_path(opts, game, &exe_dir) {
                return Ok(v);
            }
        }

        // If we cannot find a game anywhere, try seeing if this is a test environment.
        info!("Did not detect any games, falling back to test mode...",);
        let mut libs = Self::for_testing()?;
        if let Some(selected) = &opts.select_game {
            for (i, (game, _, _)) in libs.catalogs.iter().enumerate() {
                if game.test_dir == selected {
                    libs.selected_game = Some(i);
                    break;
                }
            }
        }

        Ok(libs)
    }

    fn bootstrap_game_in_path(
        opts: &LibsOpts,
        game: &'static GameInfo,
        game_path: &Path,
    ) -> Result<Self> {
        // Load libs from the installdir
        let mut catalog = Catalog::empty(game.test_dir);
        Self::populate_catalog(game, game_path, 0, &mut catalog)?;

        // If the user has not copied over the CD libs, we need to search for them.
        if !Self::found_cd_libs(game, &Self::list_directory_canonical(game_path)?) {
            let cd1_path = opts
                .cd_path
                .to_owned()
                .or_else(|| Self::detect_cd_path(game.cd_libs[0]))
                .ok_or_else(|| anyhow!("failed to find CD1 path"))?;
            info!("Using CD1 path: {:?}", cd1_path);
            Self::populate_catalog(game, &cd1_path, -10, &mut catalog)?;

            if let Some(&sentinel) = game.optional_cd_libs.first() {
                if let Some(cd2_path) = opts
                    .cd2_path
                    .to_owned()
                    .or_else(|| Self::detect_cd_path(sentinel))
                {
                    info!("Using CD2 path: {:?}", cd2_path);
                    Self::populate_catalog(game, &cd2_path, -20, &mut catalog)?;
                } else {
                    warn!("Failed to find CD2 path");
                }
            }
        }

        info!("Successfully found {} at {:?}...", game.name, game_path);

        let palette = Palette::from_bytes(catalog.read_name("PALETTE.PAL")?.as_ref())?;

        Ok(Self {
            selected_game: Some(0),
            catalogs: vec![(game, palette, catalog)],
        })
    }

    /// Build a new catalog manager with whatever test data we can scrounge up.
    pub fn for_testing() -> Result<Self> {
        // Look up until we find the disk_dumps directory or run out of up.
        let mut test_path = env::current_dir()?;
        test_path.push("disk_dumps");
        trace!("Looking for disk_dumps at: {}", test_path.to_string_lossy());
        while !test_path.exists() {
            test_path.pop();
            let at_top = !test_path.pop();
            test_path.push("disk_dumps");
            if at_top {
                error!("No disk_dumps directory in path");
                break;
            }
            trace!("Looking for disk_dumps at: {}", test_path.to_string_lossy());
        }
        ensure!(
            test_path.exists(),
            "Unable to find the 'disk_dumps' directory for testing"
        );

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
                    let mut game_catalog = Catalog::empty(game.test_dir);
                    Self::populate_catalog(game, &installdir, 1, &mut game_catalog)?;
                    Self::populate_catalog(game, &cdrom1dir, 0, &mut game_catalog)?;
                    if cdrom2dir.exists() {
                        Self::populate_catalog(game, &cdrom2dir, 0, &mut game_catalog)?;
                    }
                    let palette =
                        Palette::from_bytes(game_catalog.read_name("PALETTE.PAL")?.as_ref())?;
                    catalogs.push((game, palette, game_catalog));
                }
            }
        }

        Ok(Self {
            selected_game: None,
            catalogs,
        })
    }

    #[method]
    fn list_games(&self) -> String {
        let mut s = String::new();
        for (game, _, _) in &self.catalogs {
            writeln!(
                s,
                "{:>7} - {} ({})",
                game.test_dir,
                game.long_name,
                game.released_at()
            )
            .ok();
        }
        s
    }

    #[method]
    fn select_game(&mut self, name: &str) -> Result<()> {
        for (i, (game, _, _)) in self.catalogs.iter().enumerate() {
            if game.test_dir == name {
                self.selected_game = Some(i);
                return Ok(());
            }
        }
        bail!(
            "did not find {}; use libs.list_games() to see what is available",
            name
        )
    }

    #[method]
    fn find(&self, glob: &str) -> Result<String> {
        let mut s = String::new();
        for fid in self.catalog().find_glob(glob)? {
            let stat = self.catalog().stat(fid)?;
            writeln!(
                s,
                "{:<11} {:>4} {:>6}",
                stat.name(),
                stat.compression().unwrap_or("none"),
                stat.unpacked_size()
            )
            .ok();
        }
        Ok(s)
    }

    pub fn all(&self) -> impl Iterator<Item = (&'static GameInfo, &Palette, &Catalog)> + '_ {
        self.catalogs
            .iter()
            .map(|(gi, palette, catalog)| (*gi, palette, catalog))
    }

    pub fn selected(
        &self,
    ) -> Box<dyn Iterator<Item = (&'static GameInfo, &Palette, &Catalog)> + '_> {
        if let Some(selected) = self.selected_game {
            Box::new(
                self.catalogs
                    .iter()
                    .skip(selected)
                    .take(1)
                    .map(|(gi, palette, catalog)| (*gi, palette, catalog)),
            )
        } else {
            Box::new(self.all())
        }
    }

    pub fn catalog(&self) -> &Catalog {
        &self.catalogs[self.selected_game.unwrap_or(0)].2
    }

    pub fn palette(&self) -> &Palette {
        &self.catalogs[self.selected_game.unwrap_or(0)].1
    }

    pub fn label(&self) -> &str {
        self.catalog().label()
    }

    pub fn exists<S: AsRef<str>>(&self, name: S) -> bool {
        self.catalog().exists(name.as_ref())
    }

    pub fn read(&self, fid: FileId) -> Result<Cow<[u8]>> {
        self.catalog().read(fid)
    }

    pub fn read_name<S: AsRef<str>>(&self, name: S) -> Result<Cow<[u8]>> {
        self.catalog().read_name(name.as_ref())
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
        for entry in (fs::read_dir(path)?).flatten() {
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

    fn found_cd_libs(game: &'static GameInfo, files: &HashSet<String>) -> bool {
        game.cd_libs.iter().all(|&name| files.contains(name))
    }

    fn detect_cd_path(sentinel: &str) -> Option<PathBuf> {
        for letter in "ABCDEFGHIJKLMNOPQRSTUVWXYZ".chars() {
            let mut buf = PathBuf::new();
            buf.push(format!("{letter}:\\{sentinel}"));
            trace!("Checking for CD at {:?}", buf);
            if buf.exists() {
                return Some(buf.parent().unwrap().to_owned());
            }
        }
        None
    }

    fn detect_game_in_path(path: &Path) -> Result<Option<&'static GameInfo>> {
        Ok(Self::detect_game_from_files(
            &Self::list_directory_canonical(path)?,
        ))
    }

    fn detect_game_from_files(game_files: &HashSet<String>) -> Option<&'static GameInfo> {
        GAME_INFO.into_iter().find(|&game| {
            game.unique_files
                .iter()
                .all(|&name| game_files.contains(name))
        })
    }

    fn list_directory_canonical(path: &Path) -> Result<HashSet<String>> {
        // Filename capitalization is :shrug: so list and canonicalize everything all instead of
        // trying to rely on sane or predicable behavior.
        let mut game_files = HashSet::new();
        for p in (fs::read_dir(path)?).flatten() {
            game_files.insert(
                p.file_name()
                    .to_ascii_uppercase()
                    .to_string_lossy()
                    .into_owned(),
            );
        }
        Ok(game_files)
    }
}
