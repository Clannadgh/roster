# Roster

个人自用的多账号工作台，把 **WorkBuddy / CodeBuddy、TraeWork / Trae、Qoder** 三条产品线、国内版 / 国际版共 **6 个作用域**的账号收在一处：

- 账号登录（WorkBuddy / Qoder 浏览器授权，Trae 粘贴 `Cloud-IDE-JWT`）
- 一键签到与自动排程（分钟级时点、随机抖动、错过补签）
- 账号切换（WorkBuddy / Qoder 写客户端登录态，Trae 走登录态快照）
- 积分统计（余额、今日 / 7 天 / 本月消耗、30 天趋势）
- 开机自启 + 托盘常驻（关闭窗口不退出，排程继续运行）

数据全部保存在本机 `~/.roster/`，账号凭据经 Windows DPAPI 加密落盘，不上传任何第三方服务。

## 技术栈

Tauri 2（Rust）+ React 19 + Vite 7 + Tailwind CSS 4 + shadcn/Radix。

## 开发与构建

环境要求：Node.js ≥ 20、Rust stable（MSVC toolchain）、Visual Studio Build Tools（C++ 桌面开发）。

```bash
npm install
npm run dev                      # 浏览器里调前端
scripts\build-desktop.cmd check  # Rust 类型检查
scripts\build-desktop.cmd        # 桌面 debug 构建
scripts\build-desktop.cmd release# 打 Windows NSIS 安装包
```

安装包输出：`src-tauri/target/release/bundle/nsis/Roster_<version>_x64-setup.exe`

## 使用注意

- 切换账号前需要先退出对应客户端；客户端运行中会拒绝写入。
- Trae 切换依赖登录态快照：先在客户端登录目标账号 → 退出 → 在账号卡上「保存登录态」，之后才能切。
- WorkBuddy 国际版没有签到接口（上游限制），签到入口会禁用。
- 本工具会读写本机客户端的认证数据，请自行评估风险。

## 非官方声明

本项目为个人项目，与 WorkBuddy、CodeBuddy、TraeWork、Trae、Qoder 及其权利人不存在隶属、授权、赞助或背书关系。相关名称与图标归各自权利人所有，仅用于识别。使用本工具可能违反相关产品的服务条款，风险自负。

## 许可

[PolyForm Noncommercial License 1.0.0](./LICENSE)：仅允许个人非商业使用，禁止商业使用。部分后端逻辑派生自 [NextAgentX/qoder-trae-workbuddy-switch](https://github.com/NextAgentX/qoder-trae-workbuddy-switch)（PolyForm Noncommercial），界面布局与组件参考 [changexbc/workbuddy-switch](https://github.com/changexbc/workbuddy-switch)（MIT）。详见 [NOTICE](./NOTICE)。
