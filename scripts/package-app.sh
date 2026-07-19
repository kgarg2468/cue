#!/bin/zsh
# Packages the release CaptureDelegateApp binary into a signed .app bundle.
# Usage: scripts/package-app.sh [output-dir]   (default: .context)
set -euo pipefail

repository_root="${0:A:h:h}"
output_dir="${1:-$repository_root/.context}"
app_bundle="$output_dir/Capture Delegate.app"
contents="$app_bundle/Contents"

cd "$repository_root"
swift build -c release --disable-sandbox

rm -rf "$app_bundle"
mkdir -p "$contents/MacOS" "$contents/Resources"
ditto ".build/release/CaptureDelegateApp" "$contents/MacOS/CaptureDelegateApp"
chmod +x "$contents/MacOS/CaptureDelegateApp"

/usr/libexec/PlistBuddy "$contents/Info.plist" \
  -c "Add :CFBundleName string 'Capture Delegate'" \
  -c "Add :CFBundleDisplayName string 'Capture Delegate'" \
  -c "Add :CFBundleExecutable string CaptureDelegateApp" \
  -c "Add :CFBundleIdentifier string com.capturedelegate.app" \
  -c "Add :CFBundlePackageType string APPL" \
  -c "Add :CFBundleShortVersionString string 0.2" \
  -c "Add :CFBundleVersion string 2" \
  -c "Add :LSMinimumSystemVersion string 14.0" \
  -c "Add :NSHighResolutionCapable bool true"

# plutil, not PlistBuddy: PlistBuddy's -c parser cannot carry an apostrophe.
plutil -replace NSMicrophoneUsageDescription \
  -string "Capture Delegate records audio from this Mac's microphone only when you start a capture. Recordings stay on this Mac, encrypted." \
  "$contents/Info.plist"

# Ad-hoc signature so macOS privacy (TCC) attribution stays stable across rebuilds.
codesign --force --sign - --identifier com.capturedelegate.app "$app_bundle"
codesign --verify "$app_bundle"

print -- "Packaged: $app_bundle"
