// Run this script to generate placeholder icons:
// rustc gen_placeholder.rs -o gen_placeholder && ./gen_placeholder
//
// Or just create any valid 32x32 PNG named icon.png in this directory.

use std::io::Write;

fn main() {
    // Minimal 32x32 RGBA PNG (white square)
    let png_data = create_minimal_png(32, 32, [200, 180, 120, 255]); // cheese-ish yellow

    for (name, _) in &[
        ("icon.png", ()),
        ("32x32.png", ()),
        ("128x128.png", ()),
        ("128x128@2x.png", ()),
    ] {
        std::fs::write(name, &png_data).unwrap();
        println!("Created {name}");
    }
}

fn create_minimal_png(width: u32, height: u32, color: [u8; 4]) -> Vec<u8> {
    let mut data = Vec::new();

    // PNG signature
    data.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);

    // IHDR chunk
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(6); // color type: RGBA
    ihdr.push(0); // compression
    ihdr.push(0); // filter
    ihdr.push(0); // interlace
    write_chunk(&mut data, b"IHDR", &ihdr);

    // IDAT chunk - raw pixel data with zlib
    let mut raw = Vec::new();
    for _ in 0..height {
        raw.push(0); // filter: none
        for _ in 0..width {
            raw.extend_from_slice(&color);
        }
    }

    let compressed = deflate_raw(&raw);
    write_chunk(&mut data, b"IDAT", &compressed);

    // IEND chunk
    write_chunk(&mut data, b"IEND", &[]);

    data
}

fn write_chunk(data: &mut Vec<u8>, chunk_type: &[u8; 4], chunk_data: &[u8]) {
    data.extend_from_slice(&(chunk_data.len() as u32).to_be_bytes());
    data.extend_from_slice(chunk_type);
    data.extend_from_slice(chunk_data);
    let crc = crc32(&[chunk_type.as_slice(), chunk_data].concat());
    data.extend_from_slice(&crc.to_be_bytes());
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

fn deflate_raw(data: &[u8]) -> Vec<u8> {
    // Minimal zlib wrapper with stored (uncompressed) blocks
    let mut out = Vec::new();
    out.push(0x78); // CMF: deflate, window size 32K
    out.push(0x01); // FLG: check bits

    // Split into blocks of max 65535 bytes
    let chunks: Vec<&[u8]> = data.chunks(65535).collect();
    for (i, chunk) in chunks.iter().enumerate() {
        let is_last = i == chunks.len() - 1;
        out.push(if is_last { 0x01 } else { 0x00 }); // BFINAL + BTYPE=00 (stored)
        let len = chunk.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(chunk);
    }

    // Adler32 checksum
    let adler = adler32(data);
    out.extend_from_slice(&adler.to_be_bytes());

    out
}

fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}
