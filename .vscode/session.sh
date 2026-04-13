#!/usr/bin/env bash
set -euo pipefail

state_dir="${TGOS_DEBUG_STATE_DIR:-}"
session="${TGOS_DEBUG_SESSION:-}"

if [[ -z "${state_dir}" || -z "${session}" ]]; then
    echo "missing TGOS_DEBUG_STATE_DIR or TGOS_DEBUG_SESSION" >&2
    exit 2
fi

port="${TGOS_DEBUG_PORT:-1234}"
log_file="${state_dir}/${session}.log"
pid_file="${state_dir}/${session}.pid"
pgid_file="${state_dir}/${session}.pgid"
tee_output="${TGOS_DEBUG_TEE_OUTPUT:-1}"

cleanup() {
    if [[ -f "${pgid_file}" ]]; then
        local pgid
        pgid="$(<"${pgid_file}")"
        if [[ -n "${pgid}" ]] && kill -0 "-${pgid}" 2>/dev/null; then
            kill "-${pgid}" 2>/dev/null || true
        fi
        rm -f "${pgid_file}"
    fi

    if [[ -f "${pid_file}" ]]; then
        local pid
        pid="$(<"${pid_file}")"
        if [[ -n "${pid}" ]] && kill -0 "${pid}" 2>/dev/null; then
            kill "${pid}" 2>/dev/null || true
        fi
        rm -f "${pid_file}"
    fi

    cleanup_orphaned_session_processes
}

session_env_matches_pid() {
    local pid="$1"
    local env_file="/proc/${pid}/environ"

    [[ -r "${env_file}" ]] || return 1

    grep -z -q "^TGOS_DEBUG_SESSION=${session}$" "${env_file}" 2>/dev/null &&
        grep -z -q "^TGOS_DEBUG_STATE_DIR=${state_dir}$" "${env_file}" 2>/dev/null
}

list_orphaned_session_pgids() {
    local current_pgid
    current_pgid="$(ps -o pgid= -p $$ | tr -d ' ')"

    for proc_dir in /proc/[0-9]*; do
        local pid pgid
        pid="${proc_dir##*/}"

        if [[ "${pid}" == "$$" ]]; then
            continue
        fi

        if ! session_env_matches_pid "${pid}"; then
            continue
        fi

        pgid="$(ps -o pgid= -p "${pid}" 2>/dev/null | tr -d ' ')"
        if [[ -n "${pgid}" && "${pgid}" != "${current_pgid}" ]]; then
            printf '%s\n' "${pgid}"
        fi
    done | sort -u
}

cleanup_orphaned_session_processes() {
    local pgid
    mapfile -t orphaned_pgids < <(list_orphaned_session_pgids)

    if [[ "${#orphaned_pgids[@]}" -eq 0 ]]; then
        return 0
    fi

    for pgid in "${orphaned_pgids[@]}"; do
        kill "-${pgid}" 2>/dev/null || true
    done

    for (( i = 0; i < 20; i++ )); do
        local any_alive=0

        for pgid in "${orphaned_pgids[@]}"; do
            if kill -0 "-${pgid}" 2>/dev/null; then
                any_alive=1
                break
            fi
        done

        if [[ "${any_alive}" -eq 0 ]]; then
            return 0
        fi

        sleep 0.1
    done

    for pgid in "${orphaned_pgids[@]}"; do
        kill -9 "-${pgid}" 2>/dev/null || true
    done
}

has_qemu_process_in_group() {
    local pgid="$1"
    ps -eo pgid=,args= | awk -v pgid="${pgid}" '
        $1 == pgid && $2 ~ /^qemu-system-/ {
            found = 1
        }
        END {
            exit(found ? 0 : 1)
        }
    '
}

port_is_owned_by_group() {
    local pgid="$1"
    local pid

    while IFS= read -r pid; do
        if [[ -z "${pid}" ]]; then
            continue
        fi

        local owner_pgid
        owner_pgid="$(ps -o pgid= -p "${pid}" 2>/dev/null | tr -d ' ')"
        if [[ "${owner_pgid}" == "${pgid}" ]]; then
            return 0
        fi
    done < <(
        ss -ltnpH "( sport = :${port} )" 2>/dev/null |
            grep -oE 'pid=[0-9]+' |
            cut -d= -f2
    )

    return 1
}

wait_for_qemu_ready() {
    local pgid="$1"
    for (( i = 0; i < 200; i++ )); do
        if has_qemu_process_in_group "${pgid}" && port_is_owned_by_group "${pgid}" &&
                { : </dev/tcp/127.0.0.1/${port}; } 2>/dev/null; then
            return 0
        fi
        sleep 0.1
    done
    return 1
}

cmd="${1:-}"
case "${cmd}" in
    start)
        debug_command="${TGOS_DEBUG_COMMAND:-}"
        if [[ -z "${debug_command}" ]]; then
            echo "missing TGOS_DEBUG_COMMAND" >&2
            exit 2
        fi

        mkdir -p "${state_dir}"
        cleanup
        printf 'QEMU_DEBUG_STARTING session=%s port=%s\n' "${session}" "${port}"
        trap 'cleanup' INT TERM EXIT

        if [[ "${tee_output}" == "1" ]]; then
            # Mirror QEMU output to both the VS Code task terminal and a log file.
            setsid bash -lc "${debug_command} 2>&1 | tee '${log_file}'" &
        else
            setsid bash -lc "${debug_command}" >"${log_file}" 2>&1 &
        fi
        child_pid=$!
        printf '%s\n' "${child_pid}" >"${pid_file}"
        child_pgid="$(ps -o pgid= -p "${child_pid}" | tr -d ' ')"
        printf '%s\n' "${child_pgid}" >"${pgid_file}"

        if wait_for_qemu_ready "${child_pgid}"; then
            printf 'QEMU_GDB_READY session=%s port=%s pid=%s log=%s\n' \
                "${session}" "${port}" "${child_pid}" "${log_file}"
            wait "${child_pid}" || true
            # QEMU has exited (naturally or via `stop`). Remove state files and
            # clear the EXIT trap to skip the redundant orphan scan: all
            # processes in this session group are already gone.
            trap - EXIT
            rm -f "${pid_file}" "${pgid_file}"
            exit 0
        fi

        printf 'QEMU_DEBUG_FAILED session=%s log=%s\n' "${session}" "${log_file}"
        tail -n 80 "${log_file}" || true
        cleanup
        exit 1
        ;;
    stop)
        cleanup
        printf 'QEMU_DEBUG_STOPPED session=%s\n' "${session}"
        ;;
    *)
        echo "Usage: session.sh <start|stop>" >&2
        exit 2
        ;;
esac
