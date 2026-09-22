# 花笺 × 飞书多维表格

这是 floral-notepaper 的桌面同步扩展，不是另起一套网页便签。上游基线：`69a43ae`（1.2.0）。原界面、Markdown 编辑/预览、分类、导入导出、桌面小窗、磁贴和 MIT 许可保留。

多维表格是云端共享数据源；本地 Markdown 是离线副本和待同步编辑区。未启用同步时保持原来的本地使用方式。

## 开发验证环境（2026-09-20）

- 已有 Lark CLI 1.0.87，用户身份验证有效；未重新安装、未复制凭证。
- 已执行用户提供的 `npx --yes @lark-base-open/base-database-cli@latest skill install --target all`。安装的 CLI/技能版本为 1.1.1。
- Base Database CLI 用于飞书托管应用。本扩展保留 Tauri 桌面形态，所以运行时由 Rust 调用 Lark CLI 原生 Base 命令，不创建 Page/FaaS 或公开 Web 代理。
- 已使用独立创建的便签 Base 验证同步，未扩大分享权限。测试数据库不随源码共享；新用户需按下文连接自己的专用表。
- 当前机器的连接配置在 `lark/connection.local.json`，已忽略提交；可提交的模板为 `lark/connection.example.json`。

## 开发启动

Mac 安装包的生成、安装与首次连接步骤见 [Mac 体验说明](MAC_PREVIEW.md)。当前尚未生成安装包，不要使用上游原版 DMG 验证同步扩展。

需要 Node.js 22.22.1+（上游 lint-staged 的要求）、Rust stable，以及所在平台的 Tauri 2 系统依赖。当前环境 Node 22.16 能完成前端构建和测试，但安装时有上游工具链版本警告。

```bash
npm ci
npm run dev:lark
```

此命令沿用原项目的 Tauri/Vite 开发流程，使用独立 app identifier，并将配置和便签放在项目 `.local/desktop/config`、`.local/desktop/data`。首次启动读取连接配置；后续不会覆盖应用内已保存的设置。没有连接文件也能启动，在设置中配置即可。

Linux（Debian/Ubuntu）系统依赖示例，需由有权限的用户安装：

```bash
sudo apt-get install build-essential pkg-config libglib2.0-dev libgtk-3-dev \
  libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev patchelf
```

还需要可用的图形桌面。当前开发机缺少 GLib/GTK/WebKit 开发库，未能编译或启动原生窗口。Windows 需要 Visual Studio C++ Build Tools/WebView2；macOS 需要 Xcode Command Line Tools，系统最低版本沿用上游配置。

打包：`npm run build:lark`。安装包不内置 Lark CLI，每台电脑仍需安装并登录 CLI。LarkNote 标识的构建使用独立用户数据目录，并禁用上游自动更新、阻止安装上游原版覆盖飞书扩展。不要用原版安装包更新此分支。

只验证真实同步，无需桌面 SDK：

```bash
npm run sync:check  # 只读校验云端字段、读取行数
npm run sync:once   # 同步 .local/headless/device-a/data 下的便签
```

`sync:once` 是真实读写，不是模拟；首次运行只拉取云端已有便签。命令行测试目录与桌面开发目录分离，不要让两个进程同时同步同一数据目录。

## 连接自己的表

1. 使用当前飞书用户登录 Lark CLI。已有登录无需重新授权，可用 `lark-cli auth status --json --verify` 检查。
2. 新建专用多维表格，字段定义见 `lark/fields.json`：

   ```bash
   lark-cli base +base-create --name '花笺便签' --table-name '便签' \
     --fields @lark/fields.json --time-zone Asia/Shanghai --as user
   ```

3. 打开便签数据表，复制浏览器里的完整 HTTPS 链接。在应用“设置 → 飞书多维表格同步”里只填写这一个链接。支持 `/base/` 和 `/wiki/` 链接；无需手填 Base token、数据表 ID、CLI 路径、profile 或同步间隔。
4. 点击“连接并同步”：程序调用 `lark-cli base +url-resolve` 解析链接、检查便签字段，然后保存连接并开始同步。上传范围是**当前数据目录内所有便签**，默认每 60 秒同步；可用“暂停同步”停止，不删除两端数据。CLI 会从 PATH、用户安装目录及 Homebrew 常用目录自动查找；首次仍需在这台电脑安装并登录 CLI。
5. 多张表的 Base 必须打开便签所在的表，再复制包含 `table=` 的完整链接，程序不会默认选第一张表。只有一张表时也可粘贴 Base 本身的链接。
6. 另一台电脑粘贴同一个链接，用有访问权限的用户身份登录即可。原来保存的 Base token/表 ID 配置继续有效，重新连接后自动保存完整链接；机器专用配置与真实链接不提交公开仓库。

只读验证链接解析和自动查找（不会写云端记录）：

```bash
cargo run --manifest-path tools/sync-check/Cargo.toml -- resolve-link '<多维表格完整链接>'
```

Mac 常用快捷键见 [Mac 体验说明](MAC_PREVIEW.md#mac-快捷键)，也可在应用设置里展开“常用快捷键”。

| 字段                | 类型     | 用途                                         |
| ------------------- | -------- | -------------------------------------------- |
| 标题                | 文本     | 便签标题，主显示字段                         |
| 便签ID              | 文本     | 稳定同步键，建立同步后不要改动或复制成重复值 |
| 正文                | 文本     | Markdown 正文                                |
| 分类                | 文本     | 分类名称；空值表示未分类                     |
| 创建时间 / 修改时间 | 日期时间 | 便签元数据，不作为冲突判定的唯一依据         |
| 已删除              | 复选框   | 可恢复的删除标记                             |

可直接在表中新增行，填写标题、正文、分类；空便签ID会以该行的真实记录ID派生稳定键。不要修改已有便签ID。删除请勾选“已删除”，取消勾选即可恢复；不支持把直接移除整行视为删除信号，以免权限变化或缺行导致误删。

## 同步规则与安全边界

- 基于最后一次确认内容的 SHA-256 指纹做三方比较，不依赖设备时钟排序。
- 只有一端修改时传递该端；两端修改时云端内容保留在原便签，本地内容保留为“冲突副本”。副本在下一轮上传。删除与修改相遇时同样保留仍存在的内容。
- 窗口仍有未保存内容时暂缓拉取那条便签。慢请求期间发生的本地修改不会被回填覆盖。
- 每条成功确认后原子保存进度；失败后仍可离线编辑，后续定时/手动重试。创建重试按稳定便签ID查找已有记录，不按标题判断。
- 本地应用删除对应云端软删除，仍保留正文。云端更新覆盖/软删除本地副本之前备份 Markdown 到数据目录 `.lark-sync/backups`；图片目录不因云端删除而移除。备份当前无自动清理策略，可自行归档。
- 字段缺失、重复便签ID、损坏的同步状态、缺失云端记录或本地文件、分页版本变化均停止相应同步，不静默清空数据。不支持在已有同步状态的目录中切换另一张表；请创建独立数据目录。
- CLI 使用参数数组启动，不经 shell；正文经权限受限目录内的临时 JSON 文件传递，完成后清理；45 秒超时。前端只调用固定 Tauri 命令，不接触 access token、app secret。
- 访问权限来自登录用户对 Base 的权限，不使用 owner 代理或 bot 降级，也没有公开 HTTP 服务。这是个人多设备便签，不提供团队成员行级隔离。共享表意味着协作者能够看到或修改其中的数据。
- **并发限制**：Base 的记录更新命令没有原子 compare-and-swap；实现会在写入前复查、写入后回读，但不能保证另一设备在复查和写入的极短窗口内同时修改时完全无覆盖。不用于高并发多人实时协同编辑。
- **范围限制**：同步标题、正文、非空分类与删除状态；图片附件、空分类、磁贴位置/大小、主题、快捷键、外部文件不跨设备同步。正文最多9万字符，标题1000字符，整表最多约1万行（含删除记录）。

## 测试与维护

```bash
npm test
npm run lint
npm run build
npm run test:sync
npm run test:sync:live
```

最后一条会在连接的表中**新增验收便签**，真实验证两个隔离设备目录和云端编辑，最后只将本次测试便签标记为删除，不硬删云端行。失败时保留数据便于排查。结果写入 `.local/live-regression/verification.json`。

核心源文件：`src-tauri/src/services/lark_sync.rs`、原项目 `notes.rs` 的导入/备份入口；桌面调度与命令为 `src-tauri/src/lark_sync_commands.rs`；设置与状态展示位于 `src/features/sync/`。`tools/sync-check` 直接引用生产 Rust 源码，非另一套同步实现。

上游 `PRIVACY` 描述纯本地版本；启用此扩展后，便签会同步到用户明确指定的飞书多维表格，不能继续将其视为“完全不联网”的存储模式。
