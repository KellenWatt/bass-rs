#!/bin/bash

file="$1"
icon_folder="bass.iconset"

if [[ ! -f "$file" ]] ; then
    >&2 echo "No file provided for conversion"
    exit 1
fi

if [[ ! -d "$icon_folder" ]] ; then
    mkdir "$icon_folder"
fi

inkscape -w 16 -h 16 "$1" -o $icon_folder/icon_16x16.png
inkscape -w 32 -h 32 "$1" -o $icon_folder/icon_32x32.png
inkscape -w 64 -h 64 "$1" -o $icon_folder/icon_64x64.png
inkscape -w 128 -h 128 "$1" -o $icon_folder/icon_128x128.png
inkscape -w 256 -h 256 "$1" -o $icon_folder/icon_256x256.png
inkscape -w 512 -h 512 "$1" -o $icon_folder/icon_512x512.png
inkscape -w 1024 -h 1024 "$1" -o $icon_folder/icon_1024x1024.png
inkscape -w 32 -h 32 "$1" -o $icon_folder/icon_16x16@2x.png
inkscape -w 64 -h 64 "$1" -o $icon_folder/icon_32x32@2x.png
inkscape -w 128 -h 128 "$1" -o $icon_folder/icon_64x64@2x.png
inkscape -w 256 -h 256 "$1" -o $icon_folder/icon_128x128@2x.png
inkscape -w 512 -h 512 "$1" -o $icon_folder/icon_256x256@2x.png
inkscape -w 1024 -h 1024 "$1" -o $icon_folder/icon_512x512@2x.png
inkscape -w 2048 -h 2048 "$1" -o $icon_folder/icon_1024x1024@2x.png

iconutil -c icns -o bass.icns $icon_folder
