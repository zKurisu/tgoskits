#define _POSIX_C_SOURCE 200809L

#include "cvi_usb_camera.h"

#include <errno.h>
#include <fcntl.h>
#include <inttypes.h>
#include <signal.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <time.h>
#include <unistd.h>

#define JPEG_CAPACITY (2U * 1024U * 1024U)
#define FRAME_TIMEOUT_MS 2000U
#define TRANSFER_ACK_TIMEOUT_MS 120000U
#define ACK_POLL_MS 10U

static volatile sig_atomic_t interrupted;

static void handle_signal(int signal_number)
{
    (void)signal_number;
    interrupted = 1;
}

static int parse_u32(const char *text, uint32_t minimum, uint32_t maximum,
                     uint32_t *value)
{
    char *end = NULL;
    unsigned long parsed;

    errno = 0;
    parsed = strtoul(text, &end, 10);
    if (errno != 0 || text[0] == '\0' || end == NULL || *end != '\0' ||
        parsed < minimum || parsed > maximum) {
        return -1;
    }
    *value = (uint32_t)parsed;
    return 0;
}

static uint64_t monotonic_ms(void)
{
    struct timespec now = {0};

    if (clock_gettime(CLOCK_MONOTONIC, &now) != 0) {
        return 0;
    }
    return (uint64_t)now.tv_sec * 1000U + (uint64_t)now.tv_nsec / 1000000U;
}

static int sleep_ms(uint32_t milliseconds)
{
    struct timespec delay = {
        .tv_sec = milliseconds / 1000U,
        .tv_nsec = (long)(milliseconds % 1000U) * 1000000L,
    };

    while (nanosleep(&delay, &delay) != 0) {
        if (errno != EINTR) {
            return -1;
        }
        if (interrupted) {
            return 0;
        }
    }
    return 0;
}

static int wait_until(uint64_t deadline_ms)
{
    while (!interrupted) {
        uint64_t now_ms = monotonic_ms();

        if (now_ms >= deadline_ms) {
            return 0;
        }
        uint64_t remaining_ms = deadline_ms - now_ms;
        uint32_t delay_ms = remaining_ms > 100U ? 100U : (uint32_t)remaining_ms;
        if (sleep_ms(delay_ms) != 0) {
            return -1;
        }
    }
    return 0;
}

static bool valid_jpeg(const uint8_t *data, uint32_t length)
{
    return length >= 4U && data[0] == 0xffU && data[1] == 0xd8U &&
           data[length - 2U] == 0xffU && data[length - 1U] == 0xd9U;
}

static int write_all(int fd, const uint8_t *data, uint32_t length)
{
    uint32_t offset = 0;

    while (offset < length) {
        ssize_t written = write(fd, data + offset, length - offset);

        if (written < 0) {
            if (errno == EINTR) {
                continue;
            }
            return -1;
        }
        if (written == 0) {
            errno = EIO;
            return -1;
        }
        offset += (uint32_t)written;
    }
    return 0;
}

static int publish_frame(const char *temporary_path, const char *ready_path,
                         const uint8_t *data, uint32_t length)
{
    int fd = open(temporary_path, O_WRONLY | O_CREAT | O_TRUNC, 0600);

    if (fd < 0) {
        return -1;
    }
    if (write_all(fd, data, length) != 0) {
        int saved_errno = errno;
        close(fd);
        errno = saved_errno;
        return -1;
    }
    if (close(fd) != 0) {
        return -1;
    }
    if (rename(temporary_path, ready_path) != 0) {
        return -1;
    }
    return 0;
}

static int wait_for_transfer_ack(const char *ready_path)
{
    uint64_t deadline_ms = monotonic_ms() + TRANSFER_ACK_TIMEOUT_MS;

    while (!interrupted) {
        if (access(ready_path, F_OK) != 0) {
            if (errno == ENOENT) {
                return 0;
            }
            return -1;
        }
        if (monotonic_ms() >= deadline_ms) {
            errno = ETIMEDOUT;
            return -1;
        }
        if (sleep_ms(ACK_POLL_MS) != 0) {
            return -1;
        }
    }
    errno = EINTR;
    return -1;
}

static void usage(const char *program)
{
    fprintf(stderr,
            "Usage: %s DEVICE FRAME_COUNT INTERVAL_MS READY_JPEG\n"
            "Example: %s /dev/cvi-usb-camera0 300 500 "
            "/tmp/akars-camera-dataset.jpg\n",
            program, program);
}

int main(int argc, char **argv)
{
    const char *device;
    const char *ready_path;
    char *temporary_path = NULL;
    uint32_t target_frames;
    uint32_t interval_ms;
    uint8_t *jpeg = NULL;
    uint64_t sequence = 0;
    uint64_t next_capture_ms;
    uint32_t received = 0;
    int fd = -1;
    int status = EXIT_FAILURE;
    bool capture_started = false;

    if (argc != 5 || parse_u32(argv[2], 1U, 100000U, &target_frames) != 0 ||
        parse_u32(argv[3], 1U, 3600000U, &interval_ms) != 0) {
        usage(argv[0]);
        return EXIT_FAILURE;
    }
    device = argv[1];
    ready_path = argv[4];

    size_t temporary_length = strlen(ready_path) + sizeof(".part");
    temporary_path = malloc(temporary_length);
    if (temporary_path == NULL) {
        perror("malloc temporary path");
        goto out;
    }
    if (snprintf(temporary_path, temporary_length, "%s.part", ready_path) < 0) {
        fprintf(stderr, "cannot construct temporary path\n");
        goto out;
    }

    struct sigaction action = {0};
    action.sa_handler = handle_signal;
    if (sigemptyset(&action.sa_mask) != 0 ||
        sigaction(SIGINT, &action, NULL) != 0 ||
        sigaction(SIGTERM, &action, NULL) != 0 ||
        sigaction(SIGHUP, &action, NULL) != 0) {
        perror("sigaction");
        goto out;
    }

    (void)unlink(temporary_path);
    (void)unlink(ready_path);

    jpeg = malloc(JPEG_CAPACITY);
    if (jpeg == NULL) {
        perror("malloc JPEG buffer");
        goto out;
    }
    fd = open(device, O_RDONLY);
    if (fd < 0) {
        perror("open camera");
        goto out;
    }
    if (ioctl(fd, CVI_CAMERA_IOCTL_INIT, 0) < 0 ||
        ioctl(fd, CVI_CAMERA_IOCTL_RESET_CAPTURE_STATS, 0) < 0 ||
        ioctl(fd, CVI_CAMERA_IOCTL_START_ASYNC, 0) < 0) {
        perror("initialize async capture");
        goto out;
    }
    capture_started = true;
    next_capture_ms = monotonic_ms();

    while (received < target_frames && !interrupted) {
        if (wait_until(next_capture_ms) != 0) {
            perror("wait for capture interval");
            goto out;
        }
        if (interrupted) {
            break;
        }

        struct cvi_camera_frame_request request = {
            .buffer = (uintptr_t)jpeg,
            .capacity = JPEG_CAPACITY,
            .last_sequence = sequence,
            .timeout_ms = FRAME_TIMEOUT_MS,
        };
        if (ioctl(fd, CVI_CAMERA_IOCTL_GET_LATEST_FRAME, &request) < 0) {
            perror("get latest MJPEG frame");
            goto out;
        }
        if (request.format != CVI_CAMERA_FORMAT_MJPEG ||
            !valid_jpeg(jpeg, request.length)) {
            fprintf(stderr,
                    "invalid MJPEG frame: format=%u bytes=%" PRIu32 "\n",
                    request.format, request.length);
            errno = EPROTO;
            goto out;
        }
        sequence = request.sequence;

        if (publish_frame(temporary_path, ready_path, jpeg, request.length) != 0) {
            perror("publish temporary JPEG");
            goto out;
        }
        received++;
        printf("CVI_CAMERA_DATASET_FRAME index=%" PRIu32
               " sequence=%" PRIu64 " bytes=%" PRIu32
               " size=%ux%u path=%s\n",
               received, request.sequence, request.length, request.width,
               request.height, ready_path);
        fflush(stdout);

        if (wait_for_transfer_ack(ready_path) != 0) {
            perror("wait for SCP acknowledgement");
            goto out;
        }

        next_capture_ms += interval_ms;
        uint64_t now_ms = monotonic_ms();
        if (next_capture_ms < now_ms) {
            next_capture_ms = now_ms;
        }
    }

    if (interrupted) {
        status = 130;
    } else {
        printf("CVI_CAMERA_DATASET_PASS frames=%" PRIu32
               " interval_ms=%" PRIu32 "\n",
               received, interval_ms);
        fflush(stdout);
        status = EXIT_SUCCESS;
    }

out:
    if (capture_started && ioctl(fd, CVI_CAMERA_IOCTL_STOP_ASYNC, 0) < 0 &&
        status == EXIT_SUCCESS) {
        perror("stop async capture");
        status = EXIT_FAILURE;
    }
    if (fd >= 0) {
        close(fd);
    }
    if (temporary_path != NULL) {
        (void)unlink(temporary_path);
    }
    if (ready_path != NULL) {
        (void)unlink(ready_path);
    }
    free(jpeg);
    free(temporary_path);
    return status;
}
