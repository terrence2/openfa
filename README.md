# OpenFA

A black-box, open-source, re-implementation of the Janes Fighters Anthology's engine.

[![Download](https://img.shields.io/static/v1.svg?label=download&message=latest&color=success)](https://github.com/terrence2/openfa/releases/latest)
[![Build status](https://badge.buildkite.com/5f5710df0c75ea999ada3ed52d0967537cdc7859253fcf89ab.svg?branch=master)](https://buildkite.com/openfa/continuous-release)
[![License](https://img.shields.io/static/v1.svg?label=license&message=GPLv3&color=informational)](https://github.com/terrence2/openfa/blob/master/LICENSE)

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
      * `cargo run -p dump-xt -- FA:F22.PT`
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
      * `cargo run -p dump-pe -- FA:F22.SH`
  * Font
    * [x] FNT
  * Shape
    * [x] SH
      * `cargo run -p dump-sh -- FA:F18.Sh`
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
      * `cargo run -p show-sh -- FA:F18.Sh`
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
  * Sound
    * [ ] 11K, 8K, 5K: Raw PCM with the given sample rate
    * [ ] XMI: eXtended MIdi
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
  * Encyclopedia
    * [ ] INF: Textual encyclopedia entries
    * [ ] VDO: Video encyclopedia entries
  * Unknown Purpose
    * [ ] MC: PE
    * [ ] MUS: PE; maybe music sequencing?
    * [ ] PTS: PE
    * [ ] BIN: binary
    * [ ] CB8: binary
    * [ ] FBC: binary
    
* Game Engine
  * Graphics / Input Engine
    * [x] Webgpu-rs
    * [x] Simple frame management and buffer upload system
    * [ ] Sophisticated frame graph
    * [x] Robust key binding support
    * [x] Basic command system
    * [ ] Extensible command system
    * [ ] drop-down console
    * [ ] VR support
    * [ ] Joystick support
    * [ ] Gamepad support
  * Atmospheric Simulation
    * [x] Basic precomputed scattering: Uses [Bruneton's method](https://github.com/ebruneton/precomputed_atmospheric_scattering).
    * [ ] Dynamically changing atmospheric conditions
    * [ ] Spatially variable atmospheric parameters
  * Entity System
    * [x] Legion
    * [ ] Save/Load system
    * [ ] Replay recording
    * [ ] Network syncing
  * Shape System
    * SH
      * [x] Chunked SH upload
      * [x] Chunked Texture atlases
      * [ ] Linear filtering support
      * [ ] Regalia support
      * [ ] Correct scaling
      * [x] Instance data uploads in blocks
      * [x] Legion Integration
      * [x] Parallel simulation of embedded 386 code
      * [x] Shape classes to limit uploads depending on entity type; e.g. no per-frame positions for buildings
      * [ ] Self shadowing
    * Modern format: we want to be able to do optimistic replacement with higher quality models at some point.
      * [ ] Do discovery around modern shape formats
  * Terrain System
    * T2 Rendering
      * [x] Texture Atlas
      * [ ] Linear filtering
      * [ ] Correct alignment with globe
      * [ ] Eye relative uploads
      * [x] Atmospheric blending
      * [ ] Self shadowing
      * [ ] Shape shadowing
    * Planetary Scale Rendering; Using [Kooima's thesis](https://www.evl.uic.edu/documents/kooima-dissertation-uic.pdf).
      * [x] Patch management
      * [ ] Patch tesselation
      * [ ] Heightmap generator
      * [ ] Heightmap memory manager
      * [ ] Colormap generator
      * [ ] Colormap memory manager
      * [ ] Atmospheric blending
      * [ ] Self shadowing
      * [ ] Unified T2/Kooima terrain rendering
  * Text
    * [x] Layout management
    * [x] FNT loading
    * [x] TTF loading
    * [x] 2d screen-space rendering
    * [ ] in-world text rendering
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
  * Sound
    * [ ] Pick a framework
    * [ ] Sample management
    * [ ] Channel management and blending
    * [ ] Positional audio
    * [ ] Frequency scaling (e.g. for wind and engine noises)
    * [ ] Doppler effects
    * [ ] XMI decode and rendering
  * AI
    * [ ] Action / goal system
    * [ ] Parallel simulation
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

## Development Environment Setup

1) `git clone https://github.com/terrence2/openfa.git`
1) `cd openfa`
1) `mkdir -p test_data/{un,}packed/{USNF,USMF,ATF,ATFNATO,ATFGOLD,USNF97,FA}/installdir`
1) Copy *.LIB from the CD and Installation directory into `test_data/packed/<GAME>/`
1) Copy any loose T2 files from the Installation directory (ATFNATO and earlier only) into `test_data/packed/<GAME>/installdir/`
1) Install the Rust language via rustup.rs
1) (Optional) cd into apps/unlib and run `cargo run -- -o ../../test_data/unpacked/<GAME>/<LIB> ../../test_data/packed/<GAME>/<LIB>`
    on each of the libs that you would like to have available as raw files. Loose files generally faster and easier to
    work with when developing than the raw LIB files.
1) Run sh_explorer by changing directory into `apps/sh_explorer/` and running `cargo run -- -t <GAME>:<FILE.SH>` (for example `cargo run -- -t FA:F18.SH`)
1) Run mm_explorer by changing directory into `apps/mm_explorer/` and running `cargo run -- -t <GAME>:<FILE.MM>` (for example `cargo run -- -t FA:UKR.MM`)

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
If anyone has any information about this format, please drop us a line.
