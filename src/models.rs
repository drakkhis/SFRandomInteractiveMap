use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Metadata parsed from the uncompressed save header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveHeader {
    pub save_header_type: u32,
    pub save_version: u32,
    pub build_version: u32,
    pub save_name: String,
    pub map_name: String,
    pub map_options: String,
    pub session_name: String,
    pub play_duration_seconds: u32,
    pub save_date_time_ticks: u64,
    pub session_visibility: u8,
    pub editor_object_version: u32,
    pub mod_metadata: String,
    pub is_modded_save: bool,
    pub save_identifier: String,
    pub is_creative_mode_enabled: bool,
}

/// 3D Transform in Unreal Engine coordinate space.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct Transform {
    pub rotation: [f32; 4], // x, y, z, w quaternion
    pub translation: [f32; 3], // x, y, z
    pub scale: [f32; 3], // x, y, z
}

/// Unreal Engine Object Reference (level and path name).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct ObjectReference {
    pub level_name: String,
    pub path_name: String,
}

/// Header for an Actor in a Level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorHeader {
    pub type_path: String,
    pub root_object: String,
    pub instance_name: String,
    pub flags: u32,
    pub need_transform: bool,
    pub transform: Transform,
    pub was_placed_in_level: bool,
    #[serde(default)]
    pub translation_offset: usize,
}

/// Header for a Component in a Level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHeader {
    pub class_name: String,
    pub root_object: String,
    pub instance_name: String,
    pub flags: u32,
    pub parent_actor_name: String,
}

/// Enum for either an Actor or Component header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LevelObjectHeader {
    Actor(ActorHeader),
    Component(ComponentHeader),
}

impl LevelObjectHeader {
    pub fn instance_name(&self) -> &str {
        match self {
            LevelObjectHeader::Actor(a) => &a.instance_name,
            LevelObjectHeader::Component(c) => &c.instance_name,
        }
    }
}

/// Property Value in Unreal Engine Property Tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PropertyValue {
    Bool(bool),
    Int8(i8),
    UInt8(u8),
    Int(i32),
    UInt32(u32),
    Int64(i64),
    UInt64(u64),
    Float(f32),
    Double(f64),
    Str(String),
    ObjectRef(ObjectReference),
    ByteEnum { enum_name: String, value: String },
    Struct(StructValue),
    Array(Vec<PropertyValue>),
    Map(Vec<(PropertyValue, PropertyValue)>),
    Set(Vec<PropertyValue>),
    ModRawBlob { struct_type: String, byte_len: usize },
    RawBytes(Vec<u8>),
}

/// Struct Property Values.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StructValue {
    LinearColor { r: f32, g: f32, b: f32, a: f32 },
    Vector { x: f64, y: f64, z: f64 },
    Vector2D { x: f64, y: f64 },
    Quat { x: f64, y: f64, z: f64, w: f64 },
    Box { min: [f64; 3], max: [f64; 3], flag: bool },
    DateTime(i64),
    FluidBox(f32),
    RailroadTrackPosition { level_path_name: ObjectReference, offset: f32, forward: f32 },
    InventoryItem { item_name: String },
    ModRawBlob { struct_type: String, byte_len: usize },
    Properties(HashMap<String, PropertyValue>),
    RawBytes(Vec<u8>),
}

/// A single lightweight buildable instance extracted from `FGLightweightBuildableSubsystem`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightweightBuildableInstance {
    pub type_path: String,
    pub transform: Transform,
    #[serde(default)]
    pub translation_offset: usize,
    #[serde(skip)]
    pub inst_count_offset: usize,
    #[serde(skip)]
    pub entry_start: usize,
    #[serde(skip)]
    pub entry_end: usize,
    #[serde(skip)]
    pub subsystem_object_size_offset: usize,
    #[serde(skip)]
    pub subsystem_all_objects_size_offset: usize,
}

/// Actor and Object combined with headers and properties.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedActor {
    pub type_path: String,
    pub root_object: String,
    pub instance_name: String,
    pub flags: u32,
    pub transform: Transform,
    pub was_placed_in_level: bool,
    pub properties: HashMap<String, PropertyValue>,
    #[serde(default)]
    pub translation_offset: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lightweight_instances: Vec<LightweightBuildableInstance>,
    #[serde(skip)]
    pub fog_raw_data_range: Option<(usize, usize)>,
    #[serde(skip)]
    pub payload_start_abs: usize,
    #[serde(skip)]
    pub object_size: usize,
    #[serde(skip)]
    pub object_size_offset: usize,
    #[serde(skip)]
    pub all_objects_size_offset: usize,
    #[serde(skip)]
    pub object_ue5_version: i32,
}

/// Summary of an artifact or collectible (Somersloop or Mercer Sphere).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactSummary {
    pub instance_name: String,
    pub type_path: String,
    pub transform: Transform,
    pub artifact_type: String, // "Somersloop" or "MercerSphere"
}

/// Summary of a resource node (`BP_ResourceNode_C`, Geysers, Resource Wells, Mod Nodes).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceNodeSummary {
    pub instance_name: String,
    pub transform: Transform,
    pub purity: Option<String>,
    pub resource_class: Option<String>,
    pub resource_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_type: Option<String>, // "Standard", "Geyser", "ResourceWellCore", "ResourceWellSatellite"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mod_source: Option<String>, // "Vanilla", "RefinedPower", "SatisfactoryPlus"
    #[serde(default)]
    pub translation_offset: usize,
}

/// Summary of a resource deposit (`BP_ResourceDeposit_C`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceDepositSummary {
    pub instance_name: String,
    pub transform: Transform,
    pub resource_class: Option<String>,
}

/// Summary of a drop pod (`BP_DropPod_C`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DropPodSummary {
    pub instance_name: String,
    pub transform: Transform,
    pub power_consumption: Option<f32>,
}

/// Summary of player position and state (`Char_Player_C`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerSummary {
    pub instance_name: String,
    pub transform: Transform,
    #[serde(default)]
    pub translation_offset: usize,
}

/// Summary of a player placed structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerStructureSummary {
    pub instance_name: String,
    pub type_path: String,
    pub transform: Transform,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub mod_source: String,
    #[serde(default)]
    pub footprint: [f32; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spline_points: Option<Vec<[f64; 3]>>,
    #[serde(default)]
    pub is_lightweight: bool,
    #[serde(default)]
    pub translation_offset: usize,
}

/// Raw parsed inventory component extracted from a level's component stream.
#[derive(Debug, Clone)]
pub struct ParsedInventoryComponent {
    pub instance_name: String,
    pub parent_actor_name: String,
    pub total_slots: usize,
    pub stacks: Vec<(usize, String, i32)>,
    pub all_objects_size_offset: usize,
    pub object_size_offset: usize,
    pub payload_start_abs: usize,
    pub object_size: usize,
    pub object_ue5_version: i32,
}

/// A single enriched item stack inside an inventory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryItemStack {
    #[serde(default)]
    pub slot_index: usize,
    pub item_path: String,
    pub display_name: String,
    pub category: String,
    pub mod_source: String,
    pub count: i32,
}

/// Inventory contents belonging to a pioneer, storage container, machine, or vehicle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventorySummary {
    #[serde(default)]
    pub component_instance_name: String,
    pub owner_instance_name: String,
    pub owner_type: String,
    pub owner_display_name: String,
    pub slot_role: String,
    #[serde(default)]
    pub total_slots: usize,
    pub items: Vec<InventoryItemStack>,
}

/// Aggregated item entry in the dynamically populated save item catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemCatalogEntry {
    pub item_path: String,
    pub display_name: String,
    pub category: String,
    pub mod_source: String,
    pub is_known: bool,
    pub total_count: i64,
    pub pioneer_count: i64,
    pub storage_count: i64,
    pub machine_count: i64,
}

/// A scalar game rule / save variable (`BoolProperty`, `IntProperty`, `FloatProperty`) on
/// `FGGameRulesSubsystem`, `BP_GameState`, `BP_UnlockSubsystem`, or `BP_PlayerState`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameRuleVariable {
    pub actor_instance_name: String,
    pub actor_label: String,
    pub property_name: String,
    pub value_type: String, // "bool" | "int" | "float"
    pub bool_val: Option<bool>,
    pub int_val: Option<i32>,
    pub float_val: Option<f32>,
}

/// Creative Mode and custom/advanced save game settings extracted from the save.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SaveGameSettings {
    pub is_creative_mode_enabled: bool,
    #[serde(default)]
    pub variables: Vec<GameRuleVariable>,
}

/// Overall JSON summary extracted for Milestone 1 and Milestone 2 (`rwg_actors.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RwgActorsSummary {
    pub save_metadata: SaveHeader,
    pub total_actor_headers: usize,
    pub total_objects_parsed: usize,
    pub world_seed: Option<i32>,
    pub world_size_meters: Option<f64>,
    pub player: Option<PlayerSummary>,
    pub resource_nodes: Vec<ResourceNodeSummary>,
    pub resource_deposits: Vec<ResourceDepositSummary>,
    pub drop_pods: Vec<DropPodSummary>,
    pub player_structures: Vec<PlayerStructureSummary>,
    #[serde(default)]
    pub inventories: Vec<InventorySummary>,
    #[serde(default)]
    pub item_catalog: Vec<ItemCatalogEntry>,
    #[serde(default)]
    pub somersloops: Vec<ArtifactSummary>,
    #[serde(default)]
    pub mercer_spheres: Vec<ArtifactSummary>,
    #[serde(skip)]
    pub generated_map_png: Option<Vec<u8>>,
    #[serde(skip)]
    pub generated_heightmap: Option<Vec<u8>>,
    #[serde(default)]
    pub has_fog_of_war: bool,
    #[serde(default)]
    pub fog_revealed_pct: f32,
    #[serde(skip)]
    pub fog_of_war_png: Option<Vec<u8>>,
    #[serde(default)]
    pub game_settings: SaveGameSettings,
}

/// 2D/3D spatial bounds of the world.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct WorldBounds {
    pub min_x: f32,
    pub max_x: f32,
    pub min_y: f32,
    pub max_y: f32,
    pub min_z: f32,
    pub max_z: f32,
}

impl Default for WorldBounds {
    fn default() -> Self {
        Self {
            min_x: 0.0,
            max_x: 0.0,
            min_y: 0.0,
            max_y: 0.0,
            min_z: 0.0,
            max_z: 0.0,
        }
    }
}

/// Dynamic snapshot of the modded world returned by GET /api/world.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldSnapshot {
    pub session_name: String,
    pub map_name: String,
    pub save_version: u32,
    pub build_version: u32,
    pub play_duration_seconds: u32,
    pub world_seed: Option<i32>,
    pub world_size_meters: Option<f64>,
    pub world_bounds: WorldBounds,
    pub player: Option<PlayerSummary>,
    pub resource_nodes: Vec<ResourceNodeSummary>,
    pub resource_deposits: Vec<ResourceDepositSummary>,
    pub drop_pods: Vec<DropPodSummary>,
    pub player_structures: Vec<PlayerStructureSummary>,
    #[serde(default)]
    pub inventories: Vec<InventorySummary>,
    #[serde(default)]
    pub item_catalog: Vec<ItemCatalogEntry>,
    pub somersloops: Vec<ArtifactSummary>,
    pub mercer_spheres: Vec<ArtifactSummary>,
    pub total_nodes: usize,
    pub total_drop_pods: usize,
    pub total_somersloops: usize,
    pub total_mercer_spheres: usize,
    #[serde(default)]
    pub has_fog_of_war: bool,
    #[serde(default)]
    pub fog_revealed_pct: f32,
    #[serde(default)]
    pub game_settings: SaveGameSettings,
}

impl RwgActorsSummary {
    /// Converts the full extracted actors summary into a clean WorldSnapshot.
    pub fn to_world_snapshot(&self) -> WorldSnapshot {
        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN;
        let mut min_y = f32::MAX;
        let mut max_y = f32::MIN;
        let mut min_z = f32::MAX;
        let mut max_z = f32::MIN;

        let mut update_bounds = |pos: &[f32; 3]| {
            if pos[0] < min_x { min_x = pos[0]; }
            if pos[0] > max_x { max_x = pos[0]; }
            if pos[1] < min_y { min_y = pos[1]; }
            if pos[1] > max_y { max_y = pos[1]; }
            if pos[2] < min_z { min_z = pos[2]; }
            if pos[2] > max_z { max_z = pos[2]; }
        };

        for node in &self.resource_nodes {
            update_bounds(&node.transform.translation);
        }
        for dep in &self.resource_deposits {
            update_bounds(&dep.transform.translation);
        }
        for pod in &self.drop_pods {
            update_bounds(&pod.transform.translation);
        }
        for s in &self.somersloops {
            update_bounds(&s.transform.translation);
        }
        for m in &self.mercer_spheres {
            update_bounds(&m.transform.translation);
        }
        if let Some(ref p) = self.player {
            update_bounds(&p.transform.translation);
        }

        let world_bounds = if min_x <= max_x {
            WorldBounds {
                min_x,
                max_x,
                min_y,
                max_y,
                min_z,
                max_z,
            }
        } else {
            WorldBounds::default()
        };

        WorldSnapshot {
            session_name: self.save_metadata.session_name.clone(),
            map_name: self.save_metadata.map_name.clone(),
            save_version: self.save_metadata.save_version,
            build_version: self.save_metadata.build_version,
            play_duration_seconds: self.save_metadata.play_duration_seconds,
            world_seed: self.world_seed,
            world_size_meters: self.world_size_meters,
            world_bounds,
            player: self.player.clone(),
            resource_nodes: self.resource_nodes.clone(),
            resource_deposits: self.resource_deposits.clone(),
            drop_pods: self.drop_pods.clone(),
            player_structures: self.player_structures.clone(),
            inventories: self.inventories.clone(),
            item_catalog: self.item_catalog.clone(),
            somersloops: self.somersloops.clone(),
            mercer_spheres: self.mercer_spheres.clone(),
            total_nodes: self.resource_nodes.len(),
            total_drop_pods: self.drop_pods.len(),
            total_somersloops: self.somersloops.len(),
            total_mercer_spheres: self.mercer_spheres.len(),
            has_fog_of_war: self.has_fog_of_war,
            fog_revealed_pct: self.fog_revealed_pct,
            game_settings: self.game_settings.clone(),
        }
    }
}
