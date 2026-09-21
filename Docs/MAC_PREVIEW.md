# Mac 体验版：获取安装包与连接飞书

当前方案继续使用 **Lark CLI 原生多维表格记录同步**，不依赖未开通的 Base Database 应用能力。

## 当前交付状态

本工作区运行于 Linux x86_64，没有 Xcode/macOS SDK，也没有已连接的 Mac 构建机。**目前尚未生成 Mac 安装包**；已有的是打包脚本和可手动触发的 macOS CI 流程。不要下载上游原版 DMG 来验证飞书同步，上游包不包含本分支改动。

本地已通过118项测试、TypeScript、lint、工作流 YAML 解析及脚本语法检查。`npm run build:mac` 在 Linux 上按预期拒绝执行；这不代表 Mac 原生编译或安装验收已通过。

同步扩展仓库为 [yannisxu/floral-notepaper](https://github.com/yannisxu/floral-notepaper)，基于原作者 `Achilng/floral-notepaper`。Mac 工作流 `.github/workflows/larknote-macos-preview.yml` 仅手动触发，上传代码不会自动打包或发布 Release。

## 安装包生成方式

### 通过 GitHub Actions

授权上传至自己的仓库后，在 Actions 手动运行 **LarkNote Mac Preview**。流程分别在 macOS 构建机运行前端测试、原生 Rust 测试，并生成：

- `aarch64-apple-darwin`：Apple 芯片（M 系列）。
- `x86_64-apple-darwin`：Intel Mac。

只有构建、应用标识、架构、ad-hoc 签名及 DMG 完整性校验均成功，才上传 `.dmg` 和 `SHA256SUMS`。在该次运行的 Artifacts 下载，保留14天，不自动创建公开 Release。当前仓库是公开仓库，工作流日志和制品不应包含个人数据或凭证。

GitHub Actions 可能消耗账号的 macOS 构建额度。构建时无需飞书登录，不上传本机 `.local/` 数据、连接配置或凭证；安装包也不内置 Lark CLI 和账号。

### 在自己的 Mac 上构建

需要 macOS 15+、Node.js 24、Rust stable 和 Xcode Command Line Tools。拿到**包含当前改动的源码**后运行：

```bash
npm ci
npm run build:mac
```

默认匹配当前 Mac 芯片。也可构建双架构通用包：

```bash
npm run build:mac -- --target universal-apple-darwin
```

脚本自动安装所需 Rust target。安装包输出至 `local-build/macos/<target>/`。

此打包入口专用于预览：采用 ad-hoc 签名，未做 Apple Developer ID 签名或公证，不会使用环境中的 Apple 发布凭证。安装时 macOS 可能拦截；核对来源和 SHA256 后，按系统提示在“系统设置 → 隐私与安全性”中允许该应用。不要关闭全局 Gatekeeper。若企业安全策略禁止未公证应用，需要正式签名与公证，不能保证此预览包可运行。

## 拿到 DMG 后怎么体验

1. 根据 Mac 芯片选择对应 DMG，拖动“花笺飞书便签”到“应用程序”。目前最低要求 macOS 15，沿用上游限制。
2. **在这台 Mac 上**安装并登录 Lark CLI；开发服务器上的登录不会自动带过去：

   ```bash
   npx @larksuite/cli@latest install
   lark-cli auth login --domain base
   command -v lark-cli
   ```

   已安装/已授权可以跳过对应步骤。若 CLI 尚未配置飞书应用，先按其配置指引完成配置，再进行用户授权；登录用户必须能读写目标多维表格。无需安装 Base Database CLI。

3. 打开“设置 → 飞书多维表格同步”，填写：
   - Base token：填写自己的专用多维表格 token。建表或解析链接的方法见 [连接自己的表](LARK_SYNC.md#连接自己的表)。
   - 数据表 ID：填写同一表格下 `tbl` 开头的真实数据表 ID。
   - CLI 路径：填上一步 `command -v lark-cli` 得到的**Mac 本机绝对路径**。Finder 启动的应用可能没有终端 PATH，不能填服务器 Linux 路径。
   - profile：默认留空；使用多账号时指定已配置的 profile。
   - 同步间隔：60 秒。

4. 勾选启用，点击“保存并校验”→“立即同步”。已有云端便签会被拉取；新建的空表没有便签属于正常情况。
5. 新建便签，保存后同步；在自己连接的多维表格中确认新增行。直接修改该行正文，再同步，验证云端回传。

多维表格每条便签对应一行。删除使用“已删除”复选框，不要直接删行或修改便签ID。图片附件暂不同步。应用退出或电脑休眠时不再同步，联网且运行时再重试。

预览版使用独立的应用标识和本机 `larknote` 数据目录，默认不导入原版花笺的便签，也不会自动启用同步上传旧数据。更详细的数据安全与冲突规则见 [LARK_SYNC.md](LARK_SYNC.md)。
