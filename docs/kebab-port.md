# Skyward 移植到 OnePlus 8T（kebab）记录

日期：2026-11-17
基线：Skyward v0.3.1（`crates/` 工作区），LineageOS 24.0 的 `recovery.img`

## 1. 现状摘要

结论：**Skyward 在 kebab 上已能完整使用**：UI、120Hz、触摸、背光、振动、`/data` 解密（含用户 CE）、
文件管理、MTP（需先解密）、分区操作、日志与 SELinux 修复。
**Wi-Fi 搁置**：驱动、校准、`wlan0` 都已打通，只差 supplicant 的 vendor AIDL 注册；
recovery 联网除 Skyward 自更新外没有用途，而本移植无法提供该更新，详见 §8.1。
双系统因无 `init_boot` 而不可行（已加保护）。

状态：✅ 实机验证 ｜ 🟡 代码就绪未验证 ｜ 🟠 部分可用 ｜ ❌ 未实现 / 受阻

### 1.1 启动与系统

| 期望功能 | 状态 | 说明 |
|---|---|---|
| 刷入 recovery 分区后启动 | ✅ | 修 `recovery_dtbo_offset` 后成功（`682cc60`） |
| `fastboot boot` 临时启动 | ✅ | 全程可用 |
| 启动到 UI | ✅ | `the pages speak En` |
| A/B 槽位识别 | ✅ | `slot_suffix=_a`，super 映射正常 |
| 槽位切换（boot control HAL） | 🟡 | 代码在，未实机操作 |
| 双系统 | ❌ | 无 `init_boot`；已加库级保护（`d873027`） |
| Recovery 自更新（在线） | ❌ | 依赖 Wi-Fi |
| 「Flash Skyward again」 | 🟡 | 未验证（会写 recovery） |

### 1.2 界面与输入

| 期望功能 | 状态 | 说明 |
|---|---|---|
| UI 渲染（DRM/KMS） | ✅ | 1080×2400，面板 `connected` |
| **120Hz 高刷** | ✅ | DRM 有 60/120 两个模式，UI 切到最快 |
| 触摸 | ✅ | Synaptics S3908，evdev `input2` |
| 背光 / 亮度 | ✅ | `panel0-backlight`，normal-max 修复（`af3ceb5`） |
| 振动 | ✅ | AW8697 走 `/sys/class/leds/vibrator`；新增 LED 后端，点击/开关/键盘/滑块/熄屏双击都有反馈（`56530ac`、`7606a6a`） |
| 语言切换（简/繁/英） | 🟡 | 字体子集已覆盖 UI 全部字符，实机未切换 |
| 字体显示 | ✅ | MiSans 子集，ramdisk 39.3 → 27.2 MiB（`00b9f39`） |
| 搜索 / 主题 / 时区 | 🟡 | 代码就绪，未逐项验证 |
| About Phone | ✅ | 实机确认信息正确 |
| 终端 | ✅ | adb shell 正常 |
| 截图 `Capture` | 🟡 | 未验证 |

### 1.3 存储与刷写

| 期望功能 | 状态 | 说明 |
|---|---|---|
| super / 逻辑分区映射挂载 | ✅ | 5 个逻辑分区全部挂上 |
| persist / modem 挂载 | ✅ | `/mnt/vendor/persist`、`/vendor/firmware_mnt` |
| 分区浏览 | 🟡 | `skyward mount/lp` 可用，未细测 |
| 备份 / 恢复 | 🟡 | `[backup]` 已配置（modemst1/2、fsg、fsc、persist），未跑 |
| 刷镜像 / zip / payload OTA | 🟡 | 代码通用，未在 Skyward 内验证 |
| AVB 处理（avbctl） | 🟡 | 符号链接已注入，未测 |
| 擦除 / 格式化 / 工厂重置 | 🟡 | 未测 |
| **`/data` 解密（FBE+metadata）** | ✅ | Keymaster 4.x HIDL 客户端已实现；metadata key、wrapped key、dm-default-key、用户 CE 全部打通（见 §8.3） |
| 文件管理器（内部存储） | ✅ | `/data` 打开后可浏览 |
| MTP（内部存储） | ✅ | 已在 Nautilus 中实测：`/data` 解锁后电脑能看到并读写内部存储（2026-11-18） |

### 1.4 连接与网络

| 期望功能 | 状态 | 说明 |
|---|---|---|
| adb root shell | ✅ | File transfer 关闭时正常 |
| MTP | ✅ | 见 1.3；USB 端口问题已修，2026-11-18 实测可用 |
| **Wi-Fi** | ⏸️ 搁置 | 驱动/校准/`wlan0` 已通；卡在 supplicant 的 AIDL 注册（§8.1）。服务与 `[wlan]` 已整体注释，UI 不再显示 WLAN 页 |
| 在线检查更新（GitHub） | ❌ | 依赖 Wi-Fi |
| fastbootd | 🟡 | 未测 |

### 1.5 量化

- **完全可用**：启动、UI、120Hz、触摸、背光、振动、`/data` 解密、内部存储文件管理、分区挂载、adb、日志、SELinux 修复、About、字体
- **代码就绪待验证**：槽位切换、备份/恢复、刷写、fastbootd、语言切换（按约定暂不逐项实测）
- **未实现 / 受阻**：Wi-Fi 完整（唯一功能缺口）、在线更新（依赖 Wi-Fi）、双系统（无 `init_boot`）

### 1.6 阻塞项

| 阻塞 | 影响 | 需要的工作 |
|---|---|---|
| Wi-Fi supplicant 的 AIDL 注册 | Wi-Fi 与 Skyward 自更新 | 给 recovery 一个最小的 vendor 服务目录，见 §8.1 |
| 无 `init_boot` | 双系统 | 重新设计第二系统 first-stage 存放方式 |
| 显示提交超时 | 以前会 panic → EDL | 已注册 DRM 恢复事件（§8.3.3）；待观察是否还有超时 |
| AW8697 接口 | 振动 | LED-class 后端（小改动） |

## 2. Skyward 的工作方式（与本机相关部分）

```
bootloader
 └─ recovery 分区里的内核（原厂/ROM 构建，Skyward 不改）
     └─ ramdisk 里的 AOSP init
         └─ /system/bin/recovery = Skyward
             ├─ prepare(): 从 super 映射并挂载 system/vendor/odm...
             ├─ 读已安装系统的 init.rc / vintf，决定拉起哪些 HAL
             ├─ 挂载 persist、modem 固件分区
             ├─ 加载内核模块（可配）
             └─ ui::run(): DRM/KMS 绘制 + evdev 输入 + MTP/fastboot
```

要点：

- 打包器把 Skyward 二进制**替换** ramdisk 里的 `system/bin/recovery`，其余（init/adbd/SELinux/库）保持不变。
- 设备用户态驱动来自**已安装系统**的 `/vendor`、`/odm`；因此 recovery 内核版本必须与已安装系统一致。
- 设备差异全部收敛在 `devices/<codename>.toml`，代码里只有少量通用逻辑需要补。

## 3. 实机数据（OnePlus 8T / kebab）

采集方式见 §10。

> 数据来源：以下均取自**实机**（adb）与 `cache/recovery.img`（LineageOS 24.0）。
> `~/work/PixelOS_kebab` 仅作设备树/配置的阅读参考：该树**从未成功构建**，
> 仓库也不是最新，其 `out/` 产物不可用于刷写或恢复。

```
model            OnePlus 8T (KB2000), codename kebab
系统             Android 17 / CP2A.260605.016（LineageOS 24）
内核             4.19.325-cip136-st20-perf-g078365e5318d
槽位             _a
recovery_a/b     104,857,600 B (100 MiB)
super            7,516,192,768 B (7 GiB)
metadata/misc/persist/modem/modemst1/modemst2/fsg/fsc   均存在
```

### 3.1 显示

```
DRM modes        1080x2400x60x183357x0cmd
                 1080x2400x120x183357x0cmd
面板             samsung amb655x, dsi_cmd_mode, DSC ×2 encoder
DT 背光          qcom,mdss-dsi-bl-normal-max-level = 0x7ff (2047)
                 qcom,mdss-dsi-bl-max-level        = 0x7ff
backlight        panel0-backlight, max_brightness = 2047
```

### 3.2 触摸

```
芯片             Synaptics S3908（oncell）
驱动             oplus_touchscreen，CONFIG_TOUCHPANEL_OPLUS=y（内建）
设备节点         /dev/input/input2 (touchpanel), input3 (kpd)
上报率           60 / 90 / 120 均可
```

### 3.3 WLAN

```
芯片             QCA6390（qca6390 profile）
驱动             qca_cld3_wlan 内建；cnss2 / QMI
触发             /sys/kernel/cnss/fs_ready = 1
固件             /vendor/firmware_mnt/image/qca6390/
                 ├─ amss20.bin
                 ├─ bdwlan.elf
                 ├─ m3.bin
                 └─ Data.msc
运行配置         /vendor/firmware/wlan/qca_cld/{WCNSS_qcom_cfg.ini,wlan_mac.bin}
persist          /mnt/vendor/persist/{bdwlan.elf,bdwlanu.elf,wlan_mac.bin,wlan/}
supplicant       /vendor/bin/hw/wpa_supplicant（3.8 MB）
AIDL 服务        android.hardware.wifi.supplicant.ISupplicant/default（已注册）
```

### 3.4 加密与安全

```
fileencryption        aes-256-xts:aes-256-cts:v2+inlinecrypt_optimized+wrappedkey_v0
metadata_encryption   aes-256-xts:wrappedkey_v0
keydirectory          /metadata/vold/metadata_encryption
Keymaster             HIDL 4.0 / 4.1（IKeymasterDevice/default）
Gatekeeper            HIDL 1.0（IGatekeeper/default）
KeyMint AIDL          无
Weaver                无
```

### 3.5 振动

```
接口             /sys/class/leds/vibrator/
                 ├─ state, duration, activate, gain, brightness
                 ├─ rtp, waveform_index, f0, ...
                 └─ 无 evdev force-feedback
```

驱动（`aw8697_haptic`）的 OL 用法：`state=1` → `duration=<ms>` → `activate=1` 播放一个片段；
`duration<=500` 时片段索引由时长决定（`10` 和 `1` 是同一个最短片段），`>500` 走长片段。
`gain` 0x01–0x75（0x00 不振），官方 HAL 不写 gain。
Skyward 的映射见 §8.4。

### 3.6 字体

```
MiSansVF.ttf          19.2 MB 原始 / 15.5 MB LZ4   （完整字体）
MiSans-Subset.ttf      5.68 MB 原始 / 4.64 MB LZ4  （kebab 用）
KirkMono-Regular.ttf   264 KB / 142 KB
```

系统字体不可直接替换：

- 手机自带 Roboto / RobotoFlex 无中文字形
- NotoSansCJK 是 32 MB 的 TTC，且 index 0 是日文 face（运行时只读 index 0）
- Ubuntu 的 DroidSansFallbackFull 没有拉丁字形（`A/z/0/9` glyph 均为 0）

因此对 MiSans 做子集化：保留 UI 全部字符 + GB2312（6763 汉字）+ Big5 一级常用（5401）+ 拉丁/标点/符号，共 10,514 个字符。

结果：字形 29,773 → 10,018，`wght` 轴（150–700）保留，UI 892 个字符无缺失。

重新生成：

```bash
./venv/bin/pyftsubset resources/fonts/MiSansVF.ttf \
  --text-file=/tmp/opencode/subset-chars.txt \
  --output-file=resources/fonts/MiSans-Subset.ttf \
  --layout-features='*' --name-IDs='*' --name-legacy \
  --notdef-glyph --notdef-outline --recommended-glyphs \
  --glyph-names=no --symbol-cmap --legacy-cmap --recalc-bounds
```

回归测试：`crates/skyward-ui/src/font.rs` 的
`the_subset_typeface_keeps_its_weights_and_draws_both_scripts`。

局限：文件管理器/终端/SSID 中超出常用集的生僻字会显示为缺字方框。

## 4. 代码改动

| 提交 | 内容 |
|---|---|
| `24512cf` | `skyward-bootimg` 支持 boot header v0–v4：保留 kernel/ramdisk/second/recovery_dtbo/dtb，按版本布局读写 |
| `8cfe87d` | 新增 `devices/kebab.toml` |
| `3304ddb` | `skyward-block::boot_param` 与 `system-init` 以字节读 `/proc/cmdline`：OnePlus bootloader 在 `avbkeysha256` 后追加非 UTF-8 字节 |
| `af3ceb5` | 背光 normal-max 支持高通通用 DT 字段，找不到时回退到面板最大值 |
| `e017082` | MTP：`mtp,adb` 触发不再依赖 `sys.usb.config_disable`；`usb::switch` 失败时回退到原配置 |
| `37def68` | 本文档初版：实机数据、构建流程、排查记录 |
| `00b9f39` | 字体子集化：`MiSans-Subset.ttf`，字形 29,773 → 10,018，ramdisk 39.3 → 27.2 MiB |
| `4fdd2de` | v0–v2 header 重算 `hash_digest`（mkbootimg 的 SHA-1 摘要）；打包器支持 `--size` |
| `e403062` | 文档：恢复镜像只能取自 LineageOS 包；说明 vbmeta/boot 版本不匹配的表象 |
| `682cc60` | **关键**：v0–v2 header 的 `recovery_dtbo_offset` 按实际布局重算（修好刷入启动） |
| `d873027` | 无 `init_boot` 的手机不写 `boot`/`init_boot`（库级保护） |
| `a3eece6`/`4a32ba5` | 文档：AVB 未签名 footer 与验证模式；Wi-Fi 需要的用户态组件与卡点 |
| `bd3258c` | Keymaster 4.x HIDL 客户端：begin/update/finish/abort、exportKey、`KeyService` trait |
| `65ef709` | 硬件服务目录先于注册到它的服务启动；优先用已安装系统的 bootstrap loader |
| `2a6a375` | `phones.rs` 期望的顺序改为目录在前 |
| `bfe5a64` | **关键**：HIDL 线上格式（ASCII 接口名、4.0 名字、parent_offset、返回值顺序、KeyParameter 32 字节、空 vec 记录） |
| `38fdbda`/`4727a1e` | blob 前缀 `pKMblob` 剥离；`upgradeKey` 重试；AIDL header 按恢复/系统选择 |
| `6c3117e` | 长错误信息按屏幕宽度换行 |
| `56530ac` | 振动 LED 后端（`[haptics] led`），强度映射 gain |
| `7606a6a` | 每次「点一下就生效」的交互都振（开关、跳转行、键盘、按键选中） |
| `41b6128` | 启动时把 pstore 的上次记录复制到 `/metadata/skyward/crash/` |
| `445037e`/`af0fbc1` | 显示超时不再 panic：注册 `DRM_EVENT_SDE_HW_RECOVERY`；面板不再 rest/关断 |

### 4.1 为什么需要 boot header v2

`crates/skyward-bootimg/src/image.rs` 原本只接受 v3/v4。本机 recovery 是 v2：

```
header_version   2
page_size        4096
kernel           56,918,040 B
ramdisk          18,972,189 B（LZ4 legacy）
second           0
recovery_dtbo      884,736 B
dtb              1,428,199 B
```

v2 布局多了 `recovery_dtbo` 与 `dtb`，只按 v3 重建会丢掉它们。

### 4.2 kebab.toml 要点

```toml
[device]  codename = "kebab", brand = "OnePlus", model = "OnePlus 8T", ab = true
[display] width = 1080, height = 2400, density = 450, diagonal = 6.55, corner_radius = 24
[modules] sources = ["vendor"], load = []       # 无 vendor_boot/vendor_dlkm；驱动基本内建
[partitions] mounted = ["persist"]
[firmware] partition = "modem"
[wlan] modules = [], writes = fs_ready = 1, interface = "wlan0",
       supplicant = "/vendor/bin/hw/wpa_supplicant",
       service = "android.hardware.wifi.supplicant.ISupplicant/default",
       probe = "http://connectivitycheck.gstatic.com/generate_204"
[backup] efs = [modemst1, modemst2, fsg, fsc, persist]
```

未配置 `[haptics]`（见 §8.4）。

## 5. 构建与打包

环境：

```
Rust        1.100.0（rust-toolchain.toml）
target      aarch64-linux-android
NDK         r29  (/home/evo/bin/android-ndk-r29)
cargo-ndk   4.1.2
外部工具    lz4、unzip、adb、fastboot
```

命令：

```bash
# 手机端
cargo ndk -t arm64-v8a --platform 30 build --release -p skyward

# 双系统 first-stage（静态）
RUSTFLAGS="-C target-feature=+crt-static" cargo ndk -t arm64-v8a --platform 30 \
  build --release -p skyward-system-init --target-dir target/system-init

# 打包器
cargo build --release -p skyward-pack

# 出镜像（kebab 用子集字体）
target/release/skyward-pack recovery \
  --stock cache/recovery.img \
  --binary target/aarch64-linux-android/release/skyward \
  --system-init target/system-init/aarch64-linux-android/release/system-init \
  --config devices/kebab.toml \
  --font resources/fonts/MiSans-Subset.ttf \
  --mono resources/fonts/KirkMono-Regular.ttf \
  --mono-license resources/fonts/KirkMono-OFL.txt \
  --out skyward-kebab.img
```

### 5.1 体积预算（实测）

用完整 MiSans：

| 组成 | MiB |
|---|---|
| kernel | 54.28 |
| ramdisk（stock 18.09 + 新增 21.22） | 39.31 |
| recovery_dtbo | 0.84 |
| dtb | 1.36 |
| header + vbmeta + footer | 0.01 |
| **合计** | **95.81** |

用子集 MiSans（当前）：

| 组成 | MiB |
|---|---|
| kernel | 54.28 |
| ramdisk | 27.19 |
| recovery_dtbo | 0.84 |
| dtb | 1.36 |
| header + vbmeta + footer | 0.01 |
| **合计** | **83.68** |

stock 镜像大小 96.00 MiB，实际分区 100.00 MiB；用子集后余量 **12.32 MiB**（完整字体时 4.19 MiB）。

ramdisk 内新增：

```
system/bin/recovery                8,805,096 B   （Skyward）
system/bin/{7za,avbctl,resetprop,unzip,zstd}    → recovery 的符号链接
skyward/system-init                1,040,856 B
skyward/device.toml
skyward/MiSansVF.ttf               20,093,424 B
skyward/KirkMono-Regular.ttf       270,456 B
skyward/KirkMono-OFL.txt
```

### 5.2 stock 镜像

`cache/recovery.img`（LineageOS 24.0）：

- 96 MiB，末尾 `AVBf`
- 内核版本字符串与手机 `uname -r` 完全一致
- 与手机 `boot_a` 的内核 sha256 一致：`2e75d6f1…`
- ramdisk 含 `init`、`prop.default`、`sepolicy`、`system/bin/{adbd,init,recovery,sh}`，无 TWRP 痕迹

## 6. 实机验证记录

### 6.1 启动日志（关键行）

```
skyward: kebab 0.3.1 recovery up, misc command "" ...
skyward: mounted odm_a on /odm (erofs)
skyward: mounted product_a on /product (ext4)
skyward: mounted system_a on /mnt/system (ext4)
skyward: mounted system_ext_a on /system_ext (ext4)
skyward: mounted vendor_a on /vendor (erofs)
skyward: 119 device nodes given the owner their phone describes
skyward: modem is mounted on /vendor/firmware_mnt, shown at /firmware
skyward: persist is mounted on /mnt/vendor/persist
skyward: the service directory is told of 22 of the phone's own services
skyward: for the phone's keys: start /vendor/bin/qseecomd ...
skyward: the display driver will tell of a failed commit rather than stop the kernel
skyward: the pages speak Hans, the language chosen, and the locale named is en-US
```

### 6.2 面板

```
/sys/class/drm/card0-DSI-1/status  connected
/sys/class/drm/card0-DSI-1/modes   1080x2400x60…, 1080x2400x120…
backlight brightness               263
```

`skyward kms` 在 UI 运行时不可用（DRM master 被 UI 持有，atomic commit 返回 `Permission denied`），读 sysfs 即可。

### 6.3 触摸

```
syna-tcm probe 成功；input2 touchpanel、input3 touchpanel_kpd
fps_report_rate: 60 / 90 / 120
```

## 7. 当前镜像的启动方式

```bash
# 不写分区，最安全
adb reboot bootloader
fastboot boot /tmp/opencode/skyward-kebab.img
```

注意：`fastboot boot` 会让内核命令行带上 `androidboot.mode=fastboot`（与正常启动的 `androidboot.mode=reboot` 不同），见 §8.1。

## 8. 已知问题

### 8.1 Wi-Fi（搁置，2026-11-18）

**结论：搁置。** 除"Skyward 自更新"外，recovery 联网没有实际用途：

- 系统/ROM 更新检查依赖 `[update]`，kebab 没有配置（也没必要：线刷/侧载都走 USB）
- 安装、备份、恢复、adb、fastboot、MTP 全部走 USB 或本地存储
- Skyward 自更新需要 GitHub，而本移植不提供发布渠道，配了也取不到东西

因此 `devices/kebab.toml` 里 `[wlan]` 与 `qrtr-ns`、`cnss-daemon`、`vndservicemanager`
三个服务**整体注释保留**；UI 的 WLAN 页与 Skyward 更新页随之不再显示。

已经打通、将来恢复时不需要重做的部分：

1. 固件加载、QMI 握手、冷启动校准（`qrtr-ns` + `cnss-daemon` + `/data` 已解锁）
2. **mission 模式**：向 `/dev/wlan` 写 `"ON"` 触发 `hdd_driver_load()`；
   若冷启动校准正在进行，它会等校准完成再探测 → `wlan0`、`p2p0`、`wifi-aware0` 出现
3. supplicant 启动方式：`-B`（先加接口后注册 AIDL）、`-O<dir>`、
   `-g@android:wpa_wlan0`，并按 init 方式造 `/dev/socket/wpa_wlan0` 传 fd
4. `/dev/vndbinder` 指向 binderfs 节点，并启动 `/vendor/bin/vndservicemanager`

**唯一剩余卡点**：supplicant 同时是 vendor AIDL HAL，启动时注册
`android.hardware.wifi.supplicant.ISupplicant/default` 失败 → `wpas_aidl_init`
返回 NULL → 进程退出 255，并把刚建好的控制 socket 一起删掉。
日志特征：`AIDL Interface version: 5` 之后 3ms `Deiniting aidl control`。

可行修法（二选一）：

1. **在 Skyward 里做一个最小的 vendor 服务目录**（推荐）：作为 `/dev/vndbinder`
   的 context manager，对 `addService` 直接回 OK。需要给 `skyward-binder` 加
   `BINDER_SET_CONTEXT_MGR` 与服务端回复，约百行，不需要额外产物。
2. **LD_PRELOAD 一个 shim** 让 `AServiceManager_addService` 返回 0：
   省事，但 ramdisk 多一个 `.so`，属于掩盖。

即便修好，还需实测扫描、连接、DHCP、路由与联网检测。

#### 8.1.0 原先的 Wi-Fi 记录（保留）

> 当前状态（2026-11-18 晚）：**mission 模式已打通，`wlan0` 已出现**
> （还带出 `p2p0`、`wifi-aware0`），MAC 从 `wlan_mac.bin` 读到。
> 决定性的一步：向 **`/dev/wlan` 写入 `"ON"`**（qcacld 的 `wlan_hdd_state` 开关），
> 它会调用 `hdd_driver_load()` → `cnss_wlan_register_driver()`，
> 并在冷启动校准进行中时等待校准完成再探测芯片。之前只「打开」了这个设备节点，没有写 `ON`。
>
> **当前卡点（2026-11-18 深夜）**：`wpa_supplicant` 启动后，在
> `AServiceManager_addService` 注册 `android.hardware.wifi.supplicant.ISupplicant/default`
> 时失败，`wpas_aidl_init` 返回 NULL → `wpa_supplicant_run` 返回 -1 →
> 进程退出 255，并把自己刚建好的控制 socket 一起删掉（日志里能看到
> `AIDL Interface version: 5` 之后 3ms 就 `Deiniting aidl control`）。
>
> 已为此做的准备（都已提交）：
> - `/dev/vndbinder` 指向 binderfs 的节点（`secure.rs`），并启动手机自己的
>   `/vendor/bin/vndservicemanager`
> - 等 `cnss-daemon` 起来再写 `fs_ready`（`f017f63`）
> - supplicant 用 `-B` 启动（先加接口、后注册 AIDL），并按 init 的方式造
>   `/dev/socket/wpa_wlan0` 把 fd 传给它（`7824807`）
>
> 仍失败的注册调用看不到错误（manager 不打印、NDK 只返回非 OK）。两个可行方案：
> 1. **在 Skyward 里实现一个最小的 vendor 服务目录**（作为 `/dev/vndbinder` 的
>    context manager，对 `addService` 直接回 OK）——不需要额外产物，但要给
>    `skyward-binder` 加 `BINDER_SET_CONTEXT_MGR` 与服务端回复
> 2. **LD_PRELOAD 一个 shim** 覆盖 `AServiceManager_addService` 返回 0——省事，
>    但要在 ramdisk 里多一个 .so，且是掩盖而非解决
>
> 注意：即使 supplicant 起来，扫描/连接/DHCP 还没验证。
>
> 复现步骤（本次启动只写一次 `fs_ready`）：
>
> ```sh
> # /data 已解锁，且 qrtr-ns、cnss-daemon 在运行（daemon 需要 /data/vendor/wifi/sockets）
> /tmp/skyward wlan-up      # 写 fs_ready → 等 2s → 写 ON → 等 wlan0
> ```
>
> `cnss-daemon` 若因 `/data/vendor/wifi/sockets` 不存在而退出，会在目录建好后由服务目录重新拉起；
> 该目录由 `wlan.rs` 在 bring-up 时创建（`system:system 0770`）。
> 下方「现象」是最初的记录，保留作为对照。

#### 8.1.1 进展（2026-11-17 晚，实机验证）

WLAN 在 recovery 里需要三个 Android 侧的用户态组件，缺一不可：

| 组件 | 作用 | 缺省表现 |
|---|---|---|
| `/vendor/bin/qrtr-ns` | QMI 名称服务 | 固件停在 `MHI MISSION_MODE`，QMI 服务永远不出现 |
| `/vendor/bin/cnss-daemon` | 校准数据 / MAC 的 QMI 服务端 | 无 `FW_READY` |
| `/data/vendor/wifi/sockets` | cnss-daemon 的用户 socket 目录 | cnss-daemon 启动即退出 |

加入前两者并创建该目录后，实机日志：

```
cnss: QMI WLFW service connected
cnss: Sending host capability message
cnss: Received QMI WLFW calibration done indication
cnss: Calibration completed successfully
（cnss-daemon 生成 /data/vendor/wifi/wlfw_cal_01.bin，875832 字节）
```

`devices/kebab.toml` 已加入这两个 `[[service]]`，其中 `cnss-daemon` 以
`system` 用户、`system/inet/net_admin/wifi` 组、`NET_ADMIN` 能力运行；
Skyward 的账号表原先缺 `inet`/`net_raw`/`net_admin`，已补上（`skyward-sys/ueventd.rs`）。

仍未解决：**mission 模式**。每次写 `fs_ready` 都进入 `CALIBRATION`，
校准完成后不再上电，因此没有 `hif_pci_enable_bus` 和 `wlan0`。
Android 里由 wifi HAL 触发第二次上电；内核侧链路为
`cnss_wlan_register_driver → cnss_pci_register_driver_hdlr → cnss_pci_dev_powerup`，
对应 qcacld 的 `wlan_hdd_pld_probe → hdd_wlan_startup`。

待查方向：

- qcacld 在本机是内建（`CONFIG_QCA_CLD_WLAN=y`），驱动在**内核启动早期**就注册，
  早于 Skyward 挂载 `/vendor/firmware_mnt`；注册失败后不会重试
- 打开 `/dev/wlan` 不会触发 `hdd_wlan_startup`（实测无日志）
- 可尝试：给 qcacld 开动态调试、或在驱动注册前让固件分区可见（需要改内核/init 时序）

结论：Wi-Fi 从「完全不通」推进到「固件握手 + 校准成功」，只差 mission 模式这一步。


现象：

```
cnss: File system is ready, fs_ready is 1
cnss: COLD_BOOT_CAL_START
cnss: Setting MHI state: POWER_ON(2)
cnss_pci ...: Falling back to syfs fallback for: qca6390/amss20.bin
ueventd: loading ... took 18ms
cnss: MHI status cb ... MISSION_MODE(6)
（此后无进展；42 秒后）
cnss: fatal: Timeout waiting for FW ready indication
```

对照 Android（正常启动）：

```
5.495  fs_ready
5.545  Falling back amss20.bin
5.698  MISSION_MODE
8.373  FW_READY（第一次，校准）
16.000 FW_READY（第二次）
16.024  hif_pci_enable_bus → Target Ready → wlan0
```

已排除：

| 假设 | 结论 |
|---|---|
| 固件文件不同 | 排除：md5 一致 `8cd9cb2d…` |
| 内核不同 | 排除：与 `boot_a` 内核 sha256 一致 |
| 固件找不到 | 排除：ueventd 成功加载 |
| 时间太晚 | 排除：开机 10 秒写入结果相同 |
| debugfs 缺失 | 未验证；recovery 无 `/sys/kernel/debug` 挂载点，`/sys` 只读 |

剩余差异：

1. `androidboot.mode=fastboot`（`fastboot boot`）
2. `fastboot boot` 与正常启动的硬件初始化路径可能不同
3. 某个正常系统里由 wifi HAL 完成的动作（例如打开 `/dev/wlan`）在 recovery 中缺失

建议的决定性实验：**备份 `recovery_a` 后真正刷入该镜像，再启动测试**。若成功，说明与 `fastboot boot` 的启动路径有关，正式交付不受影响。

### 8.2 MTP 导致 USB 端口消失

根因：

- UI 默认开启 File transfer（`skyward-ui/src/app/state.rs` `file_transfer: true`）
- `usb::switch("mtp,adb")` 先切到 `none`（停 adbd、解绑 UDC）
- 注入的 `skyward-mtp.rc` 里启动 adbd 的规则带 `sys.usb.config_disable=0`，而 recovery 从不设置该属性 → 规则不触发 → 状态停在 `none` → USB 完全不枚举
- 日志：`the port did not take file transfer: sys.usb.state is Some("none") after 5000 ms, not "mtp,adb"`

已修（`e017082`）：

- 去掉 `sys.usb.config_disable` 条件
- `switch` 失败时回退到原配置
- 关闭 File transfer 可恢复 adb（已验证：`18d1:d001` → adb 正常）

**已复测（2026-11-18）**：File transfer 打开、`/data` 解锁后，Nautilus 能看到设备并读写内部存储，正常。

### 8.3 Data 解密（已可用，2026-11-17 深夜实机验证）

本机只有 Keymaster HIDL 4.0/4.1，没有 KeyMint AIDL。新增的 `skyward-crypto/src/keymaster.rs` 已完整实现 4.x HIDL 客户端并接入现有流程；recovery 里 `/data` 能挂载、能读，输入密码后用户的 CE 文件也能打开。

解密链路（与 vold 一致）：

1. `/metadata/vold/metadata_encryption/key/keymaster_key_blob`（keystore 前缀 + KM blob）
2. 去掉 `pKMblob\0` 前缀，用 KM4 `begin/update/finish` 解 `encrypted_key` → 得到 204 字节的 **hardware-wrapped key**（本身又是一个 `pKMblob` blob）
3. 再去前缀，`exportKey(RAW)` 换成这一次启动可用的 wrapped key
4. `dm-default-key` + `aes-xts-plain64`，cipher 带 `wrappedkey_v0`，内核走 inline crypto
5. 用户 CE：keystore2 数据库里的 `synthetic_password_*` blob（同样带前缀），gatekeeper 通过后 `begin/update/finish` 解开

`-62 KEY_REQUIRES_UPGRADE` 时调 `upgradeKey`（方法 12）后重试，磁盘上的 blob 不改。

#### 8.3.1 HIDL 4.x 的线上格式（踩过的坑，按重要度）

- **接口名编码**：HIDL 用 `libhidlbase` 里自己的 `android::hardware::Parcel`（日志 tag `hw-Parcel`），接口名是「ASCII + NUL + 4 字节对齐」，**不是** libbinder 的「UTF-16 计数 + UTF-16 + 结束符」。AIDL 走 libbinder，才是后者。
- **调用头**：libbinder 的调用头是 `[strictMode][workSource][kHeader][String16 接口名]`，而 `kHeader` 按构建分支不同：system=`SYST`、vendor=`VNDR`、recovery=`RECO`。recovery 里 `servicemanager` 是 recovery 构建，必须用 `RECO`；在 Android 里跑同一份二进制则用 `SYST`。现按 ramdisk 里是否有 `skyward/device.toml` 判断。
- **4.1 继承 4.0**：`getHardwareInfo`/`exportKey`/`begin` 等方法都声明在 4.0，stub 校验的是 **4.0** 接口名；拿 4.1 的名字调用会被 `enforceInterface` 拒成 `BAD_TYPE(-2147483647)`。
- **buffer 的 parent_offset**：`binder_buffer_object.parent_offset` 必须是「指针在父结构里的字节偏移」，恒写 0 会让驱动修指针时覆盖结构体首字段，内核报 `out-of-order buffer fixup` 和 `EINVAL(-22)`。
- **内联读取顺序**：`readNullableEmbeddedBuffer` 是按顺序读 40 字节记录，所以 `vec<KeyParameter>` 的 **每个元素都必须带自己的 blob 记录**（空也要带），否则后面的流全错位。
- **KeyParameter 布局**：32 字节 —— tag@0、union 值@8、blob 帧@16（地址 16..24、长度 24..28）。判别字段不参与判断，靠 tag 决定。
- **返回值顺序**：`begin` 的返回是 `(error, outParams, handle)`，handle 在最后；`vec<T>` 结果要读「帧 + 数据」两层；空 vec 的数据地址为 0，要当作空处理。
- **begin 的 inParams 不能带 `TAG_PURPOSE`**：purpose 是第一个参数，带进 params 里 QTI 会返回 `INVALID_ARGUMENT(-38)`（keystore2 里也有同样的注释）。
- **`TAG_*` 常量**：注意 TagType 前缀（ENUM=`0x1……`、ENUM_REP=`0x2……`、UINT=`0x3……`、BOOL=`0x7……`、BYTES=`0x9……`），写错会得到 `UNSUPPORTED_ALGORITHM(-4)` 之类的误导性错误。
- **blob 前缀**：keystore2 写的 blob 都是 `pKMblob\0` + HAL blob，交给 HAL 前必须去掉 8 字节。

#### 8.3.2 实机结果与仍未完成

- 已验证：`/data` 挂载（`dm-5`，`inlinecrypt`），输入密码后 user 0 与 user 10 的 CE 文件都打开（`/data/data`、`/data/user/10`、`/data/media/0` 可读）
- 旧固件生成的密钥（`-62 KEY_REQUIRES_UPGRADE`）会自动 `upgradeKey` 后重试
- recovery 里没有 keystore2 守护进程，CE 解密由 Skyward 自己按上面的链路完成
- 长错误信息以前会横向超出屏幕，现已改为按屏幕宽度换行（`widgets::toast`）

### 8.3.3 进 EDL（05c6:900e）的根因：显示驱动超时后 panic（2026-11-18 定位）

日志末尾是 `sde_dbg:[sde_evtlog_dump_all]` 与 `_sde_dump_reg_by_ranges`，即 SDE 的寄存器 dump。内核里这条路径是：

```
sde_encoder_phys_cmd_handle_ppdone_timeout / poll_timeout_wr_ptr 超时
  → 没有程序注册 DRM_EVENT_SDE_HW_RECOVERY（recovery 里没有 SurfaceFlinger）
  → SDE_DBG_DUMP("dsi_dbg_bus", "panic")
  → sde_dbg.c: _sde_dump_array(..., do_panic=true) → panic(name)
  → CONFIG_PANIC_TIMEOUT=5 → 复位 → bootloader 进 900e
```

`sde_dbg` 的 `panic_on_err` 默认就是 1（`DEFAULT_PANIC`），而 Android 里 SurfaceFlinger 会监听恢复事件、驱动只 capture 不 panic；recovery 里没人监听，所以**任何一次提交超时都直接把内核干掉**。

处置：

1. **注册恢复事件**（`surface.rs::watch_for_errors`）：用 DRM 的 `MSM_REGISTER_EVENT` ioctl
   （`DRM_IOW(0x40+0x41, struct drm_msm_event_req)`，`object_type = DRM_MODE_OBJECT_CONNECTOR`，
   `event = DRM_EVENT_SDE_HW_RECOVERY = 0x80000007`）替 SurfaceFlinger 注册那个事件。
   之后超时走 `sde_connector_event_notify` + ctl reset，而不是 `SDE_DBG_DUMP(..., "panic")`。
   启动日志会打印：`the display driver will tell of a failed commit rather than stop the kernel`
2. `[power] rest = false`：面板永不休眠/关断，只把背光降到 0、显示黑帧，减少会失败的提交种类

注意：`CONFIG_DEBUG_FS` 在**实际运行的内核里没有**（`mount -t debugfs` 报 `No such device`），
所以不能用 `/sys/kernel/debug/dri/*/debug/panic` 那条路；上面的 ioctl 是唯一可行的用户态办法。

诊断支持：`crash.rs` 每次启动会把 pstore 里的上次记录复制到 `/metadata/skyward/crash/`。
但实测 **从 EDL 恢复后 pstore 是空的**（bootloader 进 EDL 前把 ramoops 区域清掉了），
所以真正进 EDL 的那次仍然只能靠崩溃前的实时日志；pstore 只对普通重启/非 EDL 崩溃有效。

仍需观察：若面板偶发超时，日志里会出现 SDE register dump 但设备**不会**再进 EDL，
可据此继续查是哪类提交失败。

### 8.4 振动（已可用，2026-11-18）

AW8697 由内核的 `aw8697_haptic` 驱动，暴露为 LED class：`/sys/class/leds/vibrator/{state,duration,activate,gain}`。Skyward 已新增 LED 后端（`[haptics] led = "..."`），与高通 `vendor.qcom.opensource.vibrator` 的 OL 实现一致：

```sh
echo 1 > state      # 先置 state
echo 10 > duration  # ms；<=500 走短波形表，索引由 duration 决定
echo 1 > activate   # 开始播放
```

- 点击：10 ms（与 Lineage 设备 overlay 的 `config_virtualKeyVibePattern` 一致）
- 实机确认：开关/跳转行/输入法按键/解锁键盘/图案逐点/滑块/侧滑返回/熄屏双击 都有反馈
- 熄屏双击：两次 10 ms，间隔 60 ms（驱动一次只播一个片段）
- 强度：`gain` 0x01–0x75（0x00 不振）；滑块映射约 0x2F–0x75，默认档约 0x33，实测手感与 `gain=0x30` 相当
- 官方振动 HAL 不写 `gain`，所以系统里的强度设置对这台机器的开环马达本来就不起作用；Skyward 的滑块直接驱动它

复现：`/tmp/skyward haptic --config /skyward/device.toml [--position 0.5] [--ticks]`

#### 8.4.1 仍未完成

- 没有实现 HAL 的 `perform(effect)`/`compose` 波形表（长按、双击等组合效果）；目前用点击/双 tick 两种
- 长振动（>500 ms）会走长波形片段，Skyward 暂未用到

### 8.5 双系统

`skyward-systems` 硬依赖 `init_boot` 分区：

- `install.rs` 无条件读 `boot` 与 `init_boot` 的大小
- `boot.rs` 要求模板镜像 kernel 为空
- `switch.rs` 依赖 `init_boot`

kebab 没有 `init_boot`，kernel 与 ramdisk 同在一个 v2 boot 镜像中。需要重新设计「第二个系统的 first-stage 放在哪」，工作量大，暂不处理。

保护（2026-11-17 加入）：

- UI 层：`dual_boot` 要求 `boot` 与 `init_boot` 两个分区都存在（`ui.rs:1353`），
  kebab 上为 `false`，`systems_worker` 不会创建，也就不会发 `Task::Resume`。
- 库层：`install::resume()` 在没有 `init_boot` 时直接返回 `Ok(false)`；
  `install::switch_to()` 直接报错。即使上层逻辑以后变化，也不会写 `boot`/`init_boot`。
- `ensure_installed()`（fastboot 前恢复主系统 boot）只在 `fastboot && dual_boot` 时调用。

### 8.6 AVB：未签名 footer 与验证模式（2026-11-17 补充）

现象：

- 刷入打包出的镜像后，bootloader 报
  `The current image (boot/recovery) have been destroyed`
- `fastboot boot` 同样的镜像报
  `Failed to load/authenticate boot image: Load Error`
- 此时 `fastboot oem device-info` 显示 `Verity mode: true`
- 换回 stock `recovery.img` 可以启动；刷 ROM 的 `vbmeta.img` + `dtbo.img` 后可恢复系统

原因：

`skyward-pack` 写的是 `avb.rs` 里 `--algorithm NONE` 的**未签名 vbmeta footer**。
当 bootloader 的验证处于开启状态时，它无法认证这种镜像；而 stock Lineage recovery 是
签名镜像（SHA256_RSA4096），所以同样的状态下它仍能启动。

注意：`fastboot boot` 并不是「永远不校验」——验证状态打开后，它同样会拒绝未签名镜像。
排查时不要把它当作「镜像一定没问题」的证据。

处置：

1. 按 LineageOS 安装流程刷入禁用校验的 vbmeta：

   ```bash
   fastboot --disable-verity --disable-verification flash vbmeta vbmeta.img
   fastboot flash recovery skyward-kebab.img
   ```

2. 出现 `image destroyed` 提示时，用**同一个 ROM 包**里的 `vbmeta.img`、`boot.img`、`dtbo.img`
   一起恢复（实测：只刷 vbmeta 与 dtbo 仍进入该页面，补刷匹配的 `boot.img` 后恢复）。
   这说明「boot 被写坏」的表象，实际是 **vbmeta 记录的哈希/公钥与 boot 不匹配**。

3. 恢复镜像只能取自 `~/Downloads/lineage-24.0-*-kebab-signed.zip`：
   `~/work/PixelOS_kebab` 从未成功构建，其 `out/` 下的产物**不可使用**。

后续改进（可选）：

- 给打包器加签名能力（用测试密钥对 vbmeta 签名），使镜像在验证开启时也能被解析；
- 或让打包器同时产出「禁用校验」的 vbmeta，并写进安装说明。

## 9. 下一步

> 2026-11-17 收尾状态：**Wi-Fi 暂停**。实机反复触发 `fs_ready` 会走 cnss2 的失败路径
> （`CNSS_ASSERT`），导致设备进入 `05c6:900e`（crashdump/EDL）。后续应先离线分析，再上机。

> 2026-11-18 状态：**Data 解密已通**（含用户 CE），振动已通（LED 后端 + 全交互反馈）。
> 只剩 Wi-Fi，以及几项需要上机确认的功能。

按优先级：

1. ~~Wi-Fi~~ **已搁置**（2026-11-18）：见 §8.1，只剩 supplicant 的 AIDL 注册；配置整体注释保留。
   恢复时要做的：
   - 在 `078365e5318d`（本机内核提交）上读 `hdd_wlan_startup` / `cnss_pci_dev_powerup` 的完整调用链
   - 找出不依赖 wifi HAL 的触发方式（ioctl / sysfs / 驱动注册时序）
   - 上机时**每次启动只写一次 `fs_ready`**，且 `cnss-daemon` 以 system 用户 + 正确组启动
   - 相关服务目前已在 `devices/kebab.toml` 注释掉，避免 `CNSS_ASSERT` → EDL
2. ~~复测 MTP~~ 已完成（2026-11-18）：data 解锁后电脑可读写内部存储。
   注意 MTP 只有在 data 分区打开后才提供内部存储（`storage_open()`），否则电脑只看到设备、看不到存储；
   首页「文件传输」下方的脚注已写明这一点
3. **暂不实测**：备份/恢复、刷写、fastbootd、槽位切换 —— 按用户要求先记着，代码路径都在，不做逐项实机验证
4. ~~UI 加「卸载 data 分区」~~ 已完成（`1cb4975`，2026-11-18）
   - 解密页在 data 打开时显示说明与「滑动以卸载 data 分区」；安装、擦除、备份、
     文件操作、修复、第二系统等任一进行中时入口隐藏
   - 执行：先 `withhold` 收回 MTP 的内部存储，再在 worker 里调
     `wipe::release_data()`（结束占用进程 → 卸载 `/data`、`/sdcard` → `data::close()` 移除 dm 映射）；
     成功后状态回到 `Data::Closed`，文件管理与 MTP 一并失效，可随时重新解密
   - 底层与 CLI 早已存在：`wipe::release_data()`、`skyward data --close`，
     自动用于擦除、格式化/修复文件系统、安装/更新前
4. ~~Keymaster 4.x HIDL 客户端~~ 已完成（`bfe5a64` 起，见 8.3）
5. ~~LED 振动后端~~ 已完成（见 8.4）
6. ~~字体子集化~~ 已完成

不在计划内：双系统（kebab 没有 `init_boot`，需重新设计，见 8.5）

## 9.1 安全事项（务必遵守）

- 首次刷写前备份 `recovery_a`；恢复需要**同一构建**的 `vbmeta.img`、`boot.img`、`dtbo.img`
- 只在 verity 关闭（`vbmeta_a` flags=3）时刷未签名镜像
- 出现 `The current image (boot/recovery) have been destroyed`：刷回同构建的
  `vbmeta` + `boot` + `dtbo`（实测有效）
- 设备进入 `05c6:900e`（EDL/crashdump）：长按电源 15–20 秒；不行再用 MSM Download Tool
- 测试期间不要使用 Install / Wipe / Flash Skyward again
- 不要锁定 bootloader

## 10. 附录

### 10.1 实机信息采集命令

```bash
adb root
adb shell ls -l /dev/block/by-name
adb shell 'cat /sys/class/block/sda16/size'        # recovery_a 扇区数
adb shell getprop ro.build.display.id
adb shell uname -r
adb shell cat /sys/class/drm/card0-DSI-1/modes
adb shell cat /sys/class/backlight/panel0-backlight/max_brightness
adb shell ls /sys/class/leds/vibrator
adb shell ls /vendor/lib/modules
adb shell ls /vendor/firmware_mnt/image/qca6390
adb shell ls /vendor/firmware/wlan/qca_cld
adb shell ls /mnt/vendor/persist
adb shell grep -E 'userdata|metadata' /vendor/etc/fstab.qcom
adb shell lshal | grep -iE 'keymint|keymaster|gatekeeper|weaver'
adb shell service list | grep -i supplicant
```

### 10.2 日志摘录（recovery）

```
[    0.771] [TP]syna-tcm: syna_tcm_probe: enter
[    0.872] input: touchpanel as /devices/virtual/input/input2
[    1.192] [drm:dsi_display_bind] Successfully bind display panel
           'qcom,mdss_dsi_oplus20828samsung_amb655x_1080_2400_cmd'
[    2.032] [drm:dsi_panel_get_mode] default topology: lm: 2 comp_enc:2 intf: 1
[    2.815] skyward: kebab 0.3.1 recovery up, misc command "bootonce-bootloader"
...
[    9.901] cnss: Posting event: COLD_BOOT_CAL_START(5)
[    9.963] cnss_pci: Falling back to syfs fallback for: qca6390/amss20.bin
[   10.137] cnss: MHI status cb is called with reason MISSION_MODE(6)
```

### 10.3 参考

- Skyward 仓库：https://github.com/kirklin/skyward
- LineageOS kebab 安装：https://wiki.lineageos.org/devices/kebab/install
- TWRP kebab：https://twrp.me/oneplus/oneplus8t.html
- Android boot image header：https://source.android.com/docs/core/architecture/bootloader/boot-image-header