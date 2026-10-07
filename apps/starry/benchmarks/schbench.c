/*
 * schbench.c -- simplified wake-up latency benchmark (schbench-like).
 *
 * One messenger thread wakes N worker threads one at a time and records how
 * long each worker takes to actually run after being signalled. The latency is
 * dominated by the scheduler's wake-up path, so it probes wake-up latency and
 * scheduler dispatch, mirroring the role of masoncc/schbench in the RK3588
 * StarryOS-vs-Linux comparison.
 *
 * Model: each worker blocks on a pthread condition variable; the messenger
 * timestamps the moment it decides to wake a worker, signals it, and waits for
 * an ack. The worker records now_ns() - wake_ts as its latency. All collected
 * latencies are sorted and reported as p50 / p90 / p99 / p99.9.
 *
 * Build (riscv64 musl, static, pthread):
 *   riscv64-unknown-linux-musl-gcc -static -O2 -pthread -o schbench schbench.c
 *
 * Run:
 *   ./schbench [workers] [rounds]     # default 16 workers, 2000 rounds
 *
 * On success it prints one summary line and a sentinel:
 *   SCHBENCH workers=... rounds=... samples=... p50_us=... p90_us=... p99_us=...
 *   STARRY_SCHBENCH_OK
 */
#define _GNU_SOURCE

#include <errno.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

struct worker {
    pthread_mutex_t mutex;
    pthread_cond_t cond;
    int go;               /* messenger -> worker: wake up now */
    int done;             /* worker -> messenger: latency recorded */
    int running;          /* worker loop control */
    uint64_t wake_ts;     /* messenger timestamp, written under mutex */
    uint64_t latency_ns;  /* worker recorded latency, read under mutex */
    pthread_t thread;
};

static uint64_t now_ns(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0) {
        perror("clock_gettime");
        exit(1);
    }
    return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}

static void *worker_main(void *arg) {
    struct worker *w = arg;
    pthread_mutex_lock(&w->mutex);
    while (w->running) {
        while (!w->go && w->running) {
            pthread_cond_wait(&w->cond, &w->mutex);
        }
        if (!w->running) {
            break;
        }
        w->latency_ns = now_ns() - w->wake_ts;
        w->go = 0;
        w->done = 1;
        pthread_cond_signal(&w->cond);
    }
    pthread_mutex_unlock(&w->mutex);
    return NULL;
}

static int cmp_u64(const void *a, const void *b) {
    uint64_t lhs = *(const uint64_t *)a;
    uint64_t rhs = *(const uint64_t *)b;
    return (lhs > rhs) - (lhs < rhs);
}

static uint64_t percentile(const uint64_t *sorted, size_t n, double p) {
    if (n == 0) {
        return 0;
    }
    size_t idx = (size_t)(p * (double)(n - 1));
    if (idx >= n) {
        idx = n - 1;
    }
    return sorted[idx];
}

int main(int argc, char **argv) {
    int workers = 16;
    long rounds = 2000;
    if (argc > 1) {
        workers = atoi(argv[1]);
    }
    if (argc > 2) {
        rounds = atol(argv[2]);
    }
    if (workers < 1 || rounds < 1) {
        fprintf(stderr, "usage: %s [workers] [rounds]\n", argv[0]);
        return 2;
    }

    struct worker *ws = calloc((size_t)workers, sizeof(*ws));
    uint64_t *samples = calloc((size_t)workers * (size_t)rounds, sizeof(*samples));
    if (ws == NULL || samples == NULL) {
        fprintf(stderr, "out of memory\n");
        return 1;
    }

    for (int i = 0; i < workers; i++) {
        ws[i].running = 1;
        pthread_mutex_init(&ws[i].mutex, NULL);
        pthread_cond_init(&ws[i].cond, NULL);
        if (pthread_create(&ws[i].thread, NULL, worker_main, &ws[i]) != 0) {
            perror("pthread_create");
            return 1;
        }
    }

    size_t count = 0;
    for (long r = 0; r < rounds; r++) {
        for (int i = 0; i < workers; i++) {
            struct worker *w = &ws[i];
            pthread_mutex_lock(&w->mutex);
            w->wake_ts = now_ns();
            w->done = 0;
            w->go = 1;
            pthread_cond_signal(&w->cond);
            while (!w->done) {
                pthread_cond_wait(&w->cond, &w->mutex);
            }
            samples[count++] = w->latency_ns;
            pthread_mutex_unlock(&w->mutex);
        }
    }

    for (int i = 0; i < workers; i++) {
        struct worker *w = &ws[i];
        pthread_mutex_lock(&w->mutex);
        w->running = 0;
        pthread_cond_broadcast(&w->cond);
        pthread_mutex_unlock(&w->mutex);
        pthread_join(w->thread, NULL);
        pthread_mutex_destroy(&w->mutex);
        pthread_cond_destroy(&w->cond);
    }

    qsort(samples, count, sizeof(*samples), cmp_u64);
    printf(
        "SCHBENCH workers=%d rounds=%ld samples=%zu "
        "p50_us=%.2f p90_us=%.2f p99_us=%.2f p999_us=%.2f\n",
        workers, rounds, count,
        (double)percentile(samples, count, 0.50) / 1000.0,
        (double)percentile(samples, count, 0.90) / 1000.0,
        (double)percentile(samples, count, 0.99) / 1000.0,
        (double)percentile(samples, count, 0.999) / 1000.0);
    printf("STARRY_SCHBENCH_OK\n");

    free(ws);
    free(samples);
    return 0;
}
