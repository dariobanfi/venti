#!/usr/bin/env bash
# Gathers what the app takes from the owner's design pack (design/, at the
# repository's root) into an asset catalogue: the app icon, the flying icons,
# the logo and the valley art. The catalogue is generated and not committed:
# the pack is the one copy of every file. Run it before `xcodegen generate`,
# and again when the pack changes.
set -euo pipefail

root="$(cd "$(dirname "$0")/../../.." && pwd)"
pack="$root/design/assets"
catalogue="$root/apps/ios/Venti/Resources/Design.xcassets"

rm -rf "$catalogue"
mkdir -p "$catalogue"
cat > "$catalogue/Contents.json" <<'JSON'
{ "info" : { "author" : "xcode", "version" : 1 } }
JSON

# The app icon, as the pack has it.
cp -R "$pack/app-icon/ios/AppIcon.appiconset" "$catalogue/AppIcon.appiconset"

# One image set from one file.
#   $1  the set's name, by which the app asks for it
#   $2  the file
#   $3  "template" for a shape that takes the colour of what is around it,
#       "original" for a picture with colours of its own
image() {
  local name="$1" file="$2" intent="$3"
  local set="$catalogue/$name.imageset"
  mkdir -p "$set"
  cp "$file" "$set/"
  cat > "$set/Contents.json" <<JSON
{
  "images" : [ { "filename" : "$(basename "$file")", "idiom" : "universal" } ],
  "info" : { "author" : "xcode", "version" : 1 },
  "properties" : {
    "preserves-vector-representation" : true,
    "template-rendering-intent" : "$intent"
  }
}
JSON
}

# The flying icons: "icon-wind", "icon-thermal", and so on.
for file in "$pack"/icons/*.svg; do
  name="$(basename "$file" .svg)"
  [ "$name" = sprite ] && continue
  image "icon-$name" "$file" template
done

# The logo.
image "logo-mark" "$pack/logo/mark.svg" original
image "logo-mark-small" "$pack/logo/mark-small.svg" original
image "logo-mark-mono" "$pack/logo/mark-mono-current.svg" template
image "logo-lockup-on-light" "$pack/logo/lockup-on-light.svg" original
image "logo-lockup-on-dark" "$pack/logo/lockup-on-dark.svg" original

# The valley art, for arrival moments and empty states.
image "art-valley-portrait-empty" "$pack/illustration/valley-portrait-empty.svg" original
image "art-valley-wide-empty" "$pack/illustration/valley-wide-empty.svg" original
image "art-valley-strip" "$pack/illustration/valley-strip.svg" original
image "art-hero-portrait" "$pack/illustration/hero-portrait.svg" original

echo "design: $(find "$catalogue" -name '*.imageset' | wc -l | tr -d ' ') image sets and the app icon in $catalogue"
