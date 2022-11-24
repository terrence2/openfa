# OpenFA

A black-box, open-source, re-implementation of the Janes Fighters Anthology's engine.

[![Latest Release](https://gitlab.com/terrence_too/openfa/-/badges/release.svg)](https://gitlab.com/terrence_too/openfa/-/releases)
[![Build Status](https://gitlab.com/terrence_too/openfa/badges/main/pipeline.svg)](https://gitlab.com/terrence_too/openfa/-/commits/main)
[![License](https://img.shields.io/static/v1.svg?label=license&message=GPLv3&color=informational)](https://github.com/terrence2/openfa/blob/master/LICENSE)

[[_TOC_]]

## Installing OpenFA

1. Install any of the following games:
    * USNF
    * US Marine Fighters (e.g. USNF with the Marine Fighters expansion)
    * ATF: Advanced Tactical Fighters
    * ATF: Nato (e.g. ATF with the Nato expansion installed)
    * ATF: Gold
    * USNF '97
    * Fighters Anthology
2. Download the [![Latest OpenFA Release](https://gitlab.com/terrence_too/openfa/-/badges/release.svg)](https://gitlab.com/terrence_too/openfa/-/releases/permalink/latest)
   for your platform and architecture
3. Extract the downloaded zip file into the install directory of the game
    * Example: C:\JANES\FA
    * `openfa.exe should be FA.EXE`
4. Double-click openfa.exe to run
    * `Or right-click and drag to create a shortcut wherever you want one`

## Installing FA Modding Tools

1) Install any of the following games:
    * USNF
    * US Marine Fighters (e.g. USNF with the Marine Fighters expansion)
    * ATF: Advanced Tactical Fighters
    * ATF: Nato (e.g. ATF with the Nato expansion installed)
    * ATF: Gold
    * USNF '97
    * Fighters Anthology
2) Download the [![Latest OFA Tools Release](https://gitlab.com/terrence_too/openfa/-/badges/release.svg)](https://gitlab.com/terrence_too/openfa/-/releases/permalink/latest)
   for your platform and architecture
3) Extract the downloaded zip file into the install directory of the game
    * Example: C:\JANES\FA
    * `dump-pic.exe, etc should be next to FA.EXE`
4) Drag and drop assets onto the appropriate tool to perform the action
    * Example: Drag FA_2.LIB onto dump-lib.exe to extract the LIB to FA_2/
    * Example: Drag one or more PIC files onto dump-pic.exe to create PNGs next to those PIC
    * Example: Drag a SH file onto show-sh.exe to open a window showing that shape

## Using the Command Line Tools (Detailed)

The command line tools support a wide variety of uses in addition to drag-and-drop that may be accessed from the command line.

1) Install as above
   * `Note: the modding tools come packaged with the normal OpenFA install`
2) Open a command prompt
   * Example: C:\Windows\System32\cmd.exe, but others should work
3) Change directory into the game directory
   * Example: `> cd C:\JANES\FA`
4) Individual CLI command documentation
   * [dump-pic](https://gitlab.com/terrence_too/openfa/apps/dump-pic)

## Why Fighters Anthology
1) The technology of the original Fighters Anthology engine represents the best of a bygone era of computing. Reverse
   engineering it lets us understand the severe limits of that era. By extension, it lets us tap into the amazing grit
   and daring of the creators that built it, despite everything stacked against them.
1) Since there was no illusion that a game of the era could faithfully simulate a real cockpit, there was a strong focus
   on the gameplay, in addition to building a facsimile of reality. As computers became powerful enough to be "realistic",
   it left behind the broad base of people who enjoy flight, but don't have the free time to spend 30+ hours memorizing where
   all the switches are in each new cockpit. In short, I wanted my mid-tier sims back and Fighters Anthology is the best
   of the bunch.

## Project Status

* Asset Discovery
  * Libraries
    * [x] LIB
      * `cargo run -p dump-lib -- ls test_data/packed/FA/FA_1.LIB`
          ```
          K212244.PIC     PKWare     1.36 KiB    16.56 KiB  0.082x
          ROUND17.PIC     PKWare     1.41 KiB    16.56 KiB  0.085x
          $LAU10.PIC      PKWare      418 B       1.43 KiB  0.286x
          ROCKER01.PIC    PKWare      554 B       1.39 KiB  0.388x
          ...
          ```
  * Images
    * [x] PIC: Full support for texture, screen, and embedded jpg content.
      * `cargo run -p dump-pic`: extract PICs to PNG files
      * `cargo run -p pack-pic`: build new PICs from a PNG
    * [x] PAL
      * `cargo run -p dump-pal`: extract PAL to PNG
  * Objects
    * [x] OT, PT, JT, NT
      * `cargo run -p dump-xt -- -S FA F22.PT`
          ```
               ObjectType
               ==========
              struct_type: Plane
                type_size: 636
            instance_size: 609
                 ot_names: ObjectNames { short_name: "F-22", long_name: "F- 22A Raptor", file_name: "F22.PT" }
                    flags: 8416243
                obj_class: Fighter
                    shape: Some("F22.SH")
                      ...
          ```
    * [ ] GAS
    * [ ] ECM
    * [ ] SEE
  * PE
    * [x] PE Wrapper
      * `cargo run -p dump-pe -- -S FA F22.SH`
  * Font
    * [x] FNT
  * Shape
    * [x] SH
      * `cargo run -p dump-sh -- -S FA F18.SH`
        ```
        0: @0000 Header: FF FF| 0000(0) 8C00(140) 0800(8) 9900(153) 4800(72) F200(242) 
        1: @000E 2EndO: F2 00| BD 4D  (delta:4DBD, target:4DCF)
        2: UnkCE @ 0012: 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 C0 FF FF 00 01 00 00 00 E5 FF FF 00 3F 00 00 00 01 00 00 00 E5 FF FF 
        3: @003A SrcRf: f22.asm
        4: @0044 Unk7A: 7A 00| 00 00 10 00 07 00 00 00 
        5: @004E Unk78: 78 00| 00 00 4C 00 24 00 79 00 00 00 
        6: @005A ToLOD: C8 00| E8 00 10 00 21 4B  (unk0:00E8, unk1:0010 target:4B83)
        7: @0062 ToLOD: C8 00| E8 00 21 00 A8 3B  (unk0:00E8, unk1:0021 target:3C12)
        8: @006A ToDtl: A6 00| A2 3B 01 00  (level:0001, target:3C12)
        9: @0070 TexRf: _f22.PIC
        ...
        ```
      * `cargo run -p show-sh -- -S FA F18.SH`
        ![Shape Explorer Demo](assets/sh_explorer_demo-19-03-10.gif)
  * Terrain
    * [x] T2: 3 bytes with height, metadata, and color; older versions have strange packing, probably for disk
              performance reasons. The terrains are all on the order of 256x256, so can be uploaded to a GPU and
              rendered as one strip.
    * [x] LAY: A PE header to handle relocation around a pile of structs. The content appears to be largely tables that
               plug into the core rendering system. The existing solution is just robust enough to find the palette entries
               to get the colors needed to render T2 and associated PIC files.
    * [x] M / MM: Mission and Mission Map formats. These tie together the T2, the LAY providing the palette and list all
                  of the associated textures and their placement. It also contains a list of every entity in the mission.
                  This capability would presumably let the designer mutate the map in significant ways along a campaign,
                  damaging regions of the map and whatnot; it appears to be completely unused, at least in the base
                  games. It appears to be completely unused. I believe that M files are used for campaigns and loose
                  missions and MM files are the "base" maps for the mission editor, but that is just supposition at this
                  point.
    * [ ] MC: A PE file. Appears to be scripted mission events.
  * Sound
    * [ ] 11K, 8K, 5K: Raw PCM with the given sample rate
    * [ ] XMI: eXtended MIdi, probably
  * AI
    * [ ] BI: compiled AI program
    * [ ] AI: a textual version of the BI
  * Gameplay
    * [ ] CAM: textual definition of a campaign
    * [ ] MT: Mission briefing blurbs
    * [ ] SEQ: Textual representation of a scene with timelines; e.g. death or campaign completion
    * [ ] TXT: Campaign intro text
    * [ ] HGR: PE defining the hanger / plane management screens in campaigns
    * [ ] HUD: PE defining the HUD of an aircraft
  * Menu
    * [ ] DLG: PE laying out the main game menu system
    * [ ] MNU: PE laying out the in-game menu bar at the top of the pause screen
    * [ ] PTS: PE file that has something to do with how planes are laid out in the plane selection screens.
  * Encyclopedia
    * [ ] INF: Textual encyclopedia entries
    * [ ] VDO: Video encyclopedia entries
  * Unknown Purpose
    * [ ] MUS: PE; maybe music sequencing?
    * [ ] BIN: binary
    * [ ] CB8: binary
    * [ ] FBC: binary
    
* Game Engine
  * Shape System
    * SH
      * [x] Chunked SH upload
      * [x] Chunked Texture atlases
      * [ ] Linear filtering support
      * [ ] Mipmapping support
      * [ ] Regalia support
      * [ ] Correct scaling
      * [x] Instance data uploads in blocks
      * [x] Legion Integration
      * [x] Parallel simulation of embedded i386 code
      * [x] Shape classes to limit uploads depending on entity type; e.g. no per-frame positions for buildings
      * [ ] Self shadowing
    * Modern format: we want to be able to do optimistic replacement with higher quality models at some point.
      * [ ] Do discovery around modern shape formats
  * Terrain System
    * T2 Rendering
      * [ ] Linear filtering
      * [x] Correct alignment with globe
      * [x] Eye relative uploads
      * [x] Atmospheric blending
      * [ ] Self shadowing
      * [ ] Shape shadowing
  * Text
    * [\] FNT loading; not yet supporting pre-blended text
    * [x] 2d screen-space rendering
  * Basic Flight Dynamics Model
    * [ ] Basic input and reactions
    * [ ] Apply envelope data to reactions
  * HUD
    * [ ] Render Tape in 2d in screenspace
    * [ ] Render Cockpit in 2d in screenspace
    * [ ] Render onto a 3d surface and sit in a virtual cockpit
  * Mirrors
    * [ ] Rear-view mirrors
  * MFD
    * [ ] Render several cameras on screen at once
  * Inventory Management
    * [ ] Add information to legion
    * [ ] integrate stores with MFD
  * AI
    * [ ] Action / goal system
    * [ ] Parallel simulation
  * Sound
    * [ ] XMI Decode
  * Menu Bar
    * [ ] Implement menu-mode in the game loop
    * [ ] Render top-level
    * [ ] Handle clicks on menu bar
    * [ ] Render sub-menus
    * [ ] Render nested menus
    * [ ] Handle clicks on menu items
  * Game menus
    * [ ] Layout components and text on screen
    * [ ] Handle button animations
    * [ ] Hook up scroll wheel
    * [ ] Perform nice wipes/fades between menus
  * Hangars
  * Death / Campaign Events Screens
    * [ ] Build a timeline
    * [ ] Show screens and text
    * [ ] Play sounds
    * [ ] Make gameplay state changes
  * Player profile management
    * [ ] Do discovery around the existing format
  * Campaign management
  * Encyclopedia
    * [ ] Inline SH renderer
    * [ ] Image carousel
    * [ ] Video viewer
  * Opening Videos
    * [ ] VDO decode

#### Specific Format Notes

* **PAL**: PALETTE.PAL is the only file of this type. It contains palette data consisting of 256 3-byte entries.
  Each byte contains a 6-bit (VGA) color, so must be re-sampled for use in modern systems. Large parts of this
  palette contain the "transparent" color #FF00FF. These sections are used by the terrain and (presumably) the HUD/menu
  to give custom look and feel to each area and plane.
* **SH**: Shape files contain a virtual machine using word codes inside the PE wrapper, with embedded fragments of x86.
  Execution jumps between virtual and machine instructions in order to achieve most dynamic plane effects.
* **T2**: Just heights and metadata about the terrain. The textures to be rendered onto that heightmap are stored
  in the MM/M files in tmap and tdict sections. Both the textures and the base colors in the T2 itself are outside
  the range of the base PALETTE.PAL and depend on a fragment of the LAY file being copied into the right part of
  the palette. Time-of-day effects as well as distance fogging are acheived by swapping out the palette with values
  from the LAY.
* **VDO**: These video files start with RATPAC, which is probably short for Rate-Packed. This is probably a standard
  format of some sort. Unfortunately, a basic google search for files with that header turned up absolutely nothing.
  If anyone has any information about this format, please drop me a line.


## Development Environment Setup

1) Pull from git. The [main branch](https://gitlab.com/terrence_too/openfa.git) if you do not plan to submit changes, 
   or your own fork if you do.
   1) `git clone --recursive https://gitlab.com/terrence_too/openfa.git`
2) Move into the newly downloaded directory.
   1) `cd openfa`
3) Install the Rust language via rustup.rs
4) Prepare your testing environment. OpenFA expects a `disk_dumps` directory with subdirectories for
   each of the games you own, each containing a cdrom1 and installdir folder (and cdrom2 for FA). No
   game is required, but you must have at least one game.
   1) `mkdir -p disk_dumps/{USNF,MF,ATF,ATFNATO,ATFGOLD,USNF97,FA}/{cdrom1,installdir} disk_dumps/FA/cdrom2`
   2) Mount and copy the CD-ROM contents into the cdrom1 and cdrom2 (for FA) directories
   3) Install each game either natively or using dosbox/wine and copy the install folder into installdir.
5) *Optional*: Unpack any LIBs that you will need to work on directly. OpenFA will automatically detect
   unpacked LIB directories that have the same name as the game libs, but with the extension L_B (e.g. 
   with the 'I' folded down into a '_'). So the `USNF_3.LIB` would have a `USNF_3.L_B` library. OpenFA's
   dump-lib utility will automatically make this rename. OpenFA will automatically prefer files in the
   directory, over the lib if both are present.
   1) `cd disk_dumps/FA/installdir`
   2) `cargo run -p dump-lib -- unpack FA_1.LIB`
6) Run through the setup process in Nitrogen's readme. You will (for the moment) need to download and
   build terrain data locally.
7) `cargo run`; by default OpenFA will use the latest game in disk_dumps if it does not find a game in
   the current directory. You can use the `--game-path`, `--cd-path`, and `--cd2-path` to select the
   relevant directories, if needed. CD paths are not needed if you copy the LIBs on the CD into the
   game directory.

### Developing OpenFA (Windows)
There is a variety of software that is needed to build OpenFA. On Windows,
the `scoop` installer is the best way to grab most of these dependencies, but you
will still have to install some of them manually. Specifically, you will need the
shell that ships with the MSI installer for Git and the latest stable version of
Rust via Rustup.

1. Rust: visit rustup.rs and follow the directions
2. git (and the Git Bash shell on Windows)
3. C build tools (VS Build Tools on Windows, LLVM on other platforms)
4. cmake
5. ninja
6. SSL

The Git Bash shell is recommended over CMD or the Windows terminal, even the shiny new one.
In particular, I noticed issues with case insensitivity, though the build still ran fine. YMMV.

Git symlinks do not currently create windows symlinks: the mklink command is only available
from CMD and is not yet exposed in msys. Once created in a windows CMD shell, however, git will
treat the Windows symlink like a normal symlink and not be problematic. You will need to open
up a CMD instance with "Run as Administrator" and then type:
```
> cd C:\Path\To\nitrogen\libs
> del nitrogen
> mklink /D nitrogen ..\nitrogen\libs
```

Once that is set up, you can test that your development environment is ready by running a quick check.
```
$ cargo check --all --all-targets
$ cargo test --all --all-targets
```

If that succeeds, you can build the release by running:
```
$ cargo build --all --all-targets --release
```
