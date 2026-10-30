@echo off
setlocal
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
for /f "usebackq tokens=*" %%i in (`"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "VSPATH=%%i"
if not defined VSPATH (
  echo [build] Visual Studio with C++ tools not found.
  exit /b 1
)
call "%VSPATH%\VC\Auxiliary\Build\vcvars64.bat" >nul
if errorlevel 1 exit /b 1
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
cd /d "%~dp0.."
if /i "%~1"=="check" (
  cargo check --manifest-path src-tauri\Cargo.toml
  if errorlevel 1 exit /b 1
  exit /b 0
)
if /i "%~1"=="release" (
  call npm run tauri build -- --bundles nsis
  if errorlevel 1 exit /b 1
  exit /b 0
)
call npm run tauri build -- --debug --no-bundle
if errorlevel 1 exit /b 1
exit /b 0
