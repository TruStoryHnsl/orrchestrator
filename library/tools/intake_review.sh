#!/usr/bin/env bash
set -euo pipefail

# intake_review.sh — Manage instruction intake review queue
# Usage:
#   intake_review.sh write <project_dir> <raw_file> <optimized_file>
#   intake_review.sh confirm <project_dir>
#   intake_review.sh status <project_dir>

usage() {
    echo "Usage:" >&2
    echo "  intake_review.sh write <project_dir> <raw_file> <optimized_file>" >&2
    echo "  intake_review.sh confirm <project_dir>" >&2
    echo "  intake_review.sh status <project_dir>" >&2
    echo "" >&2
    echo "Commands:" >&2
    echo "  write    — create a review entry from raw and optimized instruction files" >&2
    echo "  confirm  — mark the pending review as confirmed" >&2
    echo "  status   — show current review state (pending/confirmed/none)" >&2
    exit 1
}

if [[ $# -lt 2 ]]; then
    usage
fi

command="$1"
project_dir="$2"

if [[ ! -d "$project_dir" ]]; then
    echo "Error: project directory does not exist: $project_dir" >&2
    exit 1
fi

orrch_dir="$project_dir/.orrch"
review_file="$orrch_dir/intake_review.json"

case "$command" in
    write)
        if [[ $# -lt 4 ]]; then
            echo "Error: write requires <project_dir> <raw_file> <optimized_file>" >&2
            usage
        fi
        raw_file="$3"
        optimized_file="$4"

        if [[ ! -f "$raw_file" ]]; then
            echo "Error: raw file does not exist: $raw_file" >&2
            exit 1
        fi
        if [[ ! -f "$optimized_file" ]]; then
            echo "Error: optimized file does not exist: $optimized_file" >&2
            exit 1
        fi

        mkdir -p "$orrch_dir"
        timestamp="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

        raw_content="$(cat "$raw_file")"
        optimized_content="$(cat "$optimized_file")"

        python3 -c "
import json, sys

data = {
    'state': 'pending',
    'raw_file': sys.argv[1],
    'optimized_file': sys.argv[2],
    'raw_content': sys.argv[3],
    'optimized_content': sys.argv[4],
    'created_at': sys.argv[5],
    'confirmed_at': None
}

print(json.dumps(data, indent=2))
" "$raw_file" "$optimized_file" "$raw_content" "$optimized_content" "$timestamp" > "$review_file"

        echo "Review entry created: pending (raw: $raw_file, optimized: $optimized_file)"
        ;;

    confirm)
        if [[ ! -f "$review_file" ]]; then
            echo "Error: no review entry exists at $review_file" >&2
            exit 1
        fi

        timestamp="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

        python3 -c "
import json, sys

with open(sys.argv[1], 'r') as f:
    data = json.load(f)

if data['state'] == 'confirmed':
    print('Already confirmed at ' + str(data['confirmed_at']), file=sys.stderr)
    sys.exit(1)

data['state'] = 'confirmed'
data['confirmed_at'] = sys.argv[2]

with open(sys.argv[1], 'w') as f:
    json.dump(data, f, indent=2)
    f.write('\n')

print('Review confirmed at ' + sys.argv[2])
" "$review_file" "$timestamp"
        ;;

    status)
        if [[ ! -f "$review_file" ]]; then
            echo "none"
            exit 0
        fi

        python3 -c "
import json, sys

with open(sys.argv[1], 'r') as f:
    data = json.load(f)

state = data['state']
print(state)

if state == 'pending':
    print('  Created: ' + data['created_at'])
    print('  Raw: ' + data['raw_file'])
    print('  Optimized: ' + data['optimized_file'])
elif state == 'confirmed':
    print('  Created: ' + data['created_at'])
    print('  Confirmed: ' + str(data['confirmed_at']))
    print('  Raw: ' + data['raw_file'])
    print('  Optimized: ' + data['optimized_file'])
" "$review_file"
        ;;

    *)
        echo "Error: unknown command '$command'" >&2
        usage
        ;;
esac
