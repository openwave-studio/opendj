#!/usr/bin/env bash
# Xcode runs this after its (empty) compile step: it builds the Rust binary
# named in $1 for each architecture Xcode asks for, and puts it where Xcode
# expects the app's executable. Adapted from Slint's iOS guide.
set -euvx

export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:$PATH"

# Xcode's Debug builds use the workspace's `fast` profile: unoptimised
# Rust draws the waveforms too slowly to scroll smoothly.
if [[ "${CONFIGURATION:-Release}" == "Debug" ]]; then
    PROFILE=fast
else
    PROFILE=release
fi
# Symbols, for a dSYM and readable crash reports.
export CARGO_PROFILE_RELEASE_DEBUG="${CARGO_PROFILE_RELEASE_DEBUG:-1}"
export CARGO_PROFILE_FAST_DEBUG="${CARGO_PROFILE_FAST_DEBUG:-1}"
# Keep Xcode's builds apart from the desktop ones in target/.
export CARGO_TARGET_DIR="$DERIVED_FILE_DIR/cargo"

EXECUTABLES=()
for ARCH in $ARCHS; do
    case "$ARCH" in
        arm64)
            if [[ "${PLATFORM_NAME:-}" == "iphonesimulator" ]]; then
                TARGET=aarch64-apple-ios-sim
            else
                TARGET=aarch64-apple-ios
            fi
            ;;
        x86_64) TARGET=x86_64-apple-ios ;;
        *) echo "unsupported architecture $ARCH" >&2; exit 1 ;;
    esac
    cargo build --profile "$PROFILE" --target "$TARGET" --bin "$1" "${@:2}"
    EXECUTABLES+=("$CARGO_TARGET_DIR/$TARGET/$PROFILE/$1")
done

lipo -create -output "$TARGET_BUILD_DIR/$EXECUTABLE_PATH" "${EXECUTABLES[@]}"

# The supported controllers' mappings (../controllers.txt), in a
# "controllers" folder beside the executable, where openDJ looks for them.
MAPPINGS="$TARGET_BUILD_DIR/$UNLOCALIZED_RESOURCES_FOLDER_PATH/controllers"
rm -rf "$MAPPINGS"
mkdir -p "$MAPPINGS"
grep -v '^#' "$SRCROOT/../controllers.txt" | while IFS= read -r FILE; do
    if [[ -n "$FILE" ]]; then
        cp "$SRCROOT/../../../res/controllers/$FILE" "$MAPPINGS/"
    fi
done

if [[ -n "${DWARF_DSYM_FOLDER_PATH:-}" && -n "${DWARF_DSYM_FILE_NAME:-}" ]]; then
    dsymutil "$TARGET_BUILD_DIR/$EXECUTABLE_PATH" -o "$DWARF_DSYM_FOLDER_PATH/$DWARF_DSYM_FILE_NAME"
fi

# Xcode signs before this script runs, so sign the new executable again.
if [[ "${PLATFORM_NAME:-}" != "iphonesimulator" && "${CODE_SIGNING_ALLOWED:-NO}" == "YES" && -n "${EXPANDED_CODE_SIGN_IDENTITY:-}" ]]; then
    ENTITLEMENTS="$TARGET_TEMP_DIR/$PRODUCT_NAME.app.xcent"
    if [[ -s "$ENTITLEMENTS" ]]; then
        codesign --force --sign "$EXPANDED_CODE_SIGN_IDENTITY" --entitlements "$ENTITLEMENTS" "$TARGET_BUILD_DIR/$EXECUTABLE_PATH"
    else
        codesign --force --sign "$EXPANDED_CODE_SIGN_IDENTITY" "$TARGET_BUILD_DIR/$EXECUTABLE_PATH"
    fi
fi
