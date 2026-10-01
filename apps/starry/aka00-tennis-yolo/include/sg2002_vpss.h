#ifndef SG2002_VPSS_H
#define SG2002_VPSS_H

#include <linux/ioctl.h>
#include <stdint.h>

#define CVI_VPSS_ABI_VERSION 1U

#define CVI_VPSS_FEATURE_NV12 (1U << 0)
#define CVI_VPSS_FEATURE_SCALE (1U << 1)
#define CVI_VPSS_FEATURE_BT601_CSC (1U << 2)
#define CVI_VPSS_FEATURE_ION_FD (1U << 3)
#define CVI_VPSS_FEATURE_IRQ (1U << 4)
#define CVI_VPSS_FEATURE_YUV422P_INPUT (1U << 5)
#define CVI_VPSS_FEATURE_RGB_PLANAR_OUTPUT (1U << 6)
#define CVI_VPSS_FEATURE_BORDER (1U << 7)

struct cvi_vpss_run {
    uint32_t abi_version;
    uint32_t flags;
    int32_t source_fd;
    int32_t destination_fd;

    uint64_t source_y_offset;
    uint64_t source_uv_offset;
    uint64_t destination_y_offset;
    uint64_t destination_uv_offset;
    uint64_t sequence;
    uint64_t timestamp_ns;

    uint32_t source_width;
    uint32_t source_height;
    uint32_t source_y_stride;
    uint32_t source_uv_stride;
    uint32_t crop_x;
    uint32_t crop_y;
    uint32_t crop_width;
    uint32_t crop_height;
    uint32_t destination_width;
    uint32_t destination_height;
    uint32_t destination_y_stride;
    uint32_t destination_uv_stride;

    uint32_t timeout_ms;
    int32_t status;
    uint32_t irq_status;
    uint32_t reserved0;

    uint64_t queue_enter_ns;
    uint64_t hardware_start_ns;
    uint64_t hardware_done_ns;
    uint64_t elapsed_ns;
    uint64_t output_sequence;
    uint64_t output_timestamp_ns;

    uint32_t img_debug;
    uint32_t img_axi_status;
    uint32_t scaler_status;
    uint32_t odma_debug;
    uint32_t reserved[4];
};

struct cvi_vpss_run_yuv422p {
    uint32_t abi_version;
    uint32_t flags;
    int32_t source_fd;
    int32_t destination_fd;

    uint64_t source_y_offset;
    uint64_t source_cb_offset;
    uint64_t source_cr_offset;
    uint64_t destination_y_offset;
    uint64_t destination_uv_offset;
    uint64_t sequence;
    uint64_t timestamp_ns;

    uint32_t source_width;
    uint32_t source_height;
    uint32_t source_y_stride;
    uint32_t source_c_stride;
    uint32_t crop_x;
    uint32_t crop_y;
    uint32_t crop_width;
    uint32_t crop_height;
    uint32_t destination_width;
    uint32_t destination_height;
    uint32_t destination_y_stride;
    uint32_t destination_uv_stride;

    uint32_t timeout_ms;
    int32_t status;
    uint32_t irq_status;
    uint32_t reserved0;

    uint64_t queue_enter_ns;
    uint64_t hardware_start_ns;
    uint64_t hardware_done_ns;
    uint64_t elapsed_ns;
    uint64_t output_sequence;
    uint64_t output_timestamp_ns;

    uint32_t img_debug;
    uint32_t img_axi_status;
    uint32_t scaler_status;
    uint32_t odma_debug;
    uint32_t reserved[4];
};

struct cvi_vpss_run_yuv422p_rgb {
    uint32_t abi_version;
    uint32_t flags;
    int32_t source_fd;
    int32_t destination_fd;

    uint64_t source_y_offset;
    uint64_t source_cb_offset;
    uint64_t source_cr_offset;
    uint64_t destination_r_offset;
    uint64_t destination_g_offset;
    uint64_t destination_b_offset;
    uint64_t sequence;
    uint64_t timestamp_ns;

    uint32_t source_width;
    uint32_t source_height;
    uint32_t source_y_stride;
    uint32_t source_c_stride;
    uint32_t crop_x;
    uint32_t crop_y;
    uint32_t crop_width;
    uint32_t crop_height;
    uint32_t content_x;
    uint32_t content_y;
    uint32_t content_width;
    uint32_t content_height;
    uint32_t destination_width;
    uint32_t destination_height;
    uint32_t destination_r_stride;
    uint32_t destination_gb_stride;
    uint32_t border_rgb;

    uint32_t timeout_ms;
    int32_t status;
    uint32_t irq_status;
    uint32_t reserved0;

    uint64_t queue_enter_ns;
    uint64_t hardware_start_ns;
    uint64_t hardware_done_ns;
    uint64_t elapsed_ns;
    uint64_t output_sequence;
    uint64_t output_timestamp_ns;

    uint32_t img_debug;
    uint32_t img_axi_status;
    uint32_t scaler_status;
    uint32_t odma_debug;
    uint32_t reserved[4];
};

struct cvi_vpss_info {
    uint32_t abi_version;
    uint32_t features;
    uint32_t min_dimension;
    uint32_t max_dimension;
    uint32_t stride_alignment;
    uint32_t reserved0;
    uint64_t mmio_physical;
    uint64_t mmio_size;
    uint32_t irq_domain;
    uint32_t irq_hwirq;
    uint32_t reserved[2];
};

struct cvi_vpss_stats {
    uint64_t irq_count;
    uint64_t completed_jobs;
    uint64_t program_late_errors;
    uint64_t timeout_errors;
    uint64_t spurious_irqs;
    uint32_t last_irq_status;
    uint32_t reserved0;
    uint64_t submitted_jobs;
    uint64_t failed_jobs;
    uint64_t total_elapsed_ns;
    uint64_t last_elapsed_ns;
    uint64_t max_elapsed_ns;
};

#define CVI_VPSS_IOCTL_RUN _IOWR('V', 1, struct cvi_vpss_run)
#define CVI_VPSS_IOCTL_GET_INFO _IOR('V', 2, struct cvi_vpss_info)
#define CVI_VPSS_IOCTL_GET_STATS _IOR('V', 3, struct cvi_vpss_stats)
#define CVI_VPSS_IOCTL_RESET_STATS _IO('V', 4)
#define CVI_VPSS_IOCTL_RUN_YUV422P _IOWR('V', 5, struct cvi_vpss_run_yuv422p)
#define CVI_VPSS_IOCTL_RUN_YUV422P_RGB \
    _IOWR('V', 6, struct cvi_vpss_run_yuv422p_rgb)

_Static_assert(sizeof(struct cvi_vpss_run) == 208, "cvi_vpss_run ABI size");
_Static_assert(sizeof(struct cvi_vpss_run_yuv422p) == 216,
               "cvi_vpss_run_yuv422p ABI size");
_Static_assert(sizeof(struct cvi_vpss_run_yuv422p_rgb) == 248,
               "cvi_vpss_run_yuv422p_rgb ABI size");
_Static_assert(sizeof(struct cvi_vpss_info) == 56, "cvi_vpss_info ABI size");
_Static_assert(sizeof(struct cvi_vpss_stats) == 88, "cvi_vpss_stats ABI size");

#endif
