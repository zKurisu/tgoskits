/*
 * wifi_switch — runtime Wi-Fi mode switch demo for StarryOS (sg2002 / aic8800).
 *
 * Drives the kernel's wireless-extensions ioctl path (see
 * os/StarryOS/kernel/src/file/wext.rs) to switch the wlan0 interface between
 * Station and SoftAP at runtime. Setters stage config; SIOCSIWCOMMIT applies it
 * atomically (link-layer VIF teardown + switch + IP/DHCP role reconfig).
 *
 * Build (riscv64, musl static — matches the other sg2002 rootfs binaries):
 *   riscv64-linux-musl-gcc -static -O2 -o wifi_switch wifi_switch.c
 * Then drop it into the p3 rootfs at /usr/bin/wifi_switch (chmod +x) alongside
 * tennis/test_motor/etc. See docs/sd-card-build.md.
 *
 * Usage on the board:
 *   wifi_switch ap   <ssid> [channel]      # become open SoftAP (default ch 6)
 *   wifi_switch sta  <ssid> [passphrase]   # join a network in station mode
 *   wifi_switch --selftest                 # check the PBKDF2 vector, no ioctl
 *
 * 关于 WPA2：内核的 wext.rs **只接受 PMK**，不收明文口令
 * （见它自己的测试 `encode_ext_uses_the_linux_pmk_layout_and_rejects_raw_passwords`）。
 * 所以本工具按 Linux/wpa_supplicant 的老规矩，在用户态把
 * `PMK = PBKDF2-HMAC-SHA1(passphrase, ssid, 4096, 32)` 算出来，再填进
 * `iw_encode_ext { ..., u16 alg = IW_ENCODE_ALG_PMK; u16 key_len = 32; u8 key[32]; }`。
 * `--selftest` 用 IEEE 802.11i 的标准向量核对这段实现。
 *
 * We deliberately avoid <linux/wireless.h> (the cross toolchain may lack it)
 * and lay out `struct iwreq` by hand. The layout below MUST match wext.rs:
 *   - ifr name      : offset 0,  16 bytes
 *   - iwreq_data    : offset 16, 16-byte union
 *   - MODE / FREQ   : first u32 of the union
 *   - ESSID/ENCODE  : iw_point { void *pointer; __u16 length; __u16 flags; }
 *                     pointer @ union+0 (8B on rv64), length @ union+8
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <unistd.h>
#include <errno.h>
#include <sys/ioctl.h>
#include <sys/socket.h>

/* Wireless-extensions ioctl numbers (from <linux/wireless.h>). */
#define SIOCSIWCOMMIT     0x8B00
#define SIOCSIWFREQ       0x8B04
#define SIOCSIWMODE       0x8B06
#define SIOCSIWESSID      0x8B1A
#define SIOCSIWENCODEEXT  0x8B34

/* iw_mode values. */
#define IW_MODE_INFRA     2  /* Managed / Station */
#define IW_MODE_MASTER    3  /* Master  / Access Point */

#define IFNAMSIZ          16
#define IW_ESSID_MAX_SIZE 32

/* Hand-rolled iw_point: { void *pointer; __u16 length; __u16 flags; }. */
struct iw_point_compat {
    void    *pointer;
    uint16_t length;
    uint16_t flags;
};

/*
 * Hand-rolled iwreq: 16-byte name union, then a 16-byte iwreq_data union.
 * We only ever use the u32 field (mode/freq) or the iw_point field (essid/key).
 */
struct iwreq_compat {
    char ifrn_name[IFNAMSIZ];
    union {
        uint32_t                mode;     /* SIOCSIWMODE / SIOCSIWFREQ */
        struct iw_point_compat  essid;    /* SIOCSIWESSID / ...ENCODEEXT */
        char                    pad[16];  /* keep the union exactly 16 bytes */
    } u;
};

static int wext(int fd, unsigned long cmd, struct iwreq_compat *req) {
    if (ioctl(fd, cmd, req) < 0) {
        fprintf(stderr, "ioctl 0x%lx failed: %s\n", cmd, strerror(errno));
        return -1;
    }
    return 0;
}

static void set_ifname(struct iwreq_compat *req, const char *ifname) {
    memset(req, 0, sizeof(*req));
    strncpy(req->ifrn_name, ifname, IFNAMSIZ - 1);
}

static int do_set_mode(int fd, const char *ifname, uint32_t mode) {
    struct iwreq_compat req;
    set_ifname(&req, ifname);
    req.u.mode = mode;
    return wext(fd, SIOCSIWMODE, &req);
}

static int do_set_essid(int fd, const char *ifname, const char *ssid) {
    struct iwreq_compat req;
    size_t len = strlen(ssid);
    if (len > IW_ESSID_MAX_SIZE) {
        fprintf(stderr, "ssid too long (max %d)\n", IW_ESSID_MAX_SIZE);
        return -1;
    }
    set_ifname(&req, ifname);
    req.u.essid.pointer = (void *)ssid;
    req.u.essid.length = (uint16_t)len;
    req.u.essid.flags = 1; /* SSID active */
    return wext(fd, SIOCSIWESSID, &req);
}

/* ── SHA-1 / HMAC-SHA1 / PBKDF2 ─────────────────────────────────────────
 * 只为了从口令推出 WPA2 的 PMK。自己带一份实现，是为了不依赖 openssl/libcrypto
 * （静态 musl 交叉工具链里没有），也避免把口令交给外部程序。 */
typedef struct {
    uint32_t h[5];
    unsigned char buf[64];
    size_t n;
    uint64_t bytes;
} sha1_ctx;

static uint32_t rol32(uint32_t x, int n) { return (x << n) | (x >> (32 - n)); }

static void sha1_block(sha1_ctx *s, const unsigned char *p) {
    uint32_t w[80];
    for (int i = 0; i < 16; i++)
        w[i] = (uint32_t)p[i * 4] << 24 | (uint32_t)p[i * 4 + 1] << 16 |
               (uint32_t)p[i * 4 + 2] << 8 | (uint32_t)p[i * 4 + 3];
    for (int i = 16; i < 80; i++)
        w[i] = rol32(w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16], 1);

    uint32_t a = s->h[0], b = s->h[1], c = s->h[2], d = s->h[3], e = s->h[4];
    for (int i = 0; i < 80; i++) {
        uint32_t f, k;
        if (i < 20)      { f = (b & c) | (~b & d);          k = 0x5A827999u; }
        else if (i < 40) { f = b ^ c ^ d;                   k = 0x6ED9EBA1u; }
        else if (i < 60) { f = (b & c) | (b & d) | (c & d); k = 0x8F1BBCDCu; }
        else             { f = b ^ c ^ d;                   k = 0xCA62C1D6u; }
        uint32_t tmp = rol32(a, 5) + f + e + k + w[i];
        e = d; d = c; c = rol32(b, 30); b = a; a = tmp;
    }
    s->h[0] += a; s->h[1] += b; s->h[2] += c; s->h[3] += d; s->h[4] += e;
}

static void sha1_init(sha1_ctx *s) {
    s->h[0] = 0x67452301u; s->h[1] = 0xEFCDAB89u; s->h[2] = 0x98BADCFEu;
    s->h[3] = 0x10325476u; s->h[4] = 0xC3D2E1F0u;
    s->n = 0; s->bytes = 0;
}

static void sha1_update(sha1_ctx *s, const void *data, size_t len) {
    const unsigned char *p = data;
    s->bytes += len;
    while (len) {
        size_t take = 64 - s->n;
        if (take > len) take = len;
        memcpy(s->buf + s->n, p, take);
        s->n += take; p += take; len -= take;
        if (s->n == 64) { sha1_block(s, s->buf); s->n = 0; }
    }
}

static void sha1_final(sha1_ctx *s, unsigned char out[20]) {
    uint64_t bits = s->bytes * 8;
    unsigned char pad = 0x80;
    sha1_update(s, &pad, 1);
    unsigned char zero = 0;
    while (s->n != 56) sha1_update(s, &zero, 1);
    unsigned char len[8];
    for (int i = 0; i < 8; i++) len[i] = (unsigned char)(bits >> (56 - 8 * i));
    sha1_update(s, len, 8);
    for (int i = 0; i < 5; i++) {
        out[i * 4]     = (unsigned char)(s->h[i] >> 24);
        out[i * 4 + 1] = (unsigned char)(s->h[i] >> 16);
        out[i * 4 + 2] = (unsigned char)(s->h[i] >> 8);
        out[i * 4 + 3] = (unsigned char)(s->h[i]);
    }
}

static void hmac_sha1(const unsigned char *key, size_t keylen,
                      const unsigned char *msg, size_t msglen,
                      unsigned char out[20]) {
    unsigned char k[64], ipad[64], opad[64], inner[20];
    memset(k, 0, sizeof k);
    if (keylen > 64) {
        sha1_ctx t; sha1_init(&t); sha1_update(&t, key, keylen); sha1_final(&t, k);
    } else {
        memcpy(k, key, keylen);
    }
    for (int i = 0; i < 64; i++) { ipad[i] = k[i] ^ 0x36; opad[i] = k[i] ^ 0x5c; }
    sha1_ctx s;
    sha1_init(&s); sha1_update(&s, ipad, 64); sha1_update(&s, msg, msglen); sha1_final(&s, inner);
    sha1_init(&s); sha1_update(&s, opad, 64); sha1_update(&s, inner, 20); sha1_final(&s, out);
}

/// WPA2 的 PMK：PBKDF2-HMAC-SHA1(passphrase, ssid, 4096, 32)
static int wpa2_pmk(const char *pass, const char *ssid, unsigned char pmk[32]) {
    size_t saltlen = strlen(ssid);
    if (saltlen > 32) return -1;              /* WPA 规定 SSID 最长 32 字节 */
    unsigned char salt[36];
    memcpy(salt, ssid, saltlen);
    salt[saltlen + 0] = 0; salt[saltlen + 1] = 0;
    salt[saltlen + 2] = 0; salt[saltlen + 3] = 1;   /* block index 1，大端 */

    unsigned char u[20], t[20];
    hmac_sha1((const unsigned char *)pass, strlen(pass), salt, saltlen + 4, u);
    memcpy(t, u, 20);
    for (int i = 1; i < 4096; i++) {
        hmac_sha1((const unsigned char *)pass, strlen(pass), u, 20, u);
        for (int j = 0; j < 20; j++) t[j] ^= u[j];
    }
    memcpy(pmk, t, 20);
    /* 32 字节的 PMK 要两块：第二块把 block index 换成 2 */
    salt[saltlen + 3] = 2;
    hmac_sha1((const unsigned char *)pass, strlen(pass), salt, saltlen + 4, u);
    memcpy(t, u, 20);
    for (int i = 1; i < 4096; i++) {
        hmac_sha1((const unsigned char *)pass, strlen(pass), u, 20, u);
        for (int j = 0; j < 20; j++) t[j] ^= u[j];
    }
    memcpy(pmk + 20, t, 12);
    return 0;
}

/* 内核 wext.rs 的 parse_pmk_encode_ext() 要的布局：
 *   [0..36)  留空（ext_flags/addr/reserved）
 *   [36..38) alg     = IW_ENCODE_ALG_PMK(4)，本机字节序
 *   [38..40) key_len = 32，本机字节序
 *   [40..72) PMK
 */
#define IW_ENCODE_EXT_HEADER_SIZE 40
#define IW_ENCODE_ALG_PMK 4

static int do_set_key(int fd, const char *ifname, const char *ssid, const char *pass) {
    unsigned char encoded[IW_ENCODE_EXT_HEADER_SIZE + 32];
    memset(encoded, 0, sizeof encoded);
    if (wpa2_pmk(pass, ssid, encoded + IW_ENCODE_EXT_HEADER_SIZE) != 0) {
        fprintf(stderr, "SSID 太长（WPA 上限 32 字节）\n");
        return -1;
    }
    uint16_t alg = IW_ENCODE_ALG_PMK, key_len = 32;
    memcpy(encoded + 36, &alg, sizeof alg);
    memcpy(encoded + 38, &key_len, sizeof key_len);

    struct iwreq_compat req;
    set_ifname(&req, ifname);
    req.u.essid.pointer = (void *)encoded;
    req.u.essid.length = (uint16_t)sizeof encoded;
    req.u.essid.flags = 0;
    return wext(fd, SIOCSIWENCODEEXT, &req);
}

/// IEEE 802.11i 的标准向量：SSID "IEEE" + 口令 "password" → 已知 PMK
static int selftest(void) {
    static const unsigned char want[32] = {
        0xf4, 0x2c, 0x6f, 0xc5, 0x2d, 0xf0, 0xeb, 0xef, 0x9e, 0xbb, 0x4b,
        0x90, 0xb3, 0x8a, 0x5f, 0x90, 0x2e, 0x83, 0xfe, 0x1b, 0x13, 0x5a,
        0x70, 0xe2, 0x3a, 0xed, 0x76, 0x2e, 0x97, 0x10, 0xa1, 0x2e,
    };
    unsigned char got[32];
    if (wpa2_pmk("password", "IEEE", got) != 0) return 1;
    for (int i = 0; i < 32; i++) printf("%02x", got[i]);
    printf("\n");
    if (memcmp(got, want, 32) != 0) {
        fprintf(stderr, "PBKDF2 自检失败\n");
        return 1;
    }
    printf("PBKDF2-HMAC-SHA1 selftest OK\n");
    return 0;
}

static int do_set_channel(int fd, const char *ifname, uint32_t chan) {
    struct iwreq_compat req;
    set_ifname(&req, ifname);
    req.u.mode = chan; /* wext.rs reads first u32 of the union as channel */
    return wext(fd, SIOCSIWFREQ, &req);
}

static int do_commit(int fd, const char *ifname) {
    struct iwreq_compat req;
    set_ifname(&req, ifname);
    return wext(fd, SIOCSIWCOMMIT, &req);
}

static void usage(const char *argv0) {
    fprintf(stderr,
        "usage:\n"
        "  %s ap  <ssid> [channel]      become open SoftAP (default channel 6)\n"
        "  %s sta <ssid> [passphrase]   join a network in station mode\n",
        argv0, argv0);
}

int main(int argc, char **argv) {
    const char *ifname = "wlan0";

    if (argc >= 2 && strcmp(argv[1], "--selftest") == 0)
        return selftest();

    if (argc < 3) {
        usage(argv[0]);
        return 2;
    }

    int fd = socket(AF_INET, SOCK_DGRAM, 0);
    if (fd < 0) {
        perror("socket");
        return 1;
    }

    const char *mode = argv[1];
    const char *ssid = argv[2];
    int rc = 1;

    if (strcmp(mode, "ap") == 0) {
        uint32_t chan = (argc >= 4) ? (uint32_t)atoi(argv[3]) : 6;
        printf("[wifi_switch] %s -> SoftAP ssid=\"%s\" channel=%u\n", ifname, ssid, chan);
        if (do_set_mode(fd, ifname, IW_MODE_MASTER)) goto out;
        if (do_set_essid(fd, ifname, ssid)) goto out;
        if (do_set_channel(fd, ifname, chan)) goto out;
        if (do_commit(fd, ifname)) goto out;
        printf("[wifi_switch] SoftAP commit OK\n");
        rc = 0;
    } else if (strcmp(mode, "sta") == 0) {
        const char *pass = (argc >= 4) ? argv[3] : "";
        printf("[wifi_switch] %s -> Station ssid=\"%s\" (%s)\n",
               ifname, ssid, pass[0] ? "wpa2" : "open");
        if (do_set_mode(fd, ifname, IW_MODE_INFRA)) goto out;
        if (do_set_essid(fd, ifname, ssid)) goto out;
        if (pass[0] && do_set_key(fd, ifname, ssid, pass)) goto out;
        if (do_commit(fd, ifname)) goto out;
        printf("[wifi_switch] Station commit OK\n");
        rc = 0;
    } else {
        usage(argv[0]);
        rc = 2;
    }

out:
    close(fd);
    return rc;
}
