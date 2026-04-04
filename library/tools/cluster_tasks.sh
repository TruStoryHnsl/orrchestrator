#!/usr/bin/env bash
set -euo pipefail

# cluster_tasks.sh — Group development tasks into file-based clusters for parallel agent execution
#
# Instead of batching by agent role, tasks that touch the same files are grouped together
# so each file is read by only ONE agent cluster. Clusters are assigned to waves based
# on cross-cluster dependencies.
#
# Usage:
#   cluster_tasks.sh [file]       — read from file
#   cluster_tasks.sh              — read from stdin
#   echo "..." | cluster_tasks.sh — pipe input
#
# Input format (each task block, separated by blank lines):
#   TASK <id>: <title>
#   Agent: <role>
#   Files: <file1>, <file2>, ...
#   Work: <description>
#   Acceptance: <criteria>
#   Depends: <none | task-id, task-id, ...>
#
# Output: clustered tasks with wave assignments

INPUT=""
if [[ $# -ge 1 ]]; then
    if [[ ! -f "$1" ]]; then
        echo "Error: file not found: $1" >&2
        exit 1
    fi
    INPUT="$1"
fi

# ---------------------------------------------------------------------------
# Phase 1: Parse tasks and emit structured records
#   Output format per task:  ID|AGENT|FILE1,FILE2,...|DEP1,DEP2,...
# ---------------------------------------------------------------------------

parse_tasks() {
    awk '
    BEGIN { id = ""; agent = ""; files = ""; deps = "" }

    /^TASK [^ ]+:/ {
        if (id != "") print id "|" agent "|" files "|" deps
        id    = $2; sub(/:$/, "", id)
        agent = ""; files = ""; deps = ""
        next
    }

    /^Agent:/ {
        agent = substr($0, index($0, ":") + 2)
        gsub(/^[[:space:]]+|[[:space:]]+$/, "", agent)
        next
    }

    /^Files:/ {
        files = substr($0, index($0, ":") + 2)
        gsub(/^[[:space:]]+|[[:space:]]+$/, "", files)
        gsub(/[[:space:]]*,[[:space:]]*/, ",", files)
        next
    }

    /^Depends:/ {
        deps = substr($0, index($0, ":") + 2)
        gsub(/^[[:space:]]+|[[:space:]]+$/, "", deps)
        gsub(/[[:space:]]*,[[:space:]]*/, ",", deps)
        if (tolower(deps) == "none" || deps == "") deps = "none"
        next
    }

    END { if (id != "") print id "|" agent "|" files "|" deps }
    ' "${INPUT:-/dev/stdin}"
}

# ---------------------------------------------------------------------------
# Phase 2: Union-find clustering + wave assignment
# ---------------------------------------------------------------------------

cluster_and_output() {
    awk -F'|' '

    # ── Union-find helpers (path-compressed, rank-union) ──────────────────
    function find(x,    r) {
        r = x
        while (parent[r] != r) r = parent[r]
        # path compression
        while (parent[x] != x) { nx = parent[x]; parent[x] = r; x = nx }
        return r
    }
    function unite(a, b,    ra, rb, tmp) {
        ra = find(a); rb = find(b)
        if (ra == rb) return
        if (rank[ra] < rank[rb]) { tmp = ra; ra = rb; rb = tmp }
        parent[rb] = ra
        if (rank[ra] == rank[rb]) rank[ra]++
    }

    # ── Parse each record ─────────────────────────────────────────────────
    {
        tid   = $1
        agent = $2
        files = $3
        deps  = $4

        tasks[tid]          = 1
        task_agent[tid]     = agent
        task_files[tid]     = files
        task_deps[tid]      = deps
        task_order[++n]     = tid

        # Initialise union-find node
        parent[tid] = tid
        rank[tid]   = 0

        # Union tasks that share a file
        if (files != "" && files != "none") {
            nf = split(files, fa, ",")
            for (i = 1; i <= nf; i++) {
                f = fa[i]
                gsub(/^[[:space:]]+|[[:space:]]+$/, "", f)
                if (f == "") continue
                if (f in file_owner) {
                    unite(tid, file_owner[f])
                } else {
                    file_owner[f] = tid
                }
            }
        }
    }

    END {
        # ── Resolve roots & build cluster membership ──────────────────────
        for (i = 1; i <= n; i++) {
            tid  = task_order[i]
            root = find(tid)
            task_root[tid] = root
            # Space-separated task list per cluster (in insertion order)
            cluster_tasks[root] = cluster_tasks[root] \
                (cluster_tasks[root] == "" ? "" : " ") tid
        }

        # Stable-ordered list of unique cluster roots
        n_clusters = 0
        for (i = 1; i <= n; i++) {
            tid  = task_order[i]
            root = task_root[tid]
            if (!(root in seen_cluster)) {
                seen_cluster[root]         = 1
                cluster_order[++n_clusters] = root
            }
        }

        # ── Wave assignment ───────────────────────────────────────────────
        for (ci = 1; ci <= n_clusters; ci++) cluster_wave[cluster_order[ci]] = 1

        changed = 1
        while (changed) {
            changed = 0
            for (i = 1; i <= n; i++) {
                tid = task_order[i]
                if (task_deps[tid] == "none" || task_deps[tid] == "") continue
                my_root  = task_root[tid]
                nd = split(task_deps[tid], da, ",")
                for (d = 1; d <= nd; d++) {
                    dep_id = da[d]
                    gsub(/^[[:space:]]+|[[:space:]]+$/, "", dep_id)
                    if (dep_id == "none" || dep_id == "" || !(dep_id in tasks)) continue
                    dep_root = task_root[dep_id]
                    if (dep_root == my_root) continue      # intra-cluster: no bump
                    needed = cluster_wave[dep_root] + 1
                    if (needed > cluster_wave[my_root]) {
                        cluster_wave[my_root] = needed
                        changed = 1
                    }
                }
            }
        }

        # ── Build per-cluster metadata: files & majority agent ────────────
        for (ci = 1; ci <= n_clusters; ci++) {
            croot = cluster_order[ci]
            nt = split(cluster_tasks[croot], ta, " ")

            delete cfiles_seen
            file_list = ""
            delete agent_vote
            max_votes = 0; majority_agent = "Unknown"

            for (t = 1; t <= nt; t++) {
                tid = ta[t]

                # Files
                if (task_files[tid] != "" && task_files[tid] != "none") {
                    nf = split(task_files[tid], fa, ",")
                    for (f = 1; f <= nf; f++) {
                        fname = fa[f]
                        gsub(/^[[:space:]]+|[[:space:]]+$/, "", fname)
                        if (fname == "" || (fname in cfiles_seen)) continue
                        cfiles_seen[fname] = 1
                        file_list = file_list (file_list == "" ? "" : ", ") fname
                    }
                }

                # Agent vote
                ag = task_agent[tid]
                if (ag != "") {
                    agent_vote[ag]++
                    if (agent_vote[ag] > max_votes) {
                        max_votes      = agent_vote[ag]
                        majority_agent = ag
                    }
                }
            }

            cluster_file_list[croot]  = (file_list == "") ? "(no files)" : file_list
            cluster_agent[croot]      = majority_agent
        }

        # ── Determine basenames for cluster headings ───────────────────────
        for (ci = 1; ci <= n_clusters; ci++) {
            croot = cluster_order[ci]
            fl    = cluster_file_list[croot]
            nf    = split(fl, fl_parts, ", ")
            bn_list = ""
            for (p = 1; p <= nf; p++) {
                fname = fl_parts[p]
                bn    = fname
                if (index(fname, "/") > 0) {
                    np = split(fname, pp, "/"); bn = pp[np]
                }
                bn_list = bn_list (bn_list == "" ? "" : ", ") bn
            }
            cluster_basenames[croot] = bn_list
        }

        # ── Print output ──────────────────────────────────────────────────
        max_wave = 1
        for (ci = 1; ci <= n_clusters; ci++) {
            w = cluster_wave[cluster_order[ci]]
            if (w > max_wave) max_wave = w
        }

        print "## Clusters"
        print ""

        clust_num = 0
        for (w = 1; w <= max_wave; w++) {
            # Count clusters in this wave first (skip empty waves)
            wcount = 0
            for (ci = 1; ci <= n_clusters; ci++)
                if (cluster_wave[cluster_order[ci]] == w) wcount++
            if (wcount == 0) continue

            if (w == 1) {
                print "### Wave 1"
            } else {
                print "### Wave " w " (depends on Wave " (w-1) ")"
            }
            print ""

            for (ci = 1; ci <= n_clusters; ci++) {
                croot = cluster_order[ci]
                if (cluster_wave[croot] != w) continue
                clust_num++

                # Format task list
                tl = cluster_tasks[croot]; gsub(/ /, ", ", tl)

                print "CLUSTER " clust_num ": [" cluster_basenames[croot] "]"
                print "  Tasks: " tl
                print "  Suggested agent: " cluster_agent[croot] " (majority role)"
                if (cluster_file_list[croot] != cluster_basenames[croot]) {
                    print "  Files: " cluster_file_list[croot]
                }
                print ""
            }
        }
    }
    '
}

# ---------------------------------------------------------------------------
parse_tasks | cluster_and_output
