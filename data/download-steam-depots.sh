#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

if [[ -z "${1:-}" ]]; then
  echo "Error: pass steam username as first argument." >&2
  exit 1
fi

username="$1"
appid="1030300"
depot="1030303"

download() {
    local manifest="$1"
    local beta="${2:-}"

    if [ -d "manifests/$manifest" ]; then
        echo "Manifest $manifest already downloaded"
        return
    fi

    local beta_option=""
    if [ -n "$beta" ]; then
        beta_option="-beta $beta"
    fi

    rm "depots/$depot" -fr

    depotdownloader -remember-password -username "$username" -app "$appid" -depot "$depot" -manifest "$manifest" $beta_option -manifest-only
    depotdownloader -remember-password -username "$username" -app "$appid" -depot "$depot" -manifest "$manifest" $beta_option

    mkdir -p "manifests/$manifest"
    mv "depots/$depot"/*/* "manifests/$manifest"
    rm "depots/$depot" -fr
}

# https://steamdb.info/depot/1030303/manifests
download 8384590172287463475
# download 5917697910613228100 public-beta
# download 6701825740120558137 public-beta
download 6701825740120558137
download 6373658714389144408
download 2356039248449654760 public-beta
download 7495630131038458486
download 1617544312110692774
