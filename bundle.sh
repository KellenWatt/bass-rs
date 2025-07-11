#!/bin/bash


app_dir="Bass.app"
backup="$app_dir~"

icon_dir="bass-ui/ui/images"
icons="$icon_dir/bass.icns"

error() {
    if [[ $# > 0 ]]; then
        >&2 echo $@
    fi
    if [[ -d "$app_dir" ]]; then
        rm -rf "$app_dir"
    fi
    if [[ -d "$backup" ]]; then
        mv "$backup" "$app_dir"
    fi
    exit 1
}


error_if() {
    if [[ $? != 0 ]]; then
        error $@
    fi
}

attempt() {
    $@
    error_if
}

if [[ -d "$app_dir" ]]; then
    echo "Found existing $app_dir. Archiving."
    if [[ -e "$backup" ]]; then
        attempt rm -rf "$backup"
    fi
    mv "$app_dir" "$backup"
fi

# trap "rm -rf $app_dir && test -e $backup && mv $backup $app_dir" EXIT


echo "Creating app file structure"
mkdir "$app_dir"
error_if "Could not create $app_dir. Aborting"

contents="$app_dir/Contents"

mkdir -p "$contents/MacOS"
mkdir "$contents/Resources"

echo "Writing Info.plist"
cat > "$contents/Info.plist" << END
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
        <string>Bass</string>
    <key>CFBundleIconFile</key>
        <string>bass.icns</string>
    <key>CFBundleIdentifier</key>
        <string>dev.wattsoft.bass</string>
    <key>NSHighResolutionCapable</key>
        <true/>
</dict>
</plist>
END

if [[ ! -f "$icons" ]]; then
    echo "Icon set not found. Generating new icon set"
    cd "$icon_dir"
    ./generate-icons.sh bass-db-icon-square.svg
    error_if "Something went wrong generting icons"
    cd $OLDPWD
else
    echo "Using existing icon set"
fi

attempt cp "$icons" "$contents/Resources/"

attempt cargo build --release
cp "target/release/Bass" "$contents/MacOS/"

cp -r "$app_dir" "build"
rm -r "$app_dir"

echo "$app_dir created."
rm -rf "$backup"

cd build

template="dmg-src"
mkdir dmg-src
cp -r $app_dir $template

dmg="bass.dmg"

create-dmg \
    --background ../bass-ui/ui/images/dmg-background.png \
    --volname Bass \
    --volicon ../bass-ui/ui/images/bass.icns \
    --window-size 725 465 \
    --window-pos 500 400 \
    --icon-size 64 \
    --icon Bass.app 185 205 \
    --app-drop-link 525 205 \
    bass~.dmg \
    $template

mv bass~.dmg "$dmg"
rm -r $template

