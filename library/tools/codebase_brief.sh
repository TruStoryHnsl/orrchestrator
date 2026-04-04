#!/usr/bin/env bash
# codebase_brief.sh — compact codebase snapshot for agent orientation
# Produces a ~200-line summary replacing ~60-70K tokens of redundant file reads.
# Usage: codebase_brief.sh [project_dir]
#   project_dir defaults to the project root two levels above this script.

set -euo pipefail

# ── Resolve project root ────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="${1:-$(cd "$SCRIPT_DIR/../.." && pwd)}"

if [[ ! -d "$PROJECT_ROOT/crates" ]]; then
    echo "ERROR: No crates/ directory found at $PROJECT_ROOT" >&2
    exit 1
fi

# ── Helper: extract all pub symbols from a Rust file using awk ──────────────
# Single-pass awk that tracks brace depth to collect:
#   pub mod, pub struct (+ pub field names), pub enum (+ variant names),
#   pub fn (single + multi-line signatures), pub const/static (name + type)
extract_symbols() {
    local file="$1"
    awk '
    function ltrim(s) { sub(/^[[:space:]]+/, "", s); return s }
    function rtrim(s) { sub(/[[:space:]]+$/, "", s); return s }
    function trim(s)  { return rtrim(ltrim(s)) }

    BEGIN {
        mode = "top"       # top | struct | enum | fn
        depth = 0          # global brace depth
        item_name = ""
        item_fields = ""
        fn_buf = ""
        fn_depth = 0       # brace depth when fn started
    }

    {
        line = $0
    }

    # ── Count braces to track depth ──────────────────────────────────────
    # We count braces AFTER processing the line content, except for fn mode
    # where we need to detect the opening { to know the sig ended.

    # ── pub mod ─────────────────────────────────────────────────────────
    mode == "top" && /^pub mod / {
        match(line, /^pub mod ([A-Za-z_][A-Za-z0-9_]*)/, arr)
        if (arr[1] != "") print "pub mod " arr[1]
    }

    # ── pub struct ──────────────────────────────────────────────────────
    mode == "top" && /^pub struct / {
        match(line, /^pub struct ([A-Za-z_][A-Za-z0-9_]*)/, arr)
        item_name = arr[1]
        item_fields = ""
        if (line ~ /\{/) {
            mode = "struct"
            depth++
        } else if (line ~ /;/) {
            print "pub struct " item_name
            item_name = ""
        }
        # tuple struct (single line with parens, no brace) — skip field collection
        next
    }

    mode == "struct" {
        opens = gsub(/{/, "{", line)
        closes = gsub(/}/, "}", line)
        depth += opens - closes
        # If we just closed the struct
        if (depth <= 0) {
            depth = 0
            mode = "top"
            if (item_fields != "")
                print "pub struct " item_name " { " item_fields " }"
            else
                print "pub struct " item_name
            item_name = ""
            item_fields = ""
            next
        }
        # Collect only direct pub fields (depth == 1 after counting this line)
        if (depth == 1) {
            stripped = trim(line)
            if (match(stripped, /^pub ([a-z_][a-z0-9_]*):/, farr)) {
                f = farr[1]
                item_fields = (item_fields == "") ? f : item_fields ", " f
            }
        }
        next
    }

    # ── pub enum ────────────────────────────────────────────────────────
    mode == "top" && /^pub enum / {
        match(line, /^pub enum ([A-Za-z_][A-Za-z0-9_]*)/, arr)
        item_name = arr[1]
        item_fields = ""
        if (line ~ /\{/) {
            mode = "enum"
            depth++
        }
        next
    }

    mode == "enum" {
        opens = gsub(/{/, "{", line)
        closes = gsub(/}/, "}", line)
        depth += opens - closes
        if (depth <= 0) {
            depth = 0
            mode = "top"
            if (item_fields != "")
                print "pub enum " item_name " { " item_fields " }"
            else
                print "pub enum " item_name
            item_name = ""
            item_fields = ""
            next
        }
        if (depth == 1) {
            stripped = trim(line)
            # Variant: line starts with uppercase letter, is not a comment
            if (match(stripped, /^([A-Z][A-Za-z0-9_]*)/, varr)) {
                v = varr[1]
                item_fields = (item_fields == "") ? v : item_fields ", " v
            }
        }
        next
    }

    # ── pub fn (top-level only) ──────────────────────────────────────────
    mode == "top" && /^pub (async )?fn / {
        fn_buf = line
        fn_depth = depth
        # Check if sig completes on this line
        if (line ~ /\)([[:space:]]*(->|where|\{)|[[:space:]]*$)/) {
            # Emit immediately
            sig = fn_buf
            gsub(/[[:space:]]*\{[^)]*$/, "", sig)
            gsub(/ where .*$/, "", sig)
            gsub(/  +/, " ", sig)
            print trim(sig)
            fn_buf = ""
        } else {
            mode = "fn"
        }
        next
    }

    mode == "fn" {
        fn_buf = fn_buf " " trim(line)
        if (line ~ /\)([[:space:]]*(->|where|\{)|[[:space:]]*$)/) {
            sig = fn_buf
            gsub(/[[:space:]]*\{[^)]*$/, "", sig)
            gsub(/ where .*$/, "", sig)
            gsub(/  +/, " ", sig)
            print trim(sig)
            fn_buf = ""
            mode = "top"
        }
        next
    }

    # ── pub const / pub static ───────────────────────────────────────────
    mode == "top" && /^pub (const|static) / {
        sig = line
        gsub(/ = .*$/, "", sig)
        gsub(/;$/, "", sig)
        print trim(sig)
        next
    }

    # ── Track depth for anything at top level we skip ────────────────────
    mode == "top" {
        opens = gsub(/{/, "{", line)
        closes = gsub(/}/, "}", line)
        depth += opens - closes
        if (depth < 0) depth = 0
    }
    ' "$file"
}

# ── Main output ──────────────────────────────────────────────────────────────

PROJECT_NAME="$(basename "$PROJECT_ROOT")"
TIMESTAMP="$(date -u '+%Y-%m-%dT%H:%M:%S')"

echo "# Codebase Brief — $PROJECT_NAME"
echo "# Generated: ${TIMESTAMP}Z"
echo ""

# ── Module Map ──────────────────────────────────────────────────────────────
echo "## Module Map"
echo ""

while IFS= read -r rs_file; do
    rel_path="${rs_file#$PROJECT_ROOT/}"
    line_count="$(wc -l < "$rs_file")"

    echo "### $rel_path ($line_count lines)"
    extract_symbols "$rs_file"
    echo ""
done < <(find "$PROJECT_ROOT/crates" -path "*/src/*.rs" | sort)

# ── Color Scheme ─────────────────────────────────────────────────────────────
echo "## Color Scheme"
echo ""
UI_FILE="$PROJECT_ROOT/crates/orrch-tui/src/ui.rs"
if [[ -f "$UI_FILE" ]]; then
    grep -E "^const [A-Z_]+: Color = Color::" "$UI_FILE" 2>/dev/null \
        | sed 's/^const //' \
        | sed 's/: Color = Color::/= /' \
        | sed 's/;.*//' \
        | sed 's/Rgb(\([0-9]*\), \([0-9]*\), \([0-9]*\))/Rgb(\1,\2,\3)/' \
        || true
else
    echo "(ui.rs not found)"
fi
echo ""

# ── Crate Dependencies ────────────────────────────────────────────────────────
echo "## Dependencies"
echo ""
for toml_file in "$PROJECT_ROOT"/crates/*/Cargo.toml; do
    [[ -f "$toml_file" ]] || continue
    crate_name="$(basename "$(dirname "$toml_file")")"

    deps="$(awk '
        /^\[dependencies\]/ { in_dep=1; next }
        /^\[/ { in_dep=0 }
        in_dep && /^[a-zA-Z0-9_-]/ {
            match($0, /^([a-zA-Z0-9_-]+)/, arr)
            if (arr[1] != "") print arr[1]
        }
    ' "$toml_file" | sort -u | tr '\n' ' ' | sed 's/ $//')"

    echo "$crate_name: ${deps:-(none)}"
done
echo ""

# ── Conventions ─────────────────────────────────────────────────────────────
echo "## Conventions"
echo ""
cat <<'CONVENTIONS'
- Tab enums: ALL array, label()/index() methods, next()/prev() wrap-around
- Key handlers: key_<panel>(&mut self, key: KeyCode) -> Result<()>
- UI renderers: draw_<thing>(frame: &mut Frame, app: &App, area: Rect)
- Navigation: focus_depth levels, Up/Down between depth bars, Left/Right within bar
- File preview: markdown_to_lines() for .md content in preview panes
- tmux sessions: SessionCategory::{Dev,Edit,Proc}, managed via orrch-core::windows
- One session per workflow — NOT per agent (token efficiency is a core design principle)
- Workforce format: structured markdown with pipe-delimited step tables
- API valves: per-provider on/off toggle, persisted in ~/.config/orrchestrator/valves.json
- Three-tier models: enterprise (Claude/GPT-4o), mid-tier (Mistral Large), local (Ollama)
- Skills (.md, LLM judgment): workflow orchestration, agent roles — harness-agnostic
- Tools (shell scripts): deterministic repeatable operations, no LLM judgment
CONVENTIONS
