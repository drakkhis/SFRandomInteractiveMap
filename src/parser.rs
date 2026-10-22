use crate::models::*;
use anyhow::{bail, Context, Result};
use byteorder::{LittleEndian, ReadBytesExt};
use std::collections::HashMap;
use std::io::{Cursor, Read};

/// Helper to read an Unreal Engine FString.
pub fn read_fstring(rdr: &mut Cursor<&[u8]>) -> Result<String> {
    let strlen = rdr.read_i32::<LittleEndian>()?;
    if strlen == 0 {
        return Ok(String::new());
    }

    if strlen > 0 {
        let len = strlen as usize;
        let mut buf = vec![0u8; len];
        rdr.read_exact(&mut buf)?;
        // Strip trailing null terminator if present
        if let Some(&0) = buf.last() {
            buf.pop();
        }
        Ok(String::from_utf8_lossy(&buf).to_string())
    } else {
        let char_count = (-strlen) as usize;
        let byte_count = char_count * 2;
        let mut buf = vec![0u8; byte_count];
        rdr.read_exact(&mut buf)?;

        let mut u16_chars = Vec::with_capacity(char_count);
        for chunk in buf.chunks_exact(2) {
            u16_chars.push(u16::from_le_bytes([chunk[0], chunk[1]]));
        }
        if let Some(&0) = u16_chars.last() {
            u16_chars.pop();
        }
        Ok(String::from_utf16_lossy(&u16_chars))
    }
}

/// Helper to read an Unreal Engine ObjectReference.
pub fn read_object_reference(rdr: &mut Cursor<&[u8]>) -> Result<ObjectReference> {
    let level_name = read_fstring(rdr)?;
    let path_name = read_fstring(rdr)?;
    Ok(ObjectReference {
        level_name,
        path_name,
    })
}

/// Helper to skip package name metadata in UE 5.3+ property headers.
fn skip_package_name(rdr: &mut Cursor<&[u8]>) -> Result<()> {
    let flag1 = rdr.read_u32::<LittleEndian>()? != 0;
    if flag1 {
        let _ = read_fstring(rdr)?;
        let flag2 = rdr.read_u32::<LittleEndian>()? != 0;
        if flag2 {
            let _ = read_fstring(rdr)?;
            let _ = read_fstring(rdr)?;
        }
    }
    Ok(())
}

/// Version metadata container from UE save headers.
#[derive(Debug, Clone)]
pub struct SaveObjectVersionData {
    pub ue5_version: i32,
}

/// Reads SaveObjectVersionData from the cursor.
pub fn read_save_object_version_data(rdr: &mut Cursor<&[u8]>) -> Result<SaveObjectVersionData> {
    let _version = rdr.read_u32::<LittleEndian>()?;
    let _ue4_ver = rdr.read_u32::<LittleEndian>()?;
    let ue5_ver = rdr.read_u32::<LittleEndian>()? as i32;
    let _licensee = rdr.read_u32::<LittleEndian>()?;

    let _engine_major = rdr.read_u16::<LittleEndian>()?;
    let _engine_minor = rdr.read_u16::<LittleEndian>()?;
    let _engine_patch = rdr.read_u16::<LittleEndian>()?;
    let _changelist = rdr.read_u32::<LittleEndian>()?;
    let _branch = read_fstring(rdr)?;

    let count = rdr.read_u32::<LittleEndian>()?;
    for _ in 0..count {
        let _key_a = rdr.read_u64::<LittleEndian>()?;
        let _key_b = rdr.read_u64::<LittleEndian>()?;
        let _ver = rdr.read_u32::<LittleEndian>()?;
    }

    Ok(SaveObjectVersionData {
        ue5_version: ue5_ver,
    })
}

/// Parses the uncompressed header from the beginning of the save file.
/// Returns the parsed SaveHeader and the byte offset where compressed chunks start.
pub fn parse_save_header(data: &[u8]) -> Result<(SaveHeader, usize)> {
    let mut rdr = Cursor::new(data);

    let save_header_type = rdr.read_u32::<LittleEndian>()?;
    if save_header_type != 14 && save_header_type != 13 {
        bail!("Unsupported save header version: {}", save_header_type);
    }

    let save_version = rdr.read_u32::<LittleEndian>()?;
    let build_version = rdr.read_u32::<LittleEndian>()?;

    let save_name = if save_version >= 14 {
        read_fstring(&mut rdr)?
    } else {
        String::new()
    };

    let map_name = read_fstring(&mut rdr)?;
    let map_options = read_fstring(&mut rdr)?;
    let session_name = read_fstring(&mut rdr)?;
    let play_duration_seconds = rdr.read_u32::<LittleEndian>()?;
    let save_date_time_ticks = rdr.read_u64::<LittleEndian>()?;
    let session_visibility = rdr.read_u8()?;

    let editor_object_version = if save_version >= 7 {
        rdr.read_u32::<LittleEndian>()?
    } else {
        0
    };

    let mod_metadata = if save_version >= 8 {
        read_fstring(&mut rdr)?
    } else {
        String::new()
    };

    let is_modded_save = rdr.read_u32::<LittleEndian>()? != 0;

    let save_identifier = if save_version >= 10 {
        read_fstring(&mut rdr)?
    } else {
        String::new()
    };

    let mut is_creative_mode_enabled = false;
    if save_version >= 13 {
        let _ = rdr.read_u32::<LittleEndian>()?;
        let _ = rdr.read_u32::<LittleEndian>()?;
        let _ = rdr.read_u64::<LittleEndian>()?;
        let _ = rdr.read_u64::<LittleEndian>()?;
        is_creative_mode_enabled = rdr.read_u32::<LittleEndian>()? != 0;
    }

    let chunk_start_offset = rdr.position() as usize;

    let header = SaveHeader {
        save_header_type,
        save_version,
        build_version,
        save_name,
        map_name,
        map_options,
        session_name,
        play_duration_seconds,
        save_date_time_ticks,
        session_visibility,
        editor_object_version,
        mod_metadata,
        is_modded_save,
        save_identifier,
        is_creative_mode_enabled,
    };

    Ok((header, chunk_start_offset))
}

/// Parsed level containing all its headers and parsed actors.
pub struct ParsedLevel {
    pub level_name: Option<String>,
    pub headers: Vec<LevelObjectHeader>,
    pub actors: Vec<ParsedActor>,
}

/// Result of parsing the entire decompressed save data.
pub struct ParsedSaveGame {
    pub header: SaveHeader,
    pub levels: Vec<ParsedLevel>,
}

/// Parses the full decompressed save data stream.
pub fn parse_save_game(data: &[u8], header: SaveHeader) -> Result<ParsedSaveGame> {
    let mut rdr = Cursor::new(data);

    // Skip the first 8 bytes (total uncompressed size header)
    rdr.set_position(8);

    let mut persistent_level_ue5_version = -1;
    if header.save_version >= 53 {
        let ver_data = read_save_object_version_data(&mut rdr)
            .context("Failed reading persistent level version data")?;
        persistent_level_ue5_version = ver_data.ue5_version;
    }

    // Partitions / Grids
    let partition_count = rdr.read_u32::<LittleEndian>()?;
    for _ in 0..partition_count {
        let _part_name = read_fstring(&mut rdr)?;
        let _i = rdr.read_u32::<LittleEndian>()?;
        let _grid_hex = rdr.read_u32::<LittleEndian>()?;
        let count = rdr.read_u32::<LittleEndian>()?;
        for _ in 0..count {
            let _lvl_name = read_fstring(&mut rdr)?;
            let _lhex = rdr.read_u32::<LittleEndian>()?;
        }
    }

    // Levels
    let level_count = rdr.read_u32::<LittleEndian>()? as usize;
    let mut parsed_levels = Vec::with_capacity(level_count + 1);

    for _ in 0..level_count {
        let lvl = parse_single_level(
            &mut rdr,
            data,
            header.save_version,
            -1,
            false,
        )?;
        parsed_levels.push(lvl);
    }

    // Final persistent level
    let persistent_lvl = parse_single_level(
        &mut rdr,
        data,
        header.save_version,
        persistent_level_ue5_version,
        true,
    )?;
    parsed_levels.push(persistent_lvl);

    Ok(ParsedSaveGame {
        header,
        levels: parsed_levels,
    })
}

/// Parses a single level (both ActorHeaders and Object stream).
fn parse_single_level(
    rdr: &mut Cursor<&[u8]>,
    full_data: &[u8],
    header_save_version: u32,
    persistent_level_ue5_version: i32,
    persistent_level_flag: bool,
) -> Result<ParsedLevel> {
    let level_name = if persistent_level_flag {
        None
    } else {
        Some(read_fstring(rdr)?)
    };

    let headers_and_collectables_size = rdr.read_u64::<LittleEndian>()? as u64;
    let header_start = rdr.position();
    let actor_and_component_count = rdr.read_u32::<LittleEndian>()? as usize;

    let mut object_headers = Vec::with_capacity(actor_and_component_count);

    for _ in 0..actor_and_component_count {
        let header_type = rdr.read_u32::<LittleEndian>()?;
        if header_type == 1 {
            let type_path = read_fstring(rdr)?;
            let root_object = read_fstring(rdr)?;
            let instance_name = read_fstring(rdr)?;
            let flags = rdr.read_u32::<LittleEndian>()?;
            let need_transform = rdr.read_u32::<LittleEndian>()? != 0;

            let rot_x = rdr.read_f32::<LittleEndian>()?;
            let rot_y = rdr.read_f32::<LittleEndian>()?;
            let rot_z = rdr.read_f32::<LittleEndian>()?;
            let rot_w = rdr.read_f32::<LittleEndian>()?;

            let translation_offset = rdr.position() as usize;
            let pos_x = rdr.read_f32::<LittleEndian>()?;
            let pos_y = rdr.read_f32::<LittleEndian>()?;
            let pos_z = rdr.read_f32::<LittleEndian>()?;

            let scale_x = rdr.read_f32::<LittleEndian>()?;
            let scale_y = rdr.read_f32::<LittleEndian>()?;
            let scale_z = rdr.read_f32::<LittleEndian>()?;

            let was_placed = rdr.read_u32::<LittleEndian>()? != 0;

            object_headers.push(LevelObjectHeader::Actor(ActorHeader {
                type_path,
                root_object,
                instance_name,
                flags,
                need_transform,
                transform: Transform {
                    rotation: [rot_x, rot_y, rot_z, rot_w],
                    translation: [pos_x, pos_y, pos_z],
                    scale: [scale_x, scale_y, scale_z],
                },
                was_placed_in_level: was_placed,
                translation_offset,
            }));
        } else if header_type == 0 {
            let class_name = read_fstring(rdr)?;
            let root_object = read_fstring(rdr)?;
            let instance_name = read_fstring(rdr)?;
            let flags = rdr.read_u32::<LittleEndian>()?;
            let parent_actor_name = read_fstring(rdr)?;

            object_headers.push(LevelObjectHeader::Component(ComponentHeader {
                class_name,
                root_object,
                instance_name,
                flags,
                parent_actor_name,
            }));
        } else {
            bail!("Invalid header type: {}", header_type);
        }
    }

    if persistent_level_flag {
        let level_persistent_flag = rdr.read_u32::<LittleEndian>()? != 0;
        if level_persistent_flag {
            let _ = read_fstring(rdr)?; // "Persistent_Level"
        }
    }

    // Collectables #1 (if any remaining bytes in header section)
    let current_pos = rdr.position();
    let expected_header_end = header_start + headers_and_collectables_size;
    if current_pos < expected_header_end {
        let collected_count1 = rdr.read_u32::<LittleEndian>()?;
        for _ in 0..collected_count1 {
            let _ = read_object_reference(rdr)?;
        }
    }
    rdr.set_position(expected_header_end);

    // Objects Section
    let all_objects_size = rdr.read_u64::<LittleEndian>()?;
    let object_data_start = rdr.position();
    let after_objects_pos = object_data_start + all_objects_size;

    // Advance cursor to footer of level to determine object UE5 version
    rdr.set_position(after_objects_pos);
    let _level_save_version = rdr.read_u32::<LittleEndian>()?;

    let mut level_ue5_version = persistent_level_ue5_version;
    if !persistent_level_flag {
        let collected_count2 = rdr.read_u32::<LittleEndian>()?;
        for _ in 0..collected_count2 {
            let _ = read_object_reference(rdr)?;
        }
        if header_save_version >= 53 {
            let has_version_data = rdr.read_u32::<LittleEndian>()? != 0;
            if has_version_data {
                let v_data = read_save_object_version_data(rdr)?;
                level_ue5_version = v_data.ue5_version;
            }
        }
    }

    // Now parse objects from full_data slice
    let object_slice = &full_data[object_data_start as usize..after_objects_pos as usize];
    let mut obj_rdr = Cursor::new(object_slice);

    let object_count = obj_rdr.read_u32::<LittleEndian>()? as usize;
    if object_count != actor_and_component_count {
        bail!(
            "Object count mismatch in level: {} != {}",
            object_count,
            actor_and_component_count
        );
    }

    let mut parsed_actors = Vec::new();

    for idx in 0..object_count {
        let header = &object_headers[idx];
        let object_game_version = obj_rdr.read_u32::<LittleEndian>()?;
        let _should_migrate = obj_rdr.read_u32::<LittleEndian>()? != 0;
        let object_size = obj_rdr.read_u32::<LittleEndian>()? as u64;
        let payload_start = obj_rdr.position();
        let mut jump_offset = payload_start + object_size;

        let mut object_ue5_version = level_ue5_version;
        if object_game_version >= 53 {
            obj_rdr.set_position(jump_offset);
            if obj_rdr.position() + 4 <= object_slice.len() as u64 {
                let should_serialize_version =
                    obj_rdr.read_u32::<LittleEndian>()? != 0;
                if should_serialize_version {
                    if let Ok(v_data) = read_save_object_version_data(&mut obj_rdr) {
                        object_ue5_version = v_data.ue5_version;
                    }
                }
                jump_offset = obj_rdr.position();
            }
        }

        // Return to payload_start to parse properties
        obj_rdr.set_position(payload_start);

        let mut is_actor = false;
        if let LevelObjectHeader::Actor(_) = header {
            is_actor = true;
            // Read parent object reference and component references
            let _ = read_object_reference(&mut obj_rdr)?;
            let comp_count = obj_rdr.read_u32::<LittleEndian>()?;
            for _ in 0..comp_count {
                let _ = read_object_reference(&mut obj_rdr)?;
            }
        }

        if object_ue5_version >= 1011 {
            let _ = obj_rdr.read_u8()?; // serialization control (0)
        }

        let properties = parse_properties_tree(
            &mut obj_rdr,
            object_ue5_version,
            payload_start + object_size,
        );

        if is_actor {
            if let LevelObjectHeader::Actor(actor_hdr) = header {
                parsed_actors.push(ParsedActor {
                    type_path: actor_hdr.type_path.clone(),
                    root_object: actor_hdr.root_object.clone(),
                    instance_name: actor_hdr.instance_name.clone(),
                    flags: actor_hdr.flags,
                    transform: actor_hdr.transform,
                    was_placed_in_level: actor_hdr.was_placed_in_level,
                    properties,
                    translation_offset: actor_hdr.translation_offset,
                });
            }
        }

        // Advance safely to jump_offset
        obj_rdr.set_position(jump_offset);
    }

    Ok(ParsedLevel {
        level_name,
        headers: object_headers,
        actors: parsed_actors,
    })
}

/// Parses the property tree for an object safely.
/// Handles custom mod structs prefixed with "PW" by skipping their exact byte lengths.
fn parse_properties_tree(
    rdr: &mut Cursor<&[u8]>,
    object_ue5_version: i32,
    max_payload_offset: u64,
) -> HashMap<String, PropertyValue> {
    let mut properties = HashMap::new();

    loop {
        if rdr.position() >= max_payload_offset {
            break;
        }

        let property_name = match read_fstring(rdr) {
            Ok(s) => s,
            Err(_) => break,
        };

        if property_name == "None" || property_name.is_empty() {
            break;
        }

        let property_type = match read_fstring(rdr) {
            Ok(s) => s,
            Err(_) => break,
        };

        let property_header_flag = object_ue5_version >= 1012;
        let mut struct_property_type = None;
        let mut array_type = None;
        let mut structure_sub_type = None;
        let mut enum_name = None;

        if property_header_flag {
            let header_type_a = match rdr.read_u32::<LittleEndian>() {
                Ok(v) => v,
                Err(_) => break,
            };

            if header_type_a == 1 {
                if property_type == "ArrayProperty" {
                    if let Ok(arr_t) = read_fstring(rdr) {
                        array_type = Some(arr_t);
                    }
                    if let Ok(header_type_b) = rdr.read_u32::<LittleEndian>() {
                        if header_type_b == 1 {
                            if let Ok(sub_t) = read_fstring(rdr) {
                                structure_sub_type = Some(sub_t);
                            }
                            let _ = skip_package_name(rdr);
                        } else if header_type_b == 2 {
                            if let Ok(e_name) = read_fstring(rdr) {
                                enum_name = Some(e_name);
                            }
                            let _ = skip_package_name(rdr);
                            let _ = read_fstring(rdr);
                            let _ = rdr.read_u32::<LittleEndian>();
                        }
                    }
                } else if property_type == "ByteProperty" {
                    if let Ok(e_name) = read_fstring(rdr) {
                        enum_name = Some(e_name);
                    }
                    let _ = skip_package_name(rdr);
                } else if property_type == "SetProperty" {
                    let _ = read_fstring(rdr);
                    let _ = skip_package_name(rdr);
                } else if property_type == "StructProperty" {
                    if let Ok(s_t) = read_fstring(rdr) {
                        struct_property_type = Some(s_t);
                    }
                    let _ = skip_package_name(rdr);
                }
            } else if header_type_a == 2 {
                if property_type == "EnumProperty" {
                    if let Ok(e_name) = read_fstring(rdr) {
                        enum_name = Some(e_name);
                    }
                    let _ = skip_package_name(rdr);
                    let _ = read_fstring(rdr);
                    let _ = rdr.read_u32::<LittleEndian>();
                } else if property_type == "MapProperty" {
                    let _ = read_fstring(rdr);
                    if let Ok(has_key) = rdr.read_u32::<LittleEndian>() {
                        if has_key != 0 {
                            let _ = read_fstring(rdr);
                            let _ = skip_package_name(rdr);
                        }
                    }
                    let _ = read_fstring(rdr);
                    if let Ok(has_val) = rdr.read_u32::<LittleEndian>() {
                        if has_val != 0 {
                            let _ = read_fstring(rdr);
                            let _ = skip_package_name(rdr);
                        }
                    }
                }
            }
        }

        let property_size = match rdr.read_u32::<LittleEndian>() {
            Ok(v) => v as u64,
            Err(_) => break,
        };

        if !property_header_flag {
            let _ = rdr.read_u32::<LittleEndian>();
        }

        // Parse property content and obtain exact prop_end
        let (val, prop_end) = match parse_single_property_value(
            rdr,
            &property_type,
            struct_property_type.as_deref(),
            array_type.as_deref(),
            structure_sub_type.as_deref(),
            enum_name.as_deref(),
            property_size,
            property_header_flag,
        ) {
            Ok(res) => res,
            Err(_) => break,
        };

        properties.insert(property_name, val);

        // Always ensure we advance cleanly to the end of the property
        rdr.set_position(prop_end);
    }

    properties
}

/// Parses the payload value for a single property and returns (PropertyValue, prop_end).
fn parse_single_property_value(
    rdr: &mut Cursor<&[u8]>,
    property_type: &str,
    struct_property_type: Option<&str>,
    array_type: Option<&str>,
    structure_sub_type: Option<&str>,
    enum_name: Option<&str>,
    property_size: u64,
    property_header_flag: bool,
) -> Result<(PropertyValue, u64)> {
    match property_type {
        "BoolProperty" => {
            let b = rdr.read_u8()? != 0;
            if !property_header_flag {
                let _ = rdr.read_u8()?;
            }
            let prop_start = rdr.position();
            let prop_end = prop_start + property_size;
            Ok((PropertyValue::Bool(b), prop_end))
        }
        "ByteProperty" => {
            let mut e_name = enum_name.map(|s| s.to_string());
            if !property_header_flag {
                e_name = Some(read_fstring(rdr)?);
            }
            let _ = rdr.read_u8()?; // no guid
            let prop_start = rdr.position();
            let prop_end = prop_start + property_size;
            let val = match e_name.as_deref() {
                None | Some("None") => {
                    let b = rdr.read_u8()?;
                    PropertyValue::UInt8(b)
                }
                Some(name) => {
                    let s = read_fstring(rdr)?;
                    PropertyValue::ByteEnum {
                        enum_name: name.to_string(),
                        value: s,
                    }
                }
            };
            Ok((val, prop_end))
        }
        "StructProperty" => {
            let mut s_type = struct_property_type.map(|s| s.to_string());
            if !property_header_flag {
                s_type = Some(read_fstring(rdr)?);
                let _ = rdr.read_u64::<LittleEndian>()?;
                let _ = rdr.read_u64::<LittleEndian>()?;
            }

            let struct_idx1 = rdr.read_u8()?;
            if struct_idx1 == 9 {
                let _ = rdr.read_u32::<LittleEndian>()?;
            }

            let prop_start = rdr.position();
            let prop_end = prop_start + property_size;
            let st_name = s_type.as_deref().unwrap_or("");

            // CRITICAL MOD TRAIT: Handle PWGenerationParams to extract world size
            if st_name == "PWGenerationParams" && property_size < 100_000 {
                let mut blob = vec![0u8; property_size as usize];
                if rdr.read_exact(&mut blob).is_ok() {
                    let needle = b"WorldSizeMeters";
                    let mut world_size = None;
                    if let Some(pos) = blob.windows(needle.len()).position(|w| w == needle) {
                        let slice = &blob[pos..pos.saturating_add(64).min(blob.len())];
                        if let Some(dp_pos) = slice.windows(b"DoubleProperty".len()).position(|w| w == b"DoubleProperty") {
                            let val_offset = pos + dp_pos + 15 + 4 + 4 + 1;
                            if val_offset + 8 <= blob.len() {
                                let val = f64::from_le_bytes(blob[val_offset..val_offset + 8].try_into().unwrap());
                                if val > 0.0 && val < 100_000.0 {
                                    world_size = Some(val);
                                }
                            }
                        }
                    }

                    let mut props = HashMap::new();
                    if let Some(ws) = world_size {
                        props.insert("WorldSizeMeters".to_string(), PropertyValue::Double(ws));
                    }
                    return Ok((
                        PropertyValue::Struct(StructValue::Properties(props)),
                        prop_end,
                    ));
                }
            }

            // Safely skip massive PW structs (like PWEmbeddedWorld) as raw blobs
            if st_name.starts_with("PW") {
                return Ok((
                    PropertyValue::ModRawBlob {
                        struct_type: st_name.to_string(),
                        byte_len: property_size as usize,
                    },
                    prop_end,
                ));
            }

            let val = match st_name {
                "LinearColor" => {
                    let r = rdr.read_f32::<LittleEndian>()?;
                    let g = rdr.read_f32::<LittleEndian>()?;
                    let b = rdr.read_f32::<LittleEndian>()?;
                    let a = rdr.read_f32::<LittleEndian>()?;
                    PropertyValue::Struct(StructValue::LinearColor { r, g, b, a })
                }
                "Vector" => {
                    let x = rdr.read_f64::<LittleEndian>()?;
                    let y = rdr.read_f64::<LittleEndian>()?;
                    let z = rdr.read_f64::<LittleEndian>()?;
                    PropertyValue::Struct(StructValue::Vector { x, y, z })
                }
                "Vector2D" => {
                    let x = rdr.read_f64::<LittleEndian>()?;
                    let y = rdr.read_f64::<LittleEndian>()?;
                    PropertyValue::Struct(StructValue::Vector2D { x, y })
                }
                "Quat" => {
                    let x = rdr.read_f64::<LittleEndian>()?;
                    let y = rdr.read_f64::<LittleEndian>()?;
                    let z = rdr.read_f64::<LittleEndian>()?;
                    let w = rdr.read_f64::<LittleEndian>()?;
                    PropertyValue::Struct(StructValue::Quat { x, y, z, w })
                }
                "Box" => {
                    let minx = rdr.read_f64::<LittleEndian>()?;
                    let miny = rdr.read_f64::<LittleEndian>()?;
                    let minz = rdr.read_f64::<LittleEndian>()?;
                    let maxx = rdr.read_f64::<LittleEndian>()?;
                    let maxy = rdr.read_f64::<LittleEndian>()?;
                    let maxz = rdr.read_f64::<LittleEndian>()?;
                    let flag = rdr.read_u8()? != 0;
                    PropertyValue::Struct(StructValue::Box {
                        min: [minx, miny, minz],
                        max: [maxx, maxy, maxz],
                        flag,
                    })
                }
                "DateTime" => {
                    let v = rdr.read_i64::<LittleEndian>()?;
                    PropertyValue::Struct(StructValue::DateTime(v))
                }
                "FluidBox" => {
                    let v = rdr.read_f32::<LittleEndian>()?;
                    PropertyValue::Struct(StructValue::FluidBox(v))
                }
                "InventoryItem" => {
                    let _ = rdr.read_u32::<LittleEndian>()?;
                    let item_name = read_fstring(rdr)?;
                    PropertyValue::Struct(StructValue::InventoryItem { item_name })
                }
                _ => PropertyValue::Struct(StructValue::ModRawBlob {
                    struct_type: st_name.to_string(),
                    byte_len: property_size as usize,
                }),
            };
            Ok((val, prop_end))
        }
        "ArrayProperty" => {
            let _zero_or_eight = rdr.read_u8()?;
            let prop_start = rdr.position();
            let prop_end = prop_start + property_size;

            if let Some(sub) = structure_sub_type {
                if sub.starts_with("PW") {
                    return Ok((
                        PropertyValue::ModRawBlob {
                            struct_type: sub.to_string(),
                            byte_len: property_size as usize,
                        },
                        prop_end,
                    ));
                }
            }

            let elem_count = rdr.read_u32::<LittleEndian>()? as usize;
            let val = match array_type {
                Some("ObjectProperty") | Some("InterfaceProperty") => {
                    let mut items = Vec::with_capacity(elem_count);
                    for _ in 0..elem_count {
                        if rdr.position() >= prop_end {
                            break;
                        }
                        items.push(PropertyValue::ObjectRef(read_object_reference(rdr)?));
                    }
                    PropertyValue::Array(items)
                }
                Some("StrProperty") => {
                    let mut items = Vec::with_capacity(elem_count);
                    for _ in 0..elem_count {
                        if rdr.position() >= prop_end {
                            break;
                        }
                        items.push(PropertyValue::Str(read_fstring(rdr)?));
                    }
                    PropertyValue::Array(items)
                }
                Some("IntProperty") => {
                    let mut items = Vec::with_capacity(elem_count);
                    for _ in 0..elem_count {
                        if rdr.position() >= prop_end {
                            break;
                        }
                        items.push(PropertyValue::Int(rdr.read_i32::<LittleEndian>()?));
                    }
                    PropertyValue::Array(items)
                }
                _ => PropertyValue::RawBytes(vec![]),
            };
            Ok((val, prop_end))
        }
        "SetProperty" => {
            if !property_header_flag {
                let _ = read_fstring(rdr)?;
            }
            let _ = rdr.read_u8()?;
            let prop_start = rdr.position();
            let prop_end = prop_start + property_size;
            Ok((PropertyValue::RawBytes(vec![]), prop_end))
        }
        "MapProperty" => {
            if !property_header_flag {
                let _ = read_fstring(rdr)?;
                let _ = read_fstring(rdr)?;
            }
            let _ = rdr.read_u8()?;
            let prop_start = rdr.position();
            let prop_end = prop_start + property_size;
            Ok((PropertyValue::RawBytes(vec![]), prop_end))
        }
        "Int8Property" => {
            let _ = rdr.read_u8()?;
            let prop_start = rdr.position();
            let prop_end = prop_start + property_size;
            Ok((PropertyValue::Int8(rdr.read_i8()?), prop_end))
        }
        "IntProperty" => {
            let _ = rdr.read_u8()?;
            let prop_start = rdr.position();
            let prop_end = prop_start + property_size;
            Ok((PropertyValue::Int(rdr.read_i32::<LittleEndian>()?), prop_end))
        }
        "UInt32Property" => {
            let _ = rdr.read_u8()?;
            let prop_start = rdr.position();
            let prop_end = prop_start + property_size;
            Ok((PropertyValue::UInt32(rdr.read_u32::<LittleEndian>()?), prop_end))
        }
        "Int64Property" => {
            let _ = rdr.read_u8()?;
            let prop_start = rdr.position();
            let prop_end = prop_start + property_size;
            Ok((PropertyValue::Int64(rdr.read_i64::<LittleEndian>()?), prop_end))
        }
        "UInt64Property" => {
            let _ = rdr.read_u8()?;
            let prop_start = rdr.position();
            let prop_end = prop_start + property_size;
            Ok((PropertyValue::UInt64(rdr.read_u64::<LittleEndian>()?), prop_end))
        }
        "FloatProperty" => {
            let _ = rdr.read_u8()?;
            let prop_start = rdr.position();
            let prop_end = prop_start + property_size;
            Ok((PropertyValue::Float(rdr.read_f32::<LittleEndian>()?), prop_end))
        }
        "DoubleProperty" => {
            let _ = rdr.read_u8()?;
            let prop_start = rdr.position();
            let prop_end = prop_start + property_size;
            Ok((PropertyValue::Double(rdr.read_f64::<LittleEndian>()?), prop_end))
        }
        "StrProperty" | "NameProperty" => {
            let _ = rdr.read_u8()?;
            let prop_start = rdr.position();
            let prop_end = prop_start + property_size;
            Ok((PropertyValue::Str(read_fstring(rdr)?), prop_end))
        }
        "ObjectProperty" | "InterfaceProperty" => {
            let _ = rdr.read_u8()?;
            let prop_start = rdr.position();
            let prop_end = prop_start + property_size;
            Ok((PropertyValue::ObjectRef(read_object_reference(rdr)?), prop_end))
        }
        "EnumProperty" => {
            let mut e_name = enum_name.map(|s| s.to_string());
            if !property_header_flag {
                e_name = Some(read_fstring(rdr)?);
            }
            let _ = rdr.read_u8()?;
            let prop_start = rdr.position();
            let prop_end = prop_start + property_size;
            let val = read_fstring(rdr)?;
            Ok((
                PropertyValue::ByteEnum {
                    enum_name: e_name.unwrap_or_default(),
                    value: val,
                },
                prop_end,
            ))
        }
        "TextProperty" => {
            let _ = rdr.read_u8()?;
            let prop_start = rdr.position();
            let prop_end = prop_start + property_size;
            Ok((PropertyValue::RawBytes(vec![]), prop_end))
        }
        _ => {
            let _ = rdr.read_u8();
            let prop_start = rdr.position();
            let prop_end = prop_start + property_size;
            Ok((PropertyValue::RawBytes(vec![]), prop_end))
        }
    }
}

/// Extracts the RWG Actors summary required by Milestone 1.
pub fn extract_rwg_summary(parsed_save: &ParsedSaveGame) -> RwgActorsSummary {
    let mut total_actor_headers = 0;
    let mut total_objects_parsed = 0;
    let mut player = None;
    let mut resource_nodes = Vec::new();
    let mut resource_deposits = Vec::new();
    let mut drop_pods = Vec::new();
    let mut player_structures = Vec::new();
    let mut somersloops = Vec::new();
    let mut mercer_spheres = Vec::new();
    let mut world_seed = None;
    let mut world_size_meters = None;
    let mut extracted_sfplus_nodes = std::collections::HashSet::new();

    for level in &parsed_save.levels {
        for header in &level.headers {
            if let LevelObjectHeader::Actor(_) = header {
                total_actor_headers += 1;
            }
        }
        total_objects_parsed += level.actors.len();

        for actor in &level.actors {
            let path = &actor.type_path;

            // Extract world seed and parameters from RandomWorldGenerator actor
            if path.contains("PWWorldGenerator") {
                if let Some(PropertyValue::Int(seed)) = actor.properties.get("WorldSeed") {
                    world_seed = Some(*seed);
                }
                if let Some(PropertyValue::Struct(StructValue::Properties(p))) = actor.properties.get("GenerationParams") {
                    if let Some(PropertyValue::Double(size)) = p.get("WorldSizeMeters") {
                        world_size_meters = Some(*size);
                    }
                }
            }

            // Player position
            if path.contains("Char_Player") {
                player = Some(PlayerSummary {
                    instance_name: actor.instance_name.clone(),
                    transform: actor.transform,
                    translation_offset: actor.translation_offset,
                });
            }

            // Somersloops (Artifact WAT1)
            if path.contains("BP_WAT1") {
                somersloops.push(ArtifactSummary {
                    instance_name: actor.instance_name.clone(),
                    type_path: actor.type_path.clone(),
                    transform: actor.transform,
                    artifact_type: "Somersloop".to_string(),
                });
            } else if path.contains("BP_WAT2") {
                // Mercer Spheres (Artifact WAT2)
                mercer_spheres.push(ArtifactSummary {
                    instance_name: actor.instance_name.clone(),
                    type_path: actor.type_path.clone(),
                    transform: actor.transform,
                    artifact_type: "MercerSphere".to_string(),
                });
            } else if path.contains("BP_ResourceNodeGeyser") {
                // Geysers
                let purity = actor
                    .properties
                    .get("mPurityOverride")
                    .or_else(|| actor.properties.get("mPurity"))
                    .and_then(|v| match v {
                        PropertyValue::ByteEnum { value, .. } => Some(clean_purity(value)),
                        PropertyValue::Str(s) => Some(clean_purity(s)),
                        _ => None,
                    });

                let res_class = actor
                    .properties
                    .get("mResourceClassOverride")
                    .or_else(|| actor.properties.get("mResourceClass"))
                    .and_then(|v| {
                        if let PropertyValue::ObjectRef(r) = v {
                            Some(r.path_name.clone())
                        } else {
                            None
                        }
                    })
                    .or_else(|| Some("/Game/FactoryGame/Resource/RawResources/Geyser/Desc_Geyser.Desc_Geyser_C".to_string()));

                let res_name = res_class.as_deref().map(extract_clean_name).or(Some("Geyser".to_string()));
                let mod_source = detect_mod_source(path, res_class.as_deref());

                resource_nodes.push(ResourceNodeSummary {
                    instance_name: actor.instance_name.clone(),
                    transform: actor.transform,
                    purity,
                    resource_class: res_class,
                    resource_name: res_name,
                    node_type: Some("Geyser".to_string()),
                    mod_source: Some(mod_source),
                    translation_offset: actor.translation_offset,
                });
            } else if path.contains("BP_FrackingCore") || path.contains("BP_FrackingSatellite") {
                // Resource Wells / Fracking Satellites
                let is_core = path.contains("BP_FrackingCore");
                let purity = actor
                    .properties
                    .get("mPurityOverride")
                    .or_else(|| actor.properties.get("mPurity"))
                    .and_then(|v| match v {
                        PropertyValue::ByteEnum { value, .. } => Some(clean_purity(value)),
                        PropertyValue::Str(s) => Some(clean_purity(s)),
                        _ => None,
                    });

                let res_class = actor
                    .properties
                    .get("mResourceClassOverride")
                    .or_else(|| actor.properties.get("mResourceClass"))
                    .and_then(|v| {
                        if let PropertyValue::ObjectRef(r) = v {
                            Some(r.path_name.clone())
                        } else {
                            None
                        }
                    });

                let res_name = res_class.as_deref().map(extract_clean_name);
                let mod_source = detect_mod_source(path, res_class.as_deref());

                resource_nodes.push(ResourceNodeSummary {
                    instance_name: actor.instance_name.clone(),
                    transform: actor.transform,
                    purity,
                    resource_class: res_class,
                    resource_name: res_name,
                    node_type: Some(if is_core { "ResourceWellCore".to_string() } else { "ResourceWellSatellite".to_string() }),
                    mod_source: Some(mod_source),
                    translation_offset: actor.translation_offset,
                });
            } else if (path.contains("ResourceNode") && !path.contains("Subsystem"))
                || path.contains("Node_BioWater")
                || path.contains("WaterTurbineNode")
                || path.contains("WaterDam")
            {
                // Standard and modded resource nodes (Refined Power Thorium/Water Turbine, Satisfactory Plus ores/BioWater, etc.)
                let purity = actor
                    .properties
                    .get("mPurityOverride")
                    .or_else(|| actor.properties.get("mPurity"))
                    .and_then(|v| match v {
                        PropertyValue::ByteEnum { value, .. } => Some(clean_purity(value)),
                        PropertyValue::Str(s) => Some(clean_purity(s)),
                        _ => None,
                    });

                let res_class = actor
                    .properties
                    .get("mResourceClassOverride")
                    .or_else(|| actor.properties.get("mResourceClass"))
                    .and_then(|v| {
                        if let PropertyValue::ObjectRef(r) = v {
                            Some(r.path_name.clone())
                        } else {
                            None
                        }
                    })
                    .or_else(|| {
                        if path.contains("WaterTurbineNode") {
                            Some("/RefinedPower/World/ResourceNodes/WaterTurbine/Desc_WaterTurbineNode.Desc_WaterTurbineNode_C".to_string())
                        } else if path.contains("Node_BioWater") {
                            Some("/KLib/Assets/Ressources/Raw/Ores/Fluids/BioWater/Res_LiquidBioWater.Res_LiquidBioWater_C".to_string())
                        } else {
                            None
                        }
                    });

                let res_name = res_class
                    .as_deref()
                    .map(extract_clean_name)
                    .or_else(|| Some(extract_clean_name(path)));
                let mod_source = detect_mod_source(path, res_class.as_deref());

                resource_nodes.push(ResourceNodeSummary {
                    instance_name: actor.instance_name.clone(),
                    transform: actor.transform,
                    purity,
                    resource_class: res_class,
                    resource_name: res_name,
                    node_type: Some("Standard".to_string()),
                    mod_source: Some(mod_source),
                    translation_offset: actor.translation_offset,
                });
            } else if path.contains("BP_ResourceDeposit") {
                let res_class = actor
                    .properties
                    .get("mResourceClassOverride")
                    .or_else(|| actor.properties.get("mResourceClass"))
                    .and_then(|v| {
                        if let PropertyValue::ObjectRef(r) = v {
                            Some(r.path_name.clone())
                        } else {
                            None
                        }
                    });

                resource_deposits.push(ResourceDepositSummary {
                    instance_name: actor.instance_name.clone(),
                    transform: actor.transform,
                    resource_class: res_class,
                });
            } else if path.contains("BP_DropPod") {
                let consumption = actor.properties.get("mTargetConsumption").and_then(|v| {
                    if let PropertyValue::Float(f) = v {
                        Some(*f)
                    } else {
                        None
                    }
                });

                drop_pods.push(DropPodSummary {
                    instance_name: actor.instance_name.clone(),
                    transform: actor.transform,
                    power_consumption: consumption,
                });
            } else if path.contains("Build_") {
                player_structures.push(PlayerStructureSummary {
                    instance_name: actor.instance_name.clone(),
                    type_path: actor.type_path.clone(),
                    transform: actor.transform,
                });

                // On standard maps, Satisfactory Plus spawns nodes in an unserialized sublevel (SatifactoryPlusWorld).
                // Recover any active SF+ nodes referenced by player extractors via `mExtractableResource`.
                if let Some(PropertyValue::ObjectRef(target_ref)) = actor.properties.get("mExtractableResource") {
                    let target_path = &target_ref.path_name;
                    if target_path.contains("SatifactoryPlusWorld")
                        && extracted_sfplus_nodes.insert(target_path.clone())
                    {
                        let (res_class, res_name, node_type) = if target_path.contains("Node_BioWater") {
                            (
                                Some("/KLib/Assets/Ressources/Raw/Ores/Fluids/BioWater/Res_LiquidBioWater.Res_LiquidBioWater_C".to_string()),
                                Some("Bio Water (Algae)".to_string()),
                                "Standard",
                            )
                        } else if target_path.contains("QuantumCrystal") || target_path.contains("KerrCrystal") {
                            (
                                Some("/KLib/Assets/Ressources/Raw/Nodes/QuantumCystal/Res_KerrCrystal.Res_KerrCrystal_C".to_string()),
                                Some("Kerr Crystal".to_string()),
                                "Standard",
                            )
                        } else if target_path.contains("FrackingCore") {
                            (
                                Some("/KLib/Assets/Ressources/Raw/Gas/Desc_Gas_Chlor.Desc_Gas_Chlor_C".to_string()),
                                Some("Chlorine Gas".to_string()),
                                "ResourceWellCore",
                            )
                        } else if target_path.contains("FrackingSat") {
                            (
                                Some("/KLib/Assets/Ressources/Raw/Gas/Desc_Gas_Chlor.Desc_Gas_Chlor_C".to_string()),
                                Some("Chlorine Gas".to_string()),
                                "ResourceWellSatellite",
                            )
                        } else {
                            (
                                None,
                                Some(extract_clean_name(target_path)),
                                "Standard",
                            )
                        };

                        resource_nodes.push(ResourceNodeSummary {
                            instance_name: target_path.clone(),
                            transform: actor.transform,
                            purity: Some("Normal".to_string()),
                            resource_class: res_class,
                            resource_name: res_name,
                            node_type: Some(node_type.to_string()),
                            mod_source: Some("SatisfactoryPlus".to_string()),
                            translation_offset: actor.translation_offset,
                        });
                    }
                }
            }
        }
    }

    // On standard maps (Persistent_Level) or vanilla saves, many fixed resource nodes,
    // fracking wells, and geysers are static map level actors whose properties are not written
    // to the save file until interacted with / mined.
    // Fill in any missing resource_name, resource_class, and purity from the standard map template.
    if parsed_save.header.map_name == "Persistent_Level" || world_seed.is_none() {
        use crate::vanilla_nodes_data::VANILLA_NODES_TABLE;
        use std::collections::HashMap;

        let template_map: HashMap<&str, (&str, &str, Option<&str>, &str, &str)> = VANILLA_NODES_TABLE
            .iter()
            .map(|&(name, rname, rclass, purity, ntype, mod_src)| (name, (rname, rclass, purity, ntype, mod_src)))
            .collect();

        for node in &mut resource_nodes {
            let short_name = node
                .instance_name
                .rsplit('.')
                .next()
                .unwrap_or(&node.instance_name);

            if let Some(&(tpl_rname, tpl_rclass, tpl_purity, tpl_ntype, tpl_mod)) = template_map.get(short_name) {
                // If resource name is missing or generic "BP_ResourceNode", backfill from template
                if node.resource_name.is_none()
                    || node.resource_name.as_deref() == Some("BP_ResourceNode")
                    || node.resource_name.as_deref() == Some("Unknown")
                {
                    node.resource_name = Some(tpl_rname.to_string());
                    if node.resource_class.is_none() {
                        node.resource_class = Some(tpl_rclass.to_string());
                    }
                    if node.purity.is_none() {
                        node.purity = tpl_purity.map(|s| s.to_string());
                    }
                    if node.node_type.is_none() || node.node_type.as_deref() == Some("Standard") && tpl_ntype != "Standard" {
                        node.node_type = Some(tpl_ntype.to_string());
                    }
                    if node.mod_source.is_none() || node.mod_source.as_deref() == Some("Vanilla") && tpl_mod != "Vanilla" {
                        node.mod_source = Some(tpl_mod.to_string());
                    }
                }
            }
        }
    }

    RwgActorsSummary {
        save_metadata: parsed_save.header.clone(),
        total_actor_headers,
        total_objects_parsed,
        world_seed,
        world_size_meters,
        player,
        resource_nodes,
        resource_deposits,
        drop_pods,
        player_structures,
        somersloops,
        mercer_spheres,
        generated_map_png: None,
        generated_heightmap: None,
    }
}

/// Helper to standardize purity strings.
pub fn clean_purity(val: &str) -> String {
    match val {
        "RP_Inpure" | "FF_Inpure" | "Inpure" | "Impure" => "Impure".to_string(),
        "RP_Normal" | "FF_Normal" | "Normal" => "Normal".to_string(),
        "RP_Pure" | "FF_Pure" | "Pure" => "Pure".to_string(),
        other => {
            let clean = other.strip_prefix("RP_").unwrap_or(other);
            let clean = clean.strip_prefix("FF_").unwrap_or(clean);
            clean.to_string()
        }
    }
}

/// Detects whether an actor or resource comes from Refined Power, Satisfactory Plus, Ficsit Farming, or Vanilla.
pub fn detect_mod_source(actor_path: &str, res_class: Option<&str>) -> String {
    let combined = format!("{} {}", actor_path, res_class.unwrap_or(""));
    let lower = combined.to_lowercase();

    if lower.contains("ficsitfarming")
        || lower.contains("ff_")
        || lower.contains("farming")
        || lower.contains("loam")
        || lower.contains("peat")
        || lower.contains("dirt")
    {
        "FicsitFarming".to_string()
    } else if lower.contains("refinedpower")
        || lower.contains("rp_")
        || lower.contains("refinedrd")
        || lower.contains("deanium")
    {
        "RefinedPower".to_string()
    } else if lower.contains("satisfactoryplus")
        || lower.contains("sfplus")
        || lower.contains("/kits/")
        || lower.contains("/klib/")
        || lower.contains("klib")
        || lower.contains("kerrcrystal")
        || lower.contains("biowater")
        || lower.contains("chlor")
        || lower.contains("algal")
        || lower.contains("algae")
        || lower.contains("algie")
    {
        "SatisfactoryPlus".to_string()
    } else {
        "Vanilla".to_string()
    }
}

/// Helper to get a human-friendly item/resource name from a path.
pub fn extract_clean_name(path: &str) -> String {
    let name = path.rsplit('.').next().unwrap_or(path);
    let name = name.strip_prefix("Desc_").unwrap_or(name);
    let name = name.strip_suffix("_C").unwrap_or(name);
    let name = name.strip_prefix("FF_").unwrap_or(name);
    let name = name.strip_prefix("RP_").unwrap_or(name);

    match name {
        "OreIron" | "Iron" => "Iron".to_string(),
        "OreCopper" | "Copper" => "Copper".to_string(),
        "OreBauxite" | "Bauxite" => "Bauxite".to_string(),
        "OreGold" | "Gold" => "Caterium".to_string(),
        "OreUranium" | "Uranium" => "Uranium".to_string(),
        "Stone" => "Limestone".to_string(),
        "Coal" => "Coal".to_string(),
        "RawQuartz" => "Raw Quartz".to_string(),
        "Sulfur" => "Sulfur".to_string(),
        "SAM" => "SAM".to_string(),
        "LiquidOil" | "CrudeOil" => "Crude Oil".to_string(),
        "Water" => "Water".to_string(),
        "NitrogenGas" => "Nitrogen Gas".to_string(),
        "Geyser" => "Geyser".to_string(),
        // Refined Power
        "Thorium" => "Thorium".to_string(),
        "Deanium" => "Deanium".to_string(),
        "Geothermal" => "Geothermal".to_string(),
        "HighYieldWater" => "High Yield Water".to_string(),
        "WaterTurbineNode" | "BP_WaterTurbineNode" | "WaterTurbine" => "Water Turbine Node".to_string(),
        // Ficsit Farming
        "Dirt_Fertilized" | "DirtFertilized" | "FertilizedDirt" => "Fertilized Dirt".to_string(),
        "Dirt_Wet" | "DirtWet" | "WetDirt" => "Wet Dirt".to_string(),
        "Loam" | "LoamSoil" | "OreLoam" => "Loam".to_string(),
        "Peat" | "PeatSoil" | "OrePeat" => "Peat".to_string(),
        "Dirt" | "DirtSoil" | "OreDirt" => "Dirt".to_string(),
        // Algal / Algae / Algie Mass (Satisfactory Plus & Ficsit Farming)
        "AlgalMass" | "AlgaeMass" | "AlgieMass" | "Algal" | "Algae" | "Algie" => "Algie Mass".to_string(),
        // Satisfactory Plus & KLib
        "Res_KerrCrystal" | "KerrCrystal" | "QuantumCrystal" => "Kerr Crystal".to_string(),
        "Gas_Chlor" | "GasChlor" | "Chlor" | "Chlorine" => "Chlorine Gas".to_string(),
        "Res_LiquidBioWater" | "LiquidBioWater" | "BioWater" | "Node_BioWaterSF+" => "Bio Water (Algae)".to_string(),
        "OreLead" | "Lead" => "Lead".to_string(),
        "OreZinc" | "Zinc" => "Zinc".to_string(),
        "OreSilver" | "Silver" => "Silver".to_string(),
        "OreTin" | "Tin" => "Tin".to_string(),
        "OreNickel" | "Nickel" => "Nickel".to_string(),
        "OrePlatinum" | "Platinum" => "Platinum".to_string(),
        "Salt" | "RockSalt" => "Rock Salt".to_string(),
        "Biomass" => "Biomass".to_string(),
        other => {
            let lower = other.to_lowercase();
            if lower.contains("fertil") {
                "Fertilized Dirt".to_string()
            } else if lower.contains("wet") && lower.contains("dirt") {
                "Wet Dirt".to_string()
            } else if lower.contains("loam") {
                "Loam".to_string()
            } else if lower.contains("peat") {
                "Peat".to_string()
            } else if lower.contains("dirt") {
                "Dirt".to_string()
            } else if lower.contains("alga") || lower.contains("algi") {
                "Algie Mass".to_string()
            } else if lower.contains("kerr") || lower.contains("quantumcrystal") {
                "Kerr Crystal".to_string()
            } else if lower.contains("chlor") {
                "Chlorine Gas".to_string()
            } else if lower.contains("biowater") {
                "Bio Water (Algae)".to_string()
            } else if lower.contains("waterturbine") {
                "Water Turbine Node".to_string()
            } else if lower.contains("deanium") {
                "Deanium".to_string()
            } else {
                let clean = other.strip_prefix("Ore").unwrap_or(other);
                clean.to_string()
            }
        }
    }
}
