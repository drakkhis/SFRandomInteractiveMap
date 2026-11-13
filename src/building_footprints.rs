use crate::parser::{detect_mod_source, extract_clean_name};

/// Metadata and 2D/3D footprint dimensions (in meters) for a player-built structure.
/// `length_m` is along local Unreal X (forward/backward), `width_m` is along local Unreal Y (left/right).
#[derive(Debug, Clone)]
pub struct BuildingMetadata {
    pub display_name: String,
    pub category: String,
    pub mod_source: String,
    pub width_m: f32,
    pub length_m: f32,
}

/// Resolves human-readable name, category, mod source, and `[width_m, length_m]` footprint
/// for any Satisfactory `Build_*` class path (Vanilla, SF+/KLib, Refined Power, Ficsit Farming, Mk++).
pub fn lookup_building_metadata(type_path: &str) -> BuildingMetadata {
    let mod_source = detect_mod_source(type_path, None);
    let class_name = type_path.rsplit('.').next().unwrap_or(type_path);

    if let Some((display, category, w, l)) = lookup_exact_or_pattern(class_name) {
        return BuildingMetadata {
            display_name: display.to_string(),
            category: category.to_string(),
            mod_source,
            width_m: w,
            length_m: l,
        };
    }

    // Pattern-based fallback for foundations, walls, belts, pipes, rails, poles, etc.
    let clean = extract_clean_name(type_path);
    let lower = class_name.to_ascii_lowercase();

    let (category, w, l) = if lower.contains("foundation") || lower.contains("ramp") || lower.contains("quarterpipe") || lower.contains("roof") {
        ("Foundation", 8.0, 8.0)
    } else if lower.contains("wall") || lower.contains("railing") || lower.contains("gate") {
        ("Foundation", 8.0, 0.5)
    } else if lower.contains("pillar") || lower.contains("beam") || lower.contains("catwalk") || lower.contains("walkway") {
        ("Foundation", 4.0, 4.0)
    } else if lower.contains("conveyorbelt") || lower.contains("conveyorlift") {
        ("Logistics", 2.0, 2.0)
    } else if lower.contains("pipeline") || lower.contains("hypertube") || lower.starts_with("build_pp") {
        ("Logistics", 1.5, 1.5)
    } else if lower.contains("railway") || lower.contains("rail") {
        ("Transportation", 6.0, 6.0)
    } else if lower.contains("powerpole") || lower.contains("powerline") || lower.contains("powertower") || lower.contains("polestand") || lower.contains("polewall") || lower.contains("cs_cable") {
        ("Power", 1.5, 1.5)
    } else if lower.contains("splitter") || lower.contains("merger") || lower.contains("loadbalancer") || lower.contains("junction") || lower.contains("valve") || lower.contains("pump") {
        ("Logistics", 4.0, 4.0)
    } else if lower.contains("storage") || lower.contains("tank") || lower.contains("buffer") {
        ("Logistics", 6.0, 8.0)
    } else if lower.contains("generator") || lower.contains("turbine") || lower.contains("boiler") || lower.contains("heater") || lower.contains("cooler") || lower.contains("solar") || lower.contains("reactor") {
        ("Power", 10.0, 12.0)
    } else if lower.contains("ff_") || lower.contains("plot") || lower.contains("composter") || lower.contains("greenhouse") {
        ("Farming", 8.0, 8.0)
    } else {
        ("Production", 6.0, 6.0)
    };

    BuildingMetadata {
        display_name: clean,
        category: category.to_string(),
        mod_source,
        width_m: w,
        length_m: l,
    }
}

fn lookup_exact_or_pattern(cls: &str) -> Option<(&'static str, &'static str, f32, f32)> {
    // Strip Mk2/Mk3 suffix for Mk++ variants where appropriate, or match explicitly
    match cls {
        // --- Vanilla Production ---
        "Build_SmelterMk1_C" => Some(("Smelter", "Production", 6.0, 9.0)),
        "Build_Smelter_Mk2_C" => Some(("Smelter Mk.2", "Production", 6.0, 9.0)),
        "Build_FoundryMk1_C" => Some(("Foundry", "Production", 10.0, 9.0)),
        "Build_Foundry_Mk2_C" => Some(("Foundry Mk.2", "Production", 10.0, 9.0)),
        "Build_ConstructorMk1_C" => Some(("Constructor", "Production", 8.0, 10.0)),
        "Build_Constructor_Mk2_C" => Some(("Constructor Mk.2", "Production", 8.0, 10.0)),
        "Build_Constructor_Mk3_C" => Some(("Constructor Mk.3", "Production", 8.0, 10.0)),
        "Build_AssemblerMk1_C" => Some(("Assembler", "Production", 10.0, 15.0)),
        "Build_Assembler_Mk2_C" => Some(("Assembler Mk.2", "Production", 10.0, 15.0)),
        "Build_ManufacturerMk1_C" => Some(("Manufacturer", "Production", 18.0, 20.0)),
        "Build_Manufacturer_Mk2_C" => Some(("Manufacturer Mk.2", "Production", 18.0, 20.0)),
        "Build_OilRefinery_C" => Some(("Refinery", "Production", 10.0, 20.0)),
        "Build_Refinery_Mk2_C" => Some(("Refinery Mk.2", "Production", 10.0, 20.0)),
        "Build_Refinery_Mk3_C" => Some(("Refinery Mk.3", "Production", 10.0, 20.0)),
        "Build_Blender_C" => Some(("Blender", "Production", 18.0, 16.0)),
        "Build_Packager_C" => Some(("Packager", "Production", 8.0, 8.0)),
        "Build_HadronCollider_C" => Some(("Particle Accelerator", "Production", 24.0, 38.0)),
        "Build_HadronCollider_Mk2_C" => Some(("Particle Accelerator Mk.2", "Production", 24.0, 38.0)),
        "Build_Converter_C" => Some(("Converter", "Production", 16.0, 16.0)),
        "Build_QuantumEncoder_C" => Some(("Quantum Encoder", "Production", 22.0, 50.0)),

        // --- Vanilla Extractors ---
        "Build_MinerMk1_C" => Some(("Miner Mk.1", "Production", 6.0, 14.0)),
        "Build_MinerMk2_C" => Some(("Miner Mk.2", "Production", 6.0, 14.0)),
        "Build_MinerMk3_C" => Some(("Miner Mk.3", "Production", 6.0, 14.0)),
        "Build_OilPump_C" => Some(("Oil Extractor", "Production", 12.0, 21.0)),
        "Build_OilPump_Mk2_C" => Some(("Oil Extractor Mk.2", "Production", 12.0, 21.0)),
        "Build_WaterPump_C" => Some(("Water Extractor", "Production", 20.0, 20.0)),
        "Build_FrackingSmasher_C" => Some(("Resource Well Pressurizer", "Production", 20.0, 20.0)),
        "Build_FrackingExtractor_C" => Some(("Resource Well Extractor", "Production", 4.0, 4.0)),
        "Build_FrackingExtractor_Mk2_C" => Some(("Resource Well Extractor Mk.2", "Production", 4.0, 4.0)),

        // --- Vanilla Power ---
        "Build_GeneratorBiomass_C" | "Build_GeneratorBiomass_Automated_C" => Some(("Biomass Burner", "Power", 8.0, 8.0)),
        "Build_GeneratorCoal_C" => Some(("Coal Generator", "Power", 10.0, 26.0)),
        "Build_GeneratorFuel_C" => Some(("Fuel Generator", "Power", 20.0, 20.0)),
        "Build_GeneratorNuclear_C" => Some(("Nuclear Power Plant", "Power", 36.0, 43.0)),
        "Build_GeneratorGeoThermal_C" => Some(("Geothermal Generator", "Power", 14.0, 14.0)),
        "Build_AlienPowerBuilding_C" => Some(("Alien Power Augmenter", "Power", 28.0, 28.0)),
        "Build_PowerStorage_C" => Some(("Power Storage", "Power", 6.0, 6.0)),

        // --- Vanilla Logistics & Storage ---
        "Build_StorageContainerMk1_C" | "Build_StorageContainer1_Mk2_C" => Some(("Storage Container", "Logistics", 5.0, 10.0)),
        "Build_StorageContainerMk2_C" | "Build_StorageContainer2_Mk2_C" => Some(("Industrial Storage", "Logistics", 5.0, 10.0)),
        "Build_PipeStorageTank_C" | "Build_PipeStorageTank_Mk2_C" => Some(("Fluid Buffer", "Logistics", 6.0, 6.0)),
        "Build_IndustrialTank_C" | "Build_IndustrialTank_Mk2_C" => Some(("Industrial Fluid Buffer", "Logistics", 10.0, 10.0)),
        "Build_CentralStorage_C" => Some(("Dimensional Depot", "Logistics", 4.0, 4.0)),
        "Build_ConveyorAttachmentSplitter_C" => Some(("Splitter", "Logistics", 4.0, 4.0)),
        "Build_ConveyorAttachmentSplitterSmart_C" => Some(("Smart Splitter", "Logistics", 4.0, 4.0)),
        "Build_ConveyorAttachmentSplitterProgrammable_C" => Some(("Programmable Splitter", "Logistics", 4.0, 4.0)),
        "Build_ConveyorAttachmentMerger_C" => Some(("Merger", "Logistics", 4.0, 4.0)),
        "Build_ConveyorAttachmentMergerPriority_C" => Some(("Priority Merger", "Logistics", 4.0, 4.0)),

        // --- Vanilla Transportation & Special ---
        "Build_TrainStation_C" => Some(("Train Station", "Transportation", 16.0, 34.0)),
        "Build_TrainDockingStation_C" | "Build_TrainDockingStationLiquid_C" => Some(("Freight Platform", "Transportation", 16.0, 34.0)),
        "Build_TrainPlatformEmpty_C" => Some(("Empty Platform", "Transportation", 16.0, 34.0)),
        "Build_DroneStation_C" => Some(("Drone Port", "Transportation", 20.0, 22.0)),
        "Build_TruckStation_C" => Some(("Truck Station", "Transportation", 16.0, 22.0)),
        "Build_ResourceSink_C" => Some(("AWESOME Sink", "Special", 16.0, 13.0)),
        "Build_ResourceSinkShop_C" => Some(("AWESOME Shop", "Special", 6.0, 4.0)),
        "Build_SpaceElevator_C" => Some(("Space Elevator", "Special", 54.0, 54.0)),
        "Build_TradingPost_C" | "Build_HubTerminal_C" => Some(("The HUB", "Special", 14.0, 26.0)),
        "Build_RadarTower_C" => Some(("Radar Tower", "Special", 10.0, 10.0)),
        "Build_BlueprintDesigner_C" => Some(("Blueprint Designer Mk.1", "Special", 32.0, 32.0)),
        "Build_BlueprintDesigner_Mk2_C" => Some(("Blueprint Designer Mk.2", "Special", 40.0, 40.0)),
        "Build_BlueprintDesigner_Mk3_C" => Some(("Blueprint Designer Mk.3", "Special", 48.0, 48.0)),

        // --- Satisfactory Plus (KLib / SatisfactoryPlus / AB_FluidExtras) ---
        "Build_AirCollector_C" => Some(("Air Collector", "Production", 10.0, 10.0)),
        "Build_Breeder_C" => Some(("Breeder Reactor", "Production", 16.0, 20.0)),
        "Build_ChemReactor_C" | "Build_ChemReactorMk2_C" | "Build_ChemReactorMk3_C" => Some(("Chemical Reactor", "Production", 12.0, 16.0)),
        "Build_Crusher_C" | "Build_CrusherMk2_C" | "Build_CrusherMk3_C" => Some(("Crusher", "Production", 10.0, 14.0)),
        "Build_FlexibleBlastFurnaceSmelter_C" | "Build_FlexibleBlastFurnaceSmelterMk2_C" => Some(("Blast Furnace Smelter", "Production", 10.0, 14.0)),
        "Build_Sorter_C" | "Build_SorterMk2_C" => Some(("Sorter", "Production", 8.0, 12.0)),
        "Build_AB_FluidPacker_C" | "Build_AB_FluidPackerMk2_C" | "Build_AB_FluidPackerMk3_C" => Some(("Fluid Packer", "Production", 8.0, 8.0)),
        "Build_AB_MiniEx_C" | "Build_AB_MiniExMk2_C" => Some(("Mini Extractor", "Production", 4.0, 6.0)),
        "Build_SFP_SeedExtractor_C" => Some(("Seed Extractor", "Farming", 8.0, 10.0)),
        "Build_CS_Core_C" => Some(("Modular Storage Core", "Logistics", 8.0, 8.0)),
        "Build_CS_NetworkTower_C" => Some(("Storage Network Tower", "Logistics", 6.0, 6.0)),

        // --- Refined Power ---
        "Build_RP_MP_BoilerMk1_C" | "Build_RP_MP_BoilerMk2_C" => Some(("RP Modular Boiler", "Power", 8.0, 8.0)),
        "Build_RP_MP_BiomassHeater_C" | "Build_RP_MP_CoalHeater_C" | "Build_RP_MP_NuclearHeater_C" | "Build_RP_MP_SolutionHeater_C" => Some(("RP Modular Heater", "Power", 8.0, 8.0)),
        "Build_RP_MP_Co2Cooler_C" | "Build_RP_MP_SteamCooler_C" => Some(("RP Modular Cooler", "Power", 8.0, 8.0)),
        "Build_RP_MP_HVGenerator_C" | "Build_RP_MP_MVGenerator_C" | "Build_RP_MP_SuperTurbine_C" => Some(("RP Modular Turbine/Gen", "Power", 8.0, 12.0)),
        "Build_RP_MP_BoilerPlatform_C" | "Build_RP_MP_ConverterPlatform_C" | "Build_RP_MP_CoolingPlatform_C" | "Build_RP_MP_SuperPlatform_C" => Some(("RP Modular Platform", "Foundation", 8.0, 8.0)),
        "Build_RP_WaterDam_C" => Some(("RP Water Dam", "Power", 16.0, 16.0)),

        // --- Ficsit Farming ---
        "Build_FF_Composter_C" => Some(("Composter", "Farming", 8.0, 10.0)),
        "Build_FF_CookingHub_C" => Some(("Cooking Hub", "Farming", 8.0, 12.0)),
        "Build_FF_CropPlot_C" => Some(("Crop Plot", "Farming", 8.0, 8.0)),
        "Build_FF_MyceliaPlot_C" => Some(("Mycelia Plot", "Farming", 8.0, 8.0)),
        "Build_FF_TreePlot_C" => Some(("Tree Plot", "Farming", 8.0, 8.0)),
        "Build_FF_DoggoHouse_C" => Some(("Doggo House", "Farming", 6.0, 6.0)),
        "Build_FF_FarmingPlatformMk2_C" => Some(("Farming Platform Mk.2", "Farming", 8.0, 8.0)),
        "Build_FF_PlotMod_Greenhouse_C" => Some(("Plot Greenhouse Mod", "Farming", 4.0, 4.0)),
        "Build_FF_PlotMod_Harvester_C" => Some(("Plot Harvester Mod", "Farming", 4.0, 4.0)),
        "Build_FF_PlotMod_Irrigation_C" => Some(("Plot Irrigation Mod", "Farming", 4.0, 4.0)),

        _ => None,
    }
}
