@echo off
rem Construit ACP_Excel.xlsm a partir des modules VBA (necessite Microsoft Excel)
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0construire_classeur.ps1"
pause
