# OpenFA

An attempt at a black-box, open-source re-implementation of the Janes Fighters Anthology's engine.

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
| DLG       |                 |          |
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
| FNT       | Font            |          |
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
