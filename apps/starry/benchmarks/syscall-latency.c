/*
 * syscall-latency.c -- compare the round-trip latency of a real syscall
 * (getpid) against a nonexistent syscall number (which returns ENOSYS after
 * the same trap + dispatch path).
 *
 * Purpose: locate where the ~16us getpid cost on riscv64 StarryOS goes.
 *   - If `invalid` ≈ `getpid`, the cost is almost entirely the trap entry /
 *     dispatch / context save-restore path, not the getpid handler body.
 *   - If `getpid` is clearly larger, the cost is in the handler / dispatch.
 *
 * Build (riscv64 musl, static):
 *   riscv64-unknown-linux-musl-gcc -static -O2 -o syscall-latency syscall-latency.c
 *
 * Run:
 *   ./syscall-latency [iterations]     # default 10,000,000
 *
 * Output:
 *   SYSCALL_LATENCY name=getpid  sysno=172   ns_per_call=...
 *   SYSCALL_LATENCY name=getppid sysno=173   ns_per_call=...
 *   SYSCALL_LATENCY name=invalid sysno=9999  ns_per_call=...
 *   STARRY_SYSCALL_LATENCY_OK
 */
#define _GNU_SOURCE

#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include <unistd.h>
#include <sys/syscall.h>

#ifndef SYS_getpid
#define SYS_getpid 172
#endif
#ifndef SYS_getppid
#define SYS_getppid 173
#endif

/* A syscall number that does not exist on riscv64 (asm-generic) and returns
 * ENOSYS after full trap + dispatch. */
#define INVALID_SYSNO 9999

static uint64_t now_ns(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0) {
        perror("clock_gettime");
        exit(1);
    }
    return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}

static double measure_ns_per_call(long sysno, long iterations) {
    volatile long sink = 0;
    for (long i = 0; i < 10000; i++) {
        sink += syscall(sysno);
    }

    uint64_t start = now_ns();
    for (long i = 0; i < iterations; i++) {
        sink += syscall(sysno);
    }
    uint64_t end = now_ns();

    (void)sink;
    return (double)(end - start) / (double)iterations;
}

static void report(const char *name, long sysno, double ns_per_call) {
    printf("SYSCALL_LATENCY name=%s sysno=%ld ns_per_call=%.2f\n", name, sysno, ns_per_call);
}

int main(int argc, char **argv) {
    long iterations = 10000000L;
    if (argc > 1) {
        char *end = NULL;
        long parsed = strtol(argv[1], &end, 10);
        if (end == argv[1] || *end != '\0' || parsed <= 0) {
            fprintf(stderr, "usage: %s [iterations]\n", argv[0]);
            return 2;
        }
        iterations = parsed;
    }

    double getpid_ns = measure_ns_per_call(SYS_getpid, iterations);
    double getppid_ns = measure_ns_per_call(SYS_getppid, iterations);
    double invalid_ns = measure_ns_per_call(INVALID_SYSNO, iterations);

    report("getpid", SYS_getpid, getpid_ns);
    report("getppid", SYS_getppid, getppid_ns);
    report("invalid", INVALID_SYSNO, invalid_ns);
    printf("STARRY_SYSCALL_LATENCY_OK\n");
    return 0;
}
