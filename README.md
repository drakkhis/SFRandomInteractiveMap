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

### Windows

```powershell
cargo build --release
.\target\release\sat-rwg-map.exe
```

---

## 🐧 Linux Support

`sat-rwg-map` supports Linux natively on both Wayland and X11 using GTK3 + WebKitGTK, and also offers a lightweight headless/browser mode without GUI libraries.

Pre-built Linux `x86_64` archives are published on GitHub Releases:
- `SF-Random-Interactive-Map-linux-x86_64.tar.gz` — Native desktop window (extract and run `./sat-rwg-map`, or `./linux/install.sh` to install into `~/.local/bin` and register the `.desktop` entry).
- `SF-Random-Interactive-Map-linux-x86_64-headless.tar.gz` — Pure-Rust headless server / browser / CLI binary with zero GTK or WebKitGTK dependencies.

### 1. System Dependencies (for Native Desktop Window)

- **Arch / CachyOS**:
  ```bash
  sudo pacman -S gtk3 webkit2gtk-4.1
  ```
- **Debian / Ubuntu**:
  ```bash
  sudo apt install libgtk-3-dev libwebkit2gtk-4.1-dev
  ```
- **Fedora**:
  ```bash
  sudo dnf install gtk3-devel webkit2gtk4.1-devel
  ```

### 2. Build & Run on Linux

- **Native desktop window (Wayland & X11)**:
  ```bash
  cargo build --release
  ./target/release/sat-rwg-map
  ```
- **Headless server-only build (no GTK/WebKitGTK required)**:
  ```bash
  cargo build --release --no-default-features
  ./target/release/sat-rwg-map --server
  ```

### 3. CLI Modes

- `./target/release/sat-rwg-map` — Launches the native desktop window (auto-applies `WEBKIT_DISABLE_DMABUF_RENDERER=1` on NVIDIA systems unless explicitly set).
- `./target/release/sat-rwg-map --server [--port 8080]` — Runs headless HTTP server only.
- `./target/release/sat-rwg-map --browser [--port 8080]` — Starts the local server and opens it in your default browser via `xdg-open`.
- `./target/release/sat-rwg-map --cli [path/to/save.sav]` — Parses a save file and writes `rwg_actors.json`.

### 4. Automatic Steam / Proton / Heroic / Wine Detection

Save-to-game, Blueprint export, and Mod folder scanning automatically inspect:
- Steam libraries (`libraryfolders.vdf`) and Proton prefix `526870` (`compatdata/526870/pfx/...`)
- Heroic Games Launcher prefixes (`~/Games/Heroic/Prefixes/...`) & game installs
- `$WINEPREFIX` / `~/.wine` and `~/.config/Epic/FactoryGame/Saved/SaveGames`

You can also override discovery manually via environment variables:
- `SATISFACTORY_SAVE_DIR=/path/to/SaveGames/or/profile`
- `SATISFACTORY_MODS_DIR=/path/to/Satisfactory/FactoryGame/Mods`

### 5. Desktop Launcher Installation

To install the binary into `~/.local/bin` and register a desktop application entry:

```bash
./linux/install.sh
```
