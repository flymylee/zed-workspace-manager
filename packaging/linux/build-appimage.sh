#!/usr/bin/env bash
set -euo pipefail

APP_NAME="zed-workspace-manager"
APP_VERSION=$(grep -m1 '^version' Cargo.toml | cut -d '"' -f 2)
DESK_ID="hk.sahayana.ZedWorkspaceManager"
OUTPUT_FILE="${APP_NAME}-${APP_VERSION}-x86_64.AppImage"

cargo build --release

rm -rf AppDir
mkdir -p AppDir/usr/bin \
         AppDir/usr/share/applications \
         AppDir/usr/share/icons/hicolor/scalable/apps

cp "target/release/${APP_NAME}" AppDir/usr/bin/
cp packaging/linux/AppRun AppDir/AppRun
chmod +x "AppDir/usr/bin/${APP_NAME}" AppDir/AppRun

cp "packaging/linux/${DESK_ID}.desktop" AppDir/
cp "packaging/linux/${DESK_ID}.desktop" AppDir/usr/share/applications/

cp "packaging/linux/${DESK_ID}.svg" AppDir/
cp "packaging/linux/${DESK_ID}.svg" AppDir/usr/share/icons/hicolor/scalable/apps/
ln -sf "${DESK_ID}.svg" AppDir/.DirIcon

rm -f AppDir/"${OUTPUT_FILE}"

ARCH=x86_64 appimagetool-x86_64.AppImage AppDir "${OUTPUT_FILE}"
echo "Built: ${OUTPUT_FILE}"
