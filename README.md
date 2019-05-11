# OpenFA

An attempt at a black-box, open-source re-implementation of the Janes Fighters Anthology's engine.

[![Build Status](https://badge.buildkite.com/5f5710df0c75ea999ada3ed52d0967537cdc7859253fcf89ab.svg)](https://buildkite.com/openfa/continuous-integration)
[![Download](https://img.shields.io/static/v1.svg?label=download&message=latest&color=important)](https://github.com/terrence2/openfa/releases/latest)
[![License](https://img.shields.io/static/v1.svg?label=license&message=GPLv3&color=informational)](https://github.com/terrence2/openfa/blob/master/LICENSE)

## Progress

![Shape Explorer Demo](assets/sh_explorer_demo-19-03-10.gif)

The FA engine uses many different files. Some of these are standard formats; some of them are straightforward text;
but most are extremely weird relics of a bygone computing age.

### Standard formats

| Extension | Asset           |
| --------- |:--------------- |
| 11K       | Sound           |
| 5K        | Sound           |
| 8K        | Sound           |
| XMI       | eXtended MIdi   |

### Textual

| Extension | Asset           | Parsed   |
| --------- |:--------------- | -------- |
| AI        | AI Program      |          |
| ECM       | ECM Type        |          |
| GAS       | Fuel Tank Type  |          |
| INF       | Info Page Text  |          |
| JT        | proJectile Type | x        |
| M         | Mission         | x        |
| MM        | Mission Map     | x        |
| MT        | Mission Text    |          |
| NT        | Npc Type        | x        |
| OT        | Object Type     | x        |
| PT        | Plane Type      | x        |
| SEE       | Sensor Type     |          |
| SEQ       | Scene Timelines |          |
| TXT       | Campaign Blurbs |          |

### Portable Executable Wrapper

| Extension | Asset           | Parsed   |
| --------- |:--------------- | -------- |
| BI        | AI Binary       |          |
| CAM       | Campaign        |          |
| DLG       | Dialog Menus    |          |
| FNT       | Font            | x        |
| HGR       |                 |          |
| HUD       |                 |          |
| LAY       | Terrain Palette | x        |
| MC        |                 |          |
| MNU       | Menus           |          |
| MUS       |                 |          |
| PTS       |                 |          |
| SH        | Shape           | x        |

### Custom Binary

| Extension | Asset           | Parsed   |
| --------- |:--------------- | -------- |
| BIN       |                 |          |
| CB8       |                 |          |
| FBC       |                 |          |
| PAL       | Palette         | x        |
| PIC       | Picture         | x        |
| T2        | Terrain         | x        |
| VDO       | Video           |          |

#### Specific Format Notes

* **PAL**: PALETTE.PAL is the only file of this type. It contains palette data consisting of 256 3-byte entries.
Each byte contains a 6-bit (VGA) color, so must be shifted by 2 for use in modern systems. Large parts of this
palette contain the "transparent" color #FF00FF. These sections are used by the terrain and (presumably) the HUD/menu
to give custom look and feel to each area and plane.
* **SH**: Shape files contain a virtual machine using word codes inside the PE wrapper, with embedded fragments of x86.
Execution jumps between virtual and machine instructions in order to achieve most dynamic plane effects.
* **T2**: Just heights and metadata about the terrain. The textures to be rendered onto that heightmap are stored
in the MM/M files in tmap and tdict sections. Both the textures and the base colors in the T2 itself are outside
the range of the base PALETTE.PAL and depend on a fragment of the LAY file being copied into the right part of
the palette. Time-of-day effects as well as distance fogging are acheived by swapping out the palette with values
from the LAY.
* **VDO**: These files start with RATPAC, which is probably short for Rate-Packed. This is probably a standard
format of some sort. Unfortunately, a basic google search for files with that header turned up absolutely
nothing. We need a guru who knows about ancient video encoding standards.

## Development Environment Setup

1) `git clone https://github.com/terrence2/openfa.git`
1) `cd openfa`
1) `mkdir -p test_data/{un,}packed/{USNF,USMF,ATF,ATFNATO,ATFGOLD,USNF97,FA}/installdir`
1) Copy *.LIB from the CD and Installation directory into `test_data/packed/<GAME>/`
1) Copy any loose T2 files from the Installation directory (ATFNATO and earlier only) into `test_data/packed/<GAME>/installdir/`
1) Install the Rust language via rustup.rs
1) (Optional) cd into apps/unlib and run `cargo run -- -o ../../test_data/unpacked/<GAME>/<LIB> ../../test_data/packed/<GAME>/<LIB>` on
    each of the libs that you would like to have available as raw files. This are generally faster and easier to work with when
    developing than the raw LIB files
1) Run sh_explorer by changing directory into `apps/sh_explorer/` and running `cargo run -- -t <GAME>:<FILE.SH>` (for example `cargo run -- -t FA:F18.SH`)
1) Run mm_explorer by changing directory into `apps/mm_explorer/` and running `cargo run -- -t <GAME>:<FILE.MM>` (for example `cargo run -- -t FA:UKR.MM`)