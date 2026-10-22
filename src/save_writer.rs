use anyhow::{bail, Context, Result};
use byteorder::{ByteOrder, LittleEndian, WriteBytesExt};
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::Write;

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
