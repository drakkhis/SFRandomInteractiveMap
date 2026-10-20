# Satisfactory RWG Interactive Map (`sat-rwg-map`)

A fast, standalone interactive web map and `.sav` parser built in Rust for **Satisfactory** with full support for the **RandomWorldGenerator (RWG)** procedural world mod.

## ⚡ Download & Run (Pre-Built Windows Binary — No Rust Needed)

You do **not** need to compile anything from source! A pre-built standalone Windows executable is included right in the [`release/`](./release) folder:

1. Download [`release/SF-Random-Interactive-Map-win64.zip`](./release/SF-Random-Interactive-Map-win64.zip) (or [`release/sat-rwg-map.exe`](./release/sat-rwg-map.exe)).
2. Double-click **`sat-rwg-map.exe`** (or **`Start-Map-Server.bat`**).
3. Your browser will automatically open **`http://127.0.0.1:8080`**.
4. Click **Load Save** in the top bar (or drag & drop any `.sav` file onto the browser window) to dynamically render your save's procedural map and all resource nodes!

---

## ✨ Features

- **Dynamic Procedural Map Generation (`PWEmbeddedWorld` / `PWW1`)**:
  - Extracts the $6000 \times 6000$ heightmap, void/hole mask, ocean mask, inland river/lake water levels, and 21 Satisfactory biome families directly from the loaded `.sav` file.
  - Renders directional hillshading, cliff shading, bathymetry, and biome colors aligned $1:1$ with all world coordinates.
- **Full Modded Resource Node Support**:
  - **Vanilla**: Iron, Copper, Limestone, Coal, Caterium, Raw Quartz, Sulfur, Bauxite, SAM, Uranium, Crude Oil, Nitrogen Gas, Water, Geysers, Somersloops, Mercer Spheres, and Crash Sites (Drop Pods).
  - **Satisfactory Plus (`SF+` / `KLib`)**: Lead, Tin, Nickel, Zinc, Silver, Platinum, Salt, Kerr Crystal, Chlorine Gas, Bio Water, Algie Mass, etc.
  - **Refined Power (`RP`)**: Thorium and Deanium.
  - **Ficsit Farming (`FF`)**: Dirt, Wet Dirt, Fertilized Dirt, Loam, Peat, and Algie Mass.
- **Zero External Dependencies**:
  - Single ~6.6 MB `.exe` with the Leaflet web UI embedded inside the binary.

---

## 🛠️ Building from Source (Optional)

If you want to build from source using Rust:

```powershell
cargo build --release
.\target\release\sat-rwg-map.exe
```
