username="jjakobh"

download() {
    local manifest="$1"
    local beta="${2:-}"

    local beta_option=""
    if [ -n "$beta" ]; then
        beta_option="-beta $beta"
    fi

    depotdownloader -remember-password -username "$username" -app 1030300 -depot 1030303 -manifest "$manifest" $beta_option -manifest-only
    depotdownloader -remember-password -username "$username" -app 1030300 -depot 1030303 -manifest "$manifest" $beta_option
}

# https://steamdb.info/depot/1030303/manifests
# download 8384590172287463475
# download 5917697910613228100 public-beta
# download 6701825740120558137 public-beta
download 6701825740120558137
