OpenFA
------
An attempt at an open-source re-implementation of the Janes Fighters Anthology's engine.


| Extension | Asset           | Status   |
| --------- |:--------------- | -------- |
| 11K       | Sound           | Standard |
| 5K        | Sound           | Standard |
| 8K        |                 |          |
| AI        | AI Program      |          |
| BI        | AI Binary       |          |
| BIN       |                 |          |
| CAM       |                 |          |
| CB8       |                 |          |
| DLG       |                 |          |
| ECM       |                 | Todo     |
| FBC       |                 |          |
| FNT       |                 |          |
| GAS       |                 | Todo     |
| HGR       |                 |          |
| HUD       |                 |          |
| INF       |                 |          |
| JT        | proJectile Type | Partial  |
| LAY       |                 |          |
| M         |                 |          |
| MC        |                 |          |
| MM        |                 |          |
| MNU       |                 |          |
| MT        |                 |          |
| MUS       |                 |          |
| NT        | Npc Type        | Partial  |
| OT        | Object Type     | Partial  |
| PAL       | Palette         | Complete |
| PIC       | Picture         | Complete |
| PT        | Plane Type      | Partial  |
| PTS       |                 |          |
| SEE       |                 | Todo     |
| SEQ       |                 |          |
| SH        | Shape           | Partial  |
| T2        | Terrain         | Partial  |
| TXT       |                 |          |
| VDO       | Video           | *        |
| XMI       | eXtended MIdi   | Standard |

* _Blank_ - totally unknown; needs research.
* `*` - See details further down the page
* `+` - Is in a PE wrapper, need to investigate contents
* Standard - a standard format of some sort that should be easy to support
* Todo - we know how to write a parser, but have not yet
* Parsed - we know how to parse the file, but have little or no understanding of what it does
* Partial - we know what some-to-most fields in the file do, but research is still needed on esoteric features
* Complete - we know what all parts of the file do and have implemented a reader for all features

### VDO Format
These files start with RATPAC, which is probably short for Rate-Packed. This is probably a standard
format of some sort. Unfortunately, a basic google search for files with that header turned up absolutely
nothing. We need a guru who knows about ancient video encoding standards.
