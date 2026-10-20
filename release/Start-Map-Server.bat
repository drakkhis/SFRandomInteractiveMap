@echo off
title Satisfactory RWG Interactive Map - Dragon Forge Creations
echo ===================================================================
echo  Satisfactory RWG Interactive Map (Dragon Forge Creations)
echo ===================================================================
powershell -NoProfile -ExecutionPolicy Bypass -Command "Unblock-File -Path '%~dp0sat-rwg-map.exe' -ErrorAction SilentlyContinue"
echo Starting Map Server on http://127.0.0.1:8080 ...
"%~dp0sat-rwg-map.exe"
pause
