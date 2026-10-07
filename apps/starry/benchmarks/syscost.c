/*
 * syscost.c -- measure the getpid() syscall entry/exit round-trip latency.
 *
 * The getpid() handler only reads a process id that is already in memory, so
 * the measured time is almost entirely the trap-in / dispatch / return path.
 * This is the purest probe of syscall entry overhead, and it is used to
 * compare StarryOS against Linux on the same ELF.
 *
 * The syscall is issued through syscall(SYS_getpid) directly (not the libc
 * getpid()) so a libc-level pid cache does not hide the real trap cost.
 *
 * Build (riscv64 musl, static):
 *   riscv64-unknown-linux-musl-gcc -static -O2 -o syscost syscost.c
 *
 * Run:
 *   ./syscost [iterations]     # default 10,000,000
 *
 * On success it prints one machine-readable line and a sentinel:
 *   SYSCALL_GETPID iterations=... total_ns=... ns_per_call=...
 *   STARRY_SYSCALL_GETPID_OK
 */
#define _GNU_SOURCE

#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include <unistd.h>
#include <sys/syscall.h>

static uint64_t now_ns(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0) {
        perror("clock_gettime");
        exit(1);
    }
    return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
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

    /* Warm the syscall path (icache, branch predictor) before timing. */
    long dummy = 0;
    for (long i = 0; i < 10000; i++) {
        dummy += syscall(SYS_getpid);
    }

    uint64_t start = now_ns();
    for (long i = 0; i < iterations; i++) {
        dummy += syscall(SYS_getpid);
    }
    uint64_t end = now_ns();

    double ns_per_call = (double)(end - start) / (double)iterations;
    printf("SYSCALL_GETPID iterations=%ld total_ns=%llu ns_per_call=%.2f dummy=%ld\n",
           iterations, (unsigned long long)(end - start), ns_per_call, dummy);
    printf("STARRY_SYSCALL_GETPID_OK\n");
    return 0;
}
