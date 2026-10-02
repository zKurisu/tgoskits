# AKA-00 网球 YOLO 固定图片推理测试

这个目录提供 AKA-00/SG2002/CV181x 上的最小用户态 NPU/TPU 推理测试。
它不依赖摄像头输入，而是使用固定图片和固定期望结果验证用户态推理链路是否可用。

测试覆盖的工作包括：

1. 加载 CV181x 格式的 YOLOv8 网球检测模型。
2. 读取固定 JPEG 图片并完成解码、resize、letterbox 和 RGB planar 预处理。
3. 通过 CVI runtime 调用板端 TPU 推理。
4. 对 YOLOv8 输出做后处理和 NMS。
5. 将检测类别、分数和 bbox 与预置 expected 文件比较。
6. 打印每张图片的耗时，用于后续性能观察。

## 目录结构

- `akars-validator/`：Rust 用户态校验程序源码。
- `model/yolov8n_tennis_v2.cvimodel`：CV181x 网球检测模型，来源于
  `BattiestStone4/akars`，提交 `fc15c583849a84126a24660e97e983fbf327ff69`。
- `validation/images.txt`：固定图片清单。
- `validation/*.jpg`：3 张固定测试图片，复用 RK3588 YOLO 测试里的网球图片。
- `validation/expected.txt`：SG2002 Linux 上生成并确认可用的预期检测结果。
- `thirdparty/tpu-sdk-sg200x/`：`scripts/setup.sh` 下载生成的 CVI runtime SDK 目录，仓库不直接保存其二进制内容。
- `scripts/`：Xuantie musl 工具链、TPU SDK 准备和 linker 包装脚本。
- `build-validator.sh`：构建用户态程序，并生成可部署目录。
- `init.sh`：测试入口，假定板端部署路径为 `/akars_tennis`。
- `board-aka-00-sg2002.toml`：后续接入 Starry board test 的配置入口。
- `SHA256SUMS`：模型和图片的哈希记录；工具链和 TPU SDK 压缩包哈希记录在 `scripts/env.sh` 中。

## 构建

第一次构建前准备 Xuantie musl 工具链和 Milk-V/Cvitek SG200x TPU SDK：

```bash
apps/starry/aka00-tennis-yolo/scripts/setup.sh
```

`setup.sh` 会从固定 URL 下载并校验：

- Xuantie V3.4.0 RISC-V musl 工具链。
- `milkv-duo/tpu-sdk-sg200x` 固定提交 `6fa0d80a635db13b6b9dc061d68b8da0593b79f3` 的源码归档。

如果构建环境无法直接联网，可以先准备本地压缩包，然后通过参数指定：

```bash
apps/starry/aka00-tennis-yolo/scripts/setup.sh \
  --toolchain-archive /path/to/Xuantie-900-gcc-linux-6.6.36-musl64-x86_64-V3.4.0-20260323.tar.gz \
  --sdk-archive /path/to/tpu-sdk-sg200x-6fa0d80a635db13b6b9dc061d68b8da0593b79f3.tar.gz
```

如果工具链或 TPU SDK 已存在，也可以通过环境变量指定：

```bash
AKARS_TENNIS_TOOLCHAIN_DIR=/path/to/xuantie-v3.4.0 \
AKARS_TPU_SDK_DIR=/path/to/tpu-sdk-sg200x \
  apps/starry/aka00-tennis-yolo/build-validator.sh
```

常规构建命令：

```bash
apps/starry/aka00-tennis-yolo/build-validator.sh
```

构建产物目录是：

```text
apps/starry/aka00-tennis-yolo/install/sg2002_riscv64_musl/akars_tennis/
```

`install/` 是生成物，已被 `.gitignore` 忽略。需要部署时重新运行
`build-validator.sh` 生成。

## 部署内容

板端部署目录固定为：

```text
/akars_tennis
```

需要把本地构建产物目录中的完整内容部署到板端 `/akars_tennis`：

```text
akars_tennis/
├── akars-tennis-validator
├── akars-tennis-live
├── cvi-camera-bench
├── cvi-camera-dataset
├── cvi-vpss-smoke
├── run.sh
├── run-live.sh
├── lib/
├── model/
│   └── yolov8n_tennis_v2.cvimodel
└── validation/
    ├── images.txt
    ├── expected.txt
    ├── tennis-ball-black-box.jpg
    ├── tennis-ball-close.jpg
    └── tennis-ball-plant.jpg
```

`cvi-camera-bench` 是 `/dev/cvi-usb-camera0` 异步采集 ABI 的板端验收工具；
它不参与固定图片正确性测试。
`cvi-camera-dataset` 是数据集采集辅助程序，由笔记本侧脚本临时上传到板端 `/tmp`；
正常采集不要求长期安装它。

其中：

- `akars-tennis-validator` 是交叉编译出的 RISC-V Linux 用户态程序。
- `akars-tennis-live` 从异步摄像头读取帧，支持 `mjpeg` 软件解码基线、
  `jpu-yuv` 硬件解码 + CPU 预处理基线，以及默认的 `vpss-rgb` 全硬件预处理路径，
  并串联 TPU 前向与 INT8 优先后处理。
- `run.sh` 是板端运行入口。
- `lib/` 包含 CVI runtime 以及程序运行需要的动态库。
- `model/` 包含 `.cvimodel`。
- `validation/` 包含固定图片、图片清单和预期结果。

部署方式可以按当前板卡环境选择，例如通过串口辅助、SSH、SCP、rsync、挂载根文件系统等方式完成。核心要求是板端最终存在完整的 `/akars_tennis` 目录，并在写入后执行 `sync`，确保内容落盘。

## 板端运行

在板端 Linux 上执行：

```sh
cd /akars_tennis
./run.sh
```

`run.sh` 会先检查 `/dev/cvi-tpu0`。如果 TPU 设备节点不存在，会尝试加载 rootfs 中的 CV181x TPU 相关模块：

```text
/mnt/system/ko/cv181x_sys.ko
/mnt/system/ko/cv181x_base.ko
/mnt/system/ko/cv181x_tpu.ko
```

随后运行：

```sh
./akars-tennis-validator \
  model/yolov8n_tennis_v2.cvimodel \
  validation/images.txt \
  validation/expected.txt \
  --classes 1 \
  --conf 0.5 \
  --iou 0.5
```

需要采集稳定的分阶段性能数据时，可以先预热 1 轮，再测量 5 轮：

```sh
./akars-tennis-validator \
  model/yolov8n_tennis_v2.cvimodel \
  validation/images.txt \
  validation/expected.txt \
  --classes 1 \
  --conf 0.5 \
  --iou 0.5 \
  --warmup 1 \
  --repeat 5
```

`--warmup` 的结果不进入统计；`--repeat` 必须大于 0，且每一轮测量都会
重新与 `expected.txt` 比较，避免用错误的检测结果换取更好看的耗时。
不传这两个参数时仍执行原来的单轮校验，成功标记和检测语义不变；输出行末会
增加轮次与分阶段耗时字段，并额外输出汇总行。

## 结果判定

测试通过时会打印：

```text
AKARS_TENNIS_VALIDATE_PASS images=3
STARRY_AKA00_TENNIS_DETECT_OK
```

每张图片会打印检测结果：

```text
AKARS_TENNIS_RESULT image=0 path=validation/tennis-ball-close.jpg detections=1 run=1
AKARS_TENNIS_DET image=0 cls=0 class=tennis_ball score_q10000=9531 confidence_percent=95.31 left=482 top=704 right=776 bottom=1002 run=1
```

检测结果字段含义：

- `image`：图片序号，对应 `validation/images.txt` 中的顺序。
- `run`：从 1 开始的测量轮次；预热轮次不打印逐图结果。
- `cls`：模型输出类别 id。当前模型只识别网球，`cls=0` 表示 `tennis_ball`。
- `class`：类别名称，便于直接阅读日志。
- `score_q10000`：模型置信度乘以 10000 后的整数表示，`9531` 表示约 `0.9531`。
- `confidence_percent`：同一置信度的百分比表示，`95.31` 表示 `95.31%`。
- `left/top/right/bottom`：检测框在原图中的像素坐标，分别表示左、上、右、下边界。

每张图片也会打印耗时：

```text
AKARS_TENNIS_TIMING image=0 preprocess_us=... forward_us=... postprocess_us=... total_us=... run=1 decode_us=... resize_us=...
```

字段含义：

- `decode_us`：纯 Rust JPEG 解码耗时。
- `resize_us`：双线性 resize、letterbox padding 清零和 RGB planar tensor 打包耗时。
- `preprocess_us`：CPU 侧预处理耗时，包含 JPEG 解码、resize、letterbox padding 和 RGB planar tensor 打包。
- `forward_us`：TPU 侧模型前向推理耗时，对应 CVI runtime 的 `CVI_NN_Forward` 调用。
- `postprocess_us`：CPU 侧 YOLOv8 后处理耗时，包含输出解析、分数筛选、NMS 和 bbox 坐标还原。
- `total_us`：单张图片端到端耗时，从开始处理图片到得到最终 detection 结果。

其中 `preprocess_us` 包含 `decode_us`、`resize_us` 以及少量调用开销；
`total_us` 包含 `preprocess_us`、`forward_us`、`postprocess_us` 以及少量函数
调用和统计开销。性能观察时可以用 `decode_us` 判断 JPU 等硬件解码路径的
潜在收益，用 `resize_us` 判断 resize/pack 是否仍是瓶颈，用 `forward_us`
判断 TPU 推理耗时。

全部测量轮次通过 golden 校验后，还会打印一行汇总：

```text
AKARS_TENNIS_BENCH_RESULT measured_runs=5 images=3 samples=15 decode_us_avg=... decode_us_p50=... decode_us_p95=... resize_us_avg=... resize_us_p50=... resize_us_p95=... preprocess_us_avg=... preprocess_us_p50=... preprocess_us_p95=... forward_us_avg=... forward_us_p50=... forward_us_p95=... postprocess_us_avg=... postprocess_us_p50=... postprocess_us_p95=... total_us_avg=... total_us_p50=... total_us_p95=...
```

`samples` 等于测量轮数乘图片数。p50/p95 使用 nearest-rank 定义，并只对
测量轮次统计；所有数值单位均为微秒。该行适合从串口日志提取，比较同一
硬件、模型、图片和运行参数下的 StarryOS 与 Linux 数据。

## Linux 实测耗时参考

以下数据来自 CI 环境 AKA-00/SG2002 板端 Linux，部署路径为
`/akars_tennis`。每轮都会顺序识别同一组 3 张固定图片。这是加入分阶段
自动汇总前记录的 aggregate 基线，因此没有单独的 `decode_us` 和
`resize_us`。后续可以用 `--warmup 1 --repeat 5` 自动复现相同轮数并获得
更细的瓶颈数据。该表用于后续性能对比，CI 是否通过仍以检测结果和成功
标记为准。

| 轮次 | 图片 | preprocess_us | forward_us | postprocess_us | total_us |
| --- | --- | ---: | ---: | ---: | ---: |
| 1 | 0 | 415022 | 39533 | 1086 | 455648 |
| 1 | 1 | 435312 | 39525 | 1073 | 475917 |
| 1 | 2 | 429350 | 39531 | 1073 | 469962 |
| 2 | 0 | 415454 | 39527 | 1113 | 456101 |
| 2 | 1 | 433679 | 39534 | 1086 | 474306 |
| 2 | 2 | 428220 | 39534 | 1097 | 468858 |
| 3 | 0 | 415529 | 39561 | 1087 | 456184 |
| 3 | 1 | 434228 | 39528 | 1086 | 474849 |
| 3 | 2 | 429114 | 39533 | 1076 | 469729 |
| 4 | 0 | 414924 | 39562 | 1155 | 455649 |
| 4 | 1 | 433084 | 39518 | 1074 | 473682 |
| 4 | 2 | 428085 | 39508 | 1083 | 468683 |
| 5 | 0 | 415256 | 39523 | 1080 | 455865 |
| 5 | 1 | 434598 | 39506 | 1069 | 475180 |
| 5 | 2 | 429529 | 39524 | 1064 | 470124 |

测试失败时会打印：

```text
AKARS_TENNIS_VALIDATE_FAIL reason=...
```

常见失败原因包括模型文件缺失、图片缺失、runtime 动态库缺失、TPU 驱动节点不可用、推理结果与 expected 不匹配。

## 实时整链路基线

固定图片验证通过后，先运行10帧 JPU 实时短测：

```sh
cd /akars_tennis
./run-live.sh 10
```

工具只在结束时输出三行汇总，不逐帧写盘：

```text
AKARS_LIVE_SUMMARY input=jpu_yuv frames=10 wall_us=... fps_x100=... request_avg_us=... capture_avg_us=... preprocess_avg_us=... forward_avg_us=... postprocess_avg_us=... total_avg_us=... total_max_us=...
AKARS_LIVE_RESULT first_sequence=... last_sequence=... skipped_sequences=... frames_with_detections=... detections_total=...
AKARS_LIVE_CAMERA calls=... success=... failed=... retries=... invalid=... usb_errors=... published=... overwritten=... avg_call_us=... max_frame_us=...
```

`run-live.sh` 的第二个参数选择输入路径，默认是 `jpu-yuv`：内核 JPU 把 MJPEG
解码为 planar YUV，用户态直接转换到 TPU 的 RGB planar 输入缓冲，不再执行
`zune_jpeg` 软件解码，也不再分配整张中间 RGB 图。原始软件基线仍可复现：

```sh
./run-live.sh 10 mjpeg
```

两种模式必须分别用端到端汇总比较，不能把独立 camera/VPSS smoke 数字相加冒充收益。
`request_avg_us` 在 `jpu-yuv` 模式下包含等待最新帧、JPU 解码和 YUV 用户态复制；
`preprocess_avg_us` 是 YUV 到 RGB planar、letterbox 和必要缩放的用户态耗时。

## 异步摄像头采集与统计

SG2002 摄像头设备保留原来的同步 ioctl 1–5，并增加了显式启停的后台采集：

- `START_ASYNC(6)` / `STOP_ASYNC(7)`：启停独立采集任务；
- `GET_LATEST_FRAME(8)`：只取比 `last_sequence` 更新的 MJPEG 帧；
- `GET_CAPTURE_STATS(9)` / `RESET_CAPTURE_STATS(10)`：读取或清零采集、重试、坏帧和覆盖计数；
- `GET_LATEST_YUV_FRAME(11)`：取最新 MJPEG 后使用 JPU 解码为 planar YUV；实际
  采样格式跟随 JPEG SOF，驱动通过 `format` 区分 YUV420/422/440/444/400。当前
  实机摄像头是 YUV422 planar，640×480 对应 614400 字节。
- `GET_LATEST_NV12_FRAME(12)`：仅接受 4:2:0 MJPEG，使用 JPU 的 CbCr interleave
  直接输出 NV12；4:2:2 MJPEG 在相同寄存器配置下会得到 NV16，因此驱动会拒绝，
  不会把 NV16 错标为 NV12。

用户态 ABI 定义见 `include/cvi_usb_camera.h`。驱动内部使用两块可复用 JPEG 缓冲，
但生产者最多只预取一帧；消费者取走后才开始下一次采集。这样既不排队积累多帧，
也不会在单核 C906 上为注定覆盖的帧持续执行 UVC 忙轮询。`overwritten_frames`
保留为竞态/多消费者诊断计数，正常单消费者链路应为 0。

这里的“异步”是 StarryOS 任务级解耦，不代表 UVC 底层已经变成多 transfer completion
ring。当前 `sg200x-bsp` 仍提供同步轮询采集，而且采集任务与应用共享 C906；实际能与
TPU 硬件等待重叠多少必须以板端吞吐量为准。

板端采集基准：

```sh
cd /akars_tennis
./cvi-camera-bench /dev/cvi-usb-camera0 300
```

验证异步 latest-frame 与 JPU 实时解码组合时使用：

```sh
./cvi-camera-bench /dev/cvi-usb-camera0 300 yuv
```

验证 JPU 直接输出 NV12（摄像头 MJPEG 必须为 4:2:0）时使用：

```sh
./cvi-camera-bench /dev/cvi-usb-camera0 300 nv12
```

长测可在末尾增加 `quiet`，避免逐帧终端输出和 `tee` 写盘扰动消费速度：

```sh
./cvi-camera-bench /dev/cvi-usb-camera0 300 mjpeg quiet
./cvi-camera-bench /dev/cvi-usb-camera0 300 yuv quiet
./cvi-camera-bench /dev/cvi-usb-camera0 300 nv12 quiet
```

最终的 `CVI_CAMERA_STATS` 会报告平均采集调用耗时、最大耗时、USB 错误、重试、
无效 JPEG 比例和 latest-frame 覆盖率。当前 `sg200x-bsp` 的
`uvc_capture_one_frame()` 没有导出“首包等待/USB 传输/JPEG 拼帧”分段数据，因此
这些字段使用 `UINT64_MAX`，且 `CVI_CAMERA_PROFILE_UVC_STAGES` capability 位为 0；
后续 BSP 增加分段结果时，只需实现内核中的 `uvc_capture_one_frame_profiled()`，
用户态 ABI 不变。

## 从摄像头采集校准图片

先运行 `build-validator.sh`，确保构建目录中存在 RISC-V 辅助程序，然后在**笔记本**
执行：

```sh
cd /home/jiqingjie/workspace/new_kernel/tgoskits
./apps/starry/aka00-tennis-yolo/scripts/collect-camera-images.sh \
  192.168.86.53 300 500 /home/jiqingjie/pic
```

脚本建立一条 SSH master 连接，只会询问一次机器人 root 密码。它把
`cvi-camera-dataset` 临时上传到机器人 `/tmp`，保持摄像头异步采集任务运行，每
500 ms 选择一张最新的完整 MJPEG。板端任意时刻最多只有一个固定名称的临时 JPEG；
笔记本通过 SCP 拉取并检查 SOI/EOI 后，先把本地 `.part` 原子改名为 `.jpg`，再删除
板端临时文件并允许采集下一张。正常完成、传输失败或收到退出信号时都会清理板端
PID、辅助程序和临时 JPEG，不会在板端累积采集图片。

本地文件名形如 `tennis_20261001_183000_0001.jpg`，已有文件不会被覆盖。300 张、
500 ms 间隔的理论最短时间约为 150 秒，实际还包括首次初始化和 SCP 开销。中途失败
时，已经成功传到笔记本的图片会保留，并显示完成张数；重新运行会使用新的时间戳，
不会覆盖上次结果。

## TPU-MLIR 校准和 aligned 模型

模型转换环境固定为 `tpu_mlir==1.30.2`，基础镜像也固定到 digest，避免 `latest`
漂移。首次构建：

```sh
cd /home/jiqingjie/workspace/new_kernel/tgoskits/apps/starry/aka00-tennis-yolo
docker build \
  -t akars/tpu-mlir:1.30.2 \
  model-conversion
```

转换脚本只接受本次正式采集的 300 张
`tennis_20261001_183102_*.jpg`，会完成 ONNX 固定输入、全量校准、INT8/BF16
混合精度部署、全部中间张量比较、CModel 比较和 SHA-256 记录：

```sh
./model-conversion/convert-aligned-model.sh
```

关键参数为 `--fuse_preprocess --customization_format RGB_PLANAR --aligned_input`。
纯 INT8 会使该 P2 模型的分类 Sigmoid 输出全零，不能使用；
`yolov8n-tennis-mixed.qtable` 保留检测头和一个未达到逐层比较阈值的早期卷积为
BF16。生成模型作为候选文件安装为
`model/yolov8n_tennis_p2_aligned_int8.cvimodel`，不会覆盖默认稳定模型。

需要在板端试验候选模型时：

```sh
cd /akars_tennis
AKARS_MODEL=model/yolov8n_tennis_p2_aligned_int8.cvimodel \
  ./run-live.sh 30 vpss-rgb
```

实时程序读取模型的 `aligned` 元数据。默认模型仍走 runtime TDMA 导入；候选模型
自动使用 `CVI_NN_SetTensorWithAlignedFrames()` 直绑 VPSS RGB-planar 物理帧。

注意：现用 `yolov8n_tennis_v2.cvimodel` 输出为 `1x5x8400`，而当前可取得的原始
`tennis.onnx` 是 YOLOv8n-P2，输出为 `1x5x27600`。因此候选模型不仅改变输入对齐，
还增加了 P2 检测头，必须以板端正确性和 forward 耗时为准，不能在未测试时替换
默认模型。

当端到端时延优先于精度时，可生成 384x384 的 P2 aligned 模型：

```sh
INPUT_SIZE=384 ./model-conversion/convert-aligned-model.sh
```

产物为 `model-conversion/work-384/yolov8n_tennis_p2_384_aligned_int8.cvimodel`，
输入为 `1x3x384x384`，输出为 `1x5x9936`，FLOPs 为 3.338G。384 同时满足
网络 32 倍数和 VPSS/TPU 64 字节行对齐，用户态会从模型输入合同自动配置 VPSS
输出为 RGB planar 384x384，并生成 384x288 的居中 letterbox 内容区。

## 硬件预处理直通接口

`akars-validator/src/tpu.rs` 已绑定固定 SG2002 SDK 中真实存在的：

- `CVI_NN_SetTensorPhysicalAddr()`：更新 device tensor 物理地址；
- `CVI_NN_SetTensorWithAlignedFrames()`：导入 VPSS 物理帧。官方 runtime 对普通
  `aligned=false` 模型使用 TDMA 紧凑搬运，对可直绑的 aligned 模型才更新基地址。

`infer_aligned_physical_timed()` 是严格的 aligned 模型接口；当前实时路径使用
`infer_vpss_rgb_timed()`，允许普通模型走 runtime 官方的 TDMA compact 路径。
仓库当前只有 `.cvimodel`，缺少原始 ONNX、校准集和匹配版本的模型编译器，因此没有
替换现有模型；在这些输入补齐并完成固定图片精度回归前，物理帧接口只作为已编译、
有安全前置条件的接入点。

StarryOS 现已提供 `/dev/cvi-vpss0`：它从 FDT 获取 `cvitek,vpss` MMIO 与
`sc` IRQ，接收两块 ION coherent buffer，在硬件上执行单通道裁剪/缩放。输入支持
NV12 和 JPU 所用的三平面 YUV422P，输出支持 NV12 与 RGB Planar，并保留
sequence/timestamp。RGB Planar 路径可使用 SC_V1 的 border 寄存器在硬件中生成
letterbox 黑边。
用户 ABI 见 `include/sg2002_vpss.h`。板端最小验证（默认 640×480 -> 320×240）：

```sh
./cvi-vpss-smoke /dev/cvi-vpss0 1
```

新增的官方格式路径使用独立 ioctl，离线验证命令为：

```sh
./cvi-vpss-smoke /dev/cvi-vpss0 300 yuv422p
```

JPU 与 VPSS 之间使用同一个 ION DMA buffer 的真实摄像头链路为：

```sh
./cvi-vpss-smoke /dev/cvi-vpss0 30 camera /dev/cvi-usb-camera0
```

`camera` 模式通过 camera ioctl 13 让 JPU 直接写入源 ION 的 Y/Cb/Cr 三个 plane，
随后把同一个 ION fd 和 plane offset 交给 VPSS；不会再把 614400 字节 YUV 拷贝到
用户缓冲后重新导入。输出中的 `VPSS_CAMERA request_avg_us` 包含等待最新 MJPEG 和
JPU 解码，`VPSS_PASS hw_avg_us` 是 VPSS 硬件阶段。

确定性验证 `YUV422P -> RGB Planar 640x640 + 上下各 80 行黑边`：

```sh
./cvi-vpss-smoke /dev/cvi-vpss0 30 rgb
```

真实摄像头端到端硬件预处理验证：

```sh
./cvi-vpss-smoke /dev/cvi-vpss0 30 camera-rgb /dev/cvi-usb-camera0
```

配套内核和用户态程序部署后，直接运行完整推理链路：

```sh
./run-live.sh 30
```

`run-live.sh` 默认选择 `vpss-rgb`，实际路径为：

```text
MJPEG -> JPU YUV422P/source ION -> VPSS RGB Planar/destination ION
      -> CVI_NN_SetTensorWithAlignedFrames
      -> TPU TDMA compact 到普通模型输入 -> TPU forward
```

这条路径不要求模型带 `aligned=true`。尽管 API 名称包含 `AlignedFrames`，官方
cviruntime 对 `aligned=false` 输入明确实现了 TPU TDMA compact；当前 640 字节行宽
本身满足其 64 字节 VPSS 对齐要求。destination ION 在阻塞调用期间保持存活，
用户态不读取其 uncached 映射，也没有 CPU 的 640x640x3 字节最终复制。

`AKARS_LIVE_SUMMARY` 将端到端请求拆成：`camera_ion_avg_us`（等待最新帧并由 JPU
写入 source ION）、`vpss_wall_avg_us`（VPSS ioctl 的用户态墙钟时间）、
`vpss_hw_avg_us`/`vpss_hw_max_us`（VPSS 驱动记录的硬件阶段）。在 `vpss-rgb`
模式下 `preprocess_avg_us` 记录显式 TDMA compact 的耗时，不再包含 CPU 布局转换、
缩放、letterbox 或大图复制。

## 实时识别率与逐帧时延测试

`test-live-accuracy.sh` 默认连续运行 60 秒，使用当前 384x384 aligned 模型和
`vpss-rgb` 链路：

```sh
cd /akars_tennis
./test-live-accuracy.sh
```

也可以指定测试时长和日志路径：

```sh
./test-live-accuracy.sh 120 /root/akars-live-accuracy-120s.log
```

脚本统计含球帧检出率、检测置信度、FPS、相机健康状态，并对 `request`、
`camera_ion`、`capture`、`vpss_wall`、`vpss_hw`、`preprocess`、`forward`、
`postprocess` 和端到端 `total` 输出平均值、P50、P95、P99 与最大值。实时程序在
内存中累计分位数样本，只在结束时写入汇总，避免逐帧文件 I/O 干扰测试；不保存
任何相机图片。需要逐帧调试时可单独给 `akars-tennis-live` 传 `--report-frames`。

实时测试没有人工真值框和无球负样本，因此 `positive_detection_rate_percent` 是
“已知画面中有球时的逐帧检出率”，不能替代 Precision、Recall 或 mAP。各阶段时间
存在包含关系，例如 `request` 包含等待相机/JPU及 VPSS 的墙钟过程，不能把所有阶段
简单相加。

采集结束后脚本还会直接打印中文的“单帧平均性能汇总”，无需再人工换算字段：

```text
摄像头/JPU等待 + VPSS       ... ms
TPU输入TDMA搬运             ... ms
TPU推理                     ... ms
后处理                      ... ms
其他绑定/统计开销           ... ms
----------------------------------------
总时延                      ... ms
单帧平均置信度              ... %
含球帧检出率                ... %
```

其中“其他绑定/统计开销”等于单帧平均总时延扣除前四个已列阶段，输出同时写入测试日志。

稳定性验收使用 `100000` 帧；工具检查确定性中性灰 NV12 输出、元数据、IRQ、
program-late、超时和平均/最大硬件耗时：

```sh
./cvi-vpss-smoke /dev/cvi-vpss0 100000
```

当前摄像头已经可以通过调用方持有的 ION coherent buffer完成
`JPU planar YUV422 -> VPSS RGB Planar` 零拷贝交接；原有返回用户态 YUV 和
VPSS NV12 ioctl 均保留。VPSS 输出通过 `CVI_NN_SetTensorWithAlignedFrames()` 交给
runtime；当前普通模型需要一次 TPU TDMA device-to-device compact。未来生成
`--fuse_preprocess --aligned_input` 模型后，runtime 才能把单帧物理地址直接作为
模型输入基地址，从而去掉这次 TDMA 搬运。

## 更新 expected

只有模型、图片、runtime 或后处理逻辑变化时才需要重新生成
`validation/expected.txt`。

在板端已部署 `/akars_tennis` 后执行：

```sh
cd /akars_tennis
export LD_LIBRARY_PATH=/akars_tennis/lib:${LD_LIBRARY_PATH:-}
./akars-tennis-validator \
  model/yolov8n_tennis_v2.cvimodel \
  validation/images.txt \
  validation/expected.txt \
  --classes 1 \
  --conf 0.5 \
  --iou 0.5 \
  --write-expected
sync
```

`--write-expected` 要求 `--warmup 0 --repeat 1`（也是默认值），避免把多轮
benchmark 参数误用于更新 golden 数据。

然后把板端生成的 `/akars_tennis/validation/expected.txt` 更新回仓库中的
`apps/starry/aka00-tennis-yolo/validation/expected.txt`，再重新运行
`build-validator.sh` 生成新的部署目录。

## 本地校验

源码层测试：

```bash
cargo test --manifest-path apps/starry/aka00-tennis-yolo/akars-validator/Cargo.toml
```

clippy：

```bash
cargo clippy --manifest-path apps/starry/aka00-tennis-yolo/akars-validator/Cargo.toml
```

固定资产哈希校验：

```bash
cd apps/starry/aka00-tennis-yolo
sha256sum -c SHA256SUMS
```
