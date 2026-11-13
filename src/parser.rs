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

/// Exact byte span of a header + body pair inside a level's uncompressed stream.
#[derive(Debug, Clone)]
pub struct LevelObjectSpan {
    pub instance_name: String,
    pub parent_actor_name: Option<String>,
    pub header_range: (usize, usize),
    pub body_range: (usize, usize),
    pub is_actor: bool,
}

/// Parsed level containing all its headers and parsed actors.
pub struct ParsedLevel {
    pub level_name: Option<String>,
    pub headers: Vec<LevelObjectHeader>,
    pub actors: Vec<ParsedActor>,
    pub inventory_components: Vec<ParsedInventoryComponent>,
    pub headers_size_offset: usize,
    pub header_count_offset: usize,
    pub headers_insert_offset: usize,
    pub objects_size_offset: usize,
    pub object_count_offset: usize,
    pub objects_insert_offset: usize,
    pub object_spans: Vec<LevelObjectSpan>,
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
    let mut header_ranges = Vec::with_capacity(actor_and_component_count);

    for _ in 0..actor_and_component_count {
        let h_start = rdr.position() as usize;
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
        let h_end = rdr.position() as usize;
        header_ranges.push((h_start, h_end));
    }
    let headers_insert_offset = rdr.position() as usize;

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
    let mut inventory_components = Vec::new();
    let mut object_spans = Vec::with_capacity(object_count);

    for idx in 0..object_count {
        let header = &object_headers[idx];
        let body_start = object_data_start as usize + obj_rdr.position() as usize;
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

        let mut lightweight_instances = Vec::new();
        if is_actor {
            if let LevelObjectHeader::Actor(actor_hdr) = header {
                if actor_hdr.type_path == "/Script/FactoryGame.FGLightweightBuildableSubsystem" {
                    lightweight_instances = parse_lightweight_buildables(
                        &mut obj_rdr,
                        payload_start + object_size,
                        object_data_start as usize,
                        (object_data_start + payload_start - 4) as usize,
                        (object_data_start - 8) as usize,
                    );
                }
                let fog_raw_data_range = if actor_hdr.type_path.contains("FGMapManager")
                    || properties.contains_key("mFogOfWarRawData")
                {
                    locate_fog_raw_data_range(
                        object_slice,
                        payload_start,
                        object_size,
                        object_ue5_version,
                        object_data_start as usize,
                    )
                } else {
                    None
                };

                parsed_actors.push(ParsedActor {
                    type_path: actor_hdr.type_path.clone(),
                    root_object: actor_hdr.root_object.clone(),
                    instance_name: actor_hdr.instance_name.clone(),
                    flags: actor_hdr.flags,
                    transform: actor_hdr.transform,
                    was_placed_in_level: actor_hdr.was_placed_in_level,
                    properties,
                    translation_offset: actor_hdr.translation_offset,
                    lightweight_instances,
                    fog_raw_data_range,
                    payload_start_abs: (object_data_start + payload_start) as usize,
                    object_size: object_size as usize,
                    object_size_offset: (object_data_start + payload_start - 4) as usize,
                    all_objects_size_offset: (object_data_start - 8) as usize,
                    object_ue5_version,
                });
            }
        } else if let LevelObjectHeader::Component(comp_hdr) = header {
            if comp_hdr.class_name.contains("InventoryComponent")
                || properties.contains_key("mInventoryStacks")
            {
                if let Some((total_slots, stacks)) = extract_inventory_stacks(&properties) {
                    inventory_components.push(ParsedInventoryComponent {
                        instance_name: comp_hdr.instance_name.clone(),
                        parent_actor_name: comp_hdr.parent_actor_name.clone(),
                        total_slots,
                        stacks,
                        all_objects_size_offset: (object_data_start - 8) as usize,
                        object_size_offset: (object_data_start + payload_start - 4) as usize,
                        payload_start_abs: (object_data_start + payload_start) as usize,
                        object_size: object_size as usize,
                        object_ue5_version,
                    });
                }
            }
        }

        // Advance safely to jump_offset
        obj_rdr.set_position(jump_offset);
        let body_end = object_data_start as usize + jump_offset as usize;
        let (inst_name, parent_name) = match header {
            LevelObjectHeader::Actor(a) => (a.instance_name.clone(), None),
            LevelObjectHeader::Component(c) => (c.instance_name.clone(), Some(c.parent_actor_name.clone())),
        };
        object_spans.push(LevelObjectSpan {
            instance_name: inst_name,
            parent_actor_name: parent_name,
            header_range: header_ranges[idx],
            body_range: (body_start, body_end),
            is_actor,
        });
    }

    Ok(ParsedLevel {
        level_name,
        headers: object_headers,
        actors: parsed_actors,
        inventory_components,
        headers_size_offset: (header_start - 8) as usize,
        header_count_offset: header_start as usize,
        headers_insert_offset,
        objects_size_offset: (object_data_start - 8) as usize,
        object_count_offset: object_data_start as usize,
        objects_insert_offset: after_objects_pos as usize,
        object_spans,
    })
}

struct ParsedPropHeader {
    property_name: String,
    property_type: String,
    property_header_flag: bool,
    struct_property_type: Option<String>,
    array_type: Option<String>,
    structure_sub_type: Option<String>,
    enum_name: Option<String>,
    property_size_offset: usize,
    property_size: u64,
}

fn read_next_property_header(
    rdr: &mut Cursor<&[u8]>,
    object_ue5_version: i32,
    max_payload_offset: u64,
) -> Option<ParsedPropHeader> {
    if rdr.position() >= max_payload_offset {
        return None;
    }

    let property_name = read_fstring(rdr).ok()?;
    if property_name == "None" || property_name.is_empty() {
        return None;
    }

    let property_type = read_fstring(rdr).ok()?;
    let property_header_flag = object_ue5_version >= 1012;
    let mut struct_property_type = None;
    let mut array_type = None;
    let mut structure_sub_type = None;
    let mut enum_name = None;

    if property_header_flag {
        let header_type_a = rdr.read_u32::<LittleEndian>().ok()?;
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

    let property_size_offset = rdr.position() as usize;
    let property_size = rdr.read_u32::<LittleEndian>().ok()? as u64;
    if !property_header_flag {
        let _ = rdr.read_u32::<LittleEndian>();
    }

    Some(ParsedPropHeader {
        property_name,
        property_type,
        property_header_flag,
        struct_property_type,
        array_type,
        structure_sub_type,
        enum_name,
        property_size_offset,
        property_size,
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

    while let Some(hdr) = read_next_property_header(rdr, object_ue5_version, max_payload_offset) {
        // Parse property content and obtain exact prop_end
        let (val, prop_end) = match parse_single_property_value(
            rdr,
            &hdr.property_type,
            hdr.struct_property_type.as_deref(),
            hdr.array_type.as_deref(),
            hdr.structure_sub_type.as_deref(),
            hdr.enum_name.as_deref(),
            hdr.property_size,
            hdr.property_header_flag,
        ) {
            Ok(res) => res,
            Err(_) => break,
        };

        properties.insert(hdr.property_name, val);

        // Always ensure we advance cleanly to the end of the property
        rdr.set_position(prop_end);
    }

    properties
}

#[derive(Debug, Clone)]
pub(crate) struct InventorySlotByteSpan {
    pub item_prop_size_offset: usize,
    pub item_fstring_start: usize,
    pub item_fstring_end: usize,
    pub num_items_val_offset: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct InventoryStacksByteLayout {
    pub stacks_prop_size_offset: usize,
    pub slots: Vec<InventorySlotByteSpan>,
}

/// Locates exact byte offsets for each `InventoryStack` slot in `FGInventoryComponent::mInventoryStacks`.
pub(crate) fn locate_m_inventory_stacks(
    uncompressed: &[u8],
    comp: &ParsedInventoryComponent,
) -> Result<InventoryStacksByteLayout> {
    let mut rdr = Cursor::new(uncompressed);
    rdr.set_position(comp.payload_start_abs as u64);
    let max_offset = (comp.payload_start_abs + comp.object_size) as u64;

    if comp.object_ue5_version >= 1011 {
        let _ = rdr.read_u8()?;
    }

    while let Some(hdr) = read_next_property_header(&mut rdr, comp.object_ue5_version, max_offset) {
        if hdr.property_name == "mInventoryStacks" && hdr.property_type == "ArrayProperty" {
            let _zero_or_eight = rdr.read_u8()?;
            let prop_start = rdr.position();
            let prop_end = prop_start + hdr.property_size;
            let elem_count = rdr.read_u32::<LittleEndian>()? as usize;
            let sub_ver = if hdr.property_header_flag { 1012 } else { 1000 };

            let mut slots = Vec::with_capacity(elem_count);
            for _ in 0..elem_count {
                if rdr.position() >= prop_end {
                    break;
                }
                let mut item_prop_size_offset = 0usize;
                let mut item_fstring_start = 0usize;
                let mut item_fstring_end = 0usize;
                let mut num_items_val_offset = 0usize;

                while let Some(sh) = read_next_property_header(&mut rdr, sub_ver, prop_end) {
                    if sh.property_name == "Item" && sh.property_type == "StructProperty" {
                        if !sh.property_header_flag {
                            let _ = read_fstring(&mut rdr);
                            let _ = rdr.read_u64::<LittleEndian>();
                            let _ = rdr.read_u64::<LittleEndian>();
                        }
                        let struct_idx1 = rdr.read_u8()?;
                        if struct_idx1 == 9 {
                            let _ = rdr.read_u32::<LittleEndian>()?;
                        }
                        item_prop_size_offset = sh.property_size_offset;
                        let s_start = rdr.position();
                        let _ = rdr.read_u32::<LittleEndian>()?;
                        item_fstring_start = rdr.position() as usize;
                        let _ = read_fstring(&mut rdr)?;
                        item_fstring_end = rdr.position() as usize;
                        rdr.set_position(s_start + sh.property_size);
                    } else if sh.property_name == "NumItems" && sh.property_type == "IntProperty" {
                        let _ = rdr.read_u8()?;
                        num_items_val_offset = rdr.position() as usize;
                        let _ = rdr.read_i32::<LittleEndian>()?;
                    } else {
                        let (_, s_end) = parse_single_property_value(
                            &mut rdr,
                            &sh.property_type,
                            sh.struct_property_type.as_deref(),
                            sh.array_type.as_deref(),
                            sh.structure_sub_type.as_deref(),
                            sh.enum_name.as_deref(),
                            sh.property_size,
                            sh.property_header_flag,
                        )?;
                        rdr.set_position(s_end);
                    }
                }

                if item_fstring_start > 0 && num_items_val_offset > 0 {
                    slots.push(InventorySlotByteSpan {
                        item_prop_size_offset,
                        item_fstring_start,
                        item_fstring_end,
                        num_items_val_offset,
                    });
                }
            }

            return Ok(InventoryStacksByteLayout {
                stacks_prop_size_offset: hdr.property_size_offset,
                slots,
            });
        }

        let (_, prop_end) = parse_single_property_value(
            &mut rdr,
            &hdr.property_type,
            hdr.struct_property_type.as_deref(),
            hdr.array_type.as_deref(),
            hdr.structure_sub_type.as_deref(),
            hdr.enum_name.as_deref(),
            hdr.property_size,
            hdr.property_header_flag,
        )?;
        rdr.set_position(prop_end);
    }

    bail!("mInventoryStacks not found on component '{}'", comp.instance_name)
}

fn locate_fog_raw_data_range(
    object_slice: &[u8],
    payload_start: u64,
    object_size: u64,
    object_ue5_version: i32,
    object_data_start: usize,
) -> Option<(usize, usize)> {
    let mut rdr = Cursor::new(object_slice);
    rdr.set_position(payload_start);
    let _ = read_object_reference(&mut rdr).ok()?;
    let comp_count = rdr.read_u32::<LittleEndian>().ok()?;
    for _ in 0..comp_count {
        let _ = read_object_reference(&mut rdr).ok()?;
    }
    if object_ue5_version >= 1011 {
        let _ = rdr.read_u8().ok()?;
    }
    let max_offset = payload_start + object_size;
    while let Some(hdr) = read_next_property_header(&mut rdr, object_ue5_version, max_offset) {
        if hdr.property_name == "mFogOfWarRawData" && hdr.property_type == "ArrayProperty" {
            let _zero_or_eight = rdr.read_u8().ok()?;
            let prop_start = rdr.position();
            let prop_end = prop_start + hdr.property_size;
            let elem_count = rdr.read_u32::<LittleEndian>().ok()? as usize;
            let avail = prop_end.saturating_sub(rdr.position()) as usize;
            let len = elem_count.min(avail);
            return Some((object_data_start + rdr.position() as usize, len));
        }
        let (_, prop_end) = parse_single_property_value(
            &mut rdr,
            &hdr.property_type,
            hdr.struct_property_type.as_deref(),
            hdr.array_type.as_deref(),
            hdr.structure_sub_type.as_deref(),
            hdr.enum_name.as_deref(),
            hdr.property_size,
            hdr.property_header_flag,
        )
        .ok()?;
        rdr.set_position(prop_end);
    }
    None
}

pub(crate) enum ScalarLocation {
    Found(&'static str, usize),
    InsertBeforeNone(usize),
}

/// Locates the exact byte offset of a scalar property (`BoolProperty`, `IntProperty`, `FloatProperty`)
/// inside an actor's uncompressed payload, or the offset of the terminating `"None"` property if omitted.
pub(crate) fn locate_actor_scalar_or_none(
    uncompressed: &[u8],
    actor: &ParsedActor,
    prop_name: &str,
) -> Result<ScalarLocation> {
    let mut rdr = Cursor::new(uncompressed);
    rdr.set_position(actor.payload_start_abs as u64);
    let _ = read_object_reference(&mut rdr)?;
    let comp_count = rdr.read_u32::<LittleEndian>()?;
    for _ in 0..comp_count {
        let _ = read_object_reference(&mut rdr)?;
    }
    if actor.object_ue5_version >= 1011 {
        let _ = rdr.read_u8()?;
    }
    let max_offset = (actor.payload_start_abs + actor.object_size) as u64;
    let mut none_offset = rdr.position() as usize;
    while let Some(hdr) = read_next_property_header(&mut rdr, actor.object_ue5_version, max_offset) {
        if hdr.property_name == prop_name {
            return match hdr.property_type.as_str() {
                "BoolProperty" => Ok(ScalarLocation::Found("bool", rdr.position() as usize)),
                "IntProperty" => {
                    let _ = rdr.read_u8()?;
                    Ok(ScalarLocation::Found("int", rdr.position() as usize))
                }
                "FloatProperty" => {
                    let _ = rdr.read_u8()?;
                    Ok(ScalarLocation::Found("float", rdr.position() as usize))
                }
                other => bail!("Unsupported property type '{}' for '{}'", other, prop_name),
            };
        }
        let (_, prop_end) = parse_single_property_value(
            &mut rdr,
            &hdr.property_type,
            hdr.struct_property_type.as_deref(),
            hdr.array_type.as_deref(),
            hdr.structure_sub_type.as_deref(),
            hdr.enum_name.as_deref(),
            hdr.property_size,
            hdr.property_header_flag,
        )?;
        rdr.set_position(prop_end);
        none_offset = prop_end as usize;
    }
    Ok(ScalarLocation::InsertBeforeNone(none_offset))
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
                Some("ByteProperty") => {
                    let available = prop_end.saturating_sub(rdr.position()) as usize;
                    let to_read = elem_count.min(available);
                    let mut bytes = vec![0u8; to_read];
                    if to_read > 0 {
                        let _ = std::io::Read::read_exact(rdr, &mut bytes);
                    }
                    PropertyValue::RawBytes(bytes)
                }
                Some("StructProperty")
                    if matches!(
                        structure_sub_type.as_deref(),
                        Some("SplinePointData") | Some("InventoryStack")
                    ) =>
                {
                    let mut items = Vec::with_capacity(elem_count.min(256));
                    let sub_ver = if property_header_flag { 1012 } else { 1000 };
                    for _ in 0..elem_count {
                        if rdr.position() >= prop_end {
                            break;
                        }
                        let props = parse_properties_tree(rdr, sub_ver, prop_end);
                        items.push(PropertyValue::Struct(StructValue::Properties(props)));
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
                let meta = crate::building_footprints::lookup_building_metadata(path);
                let spline_points = extract_spline_points(&actor.properties, &actor.transform);
                player_structures.push(PlayerStructureSummary {
                    instance_name: actor.instance_name.clone(),
                    type_path: actor.type_path.clone(),
                    transform: actor.transform,
                    display_name: meta.display_name,
                    category: meta.category,
                    mod_source: meta.mod_source,
                    footprint: [meta.width_m, meta.length_m],
                    spline_points,
                    is_lightweight: false,
                    translation_offset: actor.translation_offset,
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

            if !actor.lightweight_instances.is_empty() {
                for (lw_idx, inst) in actor.lightweight_instances.iter().enumerate() {
                    let meta = crate::building_footprints::lookup_building_metadata(&inst.type_path);
                    let short_cls = inst.type_path.rsplit('.').next().unwrap_or("LW");
                    player_structures.push(PlayerStructureSummary {
                        instance_name: format!("LW_{}_{}", short_cls, lw_idx),
                        type_path: inst.type_path.clone(),
                        transform: inst.transform,
                        display_name: meta.display_name,
                        category: meta.category,
                        mod_source: meta.mod_source,
                        footprint: [meta.width_m, meta.length_m],
                        spline_points: None,
                        is_lightweight: true,
                        translation_offset: inst.translation_offset,
                    });
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

    let (inventories, item_catalog) =
        build_inventory_summaries_and_catalog(parsed_save, &player_structures);

    let (has_fog_of_war, fog_revealed_pct, fog_of_war_png) = extract_fog_of_war_png(parsed_save);
    let game_settings = extract_save_game_settings(parsed_save);

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
        inventories,
        item_catalog,
        somersloops,
        mercer_spheres,
        generated_map_png: None,
        generated_heightmap: None,
        has_fog_of_war,
        fog_revealed_pct,
        fog_of_war_png,
        game_settings,
    }
}

fn classify_rule_actor_label(type_path: &str) -> Option<&'static str> {
    if type_path.contains("GameRules") {
        Some("GameRules")
    } else if type_path.contains("UnlockSubsystem") {
        Some("UnlockSubsystem")
    } else if type_path.contains("GameState") {
        Some("GameState")
    } else if type_path.contains("PlayerState") {
        Some("PlayerState")
    } else {
        None
    }
}

fn extract_save_game_settings(parsed_save: &ParsedSaveGame) -> SaveGameSettings {
    enum PresetDefault {
        Bool(bool),
        Int(i32),
        Float(f32),
    }

    const GAME_STATE_PRESETS: &[(&str, PresetDefault)] = &[
        ("mEnergyCostMultiplier", PresetDefault::Float(1.0)),
        ("mPartsCostMultiplier", PresetDefault::Float(1.0)),
        ("mSpacePartsCostMultiplier", PresetDefault::Float(1.0)),
        ("mIsCreativeModeEnabled", PresetDefault::Bool(false)),
        ("mCheatNoPower", PresetDefault::Bool(false)),
        ("mCheatNoFuel", PresetDefault::Bool(false)),
    ];

    const GAME_RULES_PRESETS: &[(&str, PresetDefault)] = &[
        ("mStartingTier", PresetDefault::Int(0)),
        ("mNoUnlockCost", PresetDefault::Bool(false)),
        ("mUnlockInstantAltRecipes", PresetDefault::Bool(false)),
        ("mUnlockAllMilestoneSchematics", PresetDefault::Bool(false)),
        ("mUnlockAllResearchSchematics", PresetDefault::Bool(false)),
        ("mUnlockAllResourceSinkSchematics", PresetDefault::Bool(false)),
    ];

    let mut variables = Vec::new();
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();

    for lvl in &parsed_save.levels {
        for actor in &lvl.actors {
            let Some(label) = classify_rule_actor_label(&actor.type_path) else {
                continue;
            };
            for (prop_name, prop_val) in &actor.properties {
                let var = match prop_val {
                    PropertyValue::Bool(b) => Some(GameRuleVariable {
                        actor_instance_name: actor.instance_name.clone(),
                        actor_label: label.to_string(),
                        property_name: prop_name.clone(),
                        value_type: "bool".to_string(),
                        bool_val: Some(*b),
                        int_val: None,
                        float_val: None,
                    }),
                    PropertyValue::Int(i) => Some(GameRuleVariable {
                        actor_instance_name: actor.instance_name.clone(),
                        actor_label: label.to_string(),
                        property_name: prop_name.clone(),
                        value_type: "int".to_string(),
                        bool_val: None,
                        int_val: Some(*i),
                        float_val: None,
                    }),
                    PropertyValue::Float(f) => Some(GameRuleVariable {
                        actor_instance_name: actor.instance_name.clone(),
                        actor_label: label.to_string(),
                        property_name: prop_name.clone(),
                        value_type: "float".to_string(),
                        bool_val: None,
                        int_val: None,
                        float_val: Some(*f),
                    }),
                    _ => None,
                };
                if let Some(v) = var {
                    seen.insert((actor.instance_name.clone(), prop_name.clone()));
                    variables.push(v);
                }
            }

            let presets: &[(&str, PresetDefault)] = match label {
                "GameState" => GAME_STATE_PRESETS,
                "GameRules" => GAME_RULES_PRESETS,
                _ => &[],
            };
            for (prop_name, default_val) in presets {
                let key = (actor.instance_name.clone(), (*prop_name).to_string());
                if seen.contains(&key) {
                    continue;
                }
                seen.insert(key);
                let var = match default_val {
                    PresetDefault::Bool(b) => GameRuleVariable {
                        actor_instance_name: actor.instance_name.clone(),
                        actor_label: label.to_string(),
                        property_name: (*prop_name).to_string(),
                        value_type: "bool".to_string(),
                        bool_val: Some(*b),
                        int_val: None,
                        float_val: None,
                    },
                    PresetDefault::Int(i) => GameRuleVariable {
                        actor_instance_name: actor.instance_name.clone(),
                        actor_label: label.to_string(),
                        property_name: (*prop_name).to_string(),
                        value_type: "int".to_string(),
                        bool_val: None,
                        int_val: Some(*i),
                        float_val: None,
                    },
                    PresetDefault::Float(f) => GameRuleVariable {
                        actor_instance_name: actor.instance_name.clone(),
                        actor_label: label.to_string(),
                        property_name: (*prop_name).to_string(),
                        value_type: "float".to_string(),
                        bool_val: None,
                        int_val: None,
                        float_val: Some(*f),
                    },
                };
                variables.push(var);
            }
        }
    }
    variables.sort_by(|a, b| {
        a.actor_label
            .cmp(&b.actor_label)
            .then_with(|| a.property_name.cmp(&b.property_name))
    });
    SaveGameSettings {
        is_creative_mode_enabled: parsed_save.header.is_creative_mode_enabled,
        variables,
    }
}

/// Encodes `FGMapManager::mFogOfWarRawData` (`512x512` BGRA8) into a transparent RGBA PNG overlay.
fn extract_fog_of_war_png(parsed_save: &ParsedSaveGame) -> (bool, f32, Option<Vec<u8>>) {
    for lvl in &parsed_save.levels {
        for actor in &lvl.actors {
            if let Some(PropertyValue::RawBytes(raw)) = actor.properties.get("mFogOfWarRawData") {
                if raw.len() < 4 || raw.len() % 4 != 0 {
                    continue;
                }
                let pixel_count = raw.len() / 4;
                let side = (pixel_count as f64).sqrt() as u32;
                if (side * side) as usize != pixel_count || side == 0 {
                    continue;
                }
                let mut rgba = vec![0u8; pixel_count * 4];
                let mut revealed = 0usize;
                for (idx, px) in raw.chunks_exact(4).enumerate() {
                    let fog_r = px[2];
                    if fog_r < 128 {
                        revealed += 1;
                    }
                    let alpha = ((fog_r as f32) * 0.78).round().clamp(0.0, 255.0) as u8;
                    let o = idx * 4;
                    rgba[o] = 8;
                    rgba[o + 1] = 12;
                    rgba[o + 2] = 22;
                    rgba[o + 3] = alpha;
                }
                let pct = (revealed as f32 / pixel_count as f32) * 100.0;
                let png_bytes = crate::map_gen::encode_rgba_png(&rgba, side, side).ok();
                return (true, pct, png_bytes);
            }
        }
    }
    (false, 100.0, None)
}

/// Extracts `(total_slots, Vec<(slot_index, item_path, count)>)` from `mInventoryStacks`.
fn extract_inventory_stacks(
    properties: &HashMap<String, PropertyValue>,
) -> Option<(usize, Vec<(usize, String, i32)>)> {
    let PropertyValue::Array(items) = properties.get("mInventoryStacks")? else {
        return None;
    };
    let total_slots = items.len();
    let mut stacks = Vec::new();
    for (slot_idx, entry) in items.iter().enumerate() {
        let PropertyValue::Struct(StructValue::Properties(map)) = entry else {
            continue;
        };
        let Some(PropertyValue::Struct(StructValue::InventoryItem { item_name })) = map.get("Item")
        else {
            continue;
        };
        if item_name.is_empty() {
            continue;
        }
        let count = match map.get("NumItems") {
            Some(PropertyValue::Int(n)) => *n,
            _ => 1,
        };
        if count > 0 {
            stacks.push((slot_idx, item_name.clone(), count));
        }
    }
    Some((total_slots, stacks))
}

fn classify_inventory_owner(
    parent_actor_name: &str,
    actor_type_path: Option<&str>,
    struct_meta: Option<(&str, &str)>,
) -> (&'static str, String) {
    let tp = actor_type_path.unwrap_or("");
    if tp.contains("Char_Player")
        || tp.contains("FGPlayerState")
        || parent_actor_name.contains("Char_Player")
        || parent_actor_name.contains("PlayerState")
    {
        return ("Pioneer", "Pioneer Inventory".to_string());
    }
    if tp.contains("Storage")
        || tp.contains("Container")
        || tp.contains("Depot")
        || tp.contains("FluidTank")
        || parent_actor_name.contains("Storage")
    {
        let name = struct_meta
            .map(|(n, _)| n.to_string())
            .unwrap_or_else(|| "Storage Container".to_string());
        return ("Storage", name);
    }
    if tp.contains("WheeledVehicle")
        || tp.contains("Locomotive")
        || tp.contains("FreightWagon")
        || tp.contains("Drone")
        || tp.contains("Tractor")
        || tp.contains("Truck")
        || tp.contains("Explorer")
    {
        let name = struct_meta
            .map(|(n, _)| n.to_string())
            .unwrap_or_else(|| "Vehicle".to_string());
        return ("Vehicle", name);
    }
    if let Some((display_name, _cat)) = struct_meta {
        return ("Machine", display_name.to_string());
    }
    if tp.contains("Build_") || parent_actor_name.contains("Build_") {
        let raw = tp
            .rsplit('.')
            .next()
            .unwrap_or(parent_actor_name.rsplit('.').next().unwrap_or("Machine"));
        return ("Machine", raw.to_string());
    }
    ("Other", "Other".to_string())
}

fn classify_slot_role(component_instance_name: &str) -> &'static str {
    let suffix = component_instance_name
        .rsplit('.')
        .next()
        .unwrap_or(component_instance_name);
    if suffix.contains("StorageInventory") {
        "Storage"
    } else if suffix.contains("InputInventory") {
        "Input"
    } else if suffix.contains("OutputInventory") {
        "Output"
    } else if suffix.contains("FuelInventory") {
        "Fuel"
    } else if suffix.contains("TrashInventory") {
        "Trash"
    } else if suffix.contains("Inventory") {
        "Inventory"
    } else {
        "Other"
    }
}

fn build_inventory_summaries_and_catalog(
    parsed_save: &ParsedSaveGame,
    player_structures: &[PlayerStructureSummary],
) -> (Vec<InventorySummary>, Vec<ItemCatalogEntry>) {
    let mut actor_types: HashMap<&str, &str> = HashMap::new();
    for lvl in &parsed_save.levels {
        for actor in &lvl.actors {
            actor_types.insert(actor.instance_name.as_str(), actor.type_path.as_str());
        }
    }
    let mut struct_lookup: HashMap<&str, (&str, &str)> = HashMap::new();
    for st in player_structures {
        struct_lookup.insert(
            st.instance_name.as_str(),
            (st.display_name.as_str(), st.category.as_str()),
        );
    }

    let mut inventories = Vec::new();
    let mut catalog_map: HashMap<String, ItemCatalogEntry> = HashMap::new();

    for lvl in &parsed_save.levels {
        for comp in &lvl.inventory_components {
            let parent = comp.parent_actor_name.as_str();
            let (owner_type, owner_display_name) = classify_inventory_owner(
                parent,
                actor_types.get(parent).copied(),
                struct_lookup.get(parent).copied(),
            );
            let slot_role = classify_slot_role(&comp.instance_name).to_string();

            let mut enriched_items = Vec::with_capacity(comp.stacks.len());
            for (slot_index, item_path, count) in &comp.stacks {
                let meta = crate::item_registry::lookup_item_metadata(item_path);
                let cnt = *count as i64;
                let entry = catalog_map
                    .entry(item_path.clone())
                    .or_insert_with(|| ItemCatalogEntry {
                        item_path: item_path.clone(),
                        display_name: meta.display_name.clone(),
                        category: meta.category.clone(),
                        mod_source: meta.mod_source.clone(),
                        is_known: meta.is_known,
                        total_count: 0,
                        pioneer_count: 0,
                        storage_count: 0,
                        machine_count: 0,
                    });
                entry.total_count += cnt;
                match owner_type {
                    "Pioneer" => entry.pioneer_count += cnt,
                    "Storage" => entry.storage_count += cnt,
                    "Machine" | "Vehicle" => entry.machine_count += cnt,
                    _ => {}
                }

                enriched_items.push(InventoryItemStack {
                    slot_index: *slot_index,
                    item_path: item_path.clone(),
                    display_name: meta.display_name,
                    category: meta.category,
                    mod_source: meta.mod_source,
                    count: *count,
                });
            }

            inventories.push(InventorySummary {
                component_instance_name: comp.instance_name.clone(),
                owner_instance_name: comp.parent_actor_name.clone(),
                owner_type: owner_type.to_string(),
                owner_display_name,
                slot_role,
                total_slots: comp.total_slots,
                items: enriched_items,
            });
        }
    }

    let mut item_catalog: Vec<ItemCatalogEntry> = catalog_map.into_values().collect();
    item_catalog.sort_by(|a, b| {
        b.total_count
            .cmp(&a.total_count)
            .then_with(|| a.display_name.cmp(&b.display_name))
    });

    (inventories, item_catalog)
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

/// Rotates a 3D vector by a unit quaternion `[qx, qy, qz, qw]`.
#[inline]
fn rotate_vec3_by_quat(v: [f64; 3], q: [f64; 4]) -> [f64; 3] {
    let [x, y, z] = v;
    let [qx, qy, qz, qw] = q;
    let cx = qy * z - qz * y;
    let cy = qz * x - qx * z;
    let cz = qx * y - qy * x;
    [
        x + 2.0 * (qw * cx + qy * cz - qz * cy),
        y + 2.0 * (qw * cy + qz * cx - qx * cz),
        z + 2.0 * (qw * cz + qx * cy - qy * cx),
    ]
}

#[inline]
fn get_vec3_prop(map: &HashMap<String, PropertyValue>, key: &str) -> Option<[f64; 3]> {
    match map.get(key) {
        Some(PropertyValue::Struct(StructValue::Vector { x, y, z })) => Some([*x, *y, *z]),
        _ => None,
    }
}

/// Extracts world-space cubic Hermite spline points `[x, y, z]` (in cm) from `mSplineData` if present.
fn extract_spline_points(
    properties: &HashMap<String, PropertyValue>,
    transform: &Transform,
) -> Option<Vec<[f64; 3]>> {
    let PropertyValue::Array(items) = properties.get("mSplineData")? else {
        return None;
    };
    if items.is_empty() {
        return None;
    }

    let q = transform.rotation.map(|v| v as f64);
    let [tx, ty, tz] = transform.translation.map(|v| v as f64);

    struct SplineNode {
        loc: [f64; 3],
        arrive: [f64; 3],
        leave: [f64; 3],
    }

    let mut nodes = Vec::with_capacity(items.len());
    for item in items {
        let PropertyValue::Struct(StructValue::Properties(map)) = item else {
            continue;
        };
        let Some(local_loc) = get_vec3_prop(map, "Location") else {
            continue;
        };
        let r_loc = rotate_vec3_by_quat(local_loc, q);
        let arrive = get_vec3_prop(map, "ArriveTangent")
            .map(|v| rotate_vec3_by_quat(v, q))
            .unwrap_or([0.0, 0.0, 0.0]);
        let leave = get_vec3_prop(map, "LeaveTangent")
            .map(|v| rotate_vec3_by_quat(v, q))
            .unwrap_or([0.0, 0.0, 0.0]);

        nodes.push(SplineNode {
            loc: [tx + r_loc[0], ty + r_loc[1], tz + r_loc[2]],
            arrive,
            leave,
        });
    }

    if nodes.len() < 2 {
        return None;
    }

    const HERMITE_STEPS: usize = 10;
    let mut pts = Vec::with_capacity((nodes.len() - 1) * HERMITE_STEPS + 1);
    pts.push(nodes[0].loc);

    for pair in nodes.windows(2) {
        let p0 = pair[0].loc;
        let t0 = pair[0].leave;
        let p1 = pair[1].loc;
        let t1 = pair[1].arrive;

        let t_mag_sq = t0[0] * t0[0] + t0[1] * t0[1] + t0[2] * t0[2]
            + t1[0] * t1[0] + t1[1] * t1[1] + t1[2] * t1[2];
        if t_mag_sq < 1e-3 {
            pts.push(p1);
            continue;
        }

        for step in 1..=HERMITE_STEPS {
            let t = step as f64 / HERMITE_STEPS as f64;
            let t2 = t * t;
            let t3 = t2 * t;
            let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
            let h10 = t3 - 2.0 * t2 + t;
            let h01 = -2.0 * t3 + 3.0 * t2;
            let h11 = t3 - t2;
            pts.push([
                h00 * p0[0] + h10 * t0[0] + h01 * p1[0] + h11 * t1[0],
                h00 * p0[1] + h10 * t0[1] + h01 * p1[1] + h11 * t1[1],
                h00 * p0[2] + h10 * t0[2] + h01 * p1[2] + h11 * t1[2],
            ]);
        }
    }

    Some(pts)
}

/// Parses `FGLightweightBuildableSubsystem`'s custom binary instance map after its property list.
fn parse_lightweight_buildables(
    rdr: &mut Cursor<&[u8]>,
    max_offset: u64,
    base_offset: usize,
    subsystem_object_size_offset: usize,
    subsystem_all_objects_size_offset: usize,
) -> Vec<LightweightBuildableInstance> {
    let mut instances = Vec::new();
    if rdr.position() + 12 > max_offset {
        return instances;
    }

    // Header after `None`: 4-byte trailing/version + 4-byte flags/version + 4-byte class map count
    // (Some versions have 4 bytes before map_count, others 8 bytes; check which yields a valid count.)
    let _v0 = rdr.read_u32::<LittleEndian>().unwrap_or(0);
    let _version = rdr.read_u32::<LittleEndian>().unwrap_or(0);
    let map_count = rdr.read_u32::<LittleEndian>().unwrap_or(0);

    if map_count == 0 || map_count > 10_000 {
        return instances;
    }

    for _ in 0..map_count {
        if rdr.position() >= max_offset {
            break;
        }
        let Ok(class_ref) = read_object_reference(rdr) else {
            break;
        };
        let inst_count_offset = base_offset + rdr.position() as usize;
        let Ok(inst_count) = rdr.read_u32::<LittleEndian>() else {
            break;
        };
        if inst_count > 500_000 {
            break;
        }
        instances.reserve((inst_count as usize).min(65_536));

        for _ in 0..inst_count {
            if rdr.position() + 80 > max_offset {
                return instances;
            }
            let entry_start = base_offset + rdr.position() as usize;
            let Ok(rx) = rdr.read_f64::<LittleEndian>() else { return instances; };
            let Ok(ry) = rdr.read_f64::<LittleEndian>() else { return instances; };
            let Ok(rz) = rdr.read_f64::<LittleEndian>() else { return instances; };
            let Ok(rw) = rdr.read_f64::<LittleEndian>() else { return instances; };
            let translation_offset = base_offset + rdr.position() as usize;
            let Ok(tx) = rdr.read_f64::<LittleEndian>() else { return instances; };
            let Ok(ty) = rdr.read_f64::<LittleEndian>() else { return instances; };
            let Ok(tz) = rdr.read_f64::<LittleEndian>() else { return instances; };
            let Ok(sx) = rdr.read_f64::<LittleEndian>() else { return instances; };
            let Ok(sy) = rdr.read_f64::<LittleEndian>() else { return instances; };
            let Ok(sz) = rdr.read_f64::<LittleEndian>() else { return instances; };

            // CustomizationData: Swatch, Material, Pattern, Skin, PrimaryColor (16B), SecondaryColor (16B), 9B flags
            for _ in 0..4 {
                if read_object_reference(rdr).is_err() {
                    return instances;
                }
            }
            if rdr.position() + 41 > max_offset {
                return instances;
            }
            rdr.set_position(rdr.position() + 41);

            // BuiltWithRecipe, BlueprintProxy, + 9B trailing flags/type
            if read_object_reference(rdr).is_err() || read_object_reference(rdr).is_err() {
                return instances;
            }
            if rdr.position() + 9 > max_offset {
                return instances;
            }
            rdr.set_position(rdr.position() + 9);
            let entry_end = base_offset + rdr.position() as usize;

            instances.push(LightweightBuildableInstance {
                type_path: class_ref.path_name.clone(),
                transform: Transform {
                    rotation: [rx as f32, ry as f32, rz as f32, rw as f32],
                    translation: [tx as f32, ty as f32, tz as f32],
                    scale: [sx as f32, sy as f32, sz as f32],
                },
                translation_offset,
                inst_count_offset,
                entry_start,
                entry_end,
                subsystem_object_size_offset,
                subsystem_all_objects_size_offset,
            });
        }
    }

    instances
}
