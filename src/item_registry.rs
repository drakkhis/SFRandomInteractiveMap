//! Item metadata registry for Vanilla Satisfactory and supported mods
//! (Satisfactory Plus / KLib, Refined Power, Ficsit Farming, Mk++).
//! Any unrecognized item path encountered in a save or scanned from a mod folder
//! is dynamically parsed into a readable display name, category, and mod source.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemMetadata {
    pub display_name: String,
    pub category: String,
    pub mod_source: String,
    pub is_known: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownItemEntry {
    pub item_path: String,
    pub display_name: String,
    pub category: String,
    pub mod_source: String,
}

/// Canonical `(full_item_path, display_name, category)` table for Vanilla + supported mods.
const STATIC_ITEMS: &[(&str, &str, &str)] = &[
    // --- Vanilla Ores & Raw Resources ---
    ("/Game/FactoryGame/Resource/RawResources/OreIron/Desc_OreIron.Desc_OreIron_C", "Iron Ore", "Ore"),
    ("/Game/FactoryGame/Resource/RawResources/OreCopper/Desc_OreCopper.Desc_OreCopper_C", "Copper Ore", "Ore"),
    ("/Game/FactoryGame/Resource/RawResources/OreGold/Desc_OreGold.Desc_OreGold_C", "Caterium Ore", "Ore"),
    ("/Game/FactoryGame/Resource/RawResources/Stone/Desc_Stone.Desc_Stone_C", "Limestone", "Ore"),
    ("/Game/FactoryGame/Resource/RawResources/Coal/Desc_Coal.Desc_Coal_C", "Coal", "Fuel & Nuclear"),
    ("/Game/FactoryGame/Resource/RawResources/RawQuartz/Desc_RawQuartz.Desc_RawQuartz_C", "Raw Quartz", "Ore"),
    ("/Game/FactoryGame/Resource/RawResources/Sulfur/Desc_Sulfur.Desc_Sulfur_C", "Sulfur", "Ore"),
    ("/Game/FactoryGame/Resource/RawResources/OreBauxite/Desc_OreBauxite.Desc_OreBauxite_C", "Bauxite", "Ore"),
    ("/Game/FactoryGame/Resource/RawResources/OreUranium/Desc_OreUranium.Desc_OreUranium_C", "Uranium", "Fuel & Nuclear"),
    ("/Game/FactoryGame/Resource/RawResources/SAM/Desc_SAM.Desc_SAM_C", "SAM", "Ore"),
    ("/Game/FactoryGame/Resource/Parts/Silica/Desc_Silica.Desc_Silica_C", "Silica", "Part"),
    ("/Game/FactoryGame/Resource/Parts/QuartzCrystal/Desc_QuartzCrystal.Desc_QuartzCrystal_C", "Quartz Crystal", "Part"),

    // --- Vanilla Ingots ---
    ("/Game/FactoryGame/Resource/Parts/IronIngot/Desc_IronIngot.Desc_IronIngot_C", "Iron Ingot", "Ingot"),
    ("/Game/FactoryGame/Resource/Parts/CopperIngot/Desc_CopperIngot.Desc_CopperIngot_C", "Copper Ingot", "Ingot"),
    ("/Game/FactoryGame/Resource/Parts/GoldIngot/Desc_GoldIngot.Desc_GoldIngot_C", "Caterium Ingot", "Ingot"),
    ("/Game/FactoryGame/Resource/Parts/SteelIngot/Desc_SteelIngot.Desc_SteelIngot_C", "Steel Ingot", "Ingot"),
    ("/Game/FactoryGame/Resource/Parts/AluminumIngot/Desc_AluminumIngot.Desc_AluminumIngot_C", "Aluminum Ingot", "Ingot"),
    ("/Game/FactoryGame/Resource/Parts/SAMIngot/Desc_SAMIngot.Desc_SAMIngot_C", "Reanimated SAM", "Ingot"),
    ("/Game/FactoryGame/Resource/Parts/FicsiteIngot/Desc_FicsiteIngot.Desc_FicsiteIngot_C", "Ficsite Ingot", "Ingot"),

    // --- Vanilla Standard & Electronic Parts ---
    ("/Game/FactoryGame/Resource/Parts/IronPlate/Desc_IronPlate.Desc_IronPlate_C", "Iron Plate", "Part"),
    ("/Game/FactoryGame/Resource/Parts/IronRod/Desc_IronRod.Desc_IronRod_C", "Iron Rod", "Part"),
    ("/Game/FactoryGame/Resource/Parts/IronScrew/Desc_IronScrew.Desc_IronScrew_C", "Screw", "Part"),
    ("/Game/FactoryGame/Resource/Parts/IronPlateReinforced/Desc_IronPlateReinforced.Desc_IronPlateReinforced_C", "Reinforced Iron Plate", "Part"),
    ("/Game/FactoryGame/Resource/Parts/ModularFrame/Desc_ModularFrame.Desc_ModularFrame_C", "Modular Frame", "Part"),
    ("/Game/FactoryGame/Resource/Parts/ModularFrameHeavy/Desc_ModularFrameHeavy.Desc_ModularFrameHeavy_C", "Heavy Modular Frame", "Part"),
    ("/Game/FactoryGame/Resource/Parts/ModularFrameFused/Desc_ModularFrameFused.Desc_ModularFrameFused_C", "Fused Modular Frame", "Part"),
    ("/Game/FactoryGame/Resource/Parts/Wire/Desc_Wire.Desc_Wire_C", "Wire", "Part"),
    ("/Game/FactoryGame/Resource/Parts/Cable/Desc_Cable.Desc_Cable_C", "Cable", "Part"),
    ("/Game/FactoryGame/Resource/Parts/CopperSheet/Desc_CopperSheet.Desc_CopperSheet_C", "Copper Sheet", "Part"),
    ("/Game/FactoryGame/Resource/Parts/HighSpeedWire/Desc_HighSpeedWire.Desc_HighSpeedWire_C", "Quickwire", "Part"),
    ("/Game/FactoryGame/Resource/Parts/SteelPlate/Desc_SteelPlate.Desc_SteelPlate_C", "Steel Beam", "Part"),
    ("/Game/FactoryGame/Resource/Parts/SteelPipe/Desc_SteelPipe.Desc_SteelPipe_C", "Steel Pipe", "Part"),
    ("/Game/FactoryGame/Resource/Parts/SteelPlateReinforced/Desc_SteelPlateReinforced.Desc_SteelPlateReinforced_C", "Encased Industrial Beam", "Part"),
    ("/Game/FactoryGame/Resource/Parts/Cement/Desc_Cement.Desc_Cement_C", "Concrete", "Part"),
    ("/Game/FactoryGame/Resource/Parts/Rotor/Desc_Rotor.Desc_Rotor_C", "Rotor", "Part"),
    ("/Game/FactoryGame/Resource/Parts/Stator/Desc_Stator.Desc_Stator_C", "Stator", "Part"),
    ("/Game/FactoryGame/Resource/Parts/Motor/Desc_Motor.Desc_Motor_C", "Motor", "Part"),
    ("/Game/FactoryGame/Resource/Parts/MotorLightweight/Desc_MotorLightweight.Desc_MotorLightweight_C", "Turbo Motor", "Part"),
    ("/Game/FactoryGame/Resource/Parts/CircuitBoard/Desc_CircuitBoard.Desc_CircuitBoard_C", "Circuit Board", "Part"),
    ("/Game/FactoryGame/Resource/Parts/CircuitBoardHighSpeed/Desc_CircuitBoardHighSpeed.Desc_CircuitBoardHighSpeed_C", "AI Limiter", "Part"),
    ("/Game/FactoryGame/Resource/Parts/HighSpeedConnector/Desc_HighSpeedConnector.Desc_HighSpeedConnector_C", "High-Speed Connector", "Part"),
    ("/Game/FactoryGame/Resource/Parts/Computer/Desc_Computer.Desc_Computer_C", "Computer", "Part"),
    ("/Game/FactoryGame/Resource/Parts/ComputerSuper/Desc_ComputerSuper.Desc_ComputerSuper_C", "Supercomputer", "Part"),
    ("/Game/FactoryGame/Resource/Parts/QuantumOscillator/Desc_QuantumOscillator.Desc_QuantumOscillator_C", "Superposition Oscillator", "Part"),
    ("/Game/FactoryGame/Resource/Parts/ComputerQuantum/Desc_ComputerQuantum.Desc_ComputerQuantum_C", "Quantum Processor", "Part"),
    ("/Game/FactoryGame/Resource/Parts/Plastic/Desc_Plastic.Desc_Plastic_C", "Plastic", "Part"),
    ("/Game/FactoryGame/Resource/Parts/Rubber/Desc_Rubber.Desc_Rubber_C", "Rubber", "Part"),
    ("/Game/FactoryGame/Resource/Parts/PolymerResin/Desc_PolymerResin.Desc_PolymerResin_C", "Polymer Resin", "Part"),
    ("/Game/FactoryGame/Resource/Parts/PetroleumCoke/Desc_PetroleumCoke.Desc_PetroleumCoke_C", "Petroleum Coke", "Fuel & Nuclear"),
    ("/Game/FactoryGame/Resource/Parts/AluminumScrap/Desc_AluminumScrap.Desc_AluminumScrap_C", "Aluminum Scrap", "Part"),
    ("/Game/FactoryGame/Resource/Parts/AluminumPlate/Desc_AluminumPlate.Desc_AluminumPlate_C", "Alclad Aluminum Sheet", "Part"),
    ("/Game/FactoryGame/Resource/Parts/AluminumCasing/Desc_AluminumCasing.Desc_AluminumCasing_C", "Aluminum Casing", "Part"),
    ("/Game/FactoryGame/Resource/Parts/CoolingSystem/Desc_CoolingSystem.Desc_CoolingSystem_C", "Cooling System", "Part"),
    ("/Game/FactoryGame/Resource/Parts/ModularFrameLightweight/Desc_ModularFrameLightweight.Desc_ModularFrameLightweight_C", "Radio Control Unit", "Part"),
    ("/Game/FactoryGame/Resource/Parts/ElectromagneticControlRod/Desc_ElectromagneticControlRod.Desc_ElectromagneticControlRod_C", "Electromagnetic Control Rod", "Part"),
    ("/Game/FactoryGame/Resource/Parts/CrystalOscillator/Desc_CrystalOscillator.Desc_CrystalOscillator_C", "Crystal Oscillator", "Part"),
    ("/Game/FactoryGame/Resource/Parts/CopperDust/Desc_CopperDust.Desc_CopperDust_C", "Copper Powder", "Part"),
    ("/Game/FactoryGame/Resource/Parts/PressureConversionCube/Desc_PressureConversionCube.Desc_PressureConversionCube_C", "Pressure Conversion Cube", "Part"),
    ("/Game/FactoryGame/Resource/Parts/NuclearPasta/Desc_NuclearPasta.Desc_NuclearPasta_C", "Nuclear Pasta", "Part"),
    ("/Game/FactoryGame/Resource/Parts/TimeCrystal/Desc_TimeCrystal.Desc_TimeCrystal_C", "Time Crystal", "Part"),
    ("/Game/FactoryGame/Resource/Parts/DarkMatter/Desc_DarkMatter.Desc_DarkMatter_C", "Dark Matter Crystal", "Part"),
    ("/Game/FactoryGame/Resource/Parts/FicsiteMesh/Desc_FicsiteMesh.Desc_FicsiteMesh_C", "Ficsite Trigon", "Part"),
    ("/Game/FactoryGame/Resource/Parts/SingularityCell/Desc_SingularityCell.Desc_SingularityCell_C", "Singularity Cell", "Part"),
    ("/Game/FactoryGame/Resource/Parts/Diamond/Desc_Diamond.Desc_Diamond_C", "Diamonds", "Part"),
    ("/Game/FactoryGame/Resource/Parts/SAMFluctuator/Desc_SAMFluctuator.Desc_SAMFluctuator_C", "SAM Fluctuator", "Part"),

    // --- Vanilla Space Elevator Parts ---
    ("/Game/FactoryGame/Resource/Parts/SpaceElevatorParts/Desc_SpaceElevatorPart_1.Desc_SpaceElevatorPart_1_C", "Smart Plating", "Space Elevator"),
    ("/Game/FactoryGame/Resource/Parts/SpaceElevatorParts/Desc_SpaceElevatorPart_2.Desc_SpaceElevatorPart_2_C", "Versatile Framework", "Space Elevator"),
    ("/Game/FactoryGame/Resource/Parts/SpaceElevatorParts/Desc_SpaceElevatorPart_3.Desc_SpaceElevatorPart_3_C", "Automated Wiring", "Space Elevator"),
    ("/Game/FactoryGame/Resource/Parts/SpaceElevatorParts/Desc_SpaceElevatorPart_4.Desc_SpaceElevatorPart_4_C", "Modular Engine", "Space Elevator"),
    ("/Game/FactoryGame/Resource/Parts/SpaceElevatorParts/Desc_SpaceElevatorPart_5.Desc_SpaceElevatorPart_5_C", "Adaptive Control Unit", "Space Elevator"),
    ("/Game/FactoryGame/Resource/Parts/SpaceElevatorParts/Desc_SpaceElevatorPart_6.Desc_SpaceElevatorPart_6_C", "Magnetic Field Generator", "Space Elevator"),
    ("/Game/FactoryGame/Resource/Parts/SpaceElevatorParts/Desc_SpaceElevatorPart_7.Desc_SpaceElevatorPart_7_C", "Assembly Director System", "Space Elevator"),
    ("/Game/FactoryGame/Resource/Parts/SpaceElevatorParts/Desc_SpaceElevatorPart_8.Desc_SpaceElevatorPart_8_C", "Thermal Propulsion Rocket", "Space Elevator"),
    ("/Game/FactoryGame/Resource/Parts/SpaceElevatorParts/Desc_SpaceElevatorPart_9.Desc_SpaceElevatorPart_9_C", "Nuclear Pasta", "Space Elevator"),
    ("/Game/FactoryGame/Resource/Parts/SpaceElevatorParts/Desc_SpaceElevatorPart_10.Desc_SpaceElevatorPart_10_C", "Biochemical Sculptor", "Space Elevator"),
    ("/Game/FactoryGame/Resource/Parts/SpaceElevatorParts/Desc_SpaceElevatorPart_11.Desc_SpaceElevatorPart_11_C", "Ballistic Warp Drive", "Space Elevator"),
    ("/Game/FactoryGame/Resource/Parts/SpaceElevatorParts/Desc_SpaceElevatorPart_12.Desc_SpaceElevatorPart_12_C", "AI Expansion Server", "Space Elevator"),

    // --- Vanilla Fluids & Gases ---
    ("/Game/FactoryGame/Resource/RawResources/Water/Desc_Water.Desc_Water_C", "Water", "Fluid"),
    ("/Game/FactoryGame/Resource/RawResources/CrudeOil/Desc_LiquidOil.Desc_LiquidOil_C", "Crude Oil", "Fluid"),
    ("/Game/FactoryGame/Resource/Parts/HeavyOilResidue/Desc_HeavyOilResidue.Desc_HeavyOilResidue_C", "Heavy Oil Residue", "Fluid"),
    ("/Game/FactoryGame/Resource/Parts/Fuel/Desc_LiquidFuel.Desc_LiquidFuel_C", "Fuel", "Fluid"),
    ("/Game/FactoryGame/Resource/Parts/TurboFuel/Desc_LiquidTurboFuel.Desc_LiquidTurboFuel_C", "Turbofuel", "Fluid"),
    ("/Game/FactoryGame/Resource/Parts/BioFuel/Desc_LiquidBiofuel.Desc_LiquidBiofuel_C", "Liquid Biofuel", "Fluid"),
    ("/Game/FactoryGame/Resource/Parts/Alumina/Desc_AluminaSolution.Desc_AluminaSolution_C", "Alumina Solution", "Fluid"),
    ("/Game/FactoryGame/Resource/Parts/SulfuricAcid/Desc_SulfuricAcid.Desc_SulfuricAcid_C", "Sulfuric Acid", "Fluid"),
    ("/Game/FactoryGame/Resource/Parts/NitricAcid/Desc_NitricAcid.Desc_NitricAcid_C", "Nitric Acid", "Fluid"),
    ("/Game/FactoryGame/Resource/RawResources/NitrogenGas/Desc_NitrogenGas.Desc_NitrogenGas_C", "Nitrogen Gas", "Fluid"),
    ("/Game/FactoryGame/Resource/Parts/RocketFuel/Desc_RocketFuel.Desc_RocketFuel_C", "Rocket Fuel", "Fluid"),
    ("/Game/FactoryGame/Resource/Parts/IonizedFuel/Desc_IonizedFuel.Desc_IonizedFuel_C", "Ionized Fuel", "Fluid"),
    ("/Game/FactoryGame/Resource/Parts/DarkEnergy/Desc_DarkEnergy.Desc_DarkEnergy_C", "Dark Matter Residue", "Fluid"),
    ("/Game/FactoryGame/Resource/Parts/DissolvedSilica/Desc_DissolvedSilica.Desc_DissolvedSilica_C", "Dissolved Silica", "Fluid"),

    // --- Vanilla Biomass, Fuel & Nuclear ---
    ("/Game/FactoryGame/Resource/Environment/Desert/Desc_Leaves.Desc_Leaves_C", "Leaves", "Biomass"),
    ("/Game/FactoryGame/Resource/Environment/Desert/Desc_Wood.Desc_Wood_C", "Wood", "Biomass"),
    ("/Game/FactoryGame/Resource/Environment/Mushroom/Desc_Mycelia.Desc_Mycelia_C", "Mycelia", "Biomass"),
    ("/Game/FactoryGame/Resource/Parts/GenericBiomass/Desc_GenericBiomass.Desc_GenericBiomass_C", "Biomass", "Biomass"),
    ("/Game/FactoryGame/Resource/Parts/BioFuel/Desc_Biofuel.Desc_Biofuel_C", "Solid Biofuel", "Fuel & Nuclear"),
    ("/Game/FactoryGame/Resource/Parts/CompactedCoal/Desc_CompactedCoal.Desc_CompactedCoal_C", "Compacted Coal", "Fuel & Nuclear"),
    ("/Game/FactoryGame/Resource/Parts/FluidCanister/Desc_FluidCanister.Desc_FluidCanister_C", "Empty Canister", "Part"),
    ("/Game/FactoryGame/Resource/Parts/GasTank/Desc_GasTank.Desc_GasTank_C", "Empty Fluid Tank", "Part"),
    ("/Game/FactoryGame/Resource/Parts/PackagedWater/Desc_PackagedWater.Desc_PackagedWater_C", "Packaged Water", "Fluid"),
    ("/Game/FactoryGame/Resource/Parts/PackagedOil/Desc_PackagedOil.Desc_PackagedOil_C", "Packaged Oil", "Fluid"),
    ("/Game/FactoryGame/Resource/Parts/Fuel/Desc_Fuel.Desc_Fuel_C", "Packaged Fuel", "Fuel & Nuclear"),
    ("/Game/FactoryGame/Resource/Parts/TurboFuel/Desc_TurboFuel.Desc_TurboFuel_C", "Packaged Turbofuel", "Fuel & Nuclear"),
    ("/Game/FactoryGame/Resource/Parts/Battery/Desc_Battery.Desc_Battery_C", "Battery", "Fuel & Nuclear"),
    ("/Game/FactoryGame/Resource/Parts/UraniumCell/Desc_UraniumCell.Desc_UraniumCell_C", "Encased Uranium Cell", "Fuel & Nuclear"),
    ("/Game/FactoryGame/Resource/Parts/NuclearFuelRod/Desc_NuclearFuelRod.Desc_NuclearFuelRod_C", "Uranium Fuel Rod", "Fuel & Nuclear"),
    ("/Game/FactoryGame/Resource/Parts/NuclearWaste/Desc_NuclearWaste.Desc_NuclearWaste_C", "Uranium Waste", "Fuel & Nuclear"),
    ("/Game/FactoryGame/Resource/Parts/NonFissibleUranium/Desc_NonFissibleUranium.Desc_NonFissibleUranium_C", "Non-Fissile Uranium", "Fuel & Nuclear"),
    ("/Game/FactoryGame/Resource/Parts/PlutoniumPellet/Desc_PlutoniumPellet.Desc_PlutoniumPellet_C", "Plutonium Pellet", "Fuel & Nuclear"),
    ("/Game/FactoryGame/Resource/Parts/PlutoniumCell/Desc_PlutoniumCell.Desc_PlutoniumCell_C", "Encased Plutonium Cell", "Fuel & Nuclear"),
    ("/Game/FactoryGame/Resource/Parts/PlutoniumFuelRod/Desc_PlutoniumFuelRod.Desc_PlutoniumFuelRod_C", "Plutonium Fuel Rod", "Fuel & Nuclear"),
    ("/Game/FactoryGame/Resource/Parts/PlutoniumWaste/Desc_PlutoniumWaste.Desc_PlutoniumWaste_C", "Plutonium Waste", "Fuel & Nuclear"),
    ("/Game/FactoryGame/Resource/Parts/Ficsonium/Desc_Ficsonium.Desc_Ficsonium_C", "Ficsonium", "Fuel & Nuclear"),
    ("/Game/FactoryGame/Resource/Parts/FicsoniumFuelRod/Desc_FicsoniumFuelRod.Desc_FicsoniumFuelRod_C", "Ficsonium Fuel Rod", "Fuel & Nuclear"),

    // --- Vanilla Equipment, Ammo & Special ---
    ("/Game/FactoryGame/Resource/Equipment/PortableMiner/BP_EquipmentDescriptorPortableMiner.BP_EquipmentDescriptorPortableMiner_C", "Portable Miner", "Equipment"),
    ("/Game/FactoryGame/Resource/Equipment/JetPack/BP_EquipmentDescriptorJetPack.BP_EquipmentDescriptorJetPack_C", "Jetpack", "Equipment"),
    ("/Game/FactoryGame/Resource/Equipment/HoverPack/BP_EquipmentDescriptorHoverPack.BP_EquipmentDescriptorHoverPack_C", "Hoverpack", "Equipment"),
    ("/Game/FactoryGame/Resource/Equipment/Parachute/BP_EquipmentDescriptorParachute.BP_EquipmentDescriptorParachute_C", "Parachute", "Equipment"),
    ("/Game/FactoryGame/Resource/Equipment/JumpingStilts/BP_EquipmentDescriptorJumpingStilts.BP_EquipmentDescriptorJumpingStilts_C", "Blade Runners", "Equipment"),
    ("/Game/FactoryGame/Resource/Equipment/GasMask/BP_EquipmentDescriptorGasMask.BP_EquipmentDescriptorGasMask_C", "Gas Mask", "Equipment"),
    ("/Game/FactoryGame/Resource/Equipment/HazmatSuit/BP_EquipmentDescriptorHazmatSuit.BP_EquipmentDescriptorHazmatSuit_C", "Hazmat Suit", "Equipment"),
    ("/Game/FactoryGame/Resource/Equipment/StunSpear/BP_EquipmentDescriptorStunSpear.BP_EquipmentDescriptorStunSpear_C", "Xeno-Basher", "Equipment"),
    ("/Game/FactoryGame/Resource/Equipment/ShockShank/BP_EquipmentDescriptorShockShank.BP_EquipmentDescriptorShockShank_C", "Xeno-Zapper", "Equipment"),
    ("/Game/FactoryGame/Resource/Equipment/Chainsaw/BP_EquipmentDescriptorChainsaw.BP_EquipmentDescriptorChainsaw_C", "Chainsaw", "Equipment"),
    ("/Game/FactoryGame/Resource/Equipment/Rifle/BP_EquipmentDescriptorRifle.BP_EquipmentDescriptorRifle_C", "Rifle", "Equipment"),
    ("/Game/FactoryGame/Resource/Equipment/NailGun/BP_EquipmentDescriptorNailGun.BP_EquipmentDescriptorNailGun_C", "Rebar Gun", "Equipment"),
    ("/Game/FactoryGame/Resource/Equipment/NobeliskDetonator/BP_EquipmentDescriptorNobeliskDetonator.BP_EquipmentDescriptorNobeliskDetonator_C", "Nobelisk Detonator", "Equipment"),
    ("/Game/FactoryGame/Resource/Equipment/Zipline/BP_EquipmentDescriptorZipline.BP_EquipmentDescriptorZipline_C", "Zipline", "Equipment"),
    ("/Game/FactoryGame/Resource/Equipment/ObjectScanner/BP_EquipmentDescriptorObjectScanner.BP_EquipmentDescriptorObjectScanner_C", "Object Scanner", "Equipment"),
    ("/Game/FactoryGame/Resource/Equipment/GasMask/Desc_Filter.Desc_Filter_C", "Gas Filter", "Equipment"),
    ("/Game/FactoryGame/Resource/Equipment/HazmatSuit/Desc_HazmatFilter.Desc_HazmatFilter_C", "Iodine-Infused Filter", "Equipment"),
    ("/Game/FactoryGame/Resource/Equipment/Medkit/Desc_Medkit.Desc_Medkit_C", "Medicinal Inhaler", "Equipment"),
    ("/Game/FactoryGame/Resource/Equipment/NailGun/Desc_SpikedRebar.Desc_SpikedRebar_C", "Iron Rebar", "Equipment"),
    ("/Game/FactoryGame/Resource/Equipment/Rifle/Desc_CartridgeStandard.Desc_CartridgeStandard_C", "Rifle Ammo", "Equipment"),
    ("/Game/FactoryGame/Resource/Equipment/NobeliskDetonator/Desc_NobeliskExplosive.Desc_NobeliskExplosive_C", "Nobelisk", "Equipment"),
    ("/Game/FactoryGame/Resource/Environment/Crystal/Desc_CrystalShard.Desc_CrystalShard_C", "Power Shard", "Special"),
    ("/Game/FactoryGame/Resource/Environment/Crystal/Desc_Crystal.Desc_Crystal_C", "Blue Power Slug", "Special"),
    ("/Game/FactoryGame/Resource/Environment/Crystal/Desc_Crystal_mk2.Desc_Crystal_mk2_C", "Yellow Power Slug", "Special"),
    ("/Game/FactoryGame/Resource/Environment/Crystal/Desc_Crystal_mk3.Desc_Crystal_mk3_C", "Purple Power Slug", "Special"),
    ("/Game/FactoryGame/Prototype/WAT/Desc_WAT1.Desc_WAT1_C", "Somersloop", "Special"),
    ("/Game/FactoryGame/Prototype/WAT/Desc_WAT2.Desc_WAT2_C", "Mercer Sphere", "Special"),
    ("/Game/FactoryGame/Resource/Parts/HardDrive/Desc_HardDrive.Desc_HardDrive_C", "Hard Drive", "Special"),
    ("/Game/FactoryGame/Resource/Parts/ResourceSinkCoupon/Desc_ResourceSinkCoupon.Desc_ResourceSinkCoupon_C", "FICSIT Coupon", "Special"),

    // --- Satisfactory Plus / KLib ---
    ("/KLib/Assets/Parts/Raw/Desc_Salt.Desc_Salt_C", "Salt", "Ore"),
    ("/KLib/Assets/Parts/Raw/Desc_OreLead.Desc_OreLead_C", "Lead Ore", "Ore"),
    ("/KLib/Assets/Parts/Raw/Desc_OreTin.Desc_OreTin_C", "Tin Ore", "Ore"),
    ("/KLib/Assets/Parts/Raw/Desc_OreZinc.Desc_OreZinc_C", "Zinc Ore", "Ore"),
    ("/KLib/Assets/Parts/Raw/Desc_OreNickel.Desc_OreNickel_C", "Nickel Ore", "Ore"),
    ("/KLib/Assets/Parts/Raw/Desc_OreSilver.Desc_OreSilver_C", "Silver Ore", "Ore"),
    ("/KLib/Assets/Parts/Raw/Desc_OrePlatinum.Desc_OrePlatinum_C", "Platinum Ore", "Ore"),
    ("/KLib/Assets/Parts/Crushed/Desc_CrushedIron.Desc_CrushedIron_C", "Crushed Iron", "Ore"),
    ("/KLib/Assets/Parts/Crushed/Desc_CrushedCopper.Desc_CrushedCopper_C", "Crushed Copper", "Ore"),
    ("/KLib/Assets/Parts/Crushed/Desc_CrushedCoal.Desc_CrushedCoal_C", "Crushed Coal", "Fuel & Nuclear"),
    ("/KLib/Assets/Parts/Crushed/Desc_CrushedCaterium.Desc_CrushedCaterium_C", "Crushed Caterium", "Ore"),
    ("/KLib/Assets/Parts/Byproducts/Desc_Slag.Desc_Slag_C", "Slag", "Part"),
    ("/KLib/Assets/Parts/Raw/Desc_Sand.Desc_Sand_C", "Sand", "Part"),
    ("/KLib/Assets/Parts/Production/Desc_Glass.Desc_Glass_C", "Glass", "Part"),
    ("/KLib/Assets/Parts/Ingots/Desc_BrassIngot.Desc_BrassIngot_C", "Brass Ingot", "Ingot"),
    ("/KLib/Assets/Parts/Ingots/Desc_BronzeIngot.Desc_BronzeIngot_C", "Bronze Ingot", "Ingot"),
    ("/KLib/Assets/Parts/Ingots/Desc_TinIngot.Desc_TinIngot_C", "Tin Ingot", "Ingot"),
    ("/KLib/Assets/Parts/Ingots/Desc_ZincIngot.Desc_ZincIngot_C", "Zinc Ingot", "Ingot"),
    ("/KLib/Assets/Parts/Ingots/Desc_LeadIngot.Desc_LeadIngot_C", "Lead Ingot", "Ingot"),
    ("/KLib/Assets/Parts/Ingots/Desc_NickelIngot.Desc_NickelIngot_C", "Nickel Ingot", "Ingot"),
    ("/SatisfactoryPlus/Resource/Desc_LiquidBioWater.Desc_LiquidBioWater_C", "Bio Water", "Fluid"),
    ("/SatisfactoryPlus/Resource/Desc_AlgalMass.Desc_AlgalMass_C", "Algal Mass", "Farming"),

    // --- Refined Power ---
    ("/RefinedPower/World/ResourceNodes/Thorium/Desc_RP_Thorium.Desc_RP_Thorium_C", "Thorium", "Fuel & Nuclear"),
    ("/RefinedPower/Fluids/Desc_RP_Steam.Desc_RP_Steam_C", "Steam", "Fluid"),
    ("/RefinedPower/Fluids/Desc_RP_CO2.Desc_RP_CO2_C", "CO2", "Fluid"),

    // --- Ficsit Farming ---
    ("/FicsitFarming/Resource/Desc_Dirt.Desc_Dirt_C", "Dirt", "Farming"),
    ("/FicsitFarming/Resource/Desc_WetDirt.Desc_WetDirt_C", "Wet Dirt", "Farming"),
    ("/FicsitFarming/Resource/Desc_FertilizedDirt.Desc_FertilizedDirt_C", "Fertilized Dirt", "Farming"),
    ("/FicsitFarming/Resource/Desc_Loam.Desc_Loam_C", "Loam", "Farming"),
    ("/FicsitFarming/Resource/Desc_Peat.Desc_Peat_C", "Peat", "Farming"),
];

/// Returns every built-in base game and supported mod item in the registry.
pub fn all_known_items() -> Vec<KnownItemEntry> {
    STATIC_ITEMS
        .iter()
        .map(|&(path, display_name, category)| {
            let class_name = path.rsplit('.').next().unwrap_or(path);
            KnownItemEntry {
                item_path: path.to_string(),
                display_name: display_name.to_string(),
                category: category.to_string(),
                mod_source: detect_item_mod_source(path, class_name),
            }
        })
        .collect()
}

/// Looks up metadata for a Satisfactory item descriptor `item_path`.
/// Falls back to dynamic name/category/mod derivation for any modded item not in the static table.
pub fn lookup_item_metadata(item_path: &str) -> ItemMetadata {
    let class_name = item_path.rsplit('.').next().unwrap_or(item_path);
    let mod_source = detect_item_mod_source(item_path, class_name);

    if let Some((_, display_name, category)) = STATIC_ITEMS
        .iter()
        .find(|&&(p, _, _)| p == item_path || p.rsplit('.').next() == Some(class_name))
    {
        return ItemMetadata {
            display_name: (*display_name).to_string(),
            category: (*category).to_string(),
            mod_source,
            is_known: true,
        };
    }

    ItemMetadata {
        display_name: humanize_item_class(class_name),
        category: infer_item_category(item_path, class_name).to_string(),
        mod_source,
        is_known: false,
    }
}

fn detect_item_mod_source(item_path: &str, class_name: &str) -> String {
    if item_path.contains("/KLib/")
        || item_path.contains("/SatisfactoryPlus/")
        || item_path.contains("/AB_FluidExtras/")
        || class_name.starts_with("Desc_KLib_")
        || class_name.starts_with("KLib_")
    {
        "SatisfactoryPlus".to_string()
    } else if item_path.contains("/RefinedPower/") || class_name.starts_with("Desc_RP_") {
        "RefinedPower".to_string()
    } else if item_path.contains("/FicsitFarming/") || class_name.starts_with("Desc_FF_") {
        "FicsitFarming".to_string()
    } else if item_path.contains("/MkPlusPlus/") || item_path.contains("/Mk22k20/") {
        "MkPlusPlus".to_string()
    } else if item_path.starts_with("/Game/") {
        "Vanilla".to_string()
    } else if let Some(rest) = item_path.strip_prefix('/') {
        rest.split('/').next().unwrap_or("Modded").to_string()
    } else {
        "Unknown".to_string()
    }
}

pub fn humanize_item_class(class_name: &str) -> String {
    let mut s = class_name.strip_suffix("_C").unwrap_or(class_name);
    for prefix in [
        "Desc_RP_",
        "Desc_FF_",
        "Desc_KLib_",
        "Desc_",
        "BP_ItemDescriptor",
        "BP_",
        "Item_",
        "KLib_",
    ] {
        if let Some(stripped) = s.strip_prefix(prefix) {
            s = stripped;
            break;
        }
    }

    let mut out = String::with_capacity(s.len() + 8);
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c == '_' {
            if !out.ends_with(' ') {
                out.push(' ');
            }
            continue;
        }
        if i > 0 && c.is_uppercase() {
            let prev = chars[i - 1];
            let next_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
            if prev.is_lowercase() || prev.is_ascii_digit() || (prev.is_uppercase() && next_lower) {
                if !out.ends_with(' ') {
                    out.push(' ');
                }
            }
        }
        out.push(c);
    }
    let trimmed = out.trim();
    if trimmed.is_empty() {
        class_name.to_string()
    } else {
        trimmed.to_string()
    }
}

fn infer_item_category(item_path: &str, class_name: &str) -> &'static str {
    let lower = class_name.to_ascii_lowercase();
    let path_lower = item_path.to_ascii_lowercase();

    if lower.contains("ore") || lower.contains("crushed") || path_lower.contains("/ores/") {
        "Ore"
    } else if lower.contains("ingot") || path_lower.contains("/ingots/") {
        "Ingot"
    } else if lower.contains("liquid")
        || lower.contains("fluid")
        || lower.contains("gas")
        || lower.contains("acid")
        || lower.contains("water")
        || lower.contains("oil")
        || lower.contains("slurry")
        || lower.contains("steam")
        || path_lower.contains("/fluids/")
    {
        "Fluid"
    } else if lower.contains("seed")
        || lower.contains("crop")
        || lower.contains("dirt")
        || lower.contains("loam")
        || lower.contains("peat")
        || lower.contains("fertiliz")
        || lower.contains("alga")
        || lower.contains("algie")
        || lower.contains("food")
        || path_lower.contains("ficsitfarming")
    {
        "Farming"
    } else if lower.contains("fuel")
        || lower.contains("coal")
        || lower.contains("uranium")
        || lower.contains("plutonium")
        || lower.contains("ficsonium")
        || lower.contains("battery")
        || lower.contains("waste")
        || lower.contains("thorium")
    {
        "Fuel & Nuclear"
    } else if lower.contains("jetpack")
        || lower.contains("hoverpack")
        || lower.contains("gun")
        || lower.contains("rifle")
        || lower.contains("rebar")
        || lower.contains("nobelisk")
        || lower.contains("chainsaw")
        || lower.contains("gasmask")
        || lower.contains("hazmat")
        || lower.contains("parachute")
        || lower.contains("bladerunners")
        || lower.contains("medkit")
        || lower.contains("zipline")
        || lower.contains("xenozapper")
        || lower.contains("xenobasher")
        || path_lower.contains("/equipment/")
    {
        "Equipment"
    } else if lower.contains("shard")
        || lower.contains("somersloop")
        || lower.contains("mercer")
        || lower.contains("harddrive")
        || lower.contains("coupon")
        || lower.contains("slug")
    {
        "Special"
    } else {
        "Part"
    }
}
