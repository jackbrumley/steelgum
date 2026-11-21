set -o pipefail
set -u

printf 'Steelgum updater starting for %s\n' "$STEELGUM_UPDATE_TAG"
: > "$STEELGUM_UPDATE_READY" || exit 1
deadline=$((SECONDS + 20))
while kill -0 "$STEELGUM_UPDATE_PARENT" 2>/dev/null; do
    if (( SECONDS >= deadline )); then
        printf 'Steelgum did not exit; installation was cancelled.\n' >&2
        exit 1
    fi
    sleep 0.2
done

if script=$(mktemp -t steelgum-update.XXXXXXXX) &&
    curl --proto '=https' --tlsv1.2 --fail --silent --show-error \
      --connect-timeout 10 --max-time 60 --output "$script" "$STEELGUM_UPDATE_SITE/install.sh" &&
    [[ -s "$script" ]] &&
    bash "$script" "${STEELGUM_UPDATE_MODE:---appimage}" --version "$STEELGUM_UPDATE_TAG" --yes --relaunch; then
    rm -f -- "$script"
    exit 0
fi
if [[ -n "$script" ]]; then rm -f -- "$script"; fi
printf 'Installation failed. Relaunching the installed Steelgum.\n' >&2
exec "$STEELGUM_UPDATE_EXECUTABLE" --update-failed
