===================================================================
 Satisfactory RWG Interactive Map (Pre-Built Windows Release)
===================================================================

Quick Start (Self-Contained Standalone App):
-------------------------------------------
1. Double-click `sat-rwg-map.exe`.
2. A clean native application window opens immediately (powered by
   Windows built-in WebView2) with zero command-prompt console window!
3. Click "Load Save" (or drag-and-drop any Satisfactory `.sav` file
   anywhere onto the window) to parse the save and dynamically
   generate the aligned procedural landmass map and resource nodes!

Advanced / Headless Modes:
--------------------------
- Double-clicking `sat-rwg-map.exe` opens the native desktop window.
- Running `sat-rwg-map.exe --server` runs headless in console mode for
  remote access or external browser hosting.
- Running `sat-rwg-map.exe --cli <path-to-save>` generates a JSON export
  directly in your terminal.

Supported Mods & Features:
--------------------------
- Dynamic Procedural World (`RandomWorldGenerator` / `PWEmbeddedWorld`)
  terrain, biome coloring, hillshading, inland rivers/lakes, and coastline
  extracted directly from the `.sav` file.
- Full 2D Leaflet interactive map and 3D WebGL Three.js terrain viewer.
- Interactive Node Relocation: Click any resource node to move it, match
  ground slope with the "Ground" button, fine-tune Pitch / Roll / Yaw tilt,
  or automatically sync satellite resource nodes in Fracking clusters!
- Vanilla Satisfactory resource nodes, Geysers, Fracking Wells,
  Somersloops, Mercer Spheres, Crash Sites (Drop Pods), and Pioneer location.
- Satisfactory Plus (`SF+` / `KLib`) ores, crystals, and wells.
- Refined Power (`RP`) Thorium & Deanium nodes.
- Ficsit Farming (`FF`) Dirt, Wet Dirt, Fertilized Dirt, Loam, Peat,
  and Algie Mass nodes.
