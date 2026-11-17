# Skyward 移植到 OnePlus 8T（kebab）记录

日期：2026-11-17
基线：Skyward v0.3.1（`crates/` 工作区），LineageOS 24.0 的 `recovery.img`

## 1. 现状摘要

| 能力 | 状态 | 说明 |
|---|---|---|
| 启动（recovery 分区镜像） | ✅ | `fastboot boot` 与打包流程均已验证 |
| UI | ✅ | 正常显示，英文界面 |
| 120Hz 高刷 | ✅ | DRM 暴露 60/120 两个模式，UI 启动后切到 120 |
| 触摸 | ✅ | Synaptics S3908，evdev `input2` |
| 分区识别 / super 挂载 | ✅ | odm/product/system/system_ext/vendor 全部挂上 |
| persist / modem 挂载 | ✅ | `/mnt/vendor/persist`、`/vendor/firmware_mnt` |
| 背光 | ✅ | `panel0-backlight`，max 2047 |
| adb（root shell） | ✅ | 需 File transfer 关闭，见 §8.2 |
| 字体 | ✅ | 子集化 MiSans，ramdisk 39.3 → 27.2 MiB，见 §3.6 |
| MTP | ⚠️ | 修复代码已提交，未复测 |
| Wi-Fi | ❌ | 固件加载成功，芯片不进入 FW ready，见 §8.1 |
| Data 解密 | ❌ | 本机只有 Keymaster 4.x HIDL，见 §8.3 |
| 振动 | ❌ | AW8697 是 LED 接口，非 evdev FF，见 §8.4 |
| 双系统 | ⛔ | 本机无 `init_boot` 分区，现有实现不可用 |

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
                 ├─ activate, duration, brightness, max_brightness
                 ├─ rtp, waveform_index, gain, f0, ...
                 └─ 无 evdev force-feedback
```

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
skyward: devices not brought up: ... no ...keymint.IKeyMintDevice/default
skyward: the pages speak En, with none chosen, and the locale named is en-US
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

### 8.1 Wi-Fi 不启动（主要未完成项）

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

待验证：重新打开 File transfer 后 MTP 是否工作。

### 8.3 Data 解密不可用

本机只有 Keymaster HIDL 4.0/4.1，没有 KeyMint AIDL。`skyward-crypto/src/keymint.rs` 只实现了 AIDL 客户端。

已有的基础：

- `metadata_encryption=aes-256-xts:wrappedkey_v0` 与 `keydirectory=/metadata/vold/metadata_encryption` 都在 Skyward 支持范围
- `crypto/keystore.rs` 已能解析 `keymaster_key_blob`
- HIDL 客户端基础设施已有（gatekeeper 用 HIDL 1.0）

需要：新增 Keymaster 4.x HIDL 客户端（getHardwareFeatures / unwrap 等），接入现有 blob 解析与 dm-default-key 流程。

### 8.4 振动

AW8697 暴露 `/sys/class/leds/vibrator`（`duration` + `activate`），而 `skyward/src/haptics.rs` 走 evdev force-feedback。当前 `devices/kebab.toml` 不配置 `[haptics]`，功能不受影响，只是没有震动反馈。

需要：增加 LED-class 后端（几十行），或保持不配置。

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

优先级从高到低：

1. **刷入前先确认 `Verity mode` 与 vbmeta 状态**（见 §8.6）；未签名镜像只在禁用校验时可启动
2. **日常测试只用 `fastboot boot`**，不再刷 recovery，直到 §8.6 的问题解决
3. 若仍不通：给 cnss2 打开调试能力（需要可用的 debugfs 挂载点），或在 ramdisk 里预留 `/sys/kernel/debug`；同时对比 wifi HAL 打开 `/dev/wlan` 后驱动行为
4. 复测 MTP（重新打开 File transfer）
5. 增加 Keymaster 4.x HIDL 客户端（解密）
6. 增加 LED 振动后端（可选）
7. ~~字体子集化~~ 已完成：`resources/fonts/MiSans-Subset.ttf`（ramdisk 39.3 → 27.2 MiB）

安全事项：

- 首次刷写前务必备份 `recovery_a`
- 不要锁定 bootloader
- 测试期间不要使用 Install / Wipe / Flash Skyward again

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