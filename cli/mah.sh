#!/usr/bin/env sh
# mah (bash/zsh/Git Bash): envoltorio fino de mah.ps1. Requiere PowerShell 7 (pwsh) o Windows PowerShell.
dir=$(cd "$(dirname "$0")" && pwd)
if command -v pwsh >/dev/null 2>&1; then ps=pwsh; else ps=powershell.exe; fi
exec "$ps" -NoProfile -ExecutionPolicy Bypass -File "$dir/mah.ps1" "$@"
