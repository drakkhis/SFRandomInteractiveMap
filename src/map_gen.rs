use anyhow::{bail, Result};
use byteorder::{LittleEndian, ReadBytesExt};
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use rayon::prelude::*;
use std::collections::HashMap;
use std::io::{Cursor, Read, Write};

/// Metadata and rendered PNG image + 3D heightmap extracted from the save's embedded procedural world (`PWWE` / `PWW1`).
#[derive(Debug, Clone)]
pub struct GeneratedMapData {
    pub png_bytes: Vec<u8>,
    pub heightmap_bytes: Vec<u8>,
    pub resolution: usize,
    pub grid_cm: f64,
    pub world_extent_cm: f64,
    pub summary_text: String,
}

#[derive(Debug, Clone, Copy)]
struct PlanCell {
    _grid_y: i32,
    _grid_x: i32,
    _base_y: i32,
    _base_x: i32,
    _rotation: i32,
    _lift_m: i32,
    _cell_id: i32,
    biome_family: i32,
    _layer: i32,
}

#[derive(Debug, Clone, Copy)]
struct BandEntry {
    sec_idx: u32,
    uncomp_offset: usize,
    uncomp_size: usize,
    comp_offset: usize,
    comp_size: usize,
}

/// Base RGB colors for the 21 Satisfactory biome families, matched to the RWG in-game preview.
const BIOME_FAMILY_COLORS: [[f32; 3]; 21] = [
    [126.0, 122.0, 113.0], //  0: Abyss Cliffs
    [108.0, 118.0, 106.0], //  1: Blue Crater
    [130.0, 146.0, 140.0], //  2: Crater Lakes
    [136.0, 124.0, 110.0], //  3: Desert Canyons
    [142.0, 130.0, 112.0], //  4: Dune Desert
    [132.0, 143.0, 134.0], //  5: Eastern Dune Forest
    [118.0, 124.0,  98.0], //  6: Grass Fields
    [134.0, 136.0, 126.0], //  7: Jungle Spires
    [122.0, 141.0, 135.0], //  8: Lake Forest
    [128.0, 128.0, 120.0], //  9: Maze Canyons
    [126.0, 134.0, 120.0], // 10: Northern Forest
    [146.0, 148.0, 144.0], // 11: Red Bamboo Fields
    [140.0, 138.0, 130.0], // 12: Red Jungle
    [136.0, 129.0, 116.0], // 13: Rocky Desert
    [120.0, 122.0, 108.0], // 14: Snaketree Forest
    [116.0, 118.0, 103.0], // 15: Southern Forest
    [130.0, 134.0, 126.0], // 16: Spire Coast
    [115.0, 129.0, 121.0], // 17: Swamp
    [136.0, 141.0, 133.0], // 18: Titan Forest
    [134.0, 126.0, 109.0], // 19: Western Beaches
    [124.0, 127.0, 116.0], // 20: Western Dune Forest
];

/// Searches the decompressed `.sav` actor stream for an embedded `PWW1` procedural world
/// and dynamically renders an aligned RGBA PNG map image.
pub fn extract_and_render_map(uncompressed_save_data: &[u8], out_size: usize) -> Result<Option<GeneratedMapData>> {
    // 1. Locate `PWW1` magic in the decompressed save stream
    let Some(pww1_pos) = find_subslice(uncompressed_save_data, b"PWW1") else {
        return Ok(None);
    };

    let blob = &uncompressed_save_data[pww1_pos..];
    if blob.len() < 0x200 {
        bail!("Truncated PWW1 header");
    }

    // 2. Read header parameters relative to `PWW1`:
    // +0x54: resolution (u32, typically 6000)
    // +0x64: grid_cm (f64, typically 200.0)
    // +0x6c: world_extent_cm (f64, typically 600000.0)
    // +0x119: summary FString length (u32)
    let mut hdr = Cursor::new(blob);
    hdr.set_position(0x54);
    let resolution = hdr.read_u32::<LittleEndian>()? as usize;
    hdr.set_position(0x64);
    let grid_cm = hdr.read_f64::<LittleEndian>()?;
    let world_extent_cm = hdr.read_f64::<LittleEndian>()?;

    if resolution == 0 || resolution > 24000 {
        bail!("Invalid PWW1 resolution: {}", resolution);
    }

    // 2b. Dynamically locate the summary FString and section table
    // In v24 headers, str_len was at 0x118/0x119; in v28 headers (v0.2.0), 24 bytes of new fields
    // were added at 0xE0..0x100, pushing str_len to 0x131.
    // Finding b"splice seed" dynamically makes this forward-compatible across all mod versions.
    let (str_len_pos, str_len) = if let Some(splice_idx) = find_subslice(blob, b"splice seed") {
        let len_pos = splice_idx - 4;
        let mut len_rdr = Cursor::new(&blob[len_pos..len_pos + 4]);
        let len = len_rdr.read_u32::<LittleEndian>()? as usize;
        (len_pos, len)
    } else {
        // Fallback: check header version at offset 4
        let version = Cursor::new(&blob[4..8]).read_u32::<LittleEndian>().unwrap_or(0);
        let len_pos = if version >= 28 { 0x131 } else { 0x119 };
        let mut len_rdr = Cursor::new(&blob[len_pos..len_pos + 4]);
        let len = len_rdr.read_u32::<LittleEndian>()? as usize;
        (len_pos, len)
    };

    if str_len_pos + 4 + str_len + 8 > blob.len() {
        bail!("Invalid PWW1 summary string length: {}", str_len);
    }
    let summary_bytes = &blob[str_len_pos + 4 .. str_len_pos + 4 + str_len];
    let summary_text = String::from_utf8_lossy(
        summary_bytes.strip_suffix(&[0]).unwrap_or(summary_bytes)
    ).to_string();

    // 3. Parse Section Table and Band Table immediately following the summary string
    let mut pos = str_len_pos + 4 + str_len;
    let mut rdr = Cursor::new(blob);
    rdr.set_position(pos as u64);

    let sec_count = rdr.read_u32::<LittleEndian>()? as usize;
    let mut sections = Vec::with_capacity(sec_count);
    let mut sec_id_to_idx = HashMap::new();
    for idx in 0..sec_count {
        let sec_id = rdr.read_u32::<LittleEndian>()?;
        let sec_uncomp_len = rdr.read_u64::<LittleEndian>()? as usize;
        sections.push((sec_id, sec_uncomp_len));
        sec_id_to_idx.insert(sec_id, idx);
    }

    let num_bands = rdr.read_u32::<LittleEndian>()? as usize;
    let mut bands = Vec::with_capacity(num_bands);
    for _ in 0..num_bands {
        let sec_idx = rdr.read_u32::<LittleEndian>()?;
        let uncomp_offset = rdr.read_u64::<LittleEndian>()? as usize;
        let uncomp_size = rdr.read_u32::<LittleEndian>()? as usize;
        let comp_size = rdr.read_u32::<LittleEndian>()? as usize;
        let _hash = rdr.read_u64::<LittleEndian>()?;
        bands.push(BandEntry {
            sec_idx,
            uncomp_offset,
            uncomp_size,
            comp_offset: 0,
            comp_size,
        });
    }

    pos = rdr.position() as usize;
    let mut cur_comp_offset = pos;
    for b in &mut bands {
        b.comp_offset = cur_comp_offset;
        cur_comp_offset += b.comp_size;
    }

    // 4. Selectively decompress only the sections needed for map rendering in parallel:
    // id=1 (Height), id=2 (Holes), id=3 (WaterLevel), id=4 (Owner), id=6 (SeaMask), id=7 (SeaWater), id=27 (Cells)
    let target_sec_ids = [1u32, 2, 3, 4, 6, 7, 27];
    let mut target_sec_indices = HashMap::new();
    for &sid in &target_sec_ids {
        if let Some(&sidx) = sec_id_to_idx.get(&sid) {
            target_sec_indices.insert(sidx as u32, sid);
        }
    }

    let relevant_bands: Vec<&BandEntry> = bands
        .iter()
        .filter(|b| target_sec_indices.contains_key(&b.sec_idx))
        .collect();

    let decompressed_bands: Result<Vec<(u32, usize, Vec<u8>)>> = relevant_bands
        .par_iter()
        .map(|b| {
            let comp_slice = &blob[b.comp_offset .. b.comp_offset + b.comp_size];
            let mut dec = ZlibDecoder::new(comp_slice);
            let mut out = Vec::with_capacity(b.uncomp_size);
            dec.read_to_end(&mut out)?;
            Ok((b.sec_idx, b.uncomp_offset, out))
        })
        .collect();

    let decompressed_bands = decompressed_bands?;

    let mut sec_buffers: HashMap<u32, Vec<u8>> = HashMap::new();
    for (&sidx, &sid) in &target_sec_indices {
        let uncomp_len = sections[sidx as usize].1;
        sec_buffers.insert(sid, vec![0u8; uncomp_len]);
    }
    for (sidx, uoff, data) in decompressed_bands {
        let sid = target_sec_indices[&sidx];
        if let Some(buf) = sec_buffers.get_mut(&sid) {
            buf[uoff .. uoff + data.len()].copy_from_slice(&data);
        }
    }

    let total_cells = resolution * resolution;

    // 5. Decode Sections in parallel / sequence
    let holes_raw = sec_buffers.get(&2).map(|v| &v[8..]).unwrap_or(&[]);
    let sea_raw = sec_buffers.get(&6).map(|v| &v[8..]).unwrap_or(&[]);
    let seawater_raw = sec_buffers.get(&7).map(|v| &v[8..]).unwrap_or(&[]);

    // Cells (Section 27)
    let mut plan_cells = Vec::new();
    if let Some(s27) = sec_buffers.get(&27) {
        if s27.len() >= 8 {
            let mut c27 = Cursor::new(s27.as_slice());
            let count = c27.read_u64::<LittleEndian>().unwrap_or(0) as usize;
            plan_cells.reserve(count);
            for _ in 0..count {
                if (c27.position() as usize) + 36 > s27.len() {
                    break;
                }
                plan_cells.push(PlanCell {
                    _grid_y: c27.read_i32::<LittleEndian>().unwrap_or(0),
                    _grid_x: c27.read_i32::<LittleEndian>().unwrap_or(0),
                    _base_y: c27.read_i32::<LittleEndian>().unwrap_or(0),
                    _base_x: c27.read_i32::<LittleEndian>().unwrap_or(0),
                    _rotation: c27.read_i32::<LittleEndian>().unwrap_or(0),
                    _lift_m: c27.read_i32::<LittleEndian>().unwrap_or(0),
                    _cell_id: c27.read_i32::<LittleEndian>().unwrap_or(0),
                    biome_family: c27.read_i32::<LittleEndian>().unwrap_or(-1),
                    _layer: c27.read_i32::<LittleEndian>().unwrap_or(0),
                });
            }
        }
    }

    // Decode Height (id=1), WaterLevel (id=3), and Owner (id=4) concurrently using rayon::join
    let ((height_grid, water_grid), owner_grid) = rayon::join(
        || {
            rayon::join(
                || decode_f32_planes(sec_buffers.get(&1).map(|v| v.as_slice()), total_cells),
                || decode_f32_planes(sec_buffers.get(&3).map(|v| v.as_slice()), total_cells),
            )
        },
        || decode_i16_planes(sec_buffers.get(&4).map(|v| v.as_slice()), total_cells),
    );

    // 6. Render `out_size x out_size` RGBA image
    // Note on Leaflet coordinate alignment:
    // - In the PWW1 grid:
    //   `gy` (0..6000) corresponds to Unreal Y from -600,000 to +600,000 cm
    //   `gx` (0..6000) corresponds to Unreal X from -600,000 to +600,000 cm
    // - In our Leaflet frontend (`index.html`), `ueToLatLng(x, y)` maps:
    //   `lat = x / 100.0` (Vertical axis: South = -6000 at bottom, North = +6000 at top)
    //   `lng = y / 100.0` (Horizontal axis: West = -6000 at left, East = +6000 at right)
    // -Wait: let's check how `test_masks_from_save.png` and `mod_preview.png` are oriented!
    //   In `test_masks_from_save.png`, row `py` (0..H top-to-bottom) was `gy` (0..6000), and col `px` (0..W left-to-right) was `gx` (0..6000).
    //   Wait: in `index.html`, `ueToLatLng(nx, ny)` sets `lat = nx / 100.0`, `lng = ny / 100.0`.
    //   Let's check whether `lat` (vertical, bottom-to-top) is `nx` or `ny`!
    //   Wait: earlier we verified that in `PWW1`:
    //   `gy = (uy + 600000) / 200` and `gx = (ux + 600000) / 200` gave 2550/2550 land hits!
    //   So in `PWW1`:
    //   - `gy` is Unreal `Y` (`-6000m` to `+6000m`)
    //   - `gx` is Unreal `X` (`-6000m` to `+6000m`)
    //   And in `index.html`:
    //   When a marker is placed at `ueToLatLng(nx, ny) = [nx / 100.0, ny / 100.0]`:
    //   - Leaflet `lat` (vertical axis, bottom = -6000, top = +6000) is `nx / 100.0` (which is `gx`!).
    //   - Leaflet `lng` (horizontal axis, left = -6000, right = +6000) is `ny / 100.0` (which is `gy`!).
    //   HOLD ON! Let's check this carefully!
    let mut rgba = vec![0u8; out_size * out_size * 4];

    rgba.par_chunks_exact_mut(out_size * 4)
        .enumerate()
        .for_each(|(py, row_bytes)| {
            // We will set gy and gx after verifying the orientation right below!
            let gy = (py * resolution) / out_size;
            let gy_prev = gy.saturating_sub(2);
            let gy_next = (gy + 2).min(resolution - 1);

            for px in 0..out_size {
                let gx = (px * resolution) / out_size;
                let gx_prev = gx.saturating_sub(2);
                let gx_next = (gx + 2).min(resolution - 1);

                let idx = gy * resolution + gx;
                let hole = holes_raw.get(idx).copied().unwrap_or(255);

                let out_off = px * 4;
                if hole != 0 {
                    // Void / terrain hole -> dark void or transparent
                    row_bytes[out_off] = 0;
                    row_bytes[out_off + 1] = 0;
                    row_bytes[out_off + 2] = 0;
                    row_bytes[out_off + 3] = 0;
                    continue;
                }

                let is_sea = sea_raw.get(idx).copied().unwrap_or(0) != 0
                    || seawater_raw.get(idx).copied().unwrap_or(0) != 0;

                let h = height_grid.get(idx).copied().unwrap_or(0.0);
                let w = water_grid.get(idx).copied().unwrap_or(f32::NAN);

                if is_sea {
                    // Ocean water with subtle depth shading
                    let depth_factor = ((h + 30.0) / 30.0).clamp(-1.0, 1.0);
                    let r = (65.0 + depth_factor * 8.0).clamp(0.0, 255.0) as u8;
                    let g = (111.0 + depth_factor * 12.0).clamp(0.0, 255.0) as u8;
                    let b = (127.0 + depth_factor * 14.0).clamp(0.0, 255.0) as u8;
                    row_bytes[out_off] = r;
                    row_bytes[out_off + 1] = g;
                    row_bytes[out_off + 2] = b;
                    row_bytes[out_off + 3] = 255;
                    continue;
                }

                // Check inland rivers / lakes
                if !w.is_nan() && w > -500.0 && w >= h - 1.5 {
                    row_bytes[out_off] = 75;
                    row_bytes[out_off + 1] = 132;
                    row_bytes[out_off + 2] = 148;
                    row_bytes[out_off + 3] = 255;
                    continue;
                }

                // Landmass pixel: look up cell owner & biome family
                let owner_idx = owner_grid.get(idx).copied().unwrap_or(-1);
                let mut base_rgb = [128.0f32, 128.0, 118.0];
                if owner_idx >= 0 && (owner_idx as usize) < plan_cells.len() {
                    let fam = plan_cells[owner_idx as usize].biome_family;
                    if fam >= 0 && (fam as usize) < BIOME_FAMILY_COLORS.len() {
                        base_rgb = BIOME_FAMILY_COLORS[fam as usize];
                    }
                }

                // Compute terrain normal for hillshading (2-cell step = 8 meters)
                let h_left = height_grid.get(gy * resolution + gx_prev).copied().unwrap_or(h);
                let h_right = height_grid.get(gy * resolution + gx_next).copied().unwrap_or(h);
                let h_up = height_grid.get(gy_prev * resolution + gx).copied().unwrap_or(h);
                let h_down = height_grid.get(gy_next * resolution + gx).copied().unwrap_or(h);

                let dz_dx = (h_right - h_left) / 8.0;
                let dz_dy = (h_down - h_up) / 8.0;
                let slope = (dz_dx * dz_dx + dz_dy * dz_dy).sqrt();

                // Directional sun vector from top-left (-0.6, -0.6, 0.53)
                let nx = -dz_dx;
                let ny = -dz_dy;
                let nz = 1.0f32;
                let inv_len = 1.0 / (nx * nx + ny * ny + nz * nz).sqrt();
                let dot = (nx * -0.55 + ny * -0.55 + nz * 0.63) * inv_len;
                let hillshade = (0.55 + 0.65 * dot).clamp(0.42, 1.32);

                // Elevation tinting (higher elevations brighten slightly toward rocky limestone)
                let elev_norm = ((h - 20.0) / 350.0).clamp(-0.2, 1.0);
                // Steep cliff darkening/graying
                let cliff_blend = ((slope - 0.6) / 1.5).clamp(0.0, 0.65);

                let mut r = base_rgb[0] * (1.0 - cliff_blend) + 102.0 * cliff_blend;
                let mut g = base_rgb[1] * (1.0 - cliff_blend) + 99.0 * cliff_blend;
                let mut b = base_rgb[2] * (1.0 - cliff_blend) + 95.0 * cliff_blend;

                r = (r + elev_norm * 22.0) * hillshade;
                g = (g + elev_norm * 20.0) * hillshade;
                b = (b + elev_norm * 18.0) * hillshade;

                row_bytes[out_off] = r.clamp(0.0, 255.0) as u8;
                row_bytes[out_off + 1] = g.clamp(0.0, 255.0) as u8;
                row_bytes[out_off + 2] = b.clamp(0.0, 255.0) as u8;
                row_bytes[out_off + 3] = 255;
            }
        });

    let png_bytes = encode_rgba_png(&rgba, out_size as u32, out_size as u32)?;

    // 7. Build 512x512 compact binary heightmap (`PWHM` format) for 3D terrain modeling
    let hm_size: usize = 512;
    let half_m = if world_extent_cm > 0.0 {
        (world_extent_cm / 100.0) as f32
    } else {
        6000.0f32
    };
    let mut heightmap_bytes = Vec::with_capacity(28 + hm_size * hm_size * 2);
    heightmap_bytes.extend_from_slice(b"PWHM");
    heightmap_bytes.extend_from_slice(&(hm_size as u32).to_le_bytes());
    heightmap_bytes.extend_from_slice(&(hm_size as u32).to_le_bytes());
    heightmap_bytes.extend_from_slice(&(-half_m).to_le_bytes());
    heightmap_bytes.extend_from_slice(&half_m.to_le_bytes());
    heightmap_bytes.extend_from_slice(&(-half_m).to_le_bytes());
    heightmap_bytes.extend_from_slice(&half_m.to_le_bytes());

    for hy in 0..hm_size {
        let gy = (hy * resolution) / hm_size;
        for hx in 0..hm_size {
            let gx = (hx * resolution) / hm_size;
            let idx = gy * resolution + gx;
            let hole = holes_raw.get(idx).copied().unwrap_or(255);
            let dm: i16 = if hole != 0 {
                i16::MIN // -32768 sentinel for void / hole cutout
            } else {
                let h = height_grid.get(idx).copied().unwrap_or(0.0);
                let w = water_grid.get(idx).copied().unwrap_or(f32::NAN);
                let is_sea = sea_raw.get(idx).copied().unwrap_or(0) != 0
                    || seawater_raw.get(idx).copied().unwrap_or(0) != 0;
                let eff_h = if is_sea {
                    h.max(-22.0)
                } else if !w.is_nan() && w > -500.0 && w > h {
                    w
                } else {
                    h
                };
                (eff_h * 10.0).round().clamp(-32760.0, 32760.0) as i16
            };
            heightmap_bytes.extend_from_slice(&dm.to_le_bytes());
        }
    }

    Ok(Some(GeneratedMapData {
        png_bytes,
        heightmap_bytes,
        resolution,
        grid_cm,
        world_extent_cm,
        summary_text,
    }))
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn decode_f32_planes(raw: Option<&[u8]>, count: usize) -> Vec<f32> {
    let Some(buf) = raw else {
        return Vec::new();
    };
    if buf.len() < 8 + count * 4 {
        return Vec::new();
    }
    let p0 = &buf[8 .. 8 + count];
    let p1 = &buf[8 + count .. 8 + count * 2];
    let p2 = &buf[8 + count * 2 .. 8 + count * 3];
    let p3 = &buf[8 + count * 3 .. 8 + count * 4];

    let mut out = Vec::with_capacity(count);
    let mut acc: u32 = 0;
    for i in 0..count {
        let delta = (p0[i] as u32)
            | ((p1[i] as u32) << 8)
            | ((p2[i] as u32) << 16)
            | ((p3[i] as u32) << 24);
        acc = acc.wrapping_add(delta);
        out.push(f32::from_bits(acc));
    }
    out
}

fn decode_i16_planes(raw: Option<&[u8]>, count: usize) -> Vec<i16> {
    let Some(buf) = raw else {
        return Vec::new();
    };
    if buf.len() < 8 + count * 2 {
        return Vec::new();
    }
    let p0 = &buf[8 .. 8 + count];
    let p1 = &buf[8 + count .. 8 + count * 2];

    let mut out = Vec::with_capacity(count);
    let mut acc: u16 = 0;
    for i in 0..count {
        let delta = (p0[i] as u16) | ((p1[i] as u16) << 8);
        acc = acc.wrapping_add(delta);
        out.push(acc as i16);
    }
    out
}

/// Encodes an RGBA8 buffer into a valid PNG image using `flate2` (zero external image crate needed).
pub fn encode_rgba_png(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
    let row_bytes = (width as usize) * 4;
    let mut raw_scanlines = Vec::with_capacity((row_bytes + 1) * (height as usize));

    // Use Sub filter (filter type 1) per row for much better PNG compression on terrain gradients
    for y in 0..(height as usize) {
        let row = &rgba[y * row_bytes .. (y + 1) * row_bytes];
        raw_scanlines.push(1u8); // Filter type 1 (Sub)
        for i in 0..row_bytes {
            let prev = if i >= 4 { row[i - 4] } else { 0 };
            raw_scanlines.push(row[i].wrapping_sub(prev));
        }
    }

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(&raw_scanlines)?;
    let idat_payload = encoder.finish()?;

    let mut png = Vec::with_capacity(idat_payload.len() + 128);
    // PNG signature
    png.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);

    // IHDR chunk
    let mut ihdr = [0u8; 13];
    ihdr[0..4].copy_from_slice(&width.to_be_bytes());
    ihdr[4..8].copy_from_slice(&height.to_be_bytes());
    ihdr[8] = 8; // bit depth 8
    ihdr[9] = 6; // color type 6 (RGBA)
    ihdr[10] = 0; // compression
    ihdr[11] = 0; // filter
    ihdr[12] = 0; // interlace
    write_png_chunk(&mut png, b"IHDR", &ihdr);

    // IDAT chunk
    write_png_chunk(&mut png, b"IDAT", &idat_payload);

    // IEND chunk
    write_png_chunk(&mut png, b"IEND", &[]);

    Ok(png)
}

fn write_png_chunk(out: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(chunk_type);
    out.extend_from_slice(data);
    let crc = crc32(chunk_type, data);
    out.extend_from_slice(&crc.to_be_bytes());
}

fn crc32(chunk_type: &[u8; 4], data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in chunk_type.iter().chain(data.iter()) {
        crc ^= b as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}
