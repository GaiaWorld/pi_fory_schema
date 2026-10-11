# fory-schema-shell-core 设计 = 需求 + 契约

## context（L0）

- 地形：Rust 2024，MSRV 声明 1.85；Cargo.toml:1-11。当前依赖 fory=1.7.3、bytes=1.12.1、本地 derive，不改变依赖。src/lib.rs 公开模块及重导出必须保持。
- 既有接口：schema/value/key/registry/document/fory_adapter/error 包含固有方法和 trait 方法体；运行时大部分返回 NotImplemented，四个只读 accessor 直接访问已有内容。类型描述、常量、标记 trait 及 derive 不新增功能。
- 测试：无 tests 目录、无内联测试；本流程不生成测试脚手架。
- 构建核验：cargo +nightly-x86_64-pc-windows-msvc check --offline --locked --all-targets；cargo +nightly-x86_64-pc-windows-msvc fmt --all -- --check。该结果不证明 MSRV 1.85 通过。
- 素材：已读主库全部 src 与 Cargo.toml；已读本技能模板和机械层协议。derive 仅作路径依赖，不修改其源码。
- 来源：用户要求壳芯代码骨架及票；用户选择 lite 本轮不加依赖；用户确认 slug；用户允许主库初始化本地 Git，具体冻结提交草单仍需 L7b 确认。组织既有方案：无。
- 范围：仅保留既有公开面的内部委托拆分；芯私有，不重导出；不实现解析、哈希、比较、注册、宏生成，不补新公开 API。
- 待验证：L1 复查是否零新增对外面；L7b 确认草单、既有 accessor 行为保持、壳芯先行提交范围及构建。rapidhash::v3 与 rapidhash=4 为后续独立集成选择，不在本轮修改或实施。
- 本节自检：来源可达；依赖范围已收窄；未修改代码、依赖或 Git；仅完成只读事实收集。

## 需求（L1）

用户已批准本节 L1；本轮仅写设计需求并机械推进至 L7。

### REQ-001 既有公开面冻结

- 保持全部既有 public 签名、泛型、生命周期、约束、接收者、参数、返回值、公开字段、类型、常量、trait、模块 path 和重导出不变；零新增对外 symbol。
- 芯为现有模块的嵌套私有 mod，不重导出，不把父模块私有字段改为 pub；lib.rs 不新增 pub 模块。
- 类型、常量、无 body 的 trait 声明及标记实现留在壳；不把注释保留的接口变成可调用 API。
- 验收：逐项对拍既有公开面；私有状态的可扩展性仅允许在 L7 提案中讨论，不视为已批准字段调整。

### REQ-002 带 body 的公开入口一句委托

- 将源码中全部 104 个显式公开 fn / 公开 trait 实现方法体收敛为一句私有芯委托；不含 derive 展开的生成方法。
- 只读枚举：schema 8、value 76、key 6、registry 7、document 2、fory_adapter 2、error 的 Display 3，共 104。
- 按现有模块拟设 schema_inner、value_inner、key_inner、registry_inner、document_inner、fory_adapter_inner、error_inner；具体私有声明待 L7/L7b 确认。
- buffer、descriptor、limits、encoding 无显式方法体，不新增方法芯；父子模块按标准 Rust 模块路径组织。
- 验收：104 个入口均有一一对应的委托去向，壳不保留运行时算法或偷偷补未确认接口。

### REQ-003 现行为完整保持

- 保持每个 NotImplemented 的错误类型及字符串标识，不以成功、panic 或 fabricated 值替代。
- 保持 StringView::as_str、BinaryView::as_bytes、NodeView::node_type、ValidatedNodeView::node_type 四个 accessor 的原借用对象、生命周期和只读行为，不解码、不分配、不验证、不消费预算。
- SchemaRegistryBuilder::new 仍创建空占位；Default 委托芯且与 new 行为一致，芯构造返回壳类型，不对外开放构造。
- 三个 Display 仍按 Debug 文本写入传入 formatter 并返回原 fmt::Result；保持 Formatter 生命周期和原签名，不新增错误包装。
- 验收：对拍委托前后源码行为；本流程不生成测试。

### REQ-004 本轮范围与质量边界

- 零新依赖、新语言、对外契约面或新增 warning；保留现有依赖版本。
- 不修改 derive、storage 实验或实现解析、验证、注册、哈希、比较、宏生成；rapidhash 仅为后续独立方向，不在本轮实施。
- 采用 lite，不安排架构审查；不生成测试、票或未确认接口。
- 后续代码核验沿用 L0 的 locked/offline check 与 fmt 命令；不得把 nightly 结果表述为 MSRV 1.85 通过。本轮设计写入不执行源码构建。
- 验收：变更路径与依赖对拍、warning 与构建门禁均在获准落壳后核验，当前不声称已验证未来代码。

### REQ-005 分阶段授权与后续芯实现边界

- 本轮仅操作本设计目录；源码只读，Git 不操作。L1 自检通过后仅推进 L7。
- L7 完整契约提案仅在回复提供；未获批准不写 L7 正文、不登记 API/CASE 表、不分配 API/CASE 编号。
- 后续 L7b 确认后才允许落壳及 Git；已允许 init 不等于已允许提交草单。冻结基线须记录 Git baseline 哈希，具体提交范围仍待确认。
- L8 票内可修改私有芯；接口未定不擅补，不能偷偷修改壳字段。全 private 占位状态是否支持仅芯真实实现须评估；必要时在 L7 提案中请求批准 opaque inner state 内部调整，未批准前不改。
- 验收：日志可追溯当前批准范围，当前阶段为 L7，L7b/L8/L9 均未执行。

### L1 自检

- 来源：用户已批准的 L1 与本轮明确执行约束；五条需求均可回溯，无本轮新增提问。
- lite 判据：不新增对外契约面、依赖或语言；芯路径和状态设计尚未落盘为契约。
- 完整性：公开面冻结、104 入口委托、现行为、范围质量、阶段授权均有验收条件。
- 冲突处理：公开字段冻结与私有状态可扩展性区分；任何壳私有字段调整必须先明确批准，不能由 L8 票擅自扩大范围。
- 本节通过；L7 保留原占位正文，仅准备未批准提案。

## 契约（L7）

待 L1 完成后提出。
