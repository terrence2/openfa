# PE File Format
The Fighters engine ships many of its assets as PE files, with a combination of virtual and
real machine instructions embedded inside. Unfortunately, it changes a bit in the header, which
makes every real-world PE library I've tried to parse these files with explode in various
entertaining ways. Luckily, only a couple of section types are used, so writing our own mini-PE
implementation was not challenging.

The following types of Fighters files use a PE encoding internally:
* BI
* CAM
* DLG
* FNT
* HGR
* HUD
* LAY
* MC
* MNU
* MUS
* PTS
* SH
