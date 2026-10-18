#!/bin/sh
# Converts the renders of render.py and room.py to the WebP shipped in the app: bases at 320 px
# (about 2x their on-screen size), glow passes at 192 px (blurred in the app anyway), rooms at
# 1000 px wide. Run from app/.
set -e
# The contact shadow is very soft: fade it out towards the edges so no square shows on the grid.
FADE="geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='if(lt(alpha(X,Y),10),0,alpha(X,Y))*min(1,min(min(X,W-1-X),min(Y,H-1-Y))/(W*0.3))'"
for png in assets/models/*/*.png assets/rooms/*/*.png; do
  [ -e "$png" ] || continue
  case "$png" in
    assets/rooms/*) size=1000:-1; q=82; fade=""; script=room.py ;;
    */base_*) size=320:320; q=84; fade=",$FADE"; script=render.py ;;
    *) size=192:192; q=72; fade=""; script=render.py ;;
  esac
  ffmpeg -y -loglevel error -i "$png" -vf "format=rgba$fade,scale=$size:flags=lanczos" -c:v libwebp -quality $q -pix_fmt yuva420p "${png%.png}.webp"
  rm "$png"
  # Provenance inside the file: what made it (no third-party assets involved).
  model=$(basename "$(dirname "$png")"); view=$(basename "${png%.png}")
  "${IMPECCABLE:-impeccable}" embed-prompt "${png%.png}.webp" --prompt "Origin: rendered by app/tools/render/$script (Blender 5.2.2 Cycles), $model, $view; geometry and materials authored for HomeMaid, no third-party assets." >/dev/null
done
du -sh assets/models assets/rooms
