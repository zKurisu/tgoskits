#define _POSIX_C_SOURCE 200809L

#include "sg2002_vpss.h"
#include "cvi_usb_camera.h"

#include <errno.h>
#include <fcntl.h>
#include <inttypes.h>
#include <linux/ioctl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>

#define ION_HEAP_DMA_COHERENT 1U
#define CAMERA_SOURCE_CAPACITY (2U * 1024U * 1024U)

struct ion_alloc_data {
    uint64_t len;
    uint32_t heap_id_mask;
    uint32_t flags;
    uint32_t fd;
    uint32_t unused;
    uint64_t paddr;
    uint8_t name[32];
};

#define ION_IOC_ALLOC _IOWR('I', 0, struct ion_alloc_data)

struct mapped_buffer {
    int fd;
    uint8_t *data;
    size_t size;
};

static uint64_t monotonic_ns(void)
{
    struct timespec value;

    if (clock_gettime(CLOCK_MONOTONIC, &value) != 0) {
        perror("clock_gettime");
        exit(EXIT_FAILURE);
    }
    return (uint64_t)value.tv_sec * UINT64_C(1000000000) + (uint64_t)value.tv_nsec;
}

static size_t align_up(size_t value, size_t alignment)
{
    return (value + alignment - 1U) & ~(alignment - 1U);
}

static struct mapped_buffer allocate_ion(int ion_fd, size_t requested_size, const char *name)
{
    struct ion_alloc_data allocation;
    struct mapped_buffer result;

    memset(&allocation, 0, sizeof(allocation));
    allocation.len = align_up(requested_size, 4096U);
    {
        /* 调试用：VPSS_ION_HEAP=<mask> 可切到 carveout(4)/system(8) 等堆，
           默认仍是原来的 DMA-coherent(2)。 */
        const char *heap_env = getenv("VPSS_ION_HEAP");

        allocation.heap_id_mask = heap_env
                                      ? (uint32_t)strtoul(heap_env, NULL, 0)
                                      : (1U << ION_HEAP_DMA_COHERENT);
    }
    (void)snprintf((char *)allocation.name, sizeof(allocation.name), "%s", name);
    if (ioctl(ion_fd, ION_IOC_ALLOC, &allocation) < 0) {
        perror("ION_IOC_ALLOC");
        exit(EXIT_FAILURE);
    }
    result.fd = (int)allocation.fd;
    result.size = (size_t)allocation.len;
    result.data = mmap(NULL, result.size, PROT_READ | PROT_WRITE, MAP_SHARED, result.fd, 0);
    if (result.data == MAP_FAILED) {
        perror("mmap ion buffer");
        close(result.fd);
        exit(EXIT_FAILURE);
    }
    return result;
}

static void release_buffer(struct mapped_buffer *buffer)
{
    if (buffer->data != MAP_FAILED) {
        (void)munmap(buffer->data, buffer->size);
        buffer->data = MAP_FAILED;
    }
    if (buffer->fd >= 0) {
        (void)close(buffer->fd);
        buffer->fd = -1;
    }
}

static int validate_constant_nv12(const uint8_t *buffer, size_t y_size, size_t uv_size)
{
    size_t index;

    for (index = 0; index < y_size; ++index) {
        if (buffer[index] < 94U || buffer[index] > 98U) {
            fprintf(stderr, "Y mismatch at %zu: %u\n", index, buffer[index]);
            return -1;
        }
    }
    for (index = 0; index < uv_size; ++index) {
        uint8_t value = buffer[y_size + index];

        if (value < 126U || value > 130U) {
            fprintf(stderr, "UV mismatch at %zu: %u\n", index, value);
            return -1;
        }
    }
    return 0;
}

static int validate_camera_output(const uint8_t *buffer, size_t size)
{
    size_t index;

    for (index = 0; index < size; ++index) {
        if (buffer[index] != 0xa5U) {
            return 0;
        }
    }
    fprintf(stderr, "camera output was not written by VPSS\n");
    return -1;
}

static int validate_rgb_letterbox(const uint8_t *buffer, uint32_t width,
                                  uint32_t height, uint32_t content_y,
                                  uint32_t content_height, int constant_input)
{
    size_t plane_size = (size_t)width * height;
    uint32_t plane;

    for (plane = 0; plane < 3U; ++plane) {
        const uint8_t *pixels = buffer + (size_t)plane * plane_size;
        uint32_t y;

        for (y = 0; y < height; ++y) {
            uint32_t x;
            int in_content = y >= content_y && y < content_y + content_height;

            for (x = 0; x < width; ++x) {
                uint8_t value = pixels[(size_t)y * width + x];

                if (!in_content && value != 0U) {
                    fprintf(stderr, "RGB border mismatch plane=%u x=%u y=%u value=%u\n",
                            plane, x, y, value);
                    return -1;
                }
                if (constant_input && in_content && (value < 88U || value > 100U)) {
                    fprintf(stderr, "RGB content mismatch plane=%u x=%u y=%u value=%u\n",
                            plane, x, y, value);
                    return -1;
                }
            }
        }
    }
    return 0;
}

static int validate_camera_rgb_content(const uint8_t *buffer, uint32_t width,
                                       uint32_t height, uint32_t content_y,
                                       uint32_t content_height)
{
    size_t plane_size = (size_t)width * height;
    uint32_t plane;

    for (plane = 0; plane < 3U; ++plane) {
        uint32_t y;
        const uint8_t *pixels = buffer + (size_t)plane * plane_size;

        for (y = content_y; y < content_y + content_height; ++y) {
            uint32_t x;

            for (x = 0; x < width; ++x) {
                if (pixels[(size_t)y * width + x] != 0xa5U) {
                    return 0;
                }
            }
        }
    }
    fprintf(stderr, "camera RGB content was not written by VPSS\n");
    return -1;
}

int main(int argc, char **argv)
{
    const char *device_path = argc > 1 ? argv[1] : "/dev/cvi-vpss0";
    uint64_t iterations = argc > 2 ? strtoull(argv[2], NULL, 10) : 1U;
    const char *input_name = argc > 3 ? argv[3] : "nv12";
    const char *camera_path = argc > 4 ? argv[4] : "/dev/cvi-usb-camera0";
    int use_rgb = strcmp(input_name, "rgb") == 0
                  || strcmp(input_name, "camera-rgb") == 0;
    int use_camera = strcmp(input_name, "camera") == 0
                     || strcmp(input_name, "camera-rgb") == 0;
    int use_yuv422p = strcmp(input_name, "yuv422p") == 0 || use_camera || use_rgb;
    const uint32_t source_width = 640U;
    const uint32_t source_height = 480U;
    const uint32_t destination_width = use_rgb ? 640U : 320U;
    const uint32_t destination_height = use_rgb ? 640U : 240U;
    const size_t source_y_size = (size_t)source_width * source_height;
    const size_t source_uv_size = source_y_size / 2U;
    const size_t source_c_size = source_y_size / 2U;
    const size_t source_size = use_yuv422p ? source_y_size + 2U * source_c_size
                                          : source_y_size + source_uv_size;
    const size_t source_capacity = use_camera ? CAMERA_SOURCE_CAPACITY : source_size;
    const size_t destination_y_size = (size_t)destination_width * destination_height;
    const size_t destination_uv_size = destination_y_size / 2U;
    const size_t destination_size = use_rgb ? 3U * destination_y_size
                                            : destination_y_size + destination_uv_size;
    struct cvi_vpss_info info;
    struct cvi_vpss_stats stats;
    struct cvi_vpss_run run;
    struct cvi_vpss_run_yuv422p run_yuv422p;
    struct cvi_vpss_run_yuv422p_rgb run_rgb;
    struct mapped_buffer source = {.fd = -1, .data = MAP_FAILED, .size = 0};
    struct mapped_buffer destination = {.fd = -1, .data = MAP_FAILED, .size = 0};
    uint64_t wall_start;
    uint64_t wall_end;
    uint64_t index;
    int ion_fd;
    int vpss_fd;
    int camera_fd = -1;
    int camera_started = 0;
    uint64_t camera_sequence = 0;
    uint64_t camera_request_total_us = 0;
    uint64_t camera_skipped_sequences = 0;
    int result = EXIT_FAILURE;

    if (iterations == 0U) {
        fprintf(stderr, "iterations must be positive\n");
        return EXIT_FAILURE;
    }
    if (!use_yuv422p && strcmp(input_name, "nv12") != 0) {
        fprintf(stderr, "input must be nv12, yuv422p, camera, rgb or camera-rgb\n");
        return EXIT_FAILURE;
    }
    ion_fd = open("/dev/ion", O_RDWR | O_CLOEXEC);
    if (ion_fd < 0) {
        perror("open /dev/ion");
        return EXIT_FAILURE;
    }
    vpss_fd = open(device_path, O_RDWR | O_CLOEXEC);
    if (vpss_fd < 0) {
        perror("open VPSS");
        (void)close(ion_fd);
        return EXIT_FAILURE;
    }
    if (use_camera) {
        camera_fd = open(camera_path, O_RDONLY | O_CLOEXEC);
        if (camera_fd < 0) {
            perror("open camera");
            goto out;
        }
        if (ioctl(camera_fd, CVI_CAMERA_IOCTL_INIT, 0) < 0
            || ioctl(camera_fd, CVI_CAMERA_IOCTL_RESET_CAPTURE_STATS, 0) < 0
            || ioctl(camera_fd, CVI_CAMERA_IOCTL_START_ASYNC, 0) < 0) {
            perror("initialize camera");
            goto out;
        }
        camera_started = 1;
    }
    memset(&info, 0, sizeof(info));
    if (ioctl(vpss_fd, CVI_VPSS_IOCTL_GET_INFO, &info) < 0) {
        perror("CVI_VPSS_IOCTL_GET_INFO");
        goto out;
    }
    {
        uint32_t required_features = CVI_VPSS_FEATURE_NV12 | CVI_VPSS_FEATURE_SCALE
                                     | CVI_VPSS_FEATURE_ION_FD | CVI_VPSS_FEATURE_IRQ;

        if (use_yuv422p) {
            required_features |= CVI_VPSS_FEATURE_YUV422P_INPUT;
        }
        if (use_rgb) {
            required_features |= CVI_VPSS_FEATURE_RGB_PLANAR_OUTPUT
                                 | CVI_VPSS_FEATURE_BORDER;
        }
        if (info.abi_version != CVI_VPSS_ABI_VERSION
            || (info.features & required_features) != required_features) {
            fprintf(stderr, "unsupported VPSS ABI/features: abi=%u features=%#x\n",
                    info.abi_version, info.features);
            goto out;
        }
    }

    source = allocate_ion(ion_fd, source_capacity, "vpss-source");
    destination = allocate_ion(ion_fd, destination_size, "vpss-destination");
    memset(source.data, 96, source_y_size);
    memset(source.data + source_y_size, 128, source_size - source_y_size);
    memset(destination.data, 0xa5, destination.size);

    memset(&run, 0, sizeof(run));
    run.abi_version = CVI_VPSS_ABI_VERSION;
    run.source_fd = source.fd;
    run.destination_fd = destination.fd;
    run.source_y_offset = 0;
    run.source_uv_offset = source_y_size;
    run.destination_y_offset = 0;
    run.destination_uv_offset = destination_y_size;
    run.source_width = source_width;
    run.source_height = source_height;
    run.source_y_stride = source_width;
    run.source_uv_stride = source_width;
    run.crop_width = source_width;
    run.crop_height = source_height;
    run.destination_width = destination_width;
    run.destination_height = destination_height;
    run.destination_y_stride = destination_width;
    run.destination_uv_stride = destination_width;
    run.timeout_ms = 100U;

    memset(&run_yuv422p, 0, sizeof(run_yuv422p));
    run_yuv422p.abi_version = CVI_VPSS_ABI_VERSION;
    run_yuv422p.source_fd = source.fd;
    run_yuv422p.destination_fd = destination.fd;
    run_yuv422p.source_y_offset = 0;
    run_yuv422p.source_cb_offset = source_y_size;
    run_yuv422p.source_cr_offset = source_y_size + source_c_size;
    run_yuv422p.destination_y_offset = 0;
    run_yuv422p.destination_uv_offset = destination_y_size;
    run_yuv422p.source_width = source_width;
    run_yuv422p.source_height = source_height;
    run_yuv422p.source_y_stride = source_width;
    run_yuv422p.source_c_stride = source_width / 2U;
    run_yuv422p.crop_width = source_width;
    run_yuv422p.crop_height = source_height;
    run_yuv422p.destination_width = destination_width;
    run_yuv422p.destination_height = destination_height;
    run_yuv422p.destination_y_stride = destination_width;
    run_yuv422p.destination_uv_stride = destination_width;
    run_yuv422p.timeout_ms = 100U;

    memset(&run_rgb, 0, sizeof(run_rgb));
    run_rgb.abi_version = CVI_VPSS_ABI_VERSION;
    run_rgb.source_fd = source.fd;
    run_rgb.destination_fd = destination.fd;
    run_rgb.source_y_offset = 0;
    run_rgb.source_cb_offset = source_y_size;
    run_rgb.source_cr_offset = source_y_size + source_c_size;
    run_rgb.destination_r_offset = 0;
    run_rgb.destination_g_offset = destination_y_size;
    run_rgb.destination_b_offset = 2U * destination_y_size;
    run_rgb.source_width = source_width;
    run_rgb.source_height = source_height;
    run_rgb.source_y_stride = source_width;
    run_rgb.source_c_stride = source_width / 2U;
    run_rgb.crop_width = source_width;
    run_rgb.crop_height = source_height;
    run_rgb.content_x = 0U;
    run_rgb.content_y = 80U;
    run_rgb.content_width = 640U;
    run_rgb.content_height = 480U;
    run_rgb.destination_width = destination_width;
    run_rgb.destination_height = destination_height;
    run_rgb.destination_r_stride = destination_width;
    run_rgb.destination_gb_stride = destination_width;
    run_rgb.border_rgb = 0U;
    run_rgb.timeout_ms = 100U;

    (void)ioctl(vpss_fd, CVI_VPSS_IOCTL_RESET_STATS, 0);
    wall_start = monotonic_ns();
    for (index = 0; index < iterations; ++index) {
        int ioctl_result;
        int32_t status;
        uint32_t irq_status;
        uint32_t img_debug;
        uint32_t img_axi_status;
        uint32_t scaler_status;
        uint32_t odma_debug;
        uint64_t output_sequence;
        uint64_t output_timestamp_ns;
        uint64_t sequence = index + 1U;
        uint64_t timestamp_ns = monotonic_ns();

        if (use_camera) {
            struct cvi_camera_ion_frame_request camera_request;
            uint64_t camera_start_ns;

            memset(&camera_request, 0, sizeof(camera_request));
            camera_request.abi_version = CVI_CAMERA_ION_ABI_VERSION;
            camera_request.ion_fd = source.fd;
            camera_request.timeout_ms = 2000U;
            camera_request.capacity = source.size;
            camera_request.last_sequence = camera_sequence;
            camera_start_ns = monotonic_ns();
            if (ioctl(camera_fd, CVI_CAMERA_IOCTL_GET_LATEST_YUV_ION,
                      &camera_request)
                < 0) {
                perror("CVI_CAMERA_IOCTL_GET_LATEST_YUV_ION");
                goto out;
            }
            camera_request_total_us += (monotonic_ns() - camera_start_ns) / 1000U;
            if (camera_request.format != CVI_CAMERA_FORMAT_YUV422_PLANAR) {
                fprintf(stderr,
                        "camera returned format=%u, expected planar YUV422 (%u)\n",
                        camera_request.format, CVI_CAMERA_FORMAT_YUV422_PLANAR);
                goto out;
            }
            if (camera_sequence != 0U && camera_request.sequence > camera_sequence + 1U) {
                camera_skipped_sequences += camera_request.sequence - camera_sequence - 1U;
            }
            camera_sequence = camera_request.sequence;
            sequence = camera_request.sequence;
            timestamp_ns = camera_request.timestamp_ns;
            run_yuv422p.source_y_offset = camera_request.y_offset;
            run_yuv422p.source_cb_offset = camera_request.cb_offset;
            run_yuv422p.source_cr_offset = camera_request.cr_offset;
            run_yuv422p.source_width = camera_request.width;
            run_yuv422p.source_height = camera_request.height;
            run_yuv422p.source_y_stride = camera_request.stride_y;
            run_yuv422p.source_c_stride = camera_request.stride_c;
            run_yuv422p.crop_width = camera_request.width;
            run_yuv422p.crop_height = camera_request.height;
            run_rgb.source_y_offset = camera_request.y_offset;
            run_rgb.source_cb_offset = camera_request.cb_offset;
            run_rgb.source_cr_offset = camera_request.cr_offset;
            run_rgb.source_width = camera_request.width;
            run_rgb.source_height = camera_request.height;
            run_rgb.source_y_stride = camera_request.stride_y;
            run_rgb.source_c_stride = camera_request.stride_c;
            run_rgb.crop_width = camera_request.width;
            run_rgb.crop_height = camera_request.height;
        }

        if (use_rgb) {
            run_rgb.sequence = sequence;
            run_rgb.timestamp_ns = timestamp_ns;
            ioctl_result = ioctl(vpss_fd, CVI_VPSS_IOCTL_RUN_YUV422P_RGB, &run_rgb);
            status = run_rgb.status;
            irq_status = run_rgb.irq_status;
            img_debug = run_rgb.img_debug;
            img_axi_status = run_rgb.img_axi_status;
            scaler_status = run_rgb.scaler_status;
            odma_debug = run_rgb.odma_debug;
            output_sequence = run_rgb.output_sequence;
            output_timestamp_ns = run_rgb.output_timestamp_ns;
        } else if (use_yuv422p) {
            run_yuv422p.sequence = sequence;
            run_yuv422p.timestamp_ns = timestamp_ns;
            ioctl_result = ioctl(vpss_fd, CVI_VPSS_IOCTL_RUN_YUV422P, &run_yuv422p);
            status = run_yuv422p.status;
            irq_status = run_yuv422p.irq_status;
            img_debug = run_yuv422p.img_debug;
            img_axi_status = run_yuv422p.img_axi_status;
            scaler_status = run_yuv422p.scaler_status;
            odma_debug = run_yuv422p.odma_debug;
            output_sequence = run_yuv422p.output_sequence;
            output_timestamp_ns = run_yuv422p.output_timestamp_ns;
        } else {
            run.sequence = sequence;
            run.timestamp_ns = timestamp_ns;
            ioctl_result = ioctl(vpss_fd, CVI_VPSS_IOCTL_RUN, &run);
            status = run.status;
            irq_status = run.irq_status;
            img_debug = run.img_debug;
            img_axi_status = run.img_axi_status;
            scaler_status = run.scaler_status;
            odma_debug = run.odma_debug;
            output_sequence = run.output_sequence;
            output_timestamp_ns = run.output_timestamp_ns;
        }
        if (ioctl_result < 0) {
            fprintf(stderr,
                    "VPSS run failed at #%" PRIu64
                    ": errno=%d status=%d irq=%#x img_dbg=%#x axi=%#x sc=%#x odma=%#x\n",
                    index + 1U, errno, status, irq_status, img_debug, img_axi_status,
                    scaler_status, odma_debug);
            goto out;
        }
        if (output_sequence != sequence || output_timestamp_ns != timestamp_ns) {
            fprintf(stderr, "metadata mismatch at #%" PRIu64 "\n", index + 1U);
            goto out;
        }
        if (index == 0U || index + 1U == iterations || (index + 1U) % 100U == 0U) {
            int validation;

            if (use_rgb) {
                validation = validate_rgb_letterbox(destination.data, destination_width,
                                                    destination_height, 80U, 480U,
                                                    !use_camera);
                if (validation == 0 && use_camera) {
                    validation = validate_camera_rgb_content(destination.data,
                                                             destination_width,
                                                             destination_height, 80U, 480U);
                }
            } else if (use_camera) {
                validation = validate_camera_output(destination.data, destination_size);
            } else {
                validation = validate_constant_nv12(destination.data, destination_y_size,
                                                    destination_uv_size);
            }
            if (validation != 0) {
                fprintf(stderr, "pixel validation failed at #%" PRIu64 "\n", index + 1U);
                goto out;
            }
        }
    }
    wall_end = monotonic_ns();
    memset(&stats, 0, sizeof(stats));
    if (ioctl(vpss_fd, CVI_VPSS_IOCTL_GET_STATS, &stats) < 0) {
        perror("CVI_VPSS_IOCTL_GET_STATS");
        goto out;
    }
    printf("VPSS_PASS input=%s frames=%" PRIu64
           " wall_avg_us=%" PRIu64 " hw_avg_us=%" PRIu64
           " hw_last_us=%" PRIu64 " hw_max_us=%" PRIu64
           " irq=%" PRIu64 " late=%" PRIu64 " timeout=%" PRIu64
           " spurious=%" PRIu64 "\n",
           input_name, iterations, (wall_end - wall_start) / iterations / 1000U,
           stats.total_elapsed_ns / stats.submitted_jobs / 1000U,
           stats.last_elapsed_ns / 1000U, stats.max_elapsed_ns / 1000U,
           stats.irq_count, stats.program_late_errors, stats.timeout_errors,
           stats.spurious_irqs);
    if (use_camera) {
        printf("VPSS_CAMERA frames=%" PRIu64
               " request_avg_us=%" PRIu64 " last_sequence=%" PRIu64
               " skipped_sequences=%" PRIu64 "\n",
               iterations, camera_request_total_us / iterations, camera_sequence,
               camera_skipped_sequences);
    }
    result = EXIT_SUCCESS;

out:
    if (camera_started && ioctl(camera_fd, CVI_CAMERA_IOCTL_STOP_ASYNC, 0) < 0) {
        perror("stop camera");
        result = EXIT_FAILURE;
    }
    if (camera_fd >= 0) {
        (void)close(camera_fd);
    }
    release_buffer(&destination);
    release_buffer(&source);
    (void)close(vpss_fd);
    (void)close(ion_fd);
    return result;
}
