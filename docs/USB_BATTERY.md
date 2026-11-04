# USB 接收器电量适配（0.6.49，实验性）

这些查询只在专用厂商 HID 集合执行，不读取键盘按键或鼠标移动报告，不发送固件升级、配对或参数修改请求。

| 型号 | VID:PID | 接口 / Usage | 电量查询 |
| --- | --- | --- | --- |
| KZZI K75 Lite | 3554:FA09 | MI_01 / FF02:0002 | 0x13 报告，05/06 身份、07 在线、4A 电量 |
| VXE R1 系列 | 3554:F58A、373B:1085 | MI_01 / FF02:0002 | 0x08 报告，03 在线、01 会话读取、04 电量 |

K75 身份需匹配 `03 00 00 00 01 44`。不同身份不会被当作 K75 Lite。VXE 的百分比必须在 0–100，命令、状态和校验和必须有效。

后台每 15 秒刷新，输入响应有超时。USB 数据不依赖 Windows 蓝牙服务；不会被误报为蓝牙连接事件。显示接收器不等于无线端已唤醒，无法得到有效电量时只显示未知。

## 实机验证

1. 退出旧版程序，运行 0.6.49。
2. 保持两个原装接收器插在 USB 口，键鼠使用 2.4G 模式。
3. 退出 KZZI / ATK 官方驱动，避免争抢厂商 HID 集合；移动鼠标、按键唤醒设备后等待 15 秒。
4. 打开“键鼠电量”页，对比官方驱动读数。若仍为未知，反馈设备管理器中的 VID/PID（不需要序列号或完整设备实例路径）。

本项目没有访问用户的实物设备。KZZI 参考项目明确只确认 K75 Lite 连接状态，其电量查询仍需验证。VXE 的 Windows 参考实测为 R1 Pro Max / F58A；SE 参考提供 373B:1085 协议。不能据此保证所有叫 R1 的硬件都通用。

## 协议来源与许可

- [NaoCoding/ShowMeTheConnection](https://github.com/NaoCoding/ShowMeTheConnection)，提交 `a4447b28c2852cd16f13604a5a475d6cdd0056f4`，KZZI 驱动参考，MIT，许可见 `docs/licenses/ShowMeTheConnection.txt`。
- [S1berla/vxe-r1-pro-max-battery](https://github.com/S1berla/vxe-r1-pro-max-battery)，提交 `e9357ae6e449a8e2a3906330b3636bbcfa77d9af`，Windows COMPX 协议参考，MIT，许可见 `docs/licenses/vxe-r1-pro-max-battery.txt`。
- [prapooskur/vxe-battery](https://github.com/prapooskur/vxe-battery)，仅核对 R1 SE USB 标识与公开报文，没有复制其源码。
- HID 访问使用 `hidapi-rs` 2.6.7 的 Windows 原生后端，MIT 许可见 `docs/licenses/hidapi-rs.txt`。

完整许可同时收录于发行包的 THIRD_PARTY_NOTICES.txt。
