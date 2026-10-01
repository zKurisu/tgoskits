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

#define DEFAULT_DEVICE "/dev/cvi-usb-camera0"
#define DEFAULT_FRAMES 100U
#define JPEG_CAPACITY (2U * 1024U * 1024U)

static volatile sig_atomic_t interrupted;

static void handle_interrupt(int signal_number)
{
    (void)signal_number;
    interrupted = 1;
}

static const char *format_name(uint8_t format)
{
    switch (format) {
    case CVI_CAMERA_FORMAT_MJPEG:
        return "mjpeg";
    case CVI_CAMERA_FORMAT_YUV420_PLANAR:
        return "yuv420p";
    case CVI_CAMERA_FORMAT_YUV422_PLANAR:
        return "yuv422p";
    case CVI_CAMERA_FORMAT_YUV440_PLANAR:
        return "yuv440p";
    case CVI_CAMERA_FORMAT_YUV444_PLANAR:
        return "yuv444p";
    case CVI_CAMERA_FORMAT_YUV400:
        return "yuv400";
    case CVI_CAMERA_FORMAT_NV12:
        return "nv12";
    default:
        return "unknown";
    }
}

static uint32_t expected_nv12_bytes(uint16_t width, uint16_t height)
{
    uint32_t aligned_width = ((uint32_t)width + 15U) & ~15U;
    uint32_t aligned_height = ((uint32_t)height + 15U) & ~15U;

    return aligned_width * aligned_height * 3U / 2U;
}

static int parse_frames(const char *text, uint32_t *frames)
{
    char *end = NULL;
    unsigned long value = strtoul(text, &end, 10);

    if (text[0] == '\0' || end == NULL || *end != '\0' || value == 0 ||
        value > UINT32_MAX) {
        return -1;
    }
    *frames = (uint32_t)value;
    return 0;
}

static double ratio_percent(uint64_t numerator, uint64_t denominator)
{
    if (denominator == 0) {
        return 0.0;
    }
    return (double)numerator * 100.0 / (double)denominator;
}

static uint64_t monotonic_us(void)
{
    struct timespec now = {0};

    if (clock_gettime(CLOCK_MONOTONIC, &now) != 0) {
        return 0;
    }
    return (uint64_t)now.tv_sec * 1000000U + (uint64_t)now.tv_nsec / 1000U;
}

int main(int argc, char **argv)
{
    const char *device = argc > 1 ? argv[1] : DEFAULT_DEVICE;
    uint32_t target_frames = DEFAULT_FRAMES;
    unsigned long frame_ioctl = CVI_CAMERA_IOCTL_GET_LATEST_FRAME;
    bool quiet = false;
    bool expect_nv12 = false;
    uint8_t *jpeg = NULL;
    int fd = -1;
    int status = EXIT_FAILURE;

    if (argc > 2 && parse_frames(argv[2], &target_frames) != 0) {
        fprintf(stderr, "invalid frame count: %s\n", argv[2]);
        return EXIT_FAILURE;
    }
    if (argc > 3) {
        if (strcmp(argv[3], "yuv") == 0) {
            frame_ioctl = CVI_CAMERA_IOCTL_GET_LATEST_YUV_FRAME;
        } else if (strcmp(argv[3], "nv12") == 0) {
            frame_ioctl = CVI_CAMERA_IOCTL_GET_LATEST_NV12_FRAME;
            expect_nv12 = true;
        } else if (strcmp(argv[3], "mjpeg") != 0) {
            fprintf(stderr, "frame format must be mjpeg, yuv or nv12: %s\n",
                    argv[3]);
            return EXIT_FAILURE;
        }
    }
    if (argc > 4) {
        if (strcmp(argv[4], "quiet") != 0) {
            fprintf(stderr, "optional fifth argument must be quiet: %s\n", argv[4]);
            return EXIT_FAILURE;
        }
        quiet = true;
    }

    struct sigaction action = {0};
    action.sa_handler = handle_interrupt;
    if (sigaction(SIGINT, &action, NULL) != 0) {
        perror("sigaction");
        return EXIT_FAILURE;
    }

    jpeg = malloc(JPEG_CAPACITY);
    if (jpeg == NULL) {
        perror("malloc");
        return EXIT_FAILURE;
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

    uint64_t sequence = 0;
    uint32_t received = 0;
    while (received < target_frames && !interrupted) {
        struct cvi_camera_frame_request request = {
            .buffer = (uintptr_t)jpeg,
            .capacity = JPEG_CAPACITY,
            .last_sequence = sequence,
            .timeout_ms = 2000,
        };
        uint64_t request_start_us = monotonic_us();
        if (ioctl(fd, frame_ioctl, &request) < 0) {
            if (interrupted && errno == EINTR) {
                goto stop;
            }
            if (expect_nv12 && errno == EINVAL) {
                fprintf(stderr,
                        "get latest NV12 frame: %s (JPU NV12 requires a 4:2:0 JPEG; a 4:2:2 source produces NV16)\n",
                        strerror(errno));
            } else {
                fprintf(stderr, "get latest frame: %s\n", strerror(errno));
            }
            goto stop;
        }
        uint64_t request_us = monotonic_us() - request_start_us;
        if (expect_nv12 &&
            (request.format != CVI_CAMERA_FORMAT_NV12 ||
             request.length != expected_nv12_bytes(request.width,
                                                    request.height))) {
            fprintf(stderr,
                    "invalid NV12 response: format=%u bytes=%" PRIu32
                    " expected=%" PRIu32 " size=%ux%u\n",
                    request.format, request.length,
                    expected_nv12_bytes(request.width, request.height),
                    request.width, request.height);
            goto stop;
        }
        sequence = request.sequence;
        received++;
        if (!quiet) {
            printf("CVI_CAMERA_FRAME index=%" PRIu32 " sequence=%" PRIu64
                   " bytes=%" PRIu32 " capture_us=%" PRIu64
                   " uvc_us=%" PRIu64 " attempts=%" PRIu32
                   " request_us=%" PRIu64 " format=%u format_name=%s size=%ux%u\n",
                   received, request.sequence, request.length,
                   request.profile.frame_total_us, request.profile.uvc_total_us,
                   request.profile.attempts, request_us, request.format,
                   format_name(request.format), request.width, request.height);
        }
    }

    status = interrupted ? 130 : EXIT_SUCCESS;

stop:
    if (ioctl(fd, CVI_CAMERA_IOCTL_STOP_ASYNC, 0) < 0) {
        perror("stop async capture");
        status = EXIT_FAILURE;
    }

    struct cvi_camera_capture_stats stats = {0};
    if (ioctl(fd, CVI_CAMERA_IOCTL_GET_CAPTURE_STATS, &stats) < 0) {
        perror("get capture stats");
        status = EXIT_FAILURE;
    } else {
        uint64_t average_us = stats.capture_calls == 0
                                  ? 0
                                  : stats.total_frame_us / stats.capture_calls;
        printf("CVI_CAMERA_STATS calls=%" PRIu64 " attempts=%" PRIu64
               " success=%" PRIu64 " failed=%" PRIu64
               " retries=%" PRIu64 " invalid=%" PRIu64
               " invalid_percent=%.2f usb_errors=%" PRIu64
               " published=%" PRIu64 " overwritten=%" PRIu64
               " overwrite_percent=%.2f avg_call_us=%" PRIu64
               " max_frame_us=%" PRIu64 "\n",
               stats.capture_calls, stats.transfer_attempts,
               stats.successful_frames, stats.failed_frames,
               stats.retry_attempts, stats.invalid_frames,
               ratio_percent(stats.invalid_frames, stats.transfer_attempts),
               stats.usb_errors, stats.published_frames,
               stats.overwritten_frames,
               ratio_percent(stats.overwritten_frames,
                             stats.published_frames),
               average_us, stats.max_frame_us);
        printf("CVI_CAMERA_PROFILE_CAPS total=%u validate=%u uvc_stages=%u\n",
               !!(stats.last_profile.capabilities &
                  CVI_CAMERA_PROFILE_UVC_TOTAL),
               !!(stats.last_profile.capabilities &
                  CVI_CAMERA_PROFILE_VALIDATE),
               !!(stats.last_profile.capabilities &
                  CVI_CAMERA_PROFILE_UVC_STAGES));
    }

out:
    if (fd >= 0) {
        close(fd);
    }
    free(jpeg);
    return status;
}
