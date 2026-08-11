//! Tiny PNG encoder — no external dependencies beyond `crc32fast`.
//!
//! PNG layout:
//!   - 8-byte signature
//!   - IHDR chunk (13 bytes data)
//!   - IDAT chunk (zlib-compressed raw scanlines)
//!   - IEND chunk (0 bytes data)
//!
//! For the zlib stream we use stored blocks (no compression) — fast and
//! small. Compression would require a deflate implementation (~500 LOC).

use crc32fast::Hasher as Crc32;

const PNG_SIG: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];

/// Encode an RGBA buffer as a PNG. Returns the full file bytes.
pub fn encode(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len() + 256);
    out.extend_from_slice(&PNG_SIG);

    // IHDR
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(6); // color type (RGBA)
    ihdr.push(0); // compression
    ihdr.push(0); // filter
    ihdr.push(0); // interlace
    write_chunk(&mut out, b"IHDR", &ihdr);

    // IDAT — zlib stream with stored (uncompressed) blocks.
    // Each scanline is prefixed with a filter byte (0 = None).
    let mut raw = Vec::with_capacity((width * height * 4 + height) as usize);
    for y in 0..height {
        raw.push(0u8); // filter byte
        let start = (y * width * 4) as usize;
        let end = start + (width * 4) as usize;
        raw.extend_from_slice(&rgba[start..end]);
    }
    let zlib = zlib_stored(&raw);
    write_chunk(&mut out, b"IDAT", &zlib);

    // IEND
    write_chunk(&mut out, b"IEND", &[]);
    out
}

fn write_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = Crc32::new();
    crc.update(kind);
    crc.update(data);
    out.extend_from_slice(&crc.finalize().to_be_bytes());
}

/// Build a zlib stream using stored (uncompressed) deflate blocks.
/// Format: 2-byte header + N blocks of [3-byte header + data] + 4-byte Adler-32.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 16);
    // zlib header.
    out.push(0x78); // CMF: deflate, 32K window
    out.push(0x01); // FLG: no preset dict, fastest compression level

    // Stored blocks. Max block size = 65535 bytes.
    let mut i = 0;
    while i < data.len() {
        let remaining = data.len() - i;
        let block_size = remaining.min(65535);
        let is_last = remaining <= 65535;
        out.push(if is_last { 1 } else { 0 }); // BFINAL + BTYPE=00 (stored)
        out.push((block_size & 0xff) as u8);
        out.push(((block_size >> 8) & 0xff) as u8);
        let nlen = !block_size as u16;
        out.push((nlen & 0xff) as u8);
        out.push(((nlen >> 8) & 0xff) as u8);
        out.extend_from_slice(&data[i..i + block_size]);
        i += block_size;
    }

    // Adler-32 checksum.
    let adler = adler32(data);
    out.extend_from_slice(&adler.to_be_bytes());
    out
}

fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65521;
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + byte as u32) % MOD;
        b = (b + a) % MOD;
    }
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_small_image() {
        let rgba = vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
        ];
        let png = encode(2, 2, &rgba);
        // Verify signature.
        assert_eq!(&png[..8], &PNG_SIG);
        // Verify IHDR is present.
        assert_eq!(&png[12..16], b"IHDR");
        // File should be > 50 bytes for a 2x2 image.
        assert!(png.len() > 50);
    }
}
