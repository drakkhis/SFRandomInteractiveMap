use anyhow::{bail, Context, Result};
use byteorder::{ByteOrder, LittleEndian, WriteBytesExt};
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::{Cursor, Write};

use crate::decompressor::{decompress_save_chunks, PACKAGE_FILE_TAG};
use crate::models::{PlayerSummary, ResourceNodeSummary};
use crate::parser::{extract_rwg_summary, parse_save_game, parse_save_header};

pub const CHUNK_SIZE: usize = 131072; // 128 KiB standard Satisfactory chunk size
pub const CHUNK_HEADER_TAG2: u32 = 0x22222222;
pub const CHUNK_HEADER_TAG3: u8 = 0x00;
pub const CHUNK_HEADER_TAG4: u32 = 0x03000000;

/// Recompresses an uncompressed Satisfactory save payload into chunked .sav format.
///
/// * `header_bytes`: The uncompressed save header prefix (from byte 0 up to `chunk_start_offset`).
/// * `uncompressed_data`: The full uncompressed payload (including the leading 8-byte payload size).
pub fn recompress_save(header_bytes: &[u8], uncompressed_data: &[u8]) -> Result<Vec<u8>> {
    if uncompressed_data.len() < 8 {
        bail!("Uncompressed data too short to be a valid Satisfactory save");
    }

    // Estimate output capacity: header + roughly 50% of uncompressed data
    let mut out = Vec::with_capacity(header_bytes.len() + uncompressed_data.len() / 2);
    out.extend_from_slice(header_bytes);

    let total_len = uncompressed_data.len();
    let mut offset = 0;

    while offset < total_len {
        let chunk_end = (offset + CHUNK_SIZE).min(total_len);
        let uncompressed_slice = &uncompressed_data[offset..chunk_end];
        let uncomp_len = uncompressed_slice.len();

        // Compress chunk slice with Zlib
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(uncompressed_slice)
            .context("Failed to write to zlib encoder")?;
        let compressed_slice = encoder.finish().context("Failed to finish zlib compression")?;
        let comp_len = compressed_slice.len();

        // Write 49-byte chunk header
        out.write_u32::<LittleEndian>(PACKAGE_FILE_TAG)?;
        out.write_u32::<LittleEndian>(CHUNK_HEADER_TAG2)?;
        out.write_u8(CHUNK_HEADER_TAG3)?;
        out.write_u32::<LittleEndian>(CHUNK_SIZE as u32)?;
        out.write_u32::<LittleEndian>(CHUNK_HEADER_TAG4)?;

        out.write_u64::<LittleEndian>(comp_len as u64)?;
        out.write_u64::<LittleEndian>(uncomp_len as u64)?;
        out.write_u64::<LittleEndian>(comp_len as u64)?;
        out.write_u64::<LittleEndian>(uncomp_len as u64)?;

        // Write compressed data
        out.extend_from_slice(&compressed_slice);

        offset = chunk_end;
    }

    Ok(out)
}

/// Locates all player characters and updates their translation coordinates in the raw save,
/// returning the newly packaged and valid `.sav` binary stream.
pub fn modify_player_coordinates(
    raw_save_bytes: &[u8],
    new_x: f32,
    new_y: f32,
    new_z: f32,
) -> Result<(Vec<u8>, Vec<PlayerSummary>)> {
    let (header, chunk_offset) = parse_save_header(raw_save_bytes)
        .context("Failed to parse save header for coordinate modification")?;

    let mut decompressed = decompress_save_chunks(raw_save_bytes, chunk_offset)
        .context("Failed to decompress save chunks")?;

    let parsed_game = parse_save_game(&decompressed.uncompressed_data, header)
        .context("Failed to parse game structure")?;

    let summary = extract_rwg_summary(&parsed_game);

    let mut modified_players = Vec::new();

    // Check primary player from summary
    if let Some(mut p) = summary.player {
        if p.translation_offset > 0 && p.translation_offset + 12 <= decompressed.uncompressed_data.len() {
            LittleEndian::write_f32(
                &mut decompressed.uncompressed_data[p.translation_offset..p.translation_offset + 4],
                new_x,
            );
            LittleEndian::write_f32(
                &mut decompressed.uncompressed_data[p.translation_offset + 4..p.translation_offset + 8],
                new_y,
            );
            LittleEndian::write_f32(
                &mut decompressed.uncompressed_data[p.translation_offset + 8..p.translation_offset + 12],
                new_z,
            );

            p.transform.translation = [new_x, new_y, new_z];
            modified_players.push(p);
        }
    }

    if modified_players.is_empty() {
        bail!("No player character (Char_Player) found in save file to modify");
    }

    let header_bytes = &raw_save_bytes[0..chunk_offset];
    let repacked_bytes = recompress_save(header_bytes, &decompressed.uncompressed_data)
        .context("Failed to recompress modified save file")?;

    Ok((repacked_bytes, modified_players))
}

/// Normalize a quaternion [x, y, z, w].
pub fn quat_normalize(q: [f32; 4]) -> [f32; 4] {
    let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if len > 1e-6 {
        [q[0] / len, q[1] / len, q[2] / len, q[3] / len]
    } else {
        [0.0, 0.0, 0.0, 1.0]
    }
}

/// Invert a unit quaternion [x, y, z, w].
pub fn quat_inv(q: [f32; 4]) -> [f32; 4] {
    [-q[0], -q[1], -q[2], q[3]]
}

/// Multiply two quaternions q1 * q2 in [x, y, z, w] order.
pub fn quat_mul(q1: [f32; 4], q2: [f32; 4]) -> [f32; 4] {
    let (x1, y1, z1, w1) = (q1[0], q1[1], q1[2], q1[3]);
    let (x2, y2, z2, w2) = (q2[0], q2[1], q2[2], q2[3]);
    [
        w1 * x2 + x1 * w2 + y1 * z2 - z1 * y2,
        w1 * y2 - x1 * z2 + y1 * w2 + z1 * x2,
        w1 * z2 + x1 * y2 - y1 * x2 + z1 * w2,
        w1 * w2 - x1 * x2 - y1 * y2 - z1 * z2,
    ]
}

/// Rotate a 3D vector [x, y, z] by a unit quaternion [x, y, z, w].
pub fn quat_rotate_vec(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let u = [q[0], q[1], q[2]];
    let w = q[3];

    // t = 2 * (u x v)
    let tx = 2.0 * (u[1] * v[2] - u[2] * v[1]);
    let ty = 2.0 * (u[2] * v[0] - u[0] * v[2]);
    let tz = 2.0 * (u[0] * v[1] - u[1] * v[0]);

    // v' = v + w * t + (u x t)
    let uxt_x = u[1] * tz - u[2] * ty;
    let uxt_y = u[2] * tx - u[0] * tz;
    let uxt_z = u[0] * ty - u[1] * tx;

    [
        v[0] + w * tx + uxt_x,
        v[1] + w * ty + uxt_y,
        v[2] + w * tz + uxt_z,
    ]
}

/// Locates a resource node by instance_name and updates its translation coordinates (and optional slope rotation).
/// If the node is a Resource Well Core (BP_FrackingCore), all associated satellites
/// within 90 meters (9000 cm) are also automatically moved (and rotated if slope rotation is applied)
/// so the entire well cluster conforms to the hillside.
pub fn modify_node_coordinates(
    raw_save_bytes: &[u8],
    node_instance_name: &str,
    new_x: f32,
    new_y: f32,
    new_z: f32,
    new_rotation: Option<[f32; 4]>,
) -> Result<(Vec<u8>, Vec<ResourceNodeSummary>)> {
    let (header, chunk_offset) = parse_save_header(raw_save_bytes)
        .context("Failed to parse save header for node coordinate modification")?;

    let mut decompressed = decompress_save_chunks(raw_save_bytes, chunk_offset)
        .context("Failed to decompress save chunks")?;

    let parsed_game = parse_save_game(&decompressed.uncompressed_data, header)
        .context("Failed to parse game structure")?;

    let summary = extract_rwg_summary(&parsed_game);

    // Match node by exact instance_name or short name suffix
    let target_node = summary.resource_nodes.iter().find(|n| {
        n.instance_name == node_instance_name
            || n.instance_name.rsplit('.').next().unwrap_or("") == node_instance_name.rsplit('.').next().unwrap_or("")
            || n.instance_name.ends_with(node_instance_name)
            || node_instance_name.ends_with(&n.instance_name)
    }).cloned();

    let target_node = match target_node {
        Some(n) => n,
        None => bail!("Resource node '{}' not found in save file", node_instance_name),
    };

    if target_node.translation_offset < 16 || target_node.translation_offset + 12 > decompressed.uncompressed_data.len() {
        bail!("Invalid translation offset for node '{}'", node_instance_name);
    }

    let old_pos = target_node.transform.translation;
    let old_rot = target_node.transform.rotation;
    let dx = new_x - old_pos[0];
    let dy = new_y - old_pos[1];
    let dz = new_z - old_pos[2];

    let mut moved_nodes = Vec::new();

    // 1. Move (and optionally rotate) the target node
    if let Some(rot) = new_rotation {
        let norm_rot = quat_normalize(rot);
        LittleEndian::write_f32(
            &mut decompressed.uncompressed_data[target_node.translation_offset - 16..target_node.translation_offset - 12],
            norm_rot[0],
        );
        LittleEndian::write_f32(
            &mut decompressed.uncompressed_data[target_node.translation_offset - 12..target_node.translation_offset - 8],
            norm_rot[1],
        );
        LittleEndian::write_f32(
            &mut decompressed.uncompressed_data[target_node.translation_offset - 8..target_node.translation_offset - 4],
            norm_rot[2],
        );
        LittleEndian::write_f32(
            &mut decompressed.uncompressed_data[target_node.translation_offset - 4..target_node.translation_offset],
            norm_rot[3],
        );
    }

    LittleEndian::write_f32(
        &mut decompressed.uncompressed_data[target_node.translation_offset..target_node.translation_offset + 4],
        new_x,
    );
    LittleEndian::write_f32(
        &mut decompressed.uncompressed_data[target_node.translation_offset + 4..target_node.translation_offset + 8],
        new_y,
    );
    LittleEndian::write_f32(
        &mut decompressed.uncompressed_data[target_node.translation_offset + 8..target_node.translation_offset + 12],
        new_z,
    );

    let mut updated_target = target_node.clone();
    updated_target.transform.translation = [new_x, new_y, new_z];
    if let Some(rot) = new_rotation {
        updated_target.transform.rotation = quat_normalize(rot);
    }
    moved_nodes.push(updated_target);

    // 2. If it's a Fracking / Resource Well Core, move all satellites that belong to this cluster
    let is_core = target_node.node_type.as_deref() == Some("ResourceWellCore")
        || target_node.instance_name.contains("BP_FrackingCore");

    if is_core {
        // Delta rotation from old core rotation to new core rotation
        let delta_rot = new_rotation.map(|new_rot| {
            quat_normalize(quat_mul(quat_normalize(new_rot), quat_inv(old_rot)))
        });

        const CLUSTER_RADIUS_CM_SQ: f32 = 9000.0 * 9000.0; // 90 meters cluster radius
        for other in &summary.resource_nodes {
            if other.instance_name == target_node.instance_name {
                continue;
            }
            let is_sat = other.node_type.as_deref() == Some("ResourceWellSatellite")
                || other.instance_name.contains("BP_FrackingSatellite");
            if is_sat {
                let sat_x = other.transform.translation[0];
                let sat_y = other.transform.translation[1];
                let dist_sq = (sat_x - old_pos[0]).powi(2) + (sat_y - old_pos[1]).powi(2);
                if dist_sq <= CLUSTER_RADIUS_CM_SQ
                    && other.translation_offset >= 16
                    && other.translation_offset + 12 <= decompressed.uncompressed_data.len()
                {
                    let (sat_new_x, sat_new_y, sat_new_z, sat_new_rot) = if let Some(q_delta) = delta_rot {
                        let rel_x = sat_x - old_pos[0];
                        let rel_y = sat_y - old_pos[1];
                        let rel_z = other.transform.translation[2] - old_pos[2];
                        let rot_offset = quat_rotate_vec(q_delta, [rel_x, rel_y, rel_z]);
                        let s_rot = quat_normalize(quat_mul(q_delta, other.transform.rotation));
                        (
                            new_x + rot_offset[0],
                            new_y + rot_offset[1],
                            new_z + rot_offset[2],
                            Some(s_rot),
                        )
                    } else {
                        (
                            sat_x + dx,
                            sat_y + dy,
                            other.transform.translation[2] + dz,
                            None,
                        )
                    };

                    if let Some(s_rot) = sat_new_rot {
                        LittleEndian::write_f32(
                            &mut decompressed.uncompressed_data[other.translation_offset - 16..other.translation_offset - 12],
                            s_rot[0],
                        );
                        LittleEndian::write_f32(
                            &mut decompressed.uncompressed_data[other.translation_offset - 12..other.translation_offset - 8],
                            s_rot[1],
                        );
                        LittleEndian::write_f32(
                            &mut decompressed.uncompressed_data[other.translation_offset - 8..other.translation_offset - 4],
                            s_rot[2],
                        );
                        LittleEndian::write_f32(
                            &mut decompressed.uncompressed_data[other.translation_offset - 4..other.translation_offset],
                            s_rot[3],
                        );
                    }

                    LittleEndian::write_f32(
                        &mut decompressed.uncompressed_data[other.translation_offset..other.translation_offset + 4],
                        sat_new_x,
                    );
                    LittleEndian::write_f32(
                        &mut decompressed.uncompressed_data[other.translation_offset + 4..other.translation_offset + 8],
                        sat_new_y,
                    );
                    LittleEndian::write_f32(
                        &mut decompressed.uncompressed_data[other.translation_offset + 8..other.translation_offset + 12],
                        sat_new_z,
                    );

                    let mut updated_sat = other.clone();
                    updated_sat.transform.translation = [sat_new_x, sat_new_y, sat_new_z];
                    if let Some(s_rot) = sat_new_rot {
                        updated_sat.transform.rotation = s_rot;
                    }
                    moved_nodes.push(updated_sat);
                }
            }
        }
    }

    let header_bytes = &raw_save_bytes[0..chunk_offset];
    let repacked_bytes = recompress_save(header_bytes, &decompressed.uncompressed_data)
        .context("Failed to recompress save with modified node coordinates")?;

    Ok((repacked_bytes, moved_nodes))
}

/// Offsets the translation coordinates `(dx, dy, dz)` in cm for all selected player structures
/// (both `Build_*` actor headers and `FGLightweightBuildableSubsystem` instances).
pub fn modify_structures_offset(
    raw_save_bytes: &[u8],
    instance_names: &[String],
    dx: f32,
    dy: f32,
    dz: f32,
) -> Result<(Vec<u8>, usize)> {
    use std::collections::HashSet;

    if instance_names.is_empty() {
        bail!("No structures selected to move");
    }
    let targets: HashSet<&str> = instance_names.iter().map(|s| s.as_str()).collect();

    let (header, chunk_offset) = parse_save_header(raw_save_bytes)
        .context("Failed to parse save header for structure modification")?;
    let mut decompressed = decompress_save_chunks(raw_save_bytes, chunk_offset)
        .context("Failed to decompress save chunks")?;
    let parsed_game = parse_save_game(&decompressed.uncompressed_data, header)
        .context("Failed to parse game structure")?;
    let summary = extract_rwg_summary(&parsed_game);

    let mut moved_count = 0usize;
    let buf_len = decompressed.uncompressed_data.len();

    for st in &summary.player_structures {
        if !targets.contains(st.instance_name.as_str()) || st.translation_offset == 0 {
            continue;
        }
        let nx = st.transform.translation[0] + dx;
        let ny = st.transform.translation[1] + dy;
        let nz = st.transform.translation[2] + dz;

        if st.is_lightweight {
            if st.translation_offset + 24 <= buf_len {
                LittleEndian::write_f64(&mut decompressed.uncompressed_data[st.translation_offset..st.translation_offset + 8], nx as f64);
                LittleEndian::write_f64(&mut decompressed.uncompressed_data[st.translation_offset + 8..st.translation_offset + 16], ny as f64);
                LittleEndian::write_f64(&mut decompressed.uncompressed_data[st.translation_offset + 16..st.translation_offset + 24], nz as f64);
                moved_count += 1;
            }
        } else if st.translation_offset + 12 <= buf_len {
            LittleEndian::write_f32(&mut decompressed.uncompressed_data[st.translation_offset..st.translation_offset + 4], nx);
            LittleEndian::write_f32(&mut decompressed.uncompressed_data[st.translation_offset + 4..st.translation_offset + 8], ny);
            LittleEndian::write_f32(&mut decompressed.uncompressed_data[st.translation_offset + 8..st.translation_offset + 12], nz);
            moved_count += 1;
        }
    }

    if moved_count == 0 {
        bail!("None of the selected structures could be located in the save");
    }

    let header_bytes = &raw_save_bytes[0..chunk_offset];
    let repacked_bytes = recompress_save(header_bytes, &decompressed.uncompressed_data)
        .context("Failed to recompress save with modified structure coordinates")?;

    Ok((repacked_bytes, moved_count))
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct InventorySlotEdit {
    pub slot_index: usize,
    pub item_path: String,
    pub count: i32,
}

fn encode_ue_fstring(s: &str) -> Vec<u8> {
    if s.is_empty() {
        return 0i32.to_le_bytes().to_vec();
    }
    let mut buf = Vec::with_capacity(4 + s.len() + 1);
    buf.extend_from_slice(&((s.len() + 1) as i32).to_le_bytes());
    buf.extend_from_slice(s.as_bytes());
    buf.push(0u8);
    buf
}

/// Modifies one or more slots in a target `FGInventoryComponent` (Pioneer, Storage, or Machine)
/// and returns the recompressed `.sav` binary stream.
pub fn modify_inventory_slots(
    raw_save_bytes: &[u8],
    component_instance_name: &str,
    edits: &[InventorySlotEdit],
) -> Result<Vec<u8>> {
    if edits.is_empty() {
        bail!("No inventory edits provided");
    }

    let (header, chunk_offset) = parse_save_header(raw_save_bytes)
        .context("Failed to parse save header for inventory modification")?;
    let mut decompressed = decompress_save_chunks(raw_save_bytes, chunk_offset)
        .context("Failed to decompress save chunks")?;
    let parsed_game = parse_save_game(&decompressed.uncompressed_data, header)
        .context("Failed to parse game structure")?;

    let comp = parsed_game
        .levels
        .iter()
        .flat_map(|l| l.inventory_components.iter())
        .find(|c| c.instance_name == component_instance_name)
        .ok_or_else(|| anyhow::anyhow!("Inventory component '{}' not found", component_instance_name))?;

    let layout = crate::parser::locate_m_inventory_stacks(&decompressed.uncompressed_data, comp)?;
    let mut sorted_edits = edits.to_vec();
    sorted_edits.sort_by(|a, b| b.slot_index.cmp(&a.slot_index));
    sorted_edits.dedup_by_key(|e| e.slot_index);

    let mut net_delta: isize = 0;
    let buf = &mut decompressed.uncompressed_data;

    for edit in &sorted_edits {
        let span = layout
            .slots
            .get(edit.slot_index)
            .ok_or_else(|| anyhow::anyhow!("Slot index {} out of range ({})", edit.slot_index, layout.slots.len()))?;

        let (new_item, new_count) = if edit.item_path.trim().is_empty() || edit.count <= 0 {
            ("", 0i32)
        } else {
            (edit.item_path.trim(), edit.count)
        };

        // Update NumItems (always after Item inside the same slot, and we process slots descending)
        LittleEndian::write_i32(&mut buf[span.num_items_val_offset..span.num_items_val_offset + 4], new_count);

        let old_len = span.item_fstring_end - span.item_fstring_start;
        let encoded = encode_ue_fstring(new_item);
        let delta = encoded.len() as isize - old_len as isize;

        if delta != 0 {
            let old_item_prop_size = LittleEndian::read_u32(&buf[span.item_prop_size_offset..span.item_prop_size_offset + 4]) as isize;
            LittleEndian::write_u32(
                &mut buf[span.item_prop_size_offset..span.item_prop_size_offset + 4],
                (old_item_prop_size + delta) as u32,
            );
            buf.splice(span.item_fstring_start..span.item_fstring_end, encoded);
            net_delta += delta;
        }
    }

    if net_delta != 0 {
        let old_stacks_size = LittleEndian::read_u32(&buf[layout.stacks_prop_size_offset..layout.stacks_prop_size_offset + 4]) as isize;
        LittleEndian::write_u32(
            &mut buf[layout.stacks_prop_size_offset..layout.stacks_prop_size_offset + 4],
            (old_stacks_size + net_delta) as u32,
        );

        let old_obj_size = LittleEndian::read_u32(&buf[comp.object_size_offset..comp.object_size_offset + 4]) as isize;
        LittleEndian::write_u32(
            &mut buf[comp.object_size_offset..comp.object_size_offset + 4],
            (old_obj_size + net_delta) as u32,
        );

        let old_all_objs = LittleEndian::read_u64(&buf[comp.all_objects_size_offset..comp.all_objects_size_offset + 8]) as isize;
        LittleEndian::write_u64(
            &mut buf[comp.all_objects_size_offset..comp.all_objects_size_offset + 8],
            (old_all_objs + net_delta) as u64,
        );

        let new_total_payload = (buf.len() - 8) as u64;
        LittleEndian::write_u64(&mut buf[0..8], new_total_payload);
    }

    let header_bytes = &raw_save_bytes[0..chunk_offset];
    recompress_save(header_bytes, &decompressed.uncompressed_data)
        .context("Failed to recompress save with modified inventory")
}

/// Clears the Map Fog of War (`FGMapManager::mFogOfWarRawData`) in-place by setting all pixels
/// to `[0, 0, 0, 255]` (fully revealed) and returns the recompressed `.sav` binary stream.
pub fn clear_map_fog(raw_save_bytes: &[u8]) -> Result<Vec<u8>> {
    let (header, chunk_offset) = parse_save_header(raw_save_bytes)
        .context("Failed to parse save header for clearing map fog")?;
    let mut decompressed = decompress_save_chunks(raw_save_bytes, chunk_offset)
        .context("Failed to decompress save chunks")?;
    let parsed_game = parse_save_game(&decompressed.uncompressed_data, header)
        .context("Failed to parse game structure")?;

    let mut cleared = false;
    for lvl in &parsed_game.levels {
        for actor in &lvl.actors {
            if let Some((start, len)) = actor.fog_raw_data_range {
                if start + len <= decompressed.uncompressed_data.len() && len >= 4 {
                    for px in decompressed.uncompressed_data[start..start + len].chunks_exact_mut(4)
                    {
                        px[0] = 0;
                        px[1] = 0;
                        px[2] = 0;
                        px[3] = 255;
                    }
                    cleared = true;
                }
            }
        }
    }

    if !cleared {
        bail!("FGMapManager / mFogOfWarRawData not found in save file");
    }

    let header_bytes = &raw_save_bytes[0..chunk_offset];
    recompress_save(header_bytes, &decompressed.uncompressed_data)
        .context("Failed to recompress save with cleared map fog")
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct GameRuleUpdate {
    pub actor_instance_name: String,
    pub property_name: String,
    #[serde(default)]
    pub value_type: Option<String>,
    pub bool_val: Option<bool>,
    pub int_val: Option<i32>,
    pub float_val: Option<f32>,
}

fn encode_scalar_property(upd: &GameRuleUpdate) -> Result<Vec<u8>> {
    let kind = upd
        .value_type
        .as_deref()
        .or_else(|| {
            if upd.bool_val.is_some() {
                Some("bool")
            } else if upd.int_val.is_some() {
                Some("int")
            } else if upd.float_val.is_some() {
                Some("float")
            } else {
                None
            }
        })
        .ok_or_else(|| anyhow::anyhow!("Missing value_type/value for '{}'", upd.property_name))?;

    let mut out = Vec::new();
    out.extend_from_slice(&encode_ue_fstring(&upd.property_name));
    match kind {
        "bool" => {
            out.extend_from_slice(&encode_ue_fstring("BoolProperty"));
            out.extend_from_slice(&0u32.to_le_bytes()); // header_type_a
            out.extend_from_slice(&0u32.to_le_bytes()); // property_size = 0
            out.push(if upd.bool_val.unwrap_or(false) { 1 } else { 0 });
        }
        "int" => {
            out.extend_from_slice(&encode_ue_fstring("IntProperty"));
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&4u32.to_le_bytes());
            out.push(0u8); // has_guid = 0
            out.extend_from_slice(&upd.int_val.unwrap_or(0).to_le_bytes());
        }
        "float" => {
            out.extend_from_slice(&encode_ue_fstring("FloatProperty"));
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&4u32.to_le_bytes());
            out.push(0u8); // has_guid = 0
            out.extend_from_slice(&upd.float_val.unwrap_or(0.0).to_le_bytes());
        }
        other => bail!("Unsupported scalar property type '{}'", other),
    }
    Ok(out)
}

/// Updates Creative Mode header flag (`is_creative_mode_enabled`) and/or scalar game-rule
/// properties (`BoolProperty`, `IntProperty`, `FloatProperty`) in-place — or inserts missing
/// properties before `"None"` — and recompresses the `.sav`.
pub fn modify_game_settings(
    raw_save_bytes: &[u8],
    creative_mode: Option<bool>,
    updates: &[GameRuleUpdate],
) -> Result<Vec<u8>> {
    let (header, chunk_offset) = parse_save_header(raw_save_bytes)
        .context("Failed to parse save header for game settings modification")?;
    let mut header_bytes = raw_save_bytes[0..chunk_offset].to_vec();

    if let Some(enabled) = creative_mode {
        if header.save_version >= 13 && chunk_offset >= 4 {
            let val = if enabled { 1u32 } else { 0u32 };
            LittleEndian::write_u32(&mut header_bytes[chunk_offset - 4..chunk_offset], val);
        }
    }

    let mut decompressed = decompress_save_chunks(raw_save_bytes, chunk_offset)
        .context("Failed to decompress save chunks")?;
    let parsed_game = parse_save_game(&decompressed.uncompressed_data, header)
        .context("Failed to parse game structure")?;

    // Resolve actor references and sort descending by payload_start_abs so splicing
    // into an actor never shifts earlier actors' offsets.
    let mut ordered: Vec<(&crate::models::ParsedActor, &GameRuleUpdate)> = Vec::with_capacity(updates.len());
    for upd in updates {
        let actor = parsed_game
            .levels
            .iter()
            .flat_map(|l| l.actors.iter())
            .find(|a| a.instance_name == upd.actor_instance_name)
            .ok_or_else(|| anyhow::anyhow!("Actor '{}' not found in save", upd.actor_instance_name))?;
        ordered.push((actor, upd));
    }
    ordered.sort_by(|a, b| b.0.payload_start_abs.cmp(&a.0.payload_start_abs));

    for (actor, upd) in ordered {
        let mut actor_view = actor.clone();
        actor_view.object_size = LittleEndian::read_u32(
            &decompressed.uncompressed_data[actor.object_size_offset..actor.object_size_offset + 4],
        ) as usize;

        match crate::parser::locate_actor_scalar_or_none(
            &decompressed.uncompressed_data,
            &actor_view,
            &upd.property_name,
        )? {
            crate::parser::ScalarLocation::Found(kind, offset) => match kind {
                "bool" => {
                    if let Some(b) = upd.bool_val {
                        decompressed.uncompressed_data[offset] = if b { 1 } else { 0 };
                    }
                }
                "int" => {
                    if let Some(v) = upd.int_val {
                        LittleEndian::write_i32(
                            &mut decompressed.uncompressed_data[offset..offset + 4],
                            v,
                        );
                    }
                }
                "float" => {
                    if let Some(f) = upd.float_val {
                        LittleEndian::write_f32(
                            &mut decompressed.uncompressed_data[offset..offset + 4],
                            f,
                        );
                    }
                }
                _ => {}
            },
            crate::parser::ScalarLocation::InsertBeforeNone(none_offset) => {
                let encoded = encode_scalar_property(upd)?;
                let delta = encoded.len();
                let buf = &mut decompressed.uncompressed_data;
                buf.splice(none_offset..none_offset, encoded);

                let old_obj_size = LittleEndian::read_u32(
                    &buf[actor.object_size_offset..actor.object_size_offset + 4],
                ) as usize;
                LittleEndian::write_u32(
                    &mut buf[actor.object_size_offset..actor.object_size_offset + 4],
                    (old_obj_size + delta) as u32,
                );

                let old_all_objs = LittleEndian::read_u64(
                    &buf[actor.all_objects_size_offset..actor.all_objects_size_offset + 8],
                ) as usize;
                LittleEndian::write_u64(
                    &mut buf[actor.all_objects_size_offset..actor.all_objects_size_offset + 8],
                    (old_all_objs + delta) as u64,
                );

                let new_total_payload = (buf.len() - 8) as u64;
                LittleEndian::write_u64(&mut buf[0..8], new_total_payload);
            }
        }
    }

    recompress_save(&header_bytes, &decompressed.uncompressed_data)
        .context("Failed to recompress save with updated game settings")
}

fn parse_lightweight_id(name: &str) -> Option<(&str, usize)> {
    let rest = name.strip_prefix("Lightweight::")?;
    let idx_pos = rest.rfind("::")?;
    let actor_name = &rest[..idx_pos];
    let lw_idx: usize = rest[idx_pos + 2..].parse().ok()?;
    Some((actor_name, lw_idx))
}

fn read_fstring_at(buf: &[u8], pos: usize) -> Result<(String, usize)> {
    if pos + 4 > buf.len() {
        bail!("Unexpected EOF reading FString length");
    }
    let len = LittleEndian::read_i32(&buf[pos..pos + 4]);
    let mut p = pos + 4;
    if len == 0 {
        return Ok((String::new(), p));
    }
    if len > 0 {
        let byte_len = len as usize;
        if p + byte_len > buf.len() {
            bail!("FString out of bounds");
        }
        let s = String::from_utf8_lossy(&buf[p..p + byte_len.saturating_sub(1)]).to_string();
        p += byte_len;
        Ok((s, p))
    } else {
        let char_len = (-len) as usize;
        let byte_len = char_len * 2;
        if p + byte_len > buf.len() {
            bail!("UTF-16 FString out of bounds");
        }
        let u16s: Vec<u16> = buf[p..p + byte_len]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let s = String::from_utf16_lossy(&u16s[..u16s.len().saturating_sub(1)]);
        p += byte_len;
        Ok((s, p))
    }
}

fn clone_actor_header(
    hdr_bytes: &[u8],
    new_instance_name: &str,
    dx: f32,
    dy: f32,
    dz: f32,
) -> Result<Vec<u8>> {
    // Layout: u32(1), FString(type_path), FString(root_object), FString(instance_name),
    // u32(flags), u32(need_transform), f32[4] rot, f32[3] trans, f32[3] scale, u32(was_placed)
    let (type_path, p1) = read_fstring_at(hdr_bytes, 4)?;
    let (root_obj, p2) = read_fstring_at(hdr_bytes, p1)?;
    let (_old_inst, p3) = read_fstring_at(hdr_bytes, p2)?;
    let mut out = Vec::with_capacity(hdr_bytes.len() + new_instance_name.len());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&encode_ue_fstring(&type_path));
    out.extend_from_slice(&encode_ue_fstring(&root_obj));
    out.extend_from_slice(&encode_ue_fstring(new_instance_name));
    let tail_start = out.len();
    out.extend_from_slice(&hdr_bytes[p3..]);
    // In tail: u32(flags) [4B] + u32(need_transform) [4B] + f32[4] rot [16B] -> translation at +24
    let trans_off = tail_start + 24;
    if trans_off + 12 <= out.len() {
        let nx = LittleEndian::read_f32(&out[trans_off..trans_off + 4]) + dx;
        let ny = LittleEndian::read_f32(&out[trans_off + 4..trans_off + 8]) + dy;
        let nz = LittleEndian::read_f32(&out[trans_off + 8..trans_off + 12]) + dz;
        LittleEndian::write_f32(&mut out[trans_off..trans_off + 4], nx);
        LittleEndian::write_f32(&mut out[trans_off + 4..trans_off + 8], ny);
        LittleEndian::write_f32(&mut out[trans_off + 8..trans_off + 12], nz);
    }
    Ok(out)
}

fn clone_component_header(
    hdr_bytes: &[u8],
    old_actor_name: &str,
    new_actor_name: &str,
) -> Result<Vec<u8>> {
    // Layout: u32(0), FString(class_name), FString(root_object), FString(instance_name), u32(flags), FString(parent_actor_name)
    let (class_name, p1) = read_fstring_at(hdr_bytes, 4)?;
    let (root_obj, p2) = read_fstring_at(hdr_bytes, p1)?;
    let (old_inst, p3) = read_fstring_at(hdr_bytes, p2)?;
    let flags = LittleEndian::read_u32(&hdr_bytes[p3..p3 + 4]);
    let (old_parent, _) = read_fstring_at(hdr_bytes, p3 + 4)?;

    let new_inst = if let Some(rest) = old_inst.strip_prefix(old_actor_name) {
        format!("{}{}", new_actor_name, rest)
    } else {
        format!("{}_dup", old_inst)
    };
    let new_parent = if old_parent == old_actor_name {
        new_actor_name.to_string()
    } else if let Some(rest) = old_parent.strip_prefix(old_actor_name) {
        format!("{}{}", new_actor_name, rest)
    } else {
        old_parent
    };

    let mut out = Vec::with_capacity(hdr_bytes.len() + new_actor_name.len());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&encode_ue_fstring(&class_name));
    out.extend_from_slice(&encode_ue_fstring(&root_obj));
    out.extend_from_slice(&encode_ue_fstring(&new_inst));
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&encode_ue_fstring(&new_parent));
    Ok(out)
}

fn rewrite_actor_refs_in_body(body: &[u8], old_actor: &str, new_actor: &str) -> Vec<u8> {
    let old_prefix_bytes = old_actor.as_bytes();
    if old_prefix_bytes.is_empty() || body.len() < 12 {
        return body.to_vec();
    }
    let old_obj_size = LittleEndian::read_u32(&body[8..12]) as isize;
    let payload_end = (12 + old_obj_size as usize).min(body.len());

    let mut out = Vec::with_capacity(body.len() + 64);
    out.extend_from_slice(&body[..12]);
    let mut i = 12usize;
    let mut delta: isize = 0;

    while i < payload_end {
        if i + 4 + old_prefix_bytes.len() + 1 <= payload_end {
            let len = LittleEndian::read_i32(&body[i..i + 4]);
            if len > old_prefix_bytes.len() as i32 && len < 1024 {
                let ulen = len as usize;
                if i + 4 + ulen <= payload_end
                    && body[i + 4 + ulen - 1] == 0
                    && &body[i + 4..i + 4 + old_prefix_bytes.len()] == old_prefix_bytes
                {
                    let rest = &body[i + 4 + old_prefix_bytes.len()..i + 4 + ulen - 1];
                    if rest.iter().all(|&b| b.is_ascii() && b != 0) {
                        let rest_str = std::str::from_utf8(rest).unwrap_or("");
                        let replaced = format!("{}{}", new_actor, rest_str);
                        let enc = encode_ue_fstring(&replaced);
                        delta += enc.len() as isize - (4 + ulen) as isize;
                        out.extend_from_slice(&enc);
                        i += 4 + ulen;
                        continue;
                    }
                }
            }
        }
        out.push(body[i]);
        i += 1;
    }
    out.extend_from_slice(&body[payload_end..]);
    LittleEndian::write_u32(&mut out[8..12], (old_obj_size + delta) as u32);
    out
}

/// Deletes the selected regular actors (plus their child components) and/or lightweight
/// buildable instances from the `.sav` and returns the recompressed binary stream.
pub fn delete_structures_in_save(
    raw_save_bytes: &[u8],
    instance_names: &[String],
) -> Result<(Vec<u8>, usize)> {
    use std::collections::HashSet;

    if instance_names.is_empty() {
        bail!("No structures selected to delete");
    }

    let (header, chunk_offset) = parse_save_header(raw_save_bytes)
        .context("Failed to parse save header for deletion")?;
    let mut decompressed = decompress_save_chunks(raw_save_bytes, chunk_offset)
        .context("Failed to decompress save chunks")?;

    let mut lw_targets: HashSet<(&str, usize)> = HashSet::new();
    let mut actor_targets: HashSet<&str> = HashSet::new();
    for name in instance_names {
        if let Some((actor_name, idx)) = parse_lightweight_id(name) {
            lw_targets.insert((actor_name, idx));
        } else {
            actor_targets.insert(name.as_str());
        }
    }

    let mut deleted_count = 0usize;

    // 1. Delete lightweight buildable instances first (descending entry_start order)
    if !lw_targets.is_empty() {
        let parsed = parse_save_game(&decompressed.uncompressed_data, header.clone())?;
        let mut lw_removals = Vec::new();
        for lvl in &parsed.levels {
            for actor in &lvl.actors {
                for (idx, inst) in actor.lightweight_instances.iter().enumerate() {
                    if lw_targets.contains(&(actor.instance_name.as_str(), idx)) {
                        lw_removals.push(inst.clone());
                    }
                }
            }
        }
        lw_removals.sort_by(|a, b| b.entry_start.cmp(&a.entry_start));
        let buf = &mut decompressed.uncompressed_data;
        for inst in lw_removals {
            let delta = inst.entry_end - inst.entry_start;
            buf.drain(inst.entry_start..inst.entry_end);
            let old_cnt = LittleEndian::read_u32(&buf[inst.inst_count_offset..inst.inst_count_offset + 4]);
            LittleEndian::write_u32(&mut buf[inst.inst_count_offset..inst.inst_count_offset + 4], old_cnt.saturating_sub(1));
            let old_obj = LittleEndian::read_u32(&buf[inst.subsystem_object_size_offset..inst.subsystem_object_size_offset + 4]) as usize;
            LittleEndian::write_u32(&mut buf[inst.subsystem_object_size_offset..inst.subsystem_object_size_offset + 4], (old_obj - delta) as u32);
            let old_all = LittleEndian::read_u64(&buf[inst.subsystem_all_objects_size_offset..inst.subsystem_all_objects_size_offset + 8]) as usize;
            LittleEndian::write_u64(&mut buf[inst.subsystem_all_objects_size_offset..inst.subsystem_all_objects_size_offset + 8], (old_all - delta) as u64);
            deleted_count += 1;
        }
        let new_total = (buf.len() - 8) as u64;
        LittleEndian::write_u64(&mut buf[0..8], new_total);
    }

    // 2. Delete regular actors + child components (levels sorted descending by objects_insert_offset)
    if !actor_targets.is_empty() {
        let mut parsed = parse_save_game(&decompressed.uncompressed_data, header)?;
        parsed.levels.sort_by(|a, b| b.objects_insert_offset.cmp(&a.objects_insert_offset));
        let buf = &mut decompressed.uncompressed_data;

        for lvl in &parsed.levels {
            let mut spans: Vec<_> = lvl
                .object_spans
                .iter()
                .filter(|sp| {
                    actor_targets.contains(sp.instance_name.as_str())
                        || sp.parent_actor_name.as_deref().map_or(false, |p| actor_targets.contains(p))
                })
                .collect();
            if spans.is_empty() {
                continue;
            }
            let actor_hits = spans.iter().filter(|s| s.is_actor).count();
            let removed_obj_count = spans.len() as u32;

            // Remove bodies first (descending body_range.0; bodies always follow headers)
            spans.sort_by(|a, b| b.body_range.0.cmp(&a.body_range.0));
            let mut body_delta = 0usize;
            for sp in &spans {
                body_delta += sp.body_range.1 - sp.body_range.0;
                buf.drain(sp.body_range.0..sp.body_range.1);
            }
            let old_obj_cnt = LittleEndian::read_u32(&buf[lvl.object_count_offset..lvl.object_count_offset + 4]);
            LittleEndian::write_u32(&mut buf[lvl.object_count_offset..lvl.object_count_offset + 4], old_obj_cnt - removed_obj_count);
            let old_objs_sz = LittleEndian::read_u64(&buf[lvl.objects_size_offset..lvl.objects_size_offset + 8]) as usize;
            LittleEndian::write_u64(&mut buf[lvl.objects_size_offset..lvl.objects_size_offset + 8], (old_objs_sz - body_delta) as u64);

            // Remove headers next (descending header_range.0)
            spans.sort_by(|a, b| b.header_range.0.cmp(&a.header_range.0));
            let mut hdr_delta = 0usize;
            for sp in &spans {
                hdr_delta += sp.header_range.1 - sp.header_range.0;
                buf.drain(sp.header_range.0..sp.header_range.1);
            }
            let old_hdr_cnt = LittleEndian::read_u32(&buf[lvl.header_count_offset..lvl.header_count_offset + 4]);
            LittleEndian::write_u32(&mut buf[lvl.header_count_offset..lvl.header_count_offset + 4], old_hdr_cnt - removed_obj_count);
            let old_hdrs_sz = LittleEndian::read_u64(&buf[lvl.headers_size_offset..lvl.headers_size_offset + 8]) as usize;
            LittleEndian::write_u64(&mut buf[lvl.headers_size_offset..lvl.headers_size_offset + 8], (old_hdrs_sz - hdr_delta) as u64);

            deleted_count += actor_hits;
        }
        let new_total = (buf.len() - 8) as u64;
        LittleEndian::write_u64(&mut buf[0..8], new_total);
    }

    if deleted_count == 0 {
        bail!("None of the selected structures could be found to delete");
    }

    let header_bytes = &raw_save_bytes[0..chunk_offset];
    let repacked = recompress_save(header_bytes, &decompressed.uncompressed_data)
        .context("Failed to recompress save after deleting structures")?;
    Ok((repacked, deleted_count))
}

/// Duplicates the selected regular actors (with child components) and/or lightweight
/// buildable instances at an `(dx, dy, dz)` cm offset and returns the recompressed `.sav`.
pub fn duplicate_structures_in_save(
    raw_save_bytes: &[u8],
    instance_names: &[String],
    dx: f32,
    dy: f32,
    dz: f32,
) -> Result<(Vec<u8>, usize)> {
    use std::collections::HashSet;

    if instance_names.is_empty() {
        bail!("No structures selected to duplicate");
    }

    let (header, chunk_offset) = parse_save_header(raw_save_bytes)
        .context("Failed to parse save header for duplication")?;
    let mut decompressed = decompress_save_chunks(raw_save_bytes, chunk_offset)
        .context("Failed to decompress save chunks")?;

    let mut lw_targets: HashSet<(&str, usize)> = HashSet::new();
    let mut actor_targets: HashSet<&str> = HashSet::new();
    for name in instance_names {
        if let Some((actor_name, idx)) = parse_lightweight_id(name) {
            lw_targets.insert((actor_name, idx));
        } else {
            actor_targets.insert(name.as_str());
        }
    }

    let mut dup_count = 0usize;

    // 1. Duplicate lightweight buildable instances (descending entry_end order)
    if !lw_targets.is_empty() {
        let parsed = parse_save_game(&decompressed.uncompressed_data, header.clone())?;
        let mut lw_clones = Vec::new();
        for lvl in &parsed.levels {
            for actor in &lvl.actors {
                for (idx, inst) in actor.lightweight_instances.iter().enumerate() {
                    if lw_targets.contains(&(actor.instance_name.as_str(), idx)) {
                        lw_clones.push(inst.clone());
                    }
                }
            }
        }
        lw_clones.sort_by(|a, b| b.entry_end.cmp(&a.entry_end));
        let buf = &mut decompressed.uncompressed_data;
        for inst in lw_clones {
            let mut cloned = buf[inst.entry_start..inst.entry_end].to_vec();
            let nx = LittleEndian::read_f64(&cloned[32..40]) + dx as f64;
            let ny = LittleEndian::read_f64(&cloned[40..48]) + dy as f64;
            let nz = LittleEndian::read_f64(&cloned[48..56]) + dz as f64;
            LittleEndian::write_f64(&mut cloned[32..40], nx);
            LittleEndian::write_f64(&mut cloned[40..48], ny);
            LittleEndian::write_f64(&mut cloned[48..56], nz);

            let delta = cloned.len();
            buf.splice(inst.entry_end..inst.entry_end, cloned);
            let old_cnt = LittleEndian::read_u32(&buf[inst.inst_count_offset..inst.inst_count_offset + 4]);
            LittleEndian::write_u32(&mut buf[inst.inst_count_offset..inst.inst_count_offset + 4], old_cnt + 1);
            let old_obj = LittleEndian::read_u32(&buf[inst.subsystem_object_size_offset..inst.subsystem_object_size_offset + 4]) as usize;
            LittleEndian::write_u32(&mut buf[inst.subsystem_object_size_offset..inst.subsystem_object_size_offset + 4], (old_obj + delta) as u32);
            let old_all = LittleEndian::read_u64(&buf[inst.subsystem_all_objects_size_offset..inst.subsystem_all_objects_size_offset + 8]) as usize;
            LittleEndian::write_u64(&mut buf[inst.subsystem_all_objects_size_offset..inst.subsystem_all_objects_size_offset + 8], (old_all + delta) as u64);
            dup_count += 1;
        }
        let new_total = (buf.len() - 8) as u64;
        LittleEndian::write_u64(&mut buf[0..8], new_total);
    }

    // 2. Duplicate regular actors + child components
    if !actor_targets.is_empty() {
        let mut parsed = parse_save_game(&decompressed.uncompressed_data, header)?;
        parsed.levels.sort_by(|a, b| b.objects_insert_offset.cmp(&a.objects_insert_offset));
        let buf = &mut decompressed.uncompressed_data;
        let mut existing_names: HashSet<String> = parsed
            .levels
            .iter()
            .flat_map(|l| l.object_spans.iter().map(|s| s.instance_name.clone()))
            .collect();
        let mut seq = 1usize;

        for lvl in &parsed.levels {
            let target_actors: Vec<_> = lvl
                .object_spans
                .iter()
                .filter(|s| s.is_actor && actor_targets.contains(s.instance_name.as_str()))
                .collect();
            if target_actors.is_empty() {
                continue;
            }

            let mut new_headers = Vec::new();
            let mut new_bodies = Vec::new();
            let mut added_count = 0u32;

            for actor_sp in target_actors {
                let mut new_actor_name = format!("{}_dup_{}", actor_sp.instance_name, seq);
                while existing_names.contains(&new_actor_name) {
                    seq += 1;
                    new_actor_name = format!("{}_dup_{}", actor_sp.instance_name, seq);
                }
                existing_names.insert(new_actor_name.clone());
                seq += 1;

                let hdr_clone = clone_actor_header(
                    &buf[actor_sp.header_range.0..actor_sp.header_range.1],
                    &new_actor_name,
                    dx,
                    dy,
                    dz,
                )?;
                let body_clone = rewrite_actor_refs_in_body(
                    &buf[actor_sp.body_range.0..actor_sp.body_range.1],
                    &actor_sp.instance_name,
                    &new_actor_name,
                );
                new_headers.extend_from_slice(&hdr_clone);
                new_bodies.extend_from_slice(&body_clone);
                added_count += 1;
                dup_count += 1;

                for comp_sp in lvl.object_spans.iter().filter(|c| {
                    !c.is_actor && c.parent_actor_name.as_deref() == Some(actor_sp.instance_name.as_str())
                }) {
                    let comp_hdr = clone_component_header(
                        &buf[comp_sp.header_range.0..comp_sp.header_range.1],
                        &actor_sp.instance_name,
                        &new_actor_name,
                    )?;
                    let comp_body = rewrite_actor_refs_in_body(
                        &buf[comp_sp.body_range.0..comp_sp.body_range.1],
                        &actor_sp.instance_name,
                        &new_actor_name,
                    );
                    new_headers.extend_from_slice(&comp_hdr);
                    new_bodies.extend_from_slice(&comp_body);
                    added_count += 1;
                }
            }

            // Insert bodies first (higher offset)
            let body_delta = new_bodies.len();
            buf.splice(lvl.objects_insert_offset..lvl.objects_insert_offset, new_bodies);
            let old_obj_cnt = LittleEndian::read_u32(&buf[lvl.object_count_offset..lvl.object_count_offset + 4]);
            LittleEndian::write_u32(&mut buf[lvl.object_count_offset..lvl.object_count_offset + 4], old_obj_cnt + added_count);
            let old_objs_sz = LittleEndian::read_u64(&buf[lvl.objects_size_offset..lvl.objects_size_offset + 8]) as usize;
            LittleEndian::write_u64(&mut buf[lvl.objects_size_offset..lvl.objects_size_offset + 8], (old_objs_sz + body_delta) as u64);

            // Insert headers next (lower offset)
            let hdr_delta = new_headers.len();
            buf.splice(lvl.headers_insert_offset..lvl.headers_insert_offset, new_headers);
            let old_hdr_cnt = LittleEndian::read_u32(&buf[lvl.header_count_offset..lvl.header_count_offset + 4]);
            LittleEndian::write_u32(&mut buf[lvl.header_count_offset..lvl.header_count_offset + 4], old_hdr_cnt + added_count);
            let old_hdrs_sz = LittleEndian::read_u64(&buf[lvl.headers_size_offset..lvl.headers_size_offset + 8]) as usize;
            LittleEndian::write_u64(&mut buf[lvl.headers_size_offset..lvl.headers_size_offset + 8], (old_hdrs_sz + hdr_delta) as u64);
        }

        let new_total = (buf.len() - 8) as u64;
        LittleEndian::write_u64(&mut buf[0..8], new_total);
    }

    if dup_count == 0 {
        bail!("None of the selected structures could be duplicated");
    }

    let header_bytes = &raw_save_bytes[0..chunk_offset];
    let repacked = recompress_save(header_bytes, &decompressed.uncompressed_data)
        .context("Failed to recompress save after duplicating structures")?;
    Ok((repacked, dup_count))
}

/// Exports the selected regular actors (plus their child components) as a Satisfactory
/// Blueprint pair `(sbp_bytes, sbpcfg_bytes)`, centered around `(0, 0, min_z)`.
pub fn export_selection_as_blueprint(
    raw_save_bytes: &[u8],
    instance_names: &[String],
    description: &str,
) -> Result<(Vec<u8>, Vec<u8>, usize)> {
    use std::collections::HashSet;

    if instance_names.is_empty() {
        bail!("No structures selected for blueprint export");
    }

    let (header, chunk_offset) = parse_save_header(raw_save_bytes)
        .context("Failed to parse save header for blueprint export")?;
    let decompressed = decompress_save_chunks(raw_save_bytes, chunk_offset)
        .context("Failed to decompress save chunks")?;
    let buf = &decompressed.uncompressed_data;
    let parsed = parse_save_game(buf, header.clone())?;

    let actor_targets: HashSet<&str> = instance_names
        .iter()
        .filter(|n| parse_lightweight_id(n).is_none())
        .map(|s| s.as_str())
        .collect();

    if actor_targets.is_empty() {
        bail!("Selected items are lightweight instances; select placed factory/building actors to export a blueprint");
    }

    // Compute bounding box center (X, Y) and min Z in cm
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut min_z = f32::INFINITY;
    let mut max_z = f32::NEG_INFINITY;
    let mut matched_actors = Vec::new();

    for lvl in &parsed.levels {
        for actor in &lvl.actors {
            if actor_targets.contains(actor.instance_name.as_str()) {
                let [x, y, z] = actor.transform.translation;
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
                min_z = min_z.min(z);
                max_z = max_z.max(z);
                matched_actors.push(actor);
            }
        }
    }

    if matched_actors.is_empty() {
        bail!("None of the selected actors were found in the save");
    }

    let cx = (min_x + max_x) * 0.5;
    let cy = (min_y + max_y) * 0.5;
    let cz = min_z;

    let dim_x = (((max_x - min_x) / 800.0).ceil() as u32).max(4);
    let dim_y = (((max_y - min_y) / 800.0).ceil() as u32).max(4);
    let dim_z = (((max_z - min_z) / 800.0).ceil() as u32).max(4);

    // Clean 18-byte empty component body: u32(18) + u8(0) + FString("None") + u64(0)
    let mut empty_comp_entry = Vec::with_capacity(22);
    empty_comp_entry.extend_from_slice(&18u32.to_le_bytes());
    empty_comp_entry.push(0u8);
    empty_comp_entry.extend_from_slice(&encode_ue_fstring("None"));
    empty_comp_entry.extend_from_slice(&0u64.to_le_bytes());

    let mut hdr_stream = Vec::new();
    let mut obj_stream = Vec::new();
    let mut total_obj_count = 0u32;
    let mut exported_actors = 0usize;

    for lvl in &parsed.levels {
        for actor_sp in lvl
            .object_spans
            .iter()
            .filter(|s| s.is_actor && actor_targets.contains(s.instance_name.as_str()))
        {
            let bp_hdr = clone_actor_header(
                &buf[actor_sp.header_range.0..actor_sp.header_range.1],
                &actor_sp.instance_name,
                -cx,
                -cy,
                -cz,
            )?;
            let bp_obj_slice = |range: (usize, usize)| -> &[u8] {
                if header.save_version >= 41 {
                    let obj_sz = LittleEndian::read_u32(&buf[range.0 + 8..range.0 + 12]) as usize;
                    &buf[range.0 + 8..range.0 + 12 + obj_sz]
                } else {
                    &buf[range.0..range.1]
                }
            };
            hdr_stream.extend_from_slice(&bp_hdr);
            obj_stream.extend_from_slice(bp_obj_slice(actor_sp.body_range));
            total_obj_count += 1;
            exported_actors += 1;

            for comp_sp in lvl.object_spans.iter().filter(|c| {
                !c.is_actor && c.parent_actor_name.as_deref() == Some(actor_sp.instance_name.as_str())
            }) {
                let comp_hdr = &buf[comp_sp.header_range.0..comp_sp.header_range.1];
                let comp_class = read_fstring_at(comp_hdr, 4)
                    .map(|(s, _)| s)
                    .unwrap_or_default();
                let comp_obj = bp_obj_slice(comp_sp.body_range);

                let should_clear_conn = if comp_class.contains("FGPipeConnection") {
                    // Never carry stale mPipeNetworkID / mFluidBox / external pipe refs into blueprints
                    true
                } else if comp_class.contains("FGFactoryConnection")
                    || comp_class.contains("FGPowerConnection")
                    || comp_class.contains("FGRailroadTrackConnection")
                {
                    // Clear if any referenced PersistentLevel.<ActorName> is outside the blueprint
                    let body_str = String::from_utf8_lossy(comp_obj);
                    body_str.match_indices("PersistentLevel.").any(|(idx, _)| {
                        let rest = &body_str[idx..];
                        let end = rest.find('\0').unwrap_or(rest.len());
                        let path = &rest[..end];
                        let prefix_start = body_str[..idx]
                            .rfind(|c: char| c == '\0' || c.is_control())
                            .map(|p| p + 1)
                            .unwrap_or(0);
                        let full_ref = format!("{}{}", &body_str[prefix_start..idx], path);
                        let actor_ref = if let Some(dot1) = full_ref.find("PersistentLevel.") {
                            let after_pl = &full_ref[dot1 + "PersistentLevel.".len()..];
                            match after_pl.find('.') {
                                Some(dot2) => &full_ref[..dot1 + "PersistentLevel.".len() + dot2],
                                None => full_ref.as_str(),
                            }
                        } else {
                            full_ref.as_str()
                        };
                        !actor_targets.contains(actor_ref)
                    })
                } else {
                    false
                };

                hdr_stream.extend_from_slice(comp_hdr);
                if should_clear_conn {
                    obj_stream.extend_from_slice(&empty_comp_entry);
                } else {
                    obj_stream.extend_from_slice(comp_obj);
                }
                total_obj_count += 1;
            }
        }
    }

    // Build uncompressed blueprint payload:
    // u32(total_minus_4) + u32(hdr_section_len) + u32(count) + hdr_stream + u32(obj_section_len) + u32(count) + obj_stream
    let hdr_section_len = (4 + hdr_stream.len()) as u32;
    let obj_section_len = (4 + obj_stream.len()) as u32;
    let total_minus_4 = 4 + hdr_section_len + 4 + obj_section_len;

    let mut bp_uncompressed = Vec::with_capacity(4 + total_minus_4 as usize);
    bp_uncompressed.extend_from_slice(&total_minus_4.to_le_bytes());
    bp_uncompressed.extend_from_slice(&hdr_section_len.to_le_bytes());
    bp_uncompressed.extend_from_slice(&total_obj_count.to_le_bytes());
    bp_uncompressed.extend_from_slice(&hdr_stream);
    bp_uncompressed.extend_from_slice(&obj_section_len.to_le_bytes());
    bp_uncompressed.extend_from_slice(&total_obj_count.to_le_bytes());
    bp_uncompressed.extend_from_slice(&obj_stream);

    // Build .sbp header
    let mut sbp_header = Vec::new();
    sbp_header.extend_from_slice(&2u32.to_le_bytes()); // Blueprint header version 2
    sbp_header.extend_from_slice(&header.save_version.to_le_bytes());
    sbp_header.extend_from_slice(&header.build_version.to_le_bytes());
    sbp_header.extend_from_slice(&dim_x.to_le_bytes());
    sbp_header.extend_from_slice(&dim_y.to_le_bytes());
    sbp_header.extend_from_slice(&dim_z.to_le_bytes());
    sbp_header.extend_from_slice(&0u32.to_le_bytes()); // Item cost count (0 -> recalculated by game)
    sbp_header.extend_from_slice(&0u32.to_le_bytes()); // Recipe count

    if header.save_version >= 53 {
        let mut vrdr = Cursor::new(&buf[8..]);
        if crate::parser::read_save_object_version_data(&mut vrdr).is_ok() {
            let ver_end = 8 + vrdr.position() as usize;
            sbp_header.extend_from_slice(&buf[8..ver_end]);
        }
    }

    let sbp_bytes = recompress_save(&sbp_header, &bp_uncompressed)
        .context("Failed to compress .sbp blueprint")?;

    // Build .sbpcfg (config version 6)
    let mut sbpcfg_bytes = Vec::with_capacity(112);
    sbpcfg_bytes.extend_from_slice(&6u32.to_le_bytes()); // Config version 6
    sbpcfg_bytes.extend_from_slice(&encode_ue_fstring(description));
    sbpcfg_bytes.extend_from_slice(&782i32.to_le_bytes()); // Default Blueprint Icon ID
    sbpcfg_bytes.extend_from_slice(&0.107f32.to_le_bytes()); // R
    sbpcfg_bytes.extend_from_slice(&0.434f32.to_le_bytes()); // G
    sbpcfg_bytes.extend_from_slice(&0.558f32.to_le_bytes()); // B
    sbpcfg_bytes.extend_from_slice(&1.0f32.to_le_bytes());   // A
    sbpcfg_bytes.extend_from_slice(&encode_ue_fstring("/Game/FactoryGame/-Shared/Blueprint/IconLibrary"));
    sbpcfg_bytes.extend_from_slice(&encode_ue_fstring("IconLibrary"));
    sbpcfg_bytes.extend_from_slice(&6u32.to_le_bytes());
    sbpcfg_bytes.push(0u8);

    Ok((sbp_bytes, sbpcfg_bytes, exported_actors))
}
