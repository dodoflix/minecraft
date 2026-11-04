#!/usr/bin/env bash
# Builds the gitignored reference material under ref/ (see ARCHITECTURE.md). Idempotent: every step is skipped when
# its output already exists; nothing existing under ref/ is deleted (use --force to regenerate generated/ + extracted/).
#
#   scripts/setup-ref.sh [--decompile] [--libs] [--force] [--no-extract]
#
#   --decompile   decompile the vanilla server jar into ref/src with Vineflower (downloads ref/vineflower.jar if missing)
#   --libs        also decompile brigadier / DataFixerUpper / authlib / ... from the bundle into ref/libs/<name>
#   --force       rerun datagen and the extractor even if their output exists
#   --no-extract  skip the Java data extractor (tools/extractor -> ref/extracted)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REF="$ROOT/ref"
VERSION="26.3"
VINEFLOWER_URL="https://repo1.maven.org/maven2/org/vineflower/vineflower/1.11.1/vineflower-1.11.1.jar"
MANIFEST_URL="https://piston-meta.mojang.com/mc/game/version_manifest_v2.json"

DECOMPILE=0 LIBS=0 FORCE=0 EXTRACT=1
for a in "$@"; do
  case "$a" in
    --decompile) DECOMPILE=1 ;;
    --libs) LIBS=1 ;;
    --force) FORCE=1 ;;
    --no-extract) EXTRACT=0 ;;
    *) echo "unknown option: $a" >&2; exit 2 ;;
  esac
done

for tool in curl unzip jq java javac sha1sum; do
  command -v "$tool" >/dev/null || { echo "missing required tool: $tool" >&2; exit 1; }
done
mkdir -p "$REF"

log() { echo "[setup-ref] $*"; }

download() { # url dest [sha1]
  local url="$1" dest="$2" sha1="${3:-}"
  curl -fL --retry 3 -o "$dest.part" "$url"
  if [[ -n "$sha1" ]]; then
    echo "$sha1  $dest.part" | sha1sum -c --quiet - || { echo "sha1 mismatch for $url" >&2; rm -f "$dest.part"; exit 1; }
  fi
  mv "$dest.part" "$dest"
}

# 1. version json (download URL of the server jar lives in it)
if [[ ! -s "$REF/$VERSION.json" ]]; then
  log "fetching $VERSION version json"
  vurl="$(curl -fsL "$MANIFEST_URL" | jq -r --arg v "$VERSION" '.versions[] | select(.id == $v) | .url')"
  [[ -n "$vurl" ]] || { echo "version $VERSION not in the Mojang manifest" >&2; exit 1; }
  download "$vurl" "$REF/$VERSION.json"
fi

# 2. server bundler jar
if [[ ! -s "$REF/server.jar" ]]; then
  log "downloading server.jar"
  download "$(jq -r '.downloads.server.url' "$REF/$VERSION.json")" "$REF/server.jar" "$(jq -r '.downloads.server.sha1' "$REF/$VERSION.json")"
fi

# 3. unbundle: server-<v>.jar + libraries (this is the classpath of scripts/vanilla-cp.sh)
INNER="$REF/bundle/META-INF/versions/$VERSION/server-$VERSION.jar"
if [[ ! -s "$INNER" ]]; then
  log "unbundling server.jar -> ref/bundle"
  mkdir -p "$REF/bundle"
  unzip -qo "$REF/server.jar" 'META-INF/versions/*' 'META-INF/libraries/*' 'META-INF/versions.list' 'META-INF/libraries.list' -d "$REF/bundle" \
    || unzip -qo "$REF/server.jar" 'META-INF/versions/*' 'META-INF/libraries/*' 'META-INF/versions.list' -d "$REF/bundle"
fi

# 4. ref/src: data pack + assets + version.json (non-class jar entries); decompiled sources only with --decompile
if [[ ! -s "$REF/src/data/minecraft/tags/block/mineable/pickaxe.json" || ! -s "$REF/src/version.json" ]]; then
  log "extracting data pack + assets -> ref/src"
  mkdir -p "$REF/src"
  unzip -qo "$INNER" -x '*.class' -d "$REF/src"
fi

if (( DECOMPILE )); then
  if [[ ! -s "$REF/vineflower.jar" ]]; then
    log "downloading vineflower"
    download "$VINEFLOWER_URL" "$REF/vineflower.jar"
  fi
  if [[ ! -s "$REF/src/net/minecraft/world/level/block/Blocks.java" || $FORCE == 1 ]]; then
    log "decompiling server jar -> ref/src (takes a few minutes)"
    java -jar "$REF/vineflower.jar" -thr="$(nproc)" "$INNER" "$REF/src"
  fi
  if (( LIBS )); then
    mkdir -p "$REF/libs"
    while IFS= read -r jar; do
      name="$(basename "$jar" .jar)"
      case "$name" in
        brigadier-*|datafixerupper-*|authlib-*|serialization-*|jtracy-*) ;;
        *) continue ;;
      esac
      if [[ ! -d "$REF/libs/$name" ]]; then
        log "decompiling $name -> ref/libs/$name"
        mkdir -p "$REF/libs/$name"
        java -jar "$REF/vineflower.jar" -thr="$(nproc)" "$jar" "$REF/libs/$name"
      fi
    done < <(find "$REF/bundle/META-INF/libraries" -name '*.jar' | sort)
  fi
fi

# 5. vanilla data generator reports + data (the real server main with the datagen entry point)
if [[ ! -s "$REF/generated/reports/blocks.json" || $FORCE == 1 ]]; then
  log "running vanilla datagen -> ref/generated"
  java -DbundlerMainClass=net.minecraft.data.Main -jar "$REF/server.jar" --all --output "$REF/generated"
fi

# 6. code-defined data the data generator does not report (tools/extractor -> ref/extracted)
if (( EXTRACT )) && [[ ! -s "$REF/extracted/manifest.json" || $FORCE == 1 ]]; then
  log "running data extractor -> ref/extracted"
  "$ROOT/scripts/extract.sh"
fi

log "done"
