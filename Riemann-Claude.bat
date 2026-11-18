@echo off
rem Starts Claude Code in this folder through Riemann-Claude.ps1 (double-click).
pwsh -NoProfile -ExecutionPolicy Bypass -File "%~dp0Riemann-Claude.ps1" 2>nul || powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0Riemann-Claude.ps1"
