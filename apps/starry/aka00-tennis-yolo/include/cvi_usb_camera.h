#ifndef CVI_USB_CAMERA_H
#define CVI_USB_CAMERA_H

#include <stdint.h>

/* Existing synchronous ABI. */
#define CVI_CAMERA_IOCTL_INIT 1U
#define CVI_CAMERA_IOCTL_GET_INFO 2U
#define CVI_CAMERA_IOCTL_GET_FRAME 3U
#define CVI_CAMERA_IOCTL_GET_YUV_FRAME 4U
#define CVI_CAMERA_IOCTL_HARD_RESET 5U

/* Asynchronous latest-frame and profiling ABI. */
#define CVI_CAMERA_IOCTL_START_ASYNC 6U
#define CVI_CAMERA_IOCTL_STOP_ASYNC 7U
#define CVI_CAMERA_IOCTL_GET_LATEST_FRAME 8U
#define CVI_CAMERA_IOCTL_GET_CAPTURE_STATS 9U
#define CVI_CAMERA_IOCTL_RESET_CAPTURE_STATS 10U
#define CVI_CAMERA_IOCTL_GET_LATEST_YUV_FRAME 11U
#define CVI_CAMERA_IOCTL_GET_LATEST_NV12_FRAME 12U
#define CVI_CAMERA_IOCTL_GET_LATEST_YUV_ION 13U

#define CVI_CAMERA_ION_ABI_VERSION 1U

#define CVI_CAMERA_FRAME_NONBLOCK (1U << 0)

#define CVI_CAMERA_FORMAT_MJPEG 1U
#define CVI_CAMERA_FORMAT_YUV420_PLANAR 2U
#define CVI_CAMERA_FORMAT_YUV422_PLANAR 3U
#define CVI_CAMERA_FORMAT_YUV440_PLANAR 4U
#define CVI_CAMERA_FORMAT_YUV444_PLANAR 5U
#define CVI_CAMERA_FORMAT_YUV400 6U
#define CVI_CAMERA_FORMAT_NV12 7U

#define CVI_CAMERA_PROFILE_UVC_TOTAL (1U << 0)
#define CVI_CAMERA_PROFILE_VALIDATE (1U << 1)
#define CVI_CAMERA_PROFILE_UVC_STAGES (1U << 2)
#define CVI_CAMERA_PROFILE_UNKNOWN_US UINT64_MAX

struct cvi_camera_capture_profile {
    uint32_t capabilities;
    uint32_t attempts;
    uint64_t frame_total_us;
    uint64_t uvc_total_us;
    uint64_t wait_first_packet_us;
    uint64_t usb_transfer_us;
    uint64_t jpeg_assemble_us;
    uint64_t validate_us;
};

struct cvi_camera_capture_stats {
    uint64_t capture_calls;
    uint64_t transfer_attempts;
    uint64_t successful_frames;
    uint64_t failed_frames;
    uint64_t retry_attempts;
    uint64_t invalid_frames;
    uint64_t invalid_soi;
    uint64_t invalid_eoi;
    uint64_t invalid_too_small;
    uint64_t usb_errors;
    uint64_t published_frames;
    uint64_t overwritten_frames;
    uint64_t total_frame_us;
    uint64_t max_frame_us;
    struct cvi_camera_capture_profile last_profile;
};

/*
 * Input fields: buffer, capacity, last_sequence, timeout_ms and flags.
 * Output fields start at sequence. On an undersized buffer the driver writes
 * the required byte count to length before returning EINVAL.
 */
struct cvi_camera_frame_request {
    uint64_t buffer;
    uint64_t capacity;
    uint64_t last_sequence;
    uint32_t timeout_ms;
    uint32_t flags;
    uint64_t sequence;
    uint64_t timestamp_ns;
    uint32_t length;
    uint16_t width;
    uint16_t height;
    uint8_t format;
    uint8_t reserved[3];
    struct cvi_camera_capture_profile profile;
};

/*
 * Decode the latest JPEG directly into a coherent ION allocation. Input:
 * abi_version, flags, ion_fd, timeout_ms, buffer_offset, capacity and
 * last_sequence. Output plane offsets are relative to the same ION fd and can
 * be passed directly to CVI_VPSS_IOCTL_RUN_YUV422P. Userspace must not access
 * the selected ION range while the ioctl is running.
 */
struct cvi_camera_ion_frame_request {
    uint32_t abi_version;
    uint32_t flags;
    int32_t ion_fd;
    uint32_t timeout_ms;
    uint64_t buffer_offset;
    uint64_t capacity;
    uint64_t last_sequence;

    uint64_t sequence;
    uint64_t timestamp_ns;
    uint64_t y_offset;
    uint64_t cb_offset;
    uint64_t cr_offset;
    uint32_t length;
    uint32_t stride_y;
    uint32_t stride_c;
    uint16_t width;
    uint16_t height;
    uint8_t format;
    uint8_t reserved[3];
    struct cvi_camera_capture_profile profile;
};

_Static_assert(sizeof(struct cvi_camera_capture_profile) == 56,
               "camera profile ABI mismatch");
_Static_assert(sizeof(struct cvi_camera_capture_stats) == 168,
               "camera stats ABI mismatch");
_Static_assert(sizeof(struct cvi_camera_frame_request) == 120,
               "camera frame request ABI mismatch");
_Static_assert(sizeof(struct cvi_camera_ion_frame_request) == 160,
               "camera ION frame request ABI mismatch");

#endif
