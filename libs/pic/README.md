#PIC file format support
*Note: this is not PICtor format, but custom to the Fighters engine.*

This is a fairly trivial file format: it is just a header that tell you where various fixed-purpose blocks
of data reside in the file, followed by those blocks. The contents may contain pixels, a custom palette,
span data (for sparse format images) and "row heads".

```rust
#[repr(C)]
#[repr(packed)]
struct Header {
    format: u16,
    width: u32,
    height: u32,
    pixels_offset: u32,
    pixels_size: u32,
    palette_offset: u32,
    palette_size: u32,
    spans_offset: u32,
    spans_size: u32,
    rowheads_offset: u32,
    rowheads_size: u32,
}
```

### Overview
Pixel data contains an array of one-byte indexes into a palette of RGB triplet pairs.
 
The local palette appears to sit on top of the default PALETTE.PAL file. I think FA probably messes around
with the palette at runtime, so more research is needed for some cases. There are also some pics that may
not be palattized at all -- heightmaps perhaps?

Rowheads is an odd duck: it's just an array of u32 with length equal to the number of rows where each offset
is the index of the start offset of that row. I.e. a simple increment by the column count for each
row: a pre-multiplied table of sorts. On a modern processor this would be insane because memory access costs
are a thousand times higher than a simple MUL, however when this game was designed, the opposite would have
been true. This table probably allows the software texturing unit to grab a texel out of the image with only
x86 indexing instructions, saving a bunch of time. Also, all of these games were shipped on CD-ROM, so the
designer(s) probably felt like they had infinite space for whatever tables would help runtime performance.
Naturally, this decoder leaves the rowheads section alone, however, it is an interesting historical note.

Spans is a simple compression technique used for overlays, like the plane HUDs, and other mostly-transparent
images. Each span in the block is 10 bytes containing a row and the start and end columns on that row,
followed by an index into pixels block to use on that row. This allows sparse images to pack the useful pixels
into a tight block, saving space.

```rust
#[repr(C)]
#[repr(packed)]
struct Span {
    _row: u16,
    _start: u16,
    _end: u16,
    _index: u32,
}
```

### Format
The `format` field in the header may be either `DENSE (0)` or `SPARSE (1)`. In DENSE mode the pixel data is
complete and spans is empty and rowhead is filled. Presumably, these images are mostly used as texture data
in game. In SPARSE mode pixels is incomplete and must by selectively output using the Spans structures.
Rowheads are naturally empty for these images. Palette data may or may not be specified independent of the
format.
