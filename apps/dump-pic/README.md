# dump-pic
Read a PIC file out of a LIB and write to a modern image format of your choice.

[[_TOC_]]

## About

The following output file types are supported:
* PNG
* JPEG
* GIF
* BMP
* ICO
* TIFF
* WebP
* AVIF
* PNM
* DDS
* TGA
* OpenEXR
* farbfeld

## Installation
See the main [README.md](https://gitlab.com/terrence_too/openfa#installation) for installation instructions.

## Examples
Dump the PIC file _F22_A.PIC from LIBs in the current game directory into a BMP file.
* `> dump-pic --output _f22_a.bmp _F22_A.PIC`

In general, _F22_A.PIC may be any PIC file in the game or mod's LIBs and _f22_a.bmp can be any name you want. The format is guessed from the extension.

Short options are also supported:
* `> dump-pic -o nose08.jpg NOSE08.PIC`

## Detailed Usage
```
dump-pic [FLAGS] [OPTIONS] [--] [inputs]...

FLAGS:
-b, --gray-scale    Output as grayscale rather than palettized
-h, --help          Prints help information
-a, --ascii         Print the image as ascii
-V, --version       Prints version information

OPTIONS:
-c, --cd-path <cd-path>              If not all required libs are found in the game path, look here. If the CD's LIB
                                     files have been copied into the game directory, this is unused
--cd2-path <cd2-path>                For Fighter's Anthology, if the second disk's LIB files have not been copied
                                     into the game directory, and you want to use the reference materials, also
                                     provide this path. There is no ability to switch the disk, currently. (Note:
                                     reference still WIP, so not much point yet.)
-d, --dump-palette <dump-palette>    Dump the palette here as a PAL
-g, --game-path <game-path>          The path to look in for game files (default: pwd)
-l, --lib-paths <lib-paths>...       Extra directories to treat as libraries
-S, --select-game <select-game>      Select the game, if there is more than one available (e.g. in test mode)
-s, --show-palette <show-palette>    Dump the palette here as a PNG
-u, --use-palette <use-palette>      Use the given palette when decoding
-o, --output <write-image>           Write the image to the given file
```