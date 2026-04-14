<div align="center">

# 🌐 tracert-gui

Windows 平台图形化网络路径追踪工具

</div>

---

## ✨ 特性

- 🎯 **可视化追踪** - 实时显示网络跳点、RTT 延迟和地理位置
- 🌐 **双栈支持** - 同时支持 IPv4 和 IPv6 协议
- 🛡️ **自动配置** - 启动时自动管理 Windows 防火墙规则
- ⚡ **实时反馈** - 流式显示追踪结果，无需等待完成

## 🚀 快速开始

### 📋 环境要求

- 💻 Windows 10/11
- 🦀 Rust 1.80+
- 🔐 管理员权限（程序启动时会请求 UAC）

### 🔧 编译运行

```bash
# 开发模式
cargo run

# 发布构建
cargo build --release
```

## 🛠️ 技术栈

| 类别 | 技术 |
|------|------|
| 🖥️ 核心 | Rust 2024 Edition |
| 🎨 GUI | Iced v0.12 |
| ⚙️ 异步 | Tokio |
| 🌍 网络 | socket2 + 原始套接字 |
| 🪟 系统 | Windows COM API (INetFwPolicy2) |
| 📍 IP 库 | ipdb-rust |

## 📝 许可证

MIT License

---

<div align="center">

**Made with ❤️ using Rust**

</div>
