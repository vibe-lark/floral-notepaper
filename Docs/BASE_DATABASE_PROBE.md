# Base Database 实际可用性验证

时间：2026-09-22 04:13（北京时间）。CLI：`@lark-base-open/base-database-cli@1.1.1`，环境：`online`，身份：`user`。账号 profile 和资源标识仅保存在本地验证记录中，不随源码公开。

## 结论

当前账号和环境能访问应用列表，但不能创建 Base Database 应用。实际初始化在应用创建接口被拒绝：

```text
POST /open-apis/base/v3/sites/create
800004011: project management feature is not enabled
```

这是服务端明确返回的功能未启用错误，不是本地安装失败，也不是 SQL/FaaS 代码错误。尚不能从该错误确定开关按用户、租户、开放平台应用还是环境控制，需由 Base Database 平台方核实开通。

此前 `doctor` 通过的依据是用户身份验证和 `GET /open-apis/base/v3/sites/list` 成功，不代表应用创建或 SQL/FaaS 可用。

## 实际操作与结果

只执行了一次初始化：

```bash
npx --yes @lark-base-open/base-database-cli@1.1.1 \
  --profile '<YOUR_LARK_PROFILE>' \
  init .local/base-database-probe --env online \
  --name 'LarkNote Base Database 隔离验证' \
  --description '验证便签数据的 SQL 查询、FaaS 写入和回读，仅使用隔离测试数据' \
  --schema .local/base-database-probe-schema.json
```

1. 独立测试 Base 创建成功。
2. `验证便签` 数据表与标题、正文、测试标识三个文本字段创建成功。
3. Base Database 应用创建返回上述错误，未获得 appToken。
4. 只读复查应用列表为空，`hasMore=false`。未重复初始化。
5. 只读复查测试表为 0 条记录；没有写入任何业务或验收便签。
6. SQL 校验、Preview 发布、SQL 执行和 FaaS 执行均未进入；不能声称这些能力通过或已经被单独测试失败。

## 本次遗留资源

- 创建了一个隔离测试 Base，包含一张“验证便签”数据表。
- 实际链接、Base token 和数据表 ID 保存在本地 `.local/base-database-probe-result.json`，不随源码公开。
- 空表保留，未删除资源，未更改分享范围。
- 原便签 Base 及同步源码未修改。

机器可读结果：`.local/base-database-probe-result.json`。CLI 未返回 LogID 或 HTTP 状态，未为取日志重复执行写操作。

## 恢复条件

由平台方确认并启用当前身份对应的 Base Database project management 能力后，再使用已有测试 Base 的 `--base-token '<TEST_BASE_TOKEN>'` 继续初始化，避免 `--schema` 再创建一张表。实际 token 从本地验证记录取得。届时按顺序完成 Preview、SQL 查询、FaaS 写入和 SQL 回读。

依照 `lark-base-database` 的诊断与验收要求，遇到明确平台阻塞时停止创建/发布，不切换身份、环境或改用普通记录接口冒充 Base Database 验证。当前既有的 Lark CLI 原生记录同步不因本次测试而变更。
