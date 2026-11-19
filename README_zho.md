<p align="center">
  <img src="images/EH3D_PHOENIX_PNP_BANNER.svg" alt="EH3D-PHOENIX-PNP banner" width="100%">
</p>

# 📖 EH3D-PHOENIX-PNP

<p align="center"><a href="README.md">🇺🇸 English</a> | <a href="README_spa.md">🇪🇸 Español</a> | <a href="README_fra.md">🇫🇷 Français</a> | <a href="README_ita.md">🇮🇹 Italiano</a> | <a href="README_deu.md">🇩🇪 Deutsch</a> | 🇨🇳 <b>中文</b> | <a href="README_jpn.md">🇯🇵 日本語</a> | <a href="README_por.md">🇵🇹 Português</a></p>

### 贴片装配：位置、供料器和贴装顺序

<p align="center">
  <img src="https://img.shields.io/badge/License-GPL%203.0--or--later-blue.svg" alt="GPL 3.0 or later">
  <img src="https://img.shields.io/badge/Stack-Rust-DEA584.svg" alt="Rust">
  <img src="https://img.shields.io/badge/Languages-8-00E5FF.svg" alt="Languages">
  <img src="https://img.shields.io/badge/Version-0.0.9-0f6cbd.svg" alt="Version">
  <img src="https://img.shields.io/badge/Maturity-scaffolding-FFB020.svg" alt="Maturity">
</p>

---

**诚实检查 - 当前真正能运行的内容：** 一个 Rust 库和一个命令行程序（`phoenix-pnp`），有 963 个可在计算机上运行的测试：读取电路板的位置文件，为每个元件分配供料器（支持优先级、托盘和料带），从文件读取机器设置，为最多六个吸嘴的贴装头规划贴装顺序（等级、吸嘴头、角度限制、移动时间、拼板），写出上料表，并在模拟机器上逐步执行计划，可注入故障。它还有一个窗口（用 `--features gui` 构建），其逻辑在没有屏幕的情况下经过测试；绘制本身不在测试范围内；程序会拍摄自己窗口的图片（`--shot`），但没有人在屏幕上检查过绘制。它不控制任何机器：没有在机器上试过，也没有用真实文件集合检查过。

---

> **安全状态：** 本程序不控制任何机器，也不是安全系统。它的输出在机器上运行之前必须由人检查；这里的任何内容都没有在真实硬件上运行过。


## 📋 速览

| | |
|---|---|
| 版本 | `0.0.9` |
| 成熟度 | 脚手架 |
| 测试 | 963 |
| 技术栈 | Rust |
| 真实硬件 | 未试过：这里的任何内容都没有在机器上运行过 |

**跳转到：** [概述](#-概述) · [架构](#-架构) · [快速开始](#-快速开始) · [测试](#-测试) · [结构](#-仓库结构) · [相关项目](#-相关项目)

## 🚦 项目状态

每一行是本仓库的一个部分，灯号表示它今天做到了哪一步。内容依据仓库能够展示的证据（测试、构建、运行），而不是计划。

🟢 已完成并验证 · 🟡 可用，有已知限制 · 🟠 部分完成或尚未验证 · 🔴 未完成或不可用

| 领域 | 状态 | 在这里的含义 |
|---|---|---|
| 核心 | 🟡 | 读取电路板的位置文件，为每个元件分配供料器（优先级、托盘、料带），为最多六个吸嘴的贴装头规划贴装顺序（等级、吸嘴头、角度限制和移动时间），规划拼板并写出上料表；已在计算机上用正常和损坏的输入测试，但没有用真实文件集合测试。 |
| 命令行 | 🟡 | `phoenix-pnp` 运行与库相同的函数（`parts`、`check`、`plan`、`schedule`、`preflight`、`simulate`、`loadsheet`、`config-check`, `check-bottom`, `tables-check`, `recipe-check`, `import-centroid`, `vision-test`）并打印结果；每个错误都以带 `PHOENIX-PNP-NNN` 代码的消息和非零退出码返回。 |
| 测试 | 🟡 | 963 个测试通过（`cargo test`），包括正常情况、边界情况、损坏的输入和小型生成的用例。 |
| 机器配置 | 🟡 | 带版本号的 `键 = 值` 文件、会指出行号的校验器以及安全限制（Z 行程、X 和 Y 的限位、重试次数、吸嘴角度）；这些值没有与真实机器比对过。 |
| 模拟器与作业 | 🟡 | 虚拟时钟、按概率（带种子）或在选定位置注入的故障、重试、警报与搁置两种策略、反复故障后停用的供料器、暂停、单步执行以及可用于恢复的已保存状态；只是模拟，没有驱动过任何机器。 |
| 安全 | 🟡 | 纯安全 Rust（`forbid(unsafe_code)`）：处理收到的文件时不会 panic，错误的值会返回带原因的错误；没有独立审查。 |
| 窗口 | 🟠 | 一个 egui 窗口（用 `--features gui` 构建；不带命令运行 `phoenix-pnp` 即可打开）：作业、零件、封装、供料器、上料表、机器、工具、问题和日志等页面，显示贴装头路径、极性标记和下一个零件的工作台地图，五步向导，带可撤销修复的问题列表，可暂停、继续和单步的模拟作业，以及八种语言。其逻辑在没有屏幕的情况下经过测试；绘制不在测试范围内；程序会拍摄自己窗口的图片（`--shot`），但没有人在屏幕上检查过绘制。还有拼板页（含基准点和移动零件的拟合）和电路板页、文件选择器、可撤销编辑的供料器与封装表、会记住的列宽，以及在 Windows 上无控制台打开的独立程序（`phoenix-pnp-gui`）。 |
| 真实硬件 | 🔴 | 未试过：这里的任何内容都没有在机器上运行过。 |
| 文档 | 🟡 | 八种语言的 README、更新日志，以及由 `docs/` 绘制的两张图。 |

## 🎯 概述

* **元件**（`src/parts.rs`）：KiCad 的 `.pos` 文件或 CSV（逗号或分号、引号、按名称查找表头），位置单位可为 mm、mil 或英寸，角度归一到 0-360，面为顶面或底面；每个不合理的行都会带行号被拒绝，重复的位号是错误。
* **不贴装的零件、示例的表格和作业报告**（`src/parts.rs`、`src/commands.rs`、`src/job.rs`）：CSV 中名为 `dnp`、`do not place` 或 `fitted` 的列（或 `--dnp R1,R2`）会让零件不进入计划、上料表和检查，并在报告中列出；`tables-check` 读取表格（`--parts`、`--packages`、`--catalog`、`--feeders`、`--slots`），并给出每个错误的代码和行号；`simulate --report-csv 文件` 把作业统计写成表头永不改变的 CSV。`examples/` 里有带 `dnp` 列的电路板、各种供料器以及常见封装表。
* **供料器**（`src/feeders.rs`）：供料器的 CSV（编号、值、封装、取料位置、剩余数量）；每个元件得到第一个值和封装相同（不区分大小写）的供料器；没有供料器的元件和将耗尽的供料器会被报告，同类的第二个供料器接替。
* **计划**（`src/plan.rs`）：按最近优先走过各贴装位置，并切分成一到六个吸嘴的批次，批内供料器也按最近优先访问，底面按板宽镜像，计划给出行程和时间估计，并写成 JSON 作业。
* **表格与设置**（`src/catalog.rs`、`src/config.rs`、`src/rotation.rs`）：数值可继承的零件表和封装表（先零件，再封装，再默认值）、`键 = 值` 格式的机器配置文件（`examples/machine.cfg` 说明每个键）、CSV 位置文件列名的更多写法，以及按封装的旋转修正。
* **不止料带的供料器**（`src/feeding.rs`）：优先级、托盘、料带和被停用的供料器；无论文件顺序如何，元件的供料器选择都相同。
* **机器的规划器**（`src/planner.rs`、`src/motion.rs`）：等级（依次贴装的块）、顺序策略、吸嘴头及其更换、吸嘴角度的限制、含加速度和加加速度的移动时间、更短的路径（2-opt）以及元件原点的修正。
* **拼板、上料表与导出**（`src/panel.rs`、`src/loadsheet.rs`、`src/export.rs`）：拼板中的电路板和拼板、每种料带的槽位、报告、缺料清单，以及只移动贴装头的 G 代码。
* **拼板的基准点**（`src/fiducial.rs`）：把基准点的实测位置拟合到设计位置（两个点用相似变换，三个及以上用仿射拟合），拼板的零件按拟合移动并旋转；每个点都有残差，某点偏离超过 `fiducial_max_residual_mm` 的拟合会被拒绝（`--fiducials 文件` 配合 `--panel`，或窗口的拼板页）。
* **标记为不良的电路板（X-out）与两级标记** (`src/marks.rs`, `src/panel.rs`)：拼板文件中带 `"x_out": true` 的子项，或 `--x-out A,B/C` 指定的项，连同其中的一切都会被排除；计划、上料表和检查都看不到它，任何电路板都没有的名称（`PHOENIX-PNP-042`）或所有电路板都被排除的拼板（`PHOENIX-PNP-043`）都是错误。电路板上的零件，若其值或封装含有 `fiducial_words` 中的词，或其位号是 `fiducial_refs` 中的前缀加数字（`FID1`），就是该电路板的标记，不会被贴装；`--fiducials` 文件随后可以命名拼板的标记（`f1`）和电路板的标记（`A/FID1`）：一块电路板的标记一起拟合，只移动该电路板的零件，没有测量标记的电路板的零件则跟随拼板的拟合。仅为模拟：从未在机器上测量过任何电路板，窗口还不使用电路板的标记。
* **规划器和模拟器的更多功能：** 托盘单元格被取空的顺序（按行、按列或蛇形）、同时计入路程和去换头位置往返的吸嘴头顺序搜索（`tip_strategy = optimize`，受 `tip_opt_max_evals` 和 `tip_opt_time_limit_s` 限制）、按封装和按供料器设置的故障概率（`sim_part_fail`、`sim_feeder_fail`）及其统计和每小时零件数，以及显示底面各规则所得角度的 `check-bottom`。
* **对位步骤** (`src/job.rs`, `src/sim.rs`, `src/model.rs`)：`align = on` 时，作业在吸取零件后用相机查看，并按所见修正贴装（贴装头移到目标位置减去零件的偏移，吸嘴少转零件的旋转量）；`pick_tolerance_mm` 会丢弃偏移更大的零件，并按 `error_policy` 重新吸取；`align_passes`（1 到 5）和 `align_settle_s` 设定查看次数和每次之前的等待，它们计入计划时间；模拟相机有偏移、误差和可注入的故障 (`sim_align_*`, `sim_camera_fail_rate`)；修正量（平均值、最大值、丢弃的零件）出现在文本、JSON 报告和 `--report-csv` 的 `align` 行中。默认关闭：此时作业与以往完全一样。未在机器上试过。 当 `align_rereads` 大于 0（最大 5，默认 0）时，对第一次查看超出公差的零件，相机会在丢弃前再看一次；落在公差内的查看视为第一次，作业报告会按供料器统计超出公差的查看、额外查看、由此保住的零件、丢弃的零件以及无读数的查看。
* **视觉处理流程**（`src/vision.rs`、`src/sim.rs`、`examples/vision.cfg`）：设置 `vision = pipeline` 后，模拟相机在内存中画出零件的图像（大小、旋转以及与吸嘴的偏差，噪声取自 `sim_seed`），并用设置文件（`--vision FILE`）中的步骤测量：掩膜、中值滤波、固定阈值或 Otsu 阈值、斑块的面积范围和最小旋转矩形，其中心和角度就是对位步骤所修正的偏差。找不到零件、斑块面积不符和出现多个斑块，都是没有读数的查看，各有自己的代码（`PHOENIX-PNP-037` 至 `PHOENIX-PNP-039`）。`phoenix-pnp vision-test` 按给定的大小、旋转、偏差和噪声画出零件，并打印测量结果及其误差。默认关闭：此时作业与以前完全相同。未在真实相机上试过。
* **作业报告** (`src/report.rs`, `src/job.rs`)：模拟作业之后，`simulate` 可以把报告写成：按 `--lang` 语言的文本 (`--report-text`)；包含全部内容、没有脚本也不引用任何外部资源、且绝不覆盖已有文件的 HTML 页面 (`--report-html`)；追加在 CSV 各行之后的分节 (`--report-csv`，表头不变)；以及 JSON 中的 `work` (`--report`)。报告给出已贴装、已搁置、被操作员跳过、仍待处理和不贴装的零件数量，机器时间花在哪里（送料与拾取、相机查看、贴装、吸头移动、更换吸嘴），首次拾取就成功的数量，按封装、供料器、吸嘴、吸嘴头和拼板中各电路板的同样统计，出现的错误及其代码，作业前的提示，以及相机修正量（X、Y 和旋转）的直方图。这是模拟结果：其中没有任何数据是在真实机器上测得的。
* **“作业”标签页与操作员的决定** (`src/gui/jobtab.rs`, `src/gui/window.rs`, `src/job.rs`)：表格显示每个贴装的状态（等待、已拾取、已对位、已贴装、已搁置、已跳过、失败）和不贴装的零件，带机器时间和剩余时间估计的进度条，以及按状态、按文本和按是否完成的筛选。错误使作业停止时，对话框会指出失败的步骤，并提供四个在模拟作业上执行的操作，之后作业继续：重试、跳过该零件、搁置该零件，或关闭供料器以便由同类的另一个供料器接手。每个决定都会计入报告。逻辑无需屏幕并已测试；没有人在屏幕上检查过界面绘制（`images/screens/` 中此标签页的图片由程序自己拍摄）。
* **真空、执行器与供料器的故障窗口** (`src/sim.rs`, `src/job.rs`, `src/config.rs`)：`vacuum = on` 时，模拟的泵在每次拾取后吸气 `vacuum_suck_s` 秒，并读取吸嘴的真空度（千帕）；低于 `vacuum_pick_min_kpa` 表示吸嘴上没有零件（`PHOENIX-PNP-044`，并按重试设置重新拾取），每次贴装后吹气 `vacuum_blow_s` 秒，高于 `vacuum_release_max_kpa` 表示零件仍粘在吸嘴上（`PHOENIX-PNP-045`）。传感器有自己的随机数生成器（`sim_vacuum_noise_kpa`、`sim_vacuum_miss_rate`、`sim_vacuum_stuck_rate`），默认关闭，因此不使用它的作业与以前完全一样。开关型设备用 `actuator = 名称 : 延迟秒数` 声明，并用 `Job::set_actuator` 切换，其延迟会加到时钟上（机器没有的名称为 `PHOENIX-PNP-046`）。`fault_action = alert` 时，在最近 `fault_window` 次拾取中达到 `fault_limit` 次故障的供料器不再自行关闭：作业停止（`PHOENIX-PNP-047`），Job 标签页的对话框提供重试、跳过该零件、搁置或关闭该供料器。
* **相机模型、机器的贴装头与封装可用的吸嘴** (`src/camera.rs`, `src/heads.rs`, `src/tips.rs`, `src/config.rs`): 相机由 `camera_units_per_px`（0 表示视觉流水线设定的尺寸）、`camera_offset_x_mm` 与 `camera_offset_y_mm`（吸嘴轴线在图像中的位置）、`camera_rotation_deg`、`camera_flip_x` 与 `camera_flip_y`，以及镜头（`camera_lens_scale`、`camera_lens_k1`）来描述；对位步骤中模拟的相机透过该模型读取零件，无论有无图像，使用默认值时作业与以前完全一样。`head = 名称 : 吸嘴数 [: 偏移X : 偏移Y : 相机X : 相机Y : 相机旋转 : 相机单位 : 相机镜头]` 最多声明四个贴装头，每个都有自己的吸嘴、偏移和相机；有两个或更多贴装头时，`schedule` 把每个零件分给够得着它的贴装头（都够不着则为 `PHOENIX-PNP-048`），为每个贴装头做计划，并在机器的时钟上计入贴装头等待另一个贴装头正在使用的供料器的时间。`tip_compat = 模式 : 吸嘴, 吸嘴, ...` 列出封装可接受的吸嘴，`tip_compat_mode`（`fewest_changes` 或 `preferred`）为每一类零件选一个吸嘴，使机器换吸嘴的次数尽可能少；表中指定的吸嘴会被保留。
* **配方** (`src/recipe.rs`, `src/job.rs`, `src/commands.rs`)：带版本号的文本文件（`examples/job.recipe`）规定作业经过的步骤（`prepare`、`pick`、`align`、`place` 和 `check`，按此顺序且每步至多一次；只有配方包含对位步骤和真空检查时它们才会启用），以及随封装、供料器或吸嘴而变的参数（`pick_retries`、`pick_tolerance_mm`、`vacuum_suck_s` 和 `vacuum_blow_s`；每个键以第一条匹配的规则为准，先看供料器，再看封装，最后看吸嘴）。`phoenix-pnp recipe-check 文件` 读取并检查它，`simulate ... --recipe 文件` 按它执行作业。其错误的代码为 `PHOENIX-PNP-049` 至 `PHOENIX-PNP-052`（无效的行、步骤顺序错误、文件超出限制、版本过新），限制是 `machine.cfg` 的键 `recipe_max_bytes` 和 `recipe_max_rules`。保存的状态还会记住最近一次拾取的供料器和被中止的拾取，因此在另一次运行中载入的作业会向操作人员提供同样的决定（旧状态仍可载入）。
* **位置文件的导入** (`src/import.rs`, `src/commands.rs`)：`phoenix-pnp import-centroid 文件` 读取质心 CSV（`--map 文件` 指定其列，见 `examples/centroid.map`；`examples/centroid.csv` 不需要映射）、EAGLE 的位置文本（`examples/board.mnt`：名称、x、y、旋转（底面为 `R90` 或 `MR90`）、值、封装）或 KiCad 的 `.pos`（`--format auto|csv|eagle|kicad-pos`；`auto` 根据第一行判别），并把零件表 `ref,val,package,x,y,rot,side,dnp` 以毫米写入 `--out`（已存在的文件绝不会被覆盖：`PHOENIX-PNP-040`）或标准输出。单位为 mm、mil 或 inch（写在表头 `X (mil)`、单元格 `0.5in`、注释 `## Unit = inch`、映射或 `import_unit` 中）；角度输出为 0 至 360；面所在的列可省略；列 `dnp` 或 `fitted`（或带位号范围的 `--dnp R1-R5,C7`）标记不贴装的零件；`import_bottom_mirror_width_mm` 镜像底面的 X，`bottom_rotation` 旋转其角度。三个读取器都是程序自带的（其他仓库没有无需增加依赖就能使用的读取器），EAGLE 文本的排布也没有与真实安装写出的文件比对过。每个错误都带有 `PHOENIX-PNP-053` 至 `PHOENIX-PNP-056` 的代码（无效的行、缺少映射所需列的表头、未知的格式、无效的位号范围）以及行号。
* **物料清单**（`src/bom.rs`、`src/cmd_tools.rs`）：`phoenix-pnp bom-check 零件 BOM` 读取物料清单（`examples/bom.csv`：单元格中的位号，可为范围或每行一个，以及有则读取的值、封装、数量和料号），并指出只有一方才有的位号、重复列出的位号、值或封装不同之处以及数量对不上的行；标记为不贴装的零件可在清单中也可不在。文件错误带有代码 `PHOENIX-PNP-057` 和行号。
* **批量生产的库存**（`src/stock.rs`）：`phoenix-pnp stock 零件 供料器 --boards N` 把板数累加，按零件种类说明 N 块板所需数量与已开启供料器中的数量之比，哪些种类不够、差多少，以及第一卷料在多少块板之后用完；有不足时以 1 退出，并用 `--out` 写出表格。`bom-order --have 供料器` 从采购清单中扣除供料器已有的数量，并给出 `on_hand` 与 `to_buy` 两列。
* **相机校准**（`src/calibrate.rs`）：`phoenix-pnp camera-calibrate 文件` 用头部已知位置上对同一标记的观测（`px,py,x_mm,y_mm` [见 `examples/camera_points.csv`]，至少三个）拟合相机模型（像素大小、旋转、镜像以及吸嘴轴线在图像中的位置），输出拟合程度和用于配置的 `camera_*` 行；不拟合镜头。其错误为代码 `PHOENIX-PNP-058`。
* **吸嘴偏心**（`src/calibrate.rs`、`src/export.rs`）：`phoenix-pnp nozzle-calibrate 文件` 用多个角度下吸嘴头的读数（`angle_deg,x_mm,y_mm` [见 `examples/nozzle_readings.csv`]）拟合圆，输出 `nozzle_runout_mm` 和 `nozzle_runout_deg`；偏心大于 0 时，`schedule --gcode` 的 G 代码会把每个贴装位置按零件角度处的偏心反方向移动。
* **网格拼板**（`src/panel.rs`、`src/cmd_tools.rs`）：`phoenix-pnp panelize 电路板 --cols N --rows N --pitch-x MM --pitch-y MM` 为同一电路板按网格复制的拼板写出拼板文件，各板旋转相同，带有隔开一定边距的三个基准点（`--fiducials MM`，按 `--board-size W,H` 给出的电路板实际尺寸），并可按名称 `P1`、`P2`……标记不良板（`--x-out P2,P5`）；已存在的文件绝不覆盖。
* **有方向零件清单**（`src/polarity.rs`）：`phoenix-pnp polarity-sheet 零件` 列出位号匹配 `polarity_refs` 或封装匹配 `polarity_packages` 的零件（默认包括二极管、LED、芯片、晶体管、有极性电容和连接器），附位置、旋转和一列空白勾选栏，用于作业后的检查；`--side` 只保留一面。
* **轴间隙（反向间隙）**（`src/sim.rs`、`src/motion.rs`）：`backlash_x_mm`、`backlash_y_mm` 和 `backlash_mode` 使模拟机器在轴反向的移动中要么短了一个间隙停下（`off`，由 `simulate` 计数），要么越过位置再从正方向返回（`approach`，耗时更多但没有误差）；间隙为 0 时一切不变。
* **采购清单**（`src/bom.rs`）：`phoenix-pnp bom-order BOM --boards N` 把物料清单中相同零件的行相加，给每一行 N 块板所需的数量加上备用量（`order_spare_percent` 百分比或 `order_spare_min`，取较大者；`--spare-percent` 和 `--spare-min` 优先），不计 `--parts 零件` 中标记为不贴装的零件；写出按封装和值排序的 CSV。
* **供料器示教**（`src/calibrate.rs`）：`phoenix-pnp feeder-teach strip` 根据第一个和最后一个零件的位置求料带的间距与角度，`feeder-teach tray` 根据料盘的三个格位求两个间距与角度（并给出列与直角的偏差）；两者都会输出它们所设定的供料器文件的列。`feeder-teach --measures FILE` 从一个文件读取多个供料器的位置，并为每个供料器输出一行说明和供料器文件的一行（`examples/feeder_measures.csv`）。
* **模拟器与作业**（`src/sim.rs`、`src/job.rs`、`src/preflight.rs`、`src/model.rs`）：作业前的检查、带虚拟时钟和可注入故障的机器，以及可暂停、单步执行、保存并恢复的作业。
* **窗口**（`src/gui.rs`、`src/gui/`）：窗口除绘制之外的工作都不需要屏幕（已载入的文件、规划及其问题与修复、模拟作业、地图的几何、共享选择的表格、五步向导和八种语言的文字），并经过测试；只有用 `--features gui` 构建的 `src/gui/window.rs` 用 egui 绘制。模拟速度、主题以及为未知封装的零件绘制的大小是 `machine.cfg` 的键（`gui_steps_per_s`、`gui_theme`、`gui_part_size_mm`）。
* **检查吸嘴上的元件**（`src/inspect.rs`、`src/cmd_vis.rs`）：`phoenix-pnp part-inspect 读数 --sizes 尺寸表`（或 `--packages 封装表`）把相机测得的每个元件（两条边、偏离轴心的位置和旋转量）与其封装尺寸比较，并给出被拒绝的原因（`part_size_tol_mm`、`part_angle_tol_deg`、`part_offset_max_mm`）；尺寸来自尺寸表，或来自封装表的 `body_long_mm` 与 `body_short_mm` 列（`examples/packages_bodies.csv`），只要有元件被拒绝或没有尺寸，命令就以 1 退出。
* **料管与散料供料器**（`src/feeding.rs`、`src/feedlib.rs`）：除料带、托盘和条带外，供料器表的 `kind` 列还接受 `tube`（零件滑向挡块的料管或导轨）和 `bulk`（盛在料盒或料碗中、方向不固定的散料）；两者都像料带一样从唯一位置 `x`、`y` 取料，零件角度取自 `rotation`，按数量耗尽，并由 `feeder-check` 检查；窗口的供料器标签页以八种语言提供全部五种类型（`examples/feeders_tubes.csv`）。
* **按供料器统计的贴装数与用时**（`src/job.rs`、`src/feedrep.rs`、`src/report.rs`）：作业会记下每个吸嘴从哪个供料器取到零件（同类的另一个供料器接替时也一样），并把该零件算作从该供料器贴装，同时累计从每个供料器取料的各步骤（前往、送料、取料和查看）所用时间；作业报告、其 JSON 以及 `simulate --report-feeders`（末尾新增的 `placed` 和 `busy_s` 列）都会显示，已保存的作业也会保留。
* **吸嘴头寿命**（`src/nozzlelife.rs`、`src/cmd_tools.rs`、`src/commands.rs`）：`simulate --nozzle-life 文件` 会把运行到结束的作业对每个吸嘴头所做的事（贴装数量和安装次数）累加到放在配置旁的小型 CSV 中；一个作业只计一次，不是吸嘴头记录的文件绝不会被覆盖，被替换的文件保留为 `文件.prev`，只有 `nozzle-life 文件 --reset 吸嘴头` 记录更换时计数才会减少。`nozzle_life_warn`（默认 500000，0 表示不提醒）是提醒查看吸嘴头的贴装数，由 `simulate` 和 `nozzle-life` 给出。新增错误码 `PHOENIX-PNP-059`。
* **哪种顺序最快**（`src/compare.rs`、`src/commands.rs`）：`phoenix-pnp order-compare 零件 供料器` 用每种贴装位置 `order`（nearest、part、height_part、pick、tips、none）和每种 `tip_strategy`（minimize、as_planned、optimize）规划同一块板，并排显示批次、更换吸嘴头次数、贴装头行程和秒数，标出每个列表中最快的和配置中使用的，并用一行说明更好的设置能节省多少秒；它接受 `schedule` 的选项，不改变任何内容。
* **计划图**（`src/preview.rs`、`src/commands.rs`）：`phoenix-pnp schedule 零件 供料器 输出.json --svg 输出.svg` 还会把计划写成一个 SVG 文件，可在机器移动之前查看、打印或随作业保存：供料器、每个零件的位置（带顺序编号和吸嘴颜色）、每个零件从供料器走的线、贴装头走遍整个作业的虚线路径以及合计；文件不含脚本和外部内容，位号或 ID 中输入的字符会被转义，已存在的图不会被覆盖。
* **用图案标定相机** (`src/pattern.rs`, `src/cmd_tools.rs`)：`phoenix-pnp camera-pattern 文件 [--pitch MM]` 读取网格上的点（棋盘格内角点或圆点中心），格式为 `col,row,px,py`（`examples/camera_pattern.csv`），并拟合像素大小（`camera_units_per_px`）和镜头径向畸变（`camera_lens_k1`），同时拟合图案纸在图像中的旋转、平移和镜像；然后给出各点在含畸变与不含畸变两种模型下的偏差以及偏差最大的点；打印配置所需的行，当有点超过 `camera_pattern_max_residual_mm` 时退出码为 1。各点间距为 `camera_pattern_pitch_mm`。相机的旋转和偏移仍由移动贴装头的 `camera-calibrate` 得出。工具标签页有同样的标定，并且不会把超出限值的拟合结果应用到机器。
* **哪个基准点有偏差** (`src/fidcheck.rs`, `src/gui/workspace.rs`, `src/gui/tables.rs`)：使用拼板的实测标记时，窗口会计算每个标记的残差（即使拟合被拒绝也会计算，以便看出原因）、唯一一个去掉后其余标记都落在 `fiducial_max_residual_mm` 以内的标记，以及旋转、比例和剪切相对各自限值的情况。基准点表用 `!` 标出超出限值的标记，用 `?` 标出与其余不一致的标记，面板用文字列出警告（日志中也有）；`fidcheck::analyze_marks`、`fidcheck::warnings` 和 `fidcheck::residual_table` 为 `fiducial-check` 命令和基准点检查工具提供同样的结果。
* **作业队列** (`src/queue.rs`, `src/cmd_queue.rs`)：依次处理多块板或拼板。`phoenix-pnp queue-add 队列 名称 来源 供料器 [--panel] [--boards N]` 把一个作业放进一个即使程序停止也能保留的小 CSV（先写入临时文件再移动到位，被替换的文件保留为 `路径.prev`，最后一行带有校验和，因此被截断或手工修改的文件会被拒绝）；`queue-status [--check]` 列出作业及其状态（pending、running、done、failed、skipped），加 `--check` 会为尚未完成的作业做计划；`queue-edit` 可跳过、重置、撤销确认或移除作业；`queue-run` 在模拟机上逐块板完成作业，并在每块板之后写入队列。没有确认就不会运行：`queue_confirm` 开启（默认）时，作业只有在 `queue-confirm` 记录了其计划的标识（计划及其所用文件的内容）之后才会运行，文件有任何改动都会使确认作废；`queue_stop_on_failure`（默认开启）使队列在第一个失败的作业处停止。只要存在 `.lock` 文件，对同一队列的第二个 `queue-run` 会被拒绝。工具标签页的队列工具用按钮完成以上所有操作。新增错误码 `PHOENIX-PNP-060`。
* **UTF-16 文本** (`src/parts.rs`)：命令和窗口读取的每个表格和设置文件（位置、供料器、封装、物料清单、拼板、实测标记）都可以是带字节顺序标记的 UTF-16，就像 Windows 程序和电子表格保存的“Unicode 文本”；读取方式与 UTF-8 相同。既不是 UTF-8 也不是 UTF-16 的文本会带提示按 Latin-1 读取，而工具标签页中读取文件的工具会拒绝它。
* **校正镜头** (`src/camera.rs`, `src/sim.rs`)：设置 `camera_correct_lens = on` 后，相机会从所见图像中去除镜头畸变（`camera_lens_k1`，即 `camera-pattern` 得到的值），因此对位步骤的读数相当于一个没有畸变的镜头；`CameraModel::to_machine_corrected` 和 `read_corrected` 通过反解 `d * (1 + k1 * d * d)` 来实现。关闭（默认）时畸变会留在读数中。此时 `camera_units_per_px` 应为像素的真实大小，即 `camera-pattern` 给出的值。
* **相对于队列文件夹的路径** (`src/queue.rs`, `src/cmd_queue.rs`)：`queue_relative_paths = on`（或 `queue-add --relative`）时，位于队列文件所在文件夹内（或其下）的作业文件以相对于该文件夹的路径保存，因此文件夹可以移动或共享；`queue-status`、`queue-confirm` 和 `queue-run` 无论从哪个文件夹运行，都以队列文件夹为基准解析。关闭（默认）保存绝对路径；两者运行结果相同，计划标识也相同。
* **带按钮的队列** (`src/gui/queuetab.rs`, `src/gui/window.rs`)：工具标签页的队列工具把队列文件读成作业列表，并提供按钮：添加作业（表单以窗口语言校验）、检查计划、确认所选作业的计划、撤销确认、跳过、重置、移除和运行。每个按钮都用窗口旁的程序运行同名命令，因此窗口和命令行以同一种方式规划、确认和运行（相同的计划标识、相同的锁）。第一次按“运行”只会询问（`queue_gui_ask_run`，默认开启）；第二次才在模拟机器上于后台开始运行，并带有停止按钮。
* **可以变差的模拟相机** (`src/vision.rs`)：`vision.cfg` 新增 `sim_blur_px`（相机失焦）、`sim_salt_pepper`（变成黑或白的像素）、`sim_gradient_levels`（光照不均）、`sim_failure_rate` 和 `sim_failure_kind`（一部分图像损坏：零件不在、灯灭了或整体过曝）；全部默认关闭，不使用它们的作业噪声不变。`vision-test` 和 `simulate --vision` 像原有的噪声一样使用它们。
* **用于试验检查的读数** (`src/visionsim.rs`, `src/cmd_vis.rs`)：`phoenix-pnp vision-readings PARTS --sizes F` 为位置文件中的每个零件画出模拟相机拍到的图像（略微偏离轴线并旋转，范围由 `--spread` 和 `--turn` 决定；`--wrong-every N` 让每第 N 个零件变成另一种尺寸），并把读数写成 `part-inspect` 读取的文件；`phoenix-pnp vision-marks DESIGN --shift X,Y --turn DEG --scale S` 观察按指定方式放置的拼板上的每个标记，写出 `fiducial-check` 读取的测得标记。模糊、噪声和故障来自 `--vision`；找不到任何东西的图像不给读数并说明原因；相同的 `--seed` 给出相同的读数。
* **可保存的作业报告** (`src/report.rs`)：`simulate --report-html FILE`（没有脚本、不引用外部内容的单个页面）、`--report-text`、`--report`（JSON）和 `--report-csv` 现在还会按供料器给出取料、贴装、搁置和跳过的零件数，按吸嘴头给出本次作业增加的寿命（`--nozzle-life` 提供作业前的记录）、使用前后的次数，以及吸嘴头达到 `nozzle_life_warn` 时的警告；时间、检查结论和带代码的错误原先已有。
* **更准确的计划时间** (`src/planner.rs`)：在 `backlash_mode = approach` 下，机器会越过位置一个轴的间隙再退回，这需要时间；计划以前没有计入，现在逐段计入（仅在限位前有余量时），因此计划的秒数与模拟作业的秒数一致，`AdvancedPlan::approach_s`、计划报告及其 JSON 会说明第二次移动占了多少秒。吸嘴头仍按批分组，每批的供料器从近到远访问；不使用 approach 时没有任何变化。
* **常用封装的尺寸** (`src/pkggen.rs`, `src/cmd_tools.rs`)：`phoenix-pnp package-gen 0603 SOT-23-5 SOIC-8 QFN-32-5x5 LQFP-48`（或用 `--from PARTS` 取位置文件中的封装）会根据 EIA 编码的贴片元件、SOT、SOD、SOIC（窄体和宽体）、TSSOP、QFN、DFN 和 QFP 的标称尺寸，写出封装表的行（`package,height_mm,tape_mm,body_long_mm,body_short_mm`），并给出容纳本体的料带宽度；`part-inspect` 的 `--packages` 和目录会读取该结果。未知的名称会报错并说明原因，绝不猜测。 `phoenix-pnp package-kicad PATH...` 可从 KiCad 封装文件做同样的事（`src/kicadmod.rs`、`examples/chip_0603.kicad_mod`）：本体是制造层图形的外框（没有时用占位区并给出提示），高度在名称已知时取该封装的值，否则用 `--height`；读取在大小、深度和元素数上都有限制，读不懂的文件会连同原因一起指出。
* **学习得到的贴装偏移** (`src/learn.rs`, `src/cmd_tools.rs`, `src/job.rs`)：`phoenix-pnp place-learn READINGS --state F` 读取 `package,dx_mm,dy_mm` 行（零件最终偏离其位置多远），把每种封装的平均偏移保存在带校验号的文件中。与同封装其他读数相差很大的读数会被剔除，读数太少的封装保持不变，已知偏移最多按 `place_learn_memory` 个读数计算，以便跟随漂移的机器，任何偏移都不会超过 `place_learn_cap_mm`；`--residual` 用于已应用偏移后测得的读数，`--dry-run` 不写任何东西，`--show` 与 `--forget` 读取并修剪该文件。`place_learn` 打开前学习保持关闭，只有 `place_learn_apply` 打开时，`simulate --place-offsets FILE` 才会让贴装头前往每个位置减去其封装偏移之处；被截断或被手工改动的文件会被拒绝。
* **供其他程序使用的作业文件** (`src/exchange.rs`, `src/commands.rs`, `src/cmd_tools.rs`)：`phoenix-pnp job-export PARTS FEEDERS OUT [--state F]` 把计划（即 `schedule` 的文档）、每个零件的状态（`pending`、`placed`、带原因的 `deferred`、`skipped`）和作业阶段（就绪、暂停、停止、完成）写入一个带版本和类型的 JSON 文件；给出 `simulate` 的已保存状态时，会说明该作业进行到了哪里。`phoenix-pnp job-import FILE [--csv F]` 带全部检查读取这样的文件（大小和贴装数上限、100000 mm 以内的有限数值、长度受限且无控制字符的名称和原因、每个引用只出现一次、批次和吸嘴的计数、每个贴装都有指向同一引用的状态行，以及与各状态一致的阶段），说明其内容，并可把贴装连同状态写成表格。不会从中运行任何东西，也不会对机器应用任何东西。
* **钩子（默认关闭）** (`src/hooks.rs`, `src/commands.rs`, `examples/hooks.csv`)：`simulate --hooks FILE` 会针对作业的事件（`job-finished`、`job-paused`、`job-stopped`、`part-aside`、`feeder-off`）运行操作员自己的程序，用来点亮指示灯、播放声音或发送消息。运行程序是本库最有风险的操作，因此在 `hooks` 打开之前一律关闭（`--hooks-dry-run` 只列出将要运行的内容）。规则文件（`event,program,argument...`）就是允许运行内容的清单：程序是规则文件所在文件夹中的文件，名称只含字母、数字、`.`、`_` 和 `-`，绝不能是路径、系统搜索、shell、指向文件夹之外的链接或批处理文件；参数逐个传递，事件数据通过 `{event}`、`{reference}`、`{feeder}`、`{reason}`、`{phase}`、`{placed}` 和 `{total}` 填入。规则文件有大小上限且最多 50 条规则，每个作业最多运行 `hooks_max_runs` 个程序，每个程序有 `hooks_timeout_s` 秒，输出被截断为 4096 字节；失败的钩子不会改变作业的任何内容。
* **模拟作业的日志** (`src/journal.rs`, `src/commands.rs`)：`simulate --journal FILE` 为作业的每一步写一行：零件、计划取料的供料器、吸嘴、机器时钟的秒数、这一步用时，以及结果（`ok`，或使作业停止的错误代码）。记录日志的作业与不记录的作业执行完全相同的步骤；作业因错误停止时日志仍会保留，失败的那一步是最后一行；已存在的文件绝不会被覆盖。
* **两个零件表之间的变化** (`src/partsdiff.rs`, `src/cmd_more.rs`)：`phoenix-pnp parts-diff FIRST SECOND [--tol MM] [--rot-tol DEG] [--out F]` 按位号比较两个位置文件，逐个零件说明哪些被添加或删除、移动、旋转、改了值或封装、换到了另一面；位置在 `--tol` 内、旋转在 `--rot-tol` 内（按较短的方向计算）视为相同，仅大小写不同的值或封装也视为相同，`--out` 把变化写成 CSV。两个表不同时退出码为 1。
* **相机联合标定** (`src/combined.rs`, `src/cmd_tools.rs`, `src/gui/toolbox.rs`)：`phoenix-pnp camera-combined FILE` 读取 `head_x,head_y,col,row,px,py`（`examples/camera_combined.csv`：图案固定在台面上，头移动到多个位置），并结合图案的位置（`camera_pattern_origin_x_mm`、`camera_pattern_origin_y_mm`）和间距，一次拟合出像素大小、旋转、镜像、吸嘴偏移、镜头畸变以及图案在台面上的旋转角；相当于把 `camera-calibrate` 的相机和 `camera-pattern` 的镜头一并完成。默认关闭（`camera_combined = off`；两个独立命令不依赖它），头必须位于两个或更多位置，偏移的精度取决于原点的测量精度。工具标签页有同样的标定；应用到机器不会开启 `camera_correct_lens`。 开启 `camera_combined_touch = on`（或使用 `--touch`）后，纸板的位置不再取自设置，而是取自文件中的 `origin,X,Y` 行（喷嘴接触点 0,0 时的位置）；默认关闭。
* **工具标签页新增面板** (`src/gui/toolbox.rs`)：吸嘴头使用情况（`nozzle-life` 保存的记录，达到 `nozzle_life_warn` 的吸嘴头会被标出）、吸嘴上的元件与已加载封装尺寸的核对、基准点检查（每个标记的残差、不一致的标记，以及超过 `fiducial_max_residual_mm` 的标记警告）、供料器表检查、顺序与换头方式的比较（最快的方案能节省多少）以及方案的 SVG 图；每一项都以各自的名称把结果写入输出文件夹，且不会覆盖已有文件。标签页的准备清单在键入或读取吸嘴头记录后会读取它，并指出哪个吸嘴头已磨损。
* **准备清单**（`src/setup.rs`、`src/commands.rs`、`src/gui/toolbox.rs`）：`phoenix-pnp setup-sheet 零件 供料器 --boards N` 以八种语言把以下内容汇总到一页：按槽位顺序排列的待装载供料器（类型、零件、料带、吸嘴头、所含数量、位置）、N 块板缺少的零件、本批次的吸嘴头及其相对 `nozzle_life_warn` 的磨损（`--life` 读取 `simulate --nozzle-life` 的记录）、作业所依赖的校准及其是否已设置（相机、吸嘴偏心、轴间隙、吸嘴头更换器位置、拼板基准点）以及警告（已关闭但装有电路板所需零件的供料器、装有有方向零件的散料供料器）；`--out` 把供料器写成 CSV，缺零件时命令以 1 退出。工具标签页提供同一份清单，可作为 `setup_sheet.txt` 打印。
* **工具页**（`src/gui/toolbox.rs`）：轴间隙、吸嘴偏心、相机校准、供料器示教、有方向零件表、物料清单的核对与订购、批量库存，以及电路板网格拼板，各自使用与对应命令相同的库函数；文本可以键入或从文件读取，结果显示在屏幕上，会生成文件的工具有一个按钮，把文件写入输出文件夹且不会覆盖已有文件；相机和偏心可以应用到机器上。表单的初始值是 `machine.cfg` 的键（`order_boards`、`panel_cols`、`panel_rows`、`panel_pitch_x_mm`、`panel_pitch_y_mm`、`panel_fiducial_mm`、`panel_board_width_mm`、`panel_board_height_mm`）。

## 🧩 在 EH3D 中的角色

EH3D-PHOENIX-PNP 是 EH3D Fabrication Suite 的一个程序。目前它以库、命令行和窗口（用 `--features gui` 构建）的形式工作；窗口还没有套件其他程序共用的外观。它读写简单的文件，因此可以单独使用，也可以用在其他程序之间。

## 🏛️ 架构

<p align="center">
  <img src="images/architecture.svg" alt="EH3D-PHOENIX-PNP 的架构：谁在使用、包含什么、依赖什么" width="100%">
</p>


本仓库各部分如何配合：谁使用它、它包含什么、它依赖什么。图由 `docs/architecture.json` 绘制并与代码核对。

## ✨ 功能

* **库加命令行：** `phoenix-pnp` 的每个命令都是库中的函数，套件中的其他程序无需启动进程即可调用。
* **严格的命令行：** 写错的选项（`--powr`）或多余的词会产生指明它的错误，而不是被忽略；否则程序会用默认值生成输出。
* **损坏输入的测试：** 测试还会输入空文件、乱码、巨大数字和被截断的文件，任何情况都不允许 panic。
* **不猜测：** 没有意义的值是一个说明哪个值、为什么的错误，绝不是只是看起来正确的结果。
* **没有 unsafe 代码：** 库和程序中都有 `#![forbid(unsafe_code)]`。
* **两张表用同一个读取器，大板也很快：** 位置文件和供料器文件由同一个读取器读取（逗号或分号，带引号的字段可含分隔符或换行），五万个元件的检查、分配和规划瞬间完成，计划绝不会覆盖它所依据的文件，未知选项会报错，镜像底面时 `--origin-x` 指明板子左边缘的位置。

## 🔒 安全与防护

| 威胁 | 防御 |
|---|---|
| 损坏、被截断或为空的文件 | 不会 panic：测试在每个位置截断有效输入并输入乱码 |
| 没有意义的数字（负数、非有限值、过大的数） | 以指出该值的错误拒绝；不会被舍入成看似合理的值 |
| 不安全的内存使用 | 带 `forbid(unsafe_code)` 的纯安全 Rust |

**没有防御的内容：** 无错误产生的结果不等于经过验证的结果。这里没有任何内容了解机器的极限，程序也没有经过独立审查。请私下报告漏洞（见 [SECURITY.md](SECURITY.md)）。

## 🔌 接口

库为 `eh3d_phoenix_pnp`（`use eh3d_phoenix_pnp::...`），命令行为 `phoenix-pnp`。命令如下；每个命令把结果打印到标准输出，把错误以非零退出码报告到标准错误。没有网络访问。

```text
phoenix-pnp [gui [FILES...]]                             窗口（用 --features gui 构建）
phoenix-pnp parts PARTS                                  位置文件中的零件，单位为毫米
phoenix-pnp check PARTS FEEDERS                          没有供料器的零件和将用完的供料器
phoenix-pnp plan PARTS FEEDERS OUT.json                  简单计划，作为 JSON 作业
phoenix-pnp schedule PARTS FEEDERS OUT.json              使用全部配置、拼板和基准点的计划
phoenix-pnp preflight PARTS FEEDERS                      作业前的检查
phoenix-pnp simulate PARTS FEEDERS                       在模拟机器上运行作业，并给出报告
phoenix-pnp loadsheet PARTS SLOTS OUT.csv                上料表
phoenix-pnp config-check FILE                            配置文件是否有效
phoenix-pnp check-bottom PARTS                           底面各规则给出的角度
phoenix-pnp tables-check [--parts F] [--feeders F] ...   各表是否有效，并给出代码和行号
phoenix-pnp recipe-check FILE                            配方是否有效，并给出代码和行号
phoenix-pnp import-centroid FILE [--map F] [--out F]     把 CAD 的位置文件导入为零件表
phoenix-pnp bom-check PARTS BOM [--config F]              物料清单与零件表是否一致
phoenix-pnp camera-calibrate FILE [--config F]            根据对标记的多次观测得到相机设置及拟合程度
phoenix-pnp nozzle-calibrate FILE [--config F]            根据多个角度的读数求吸嘴头的偏心
phoenix-pnp panelize BOARD --cols N --rows N --pitch-x MM --pitch-y MM [--out F] 由同一电路板按网格复制而成的拼板文件
phoenix-pnp polarity-sheet PARTS [--side top|bottom] [--out F] 有方向的零件，以 CSV 形式用于作业后的检查
phoenix-pnp bom-order BOM [--boards N] [--spare-percent P] [--out F] N 块板所需采购的零件，含备用量
phoenix-pnp feeder-teach strip|tray [places and counts] | --measures FILE   根据在机器上测得的位置求料带或料盘的间距与角度
phoenix-pnp stock PARTS FEEDERS [--boards N] [--side top|bottom] [--out F] 供料器中的零件是否够做 N 块板，按种类核对（不够则以 1 退出）
phoenix-pnp part-inspect READINGS [--sizes F] [--packages F] [--out F] 将吸嘴上的元件与其封装尺寸对照（有被拒绝的则以 1 退出）
phoenix-pnp fiducial-check DESIGN MEASURED [--config F] 测得的标记使拼板产生的旋转、缩放、剪切和位移，与限值对照
phoenix-pnp feeder-check FEEDERS [--packages F] [--catalog F] 供料器表自身是否一致（槽位、间距、格子、冲突位置）
phoenix-pnp nozzle-life FILE [--warn N] [--reset TIP] 根据放在配置旁的文件显示每个吸嘴头已贴装的数量（有吸嘴头达到警告阈值则以 1 退出）
phoenix-pnp setup-sheet PARTS FEEDERS [--boards N] [--life F] [--out F] 哪个槽位装什么、N 块板缺少的零件、吸嘴头及其磨损、作业所依赖的校准，使用 --lang 的语言输出（缺零件则以 1 退出）
phoenix-pnp order-compare PARTS FEEDERS [OPTIONS] 用每种贴装顺序和每种更换吸嘴头的方式并排规划同一块板，并标出最快的
phoenix-pnp camera-pattern FILE [--pitch MM]             根据网格图案上的点得到像素大小和镜头畸变
phoenix-pnp queue-add QUEUE NAME SOURCE FEEDERS          把作业（板或拼板）加到作业队列末尾
phoenix-pnp queue-status QUEUE [--check]                 队列中的作业及其状态；--check 为尚未完成的作业做计划
phoenix-pnp queue-confirm QUEUE NAME                     记录某作业当前的计划可以运行
phoenix-pnp queue-edit QUEUE NAME skip|reset|revoke|remove   跳过、重新开始、撤销确认或移除作业
phoenix-pnp queue-run QUEUE [--only NAME]                在模拟机上逐块板完成已确认的作业
phoenix-pnp camera-combined FILE [--pitch MM] [--origin X,Y] [--touch]   用固定在台面上的图案并移动头，得到相机、图案的旋转和镜头畸变（需要 camera_combined = on）
phoenix-pnp vision-readings PARTS --sizes F [--seed N]   模拟相机对零件的读数，写成 part-inspect 读取的文件
phoenix-pnp vision-marks DESIGN [--shift X,Y] [--turn DEG]   按指定方式放置的拼板上的标记，经模拟相机读取后的测得值，供 fiducial-check 使用
phoenix-pnp package-gen [NAME...] [--from PARTS]          根据 0603、SOT-23、SOIC-8、QFN-32 等标准名称生成封装表的行（本体、高度、料带）
phoenix-pnp package-kicad PATH... [--height MM]            根据 KiCad 封装文件（.kicad_mod）或其文件夹生成封装表的行（本体、高度、料带）
phoenix-pnp place-learn READINGS --state F                根据读数（package,dx_mm,dy_mm）学习每种封装贴装的平均偏移，剔除离群值并设上限（需要 place_learn = on）
phoenix-pnp job-export PARTS FEEDERS OUT [--state F]       把计划和每个零件的状态写入供其他程序使用的 JSON 文件
phoenix-pnp job-import FILE [--csv F]                      读取并检查这样的文件，说明其内容，并可把贴装连同状态写成表格
phoenix-pnp parts-diff FIRST SECOND [--tol MM] [--rot-tol DEG] [--out F] 两个零件表之间的变化，逐个零件：新增、删除、移动、旋转、值、封装、面（有差异则以 1 退出）
phoenix-pnp vision-test [--turn DEG] [--dx MM] ...       测量在内存中生成的零件图像
```

`phoenix-pnp help` 会列出所有选项。`schedule`、`preflight`、`simulate` 和 `loadsheet` 共用的选项有 `--config`, `--packages`, `--catalog`, `--panel`, `--fiducials`, `--x-out`, `--dnp`, `--side`, `--board-width`, `--origin-x`, `--nozzles`, `--speed`, `--order` 和 `--lang`；`--shot` 及其配套选项用于拍摄窗口的图片（见快速开始）。

## ⚙️ 配置

机器和规划器通过由 `键 = 值` 行组成的文本文件配置（`--config 文件`），格式与套件中其他程序相同；`examples/machine.cfg` 列出每个键的默认值及作用。缺少的键保持默认值；未知的键、重复书写的键或超出范围的键都会产生指出行号的错误。命令行选项（`--nozzles`、`--speed`、`--order`、`--side`）优先于文件。

标记零件不贴装的列及其用词也是设置：`header_dnp` 和 `header_fitted` 增加列名，`dnp_yes_words` 和 `dnp_no_words` 增加表示“是”和“否”的词。名为 `dnp`、`do not place`、`do not populate`、`do not mount`、`do_not_place`、`not fitted` 或 `no montar` 的列，遇到“是”（`yes`、`y`、`true`、`1`、`x`）就排除该零件；名为 `fitted`、`populate`、`populated`、`mount`、`mounted` 或 `montar` 的列，遇到“否”（`no`、`n`、`false`、`0`、`-`）就排除该零件；空单元格表示贴装，其他任何词都是指出行号的错误。

哪些零件是所在电路板的标记也是设置：`fiducial_words`（默认 `fiducial`）和 `fiducial_refs`（默认 `fid, fd`）；列表为空则关闭该规则。

封装表和零件表（`--packages`、`--catalog`）是 CSV 文件，空单元格会继承；拼板（`--panel`）是 JSON 文件；机器的槽位（`loadsheet`）是带 `id,x,y,max_width_mm` 的 CSV。超过 `max_file_bytes` 的文件会被拒绝。每个错误都有代码 `PHOENIX-PNP-NNN`，其消息使用键 `language` 或 `--lang` 指定的语言（`en`、`es`、`de`、`fr`、`it`、`ja`、`zh` 或 `pt`）：

| 代码 | 含义 |
|---|---|
| `PHOENIX-PNP-001` | 文件超过允许的大小 |
| `PHOENIX-PNP-002` | 零件数量超过上限 |
| `PHOENIX-PNP-003` | 配置中有一行不是“键 = 值” |
| `PHOENIX-PNP-004` | 配置中有未知的键 |
| `PHOENIX-PNP-005` | 配置中的某个值无效 |
| `PHOENIX-PNP-006` | 配置中的某个值超出范围 |
| `PHOENIX-PNP-007` | 该配置是为更新版本的程序编写的 |
| `PHOENIX-PNP-008` | 配置中有键重复 |
| `PHOENIX-PNP-009` | 某项设置超出安全限制 |
| `PHOENIX-PNP-010` | 零件或封装表中有无效的行 |
| `PHOENIX-PNP-011` | 零件或封装被重复定义 |
| `PHOENIX-PNP-012` | 旋转偏移规则无效 |
| `PHOENIX-PNP-013` | 供料器表中有无效的行 |
| `PHOENIX-PNP-014` | 拼板文件无效 |
| `PHOENIX-PNP-015` | 拼板嵌套过深或电路板过多 |
| `PHOENIX-PNP-016` | 槽位表无效 |
| `PHOENIX-PNP-017` | 计划的某项设置无效 |
| `PHOENIX-PNP-018` | 吸嘴无法在其限位内转到该角度 |
| `PHOENIX-PNP-019` | 吸嘴已吸着零件，不能再取另一个 |
| `PHOENIX-PNP-020` | 吸嘴上没有可贴装的零件 |
| `PHOENIX-PNP-021` | 该零件没有可用的供料器 |
| `PHOENIX-PNP-022` | 多次尝试后仍无法拾取该零件 |
| `PHOENIX-PNP-023` | 多次尝试后供料器仍无法前进 |
| `PHOENIX-PNP-024` | 多次尝试后仍无法贴装该零件 |
| `PHOENIX-PNP-025` | 保存的状态属于另一个作业 |
| `PHOENIX-PNP-026` | 保存的作业状态无效 |
| `PHOENIX-PNP-027` | 作业已暂停、已完成或因错误而停止 |
| `PHOENIX-PNP-028` | 作业前检查发现了错误 |
| `PHOENIX-PNP-029` | 长度或角度无效 |
| `PHOENIX-PNP-030` | 驱动程序未接受该命令 |
| `PHOENIX-PNP-031` | 模拟器注入了故障 |
| `PHOENIX-PNP-032` | 基准点无效，或无法确定位置 |
| `PHOENIX-PNP-033` | 装配标记既不是“是”也不是“否”，或要排除的位号未知 |
| `PHOENIX-PNP-034` | 位置文件无效 |
| `PHOENIX-PNP-035` | 相机发现零件的偏移超过了容差 |
| `PHOENIX-PNP-036` | 相机没有给出零件的测量值 |
| `PHOENIX-PNP-037` | 在图像中没有找到零件 |
| `PHOENIX-PNP-038` | 图像中找到的斑块面积超出允许范围 |
| `PHOENIX-PNP-039` | 找到了不止一个可能是零件的斑块 |
| `PHOENIX-PNP-040` | 报告文件已存在，不会被覆盖 |
| `PHOENIX-PNP-041` | 该操作不适用于作业的当前状态 |
| `PHOENIX-PNP-042` | 标记为不良的板（X-out）不在拼板中 |
| `PHOENIX-PNP-043` | 拼板中的所有板都被标记为不良（X-out）：没有可贴装的内容 |
| `PHOENIX-PNP-044` | 拾取后真空显示吸嘴上没有零件 |
| `PHOENIX-PNP-045` | 放置后真空显示零件仍留在吸嘴上 |
| `PHOENIX-PNP-046` | 机器没有该名称的执行器 |
| `PHOENIX-PNP-047` | 供料器最近的拾取中故障过多：由操作人员决定 |
| `PHOENIX-PNP-048` | 机器的任何一个贴装头都够不到该零件 |
| `PHOENIX-PNP-049` | 配方中有一行无效 |
| `PHOENIX-PNP-050` | 配方的步骤不符合作业的顺序 |
| `PHOENIX-PNP-051` | 配方超出允许的限制 |
| `PHOENIX-PNP-052` | 该配方是为更新版本的程序编写的 |
| `PHOENIX-PNP-053` | 位置文件中有一行无效 |
| `PHOENIX-PNP-054` | 列映射或其所应用的表头没有给出所需的列 |
| `PHOENIX-PNP-055` | 无法识别位置文件的格式 |
| `PHOENIX-PNP-056` | 位号范围无效 |
| `PHOENIX-PNP-057` | 物料清单中的某一行无效 |
| `PHOENIX-PNP-058` | 校准点无效或无法得出结果 |
| `PHOENIX-PNP-059` | 吸嘴头使用次数文件无效 |
| `PHOENIX-PNP-060` | 作业队列无效，或已被截断或手工修改 |

## 🚀 快速开始

需要稳定版 Rust 工具链（1.95 或更新）。

```bash
cargo build --release
cargo test
cargo run --release -- plan examples/board.pos examples/feeders.csv examples/plan.json --nozzles 2
cargo run --release -- schedule examples/board.pos examples/feeders.csv examples/schedule.json --config examples/machine.cfg --packages examples/packages.csv --catalog examples/catalog.csv
cargo run --release -- simulate examples/board.pos examples/feeders.csv --config examples/machine.cfg --state examples/job_state.json --steps 5
cargo run --release -- loadsheet examples/board.pos examples/slots.csv examples/loadsheet.csv --packages examples/packages.csv
cargo run --release -- check-bottom examples/board.pos
cargo run --release -- schedule examples/board_dnp.csv examples/feeders.csv examples/schedule_dnp.json --config examples/machine.cfg --packages examples/packages.csv --catalog examples/catalog.csv
cargo run --release -- tables-check --parts examples/board_dnp.csv --packages examples/packages_common.csv --feeders examples/feeders_kinds.csv --slots examples/slots.csv --config examples/machine.cfg
cargo run --release -- simulate examples/board.pos examples/feeders.csv --config examples/machine.cfg --report-csv examples/job_report.csv
cargo run --release -- simulate examples/board.pos examples/feeders.csv --config examples/machine.cfg --report-text examples/job_report.txt --report-html examples/job_report.html
cargo run --release -- schedule examples/feeders.csv examples/schedule_panel.json --panel examples/panel.json --fiducials examples/marks.csv --config examples/machine.cfg
cargo run --release -- schedule examples/feeders.csv examples/schedule_xout.json --panel examples/panel.json --x-out B --config examples/machine.cfg
cargo run --release -- vision-test --vision examples/vision.cfg --turn 30 --dx 0.5 --dy -0.3
cargo run --release -- import-centroid examples/centroid_custom.csv --map examples/centroid.map --out examples/parts_from_centroid.csv
cargo run --release -- import-centroid examples/board.mnt --dnp R1-R2
cargo run --release --features gui -- --shot screen.png examples/board.pos examples/feeders.csv examples/machine.cfg   # a picture of the window
cargo run --release --features gui -- --shot job.png --window job --steps 9 examples/board.pos examples/feeders.csv examples/machine.cfg   # the Job tab with the job on its way
cargo run --release --features gui
```

要在不手动打开窗口的情况下截取窗口图像，请使用 `--shot FILE.png` 运行（还可用 `--size WxH`, `--theme phoenix|light|contrast`, `--wait N`, `--window TAB`, `--steps N`, `--lang CODE`, `--open FILE`, `--replace`）：它按该尺寸打开，保存 PNG，不保留任何设置并自动关闭（写入文件成功时退出码为 0，否则为 1，参数错误时为 2）。`--window` 选择截图的标签页（`job`, `parts`, `packages`, `feeders`, `panels`, `boards`, `loadsheet`, `machine`, `problems` 或 `log`），`--steps N` 在截取前让模拟作业走 N 步，因此“作业”标签页会显示进行中的作业。

<p align="center">
  <img src="images/screens/job_zh.png" alt="显示模拟作业进行中的“作业”标签页" width="100%">
</p>

最后一行会打开窗口；用 `--features gui` 构建时，不带命令运行 `phoenix-pnp` 也会打开（在 Linux 上，窗口需要 X11 或 Wayland 以及 OpenGL 的开发文件）。带上命令时，程序仍是命令行。在 Windows 上，`phoenix-pnp-gui`（用同一特性构建）会在没有控制台的情况下打开窗口；见 `docs/windows-console.md`。

## 🧪 测试

测试套件共有 963 个测试（`cargo test`，在 CI 中于 Windows 和 Linux 上运行）。它们检查正常情况、边界情况和损坏的输入：空文件、乱码和被截断的文件绝不会引发 panic。由固定种子生成的小型用例（每次运行得到相同的数字）检查：计划不依赖文件顺序，路径绝不会变长，在任意位置保存并恢复的作业与从未停止的作业结果相同，长度和角度可来回换算。CI 还会运行 `cargo fmt --check`、把警告当作错误的 `cargo clippy` 以及 `python -I tools/ci_validate.py`。

## 🏗️ 构建与发布

版本号位于 `Cargo.toml` 和 `eh3d.project.json`；每个版本在 [CHANGELOG.md](CHANGELOG.md) 中都有条目。`build.bat` / `build.sh` 构建程序，`test.bat` / `test.sh` 运行测试，`run.bat` / `run.sh` 用示例运行。版本发布在 GitHub 上。

## 📂 仓库结构

<p align="center">
  <img src="images/repository-structure.svg" alt="EH3D-PHOENIX-PNP 的仓库结构" width="100%">
</p>

<details>
<summary>目录树的文本版</summary>

```text
EH3D-PHOENIX-PNP/
├── src/
│   ├── bom.rs                                   物料清单：读取、与电路板核对、订购
│   ├── calibrate.rs                             相机和吸嘴头的校准，以及供料器的示教
│   ├── camera.rs                                相机模型：像素尺寸、偏移、旋转、镜像和镜头
│   ├── catalog.rs                               零件表和封装表及其属性的继承
│   ├── cli_args.rs                              命令行参数，严格：未知选项即错误
│   ├── cmd_more.rs                              命令 parts-diff、config-diff、board-stats、queue-stock 和 feeder-deplete
│   ├── cmd_queue.rs                             作业队列的命令：添加、状态、确认、编辑、运行
│   ├── cmd_tools.rs                             检查、校准和生成文件的命令：物料清单、校准、拼板
│   ├── cmd_vis.rs                               命令 part-inspect、fiducial-check 和 feeder-check
│   ├── combined.rs                              固定图案加头移动的相机标定
│   ├── commands.rs                              使用配置的命令、tables-check 和 check-bottom
│   ├── compare.rs                               用每种顺序和吸嘴头策略并排规划同一块板
│   ├── config.rs                                机器的配置文件：键、范围和安全限制
│   ├── error.rs                                 错误代码 PHOENIX-PNP-NNN 及其八种语言的含义
│   ├── exchange.rs                              供其他程序使用的作业 JSON 文件：计划和每个零件的状态
│   ├── export.rs                                报告、缺料清单和移动的 G 代码
│   ├── feeders.rs                               逻辑的一部分
│   ├── feeding.rs                               带优先级、托盘和料带的供料器，以及元件供料器的选择
│   ├── feedlib.rs                               供料器库是否自洽的检查
│   ├── feedrep.rs                               作业中每个供料器的吸取情况
│   ├── fidcheck.rs                              基准点拟合的判断：旋转、缩放、剪切、位移、不一致的标记
│   ├── fiducial.rs                              拼板的基准点：测量基准点的拟合与零件的校正
│   ├── gui.rs                                   窗口：无需屏幕的模块，以及 window.rs 中的绘制
│   ├── gui/app.rs                               窗口的状态：载入、规划、开始、暂停、单步、修复、保存
│   ├── gui/browse.rs                            文件选择器：文件夹和文件、上一级、输入的路径
│   ├── gui/jobtab.rs                            不依赖窗口的作业标签页：状态、筛选、进度、错误对话框
│   ├── gui/log.rs                               带级别的日志，以及可复制的诊断文本
│   ├── gui/problems.rs                          作业的问题及解决它们的修复
│   ├── gui/queuetab.rs                          不含窗口的队列工具：作业列表、表单和按钮
│   ├── gui/samples.rs                           示例，从小型电路板到大型电路板
│   ├── gui/session.rs                           可开始、暂停、继续和单步的模拟作业
│   ├── gui/settings.rs                          窗口记住的内容：语言、主题、图层、文件夹和列宽
│   ├── gui/shot.rs                              窗口的截图模式（--shot）：其参数
│   ├── gui/tables.rs                            表格的行、排序和共享的选择
│   ├── gui/texts.rs                             窗口的每个词（八种语言）
│   ├── gui/theme.rs                             配色：凤凰绿、浅色和高对比度
│   ├── gui/toolbox.rs                           不含窗口的工具页：工具、表单及其写出的内容
│   ├── gui/view.rs                              地图的几何：元件外形、极性标记、适应、缩放和点选
│   ├── gui/window.rs                            用 egui 绘制（用 gui 特性构建）：页面、地图和表格
│   ├── gui/wizard.rs                            从零到模拟作业的五步向导
│   ├── gui/workspace.rs                         已载入的文件、规划和编辑，无需窗口
│   ├── gui_main.rs                              作为独立程序的窗口，在 Windows 上没有控制台
│   ├── heads.rs                                 多贴装头：可达范围、零件分配、等待供料器
│   ├── hooks.rs                                 针对作业事件运行的程序，默认关闭，清单和时间均有限制
│   ├── import.rs                                位置文件的导入：带列映射的质心 CSV、EAGLE 文本、KiCad 的 .pos
│   ├── inspect.rs                               对吸嘴上元件的检查：尺寸、旋转和位置
│   ├── job.rs                                   作业：步骤、暂停、重试、策略、统计和已保存状态
│   ├── journal.rs                               模拟作业的日志：每一步一行，并记录结果
│   ├── kicadmod.rs                              从 KiCad 封装文件读取的封装本体尺寸
│   ├── learn.rs                                 根据读数学习的每种封装的贴装偏移
│   ├── lib.rs                                   库：每个命令所使用的部分
│   ├── loadsheet.rs                             上料表：每种料带的槽位
│   ├── main.rs                                  命令行，以及不带命令运行时窗口的入口
│   ├── marks.rs                                 两级标记：作为标记的零件，以及拼板和每块电路板的拟合
│   ├── model.rs                                 以 trait 表示的机器部件：机器、吸嘴、供料器、执行器、相机、驱动
│   ├── motion.rs                                包含加速度和加加速度的移动时间
│   ├── nozzlelife.rs                            吸嘴头的使用情况：贴装数、安装次数、更换次数和警告阈值
│   ├── panel.rs                                 拼板：拼板中的电路板和拼板，各在其位
│   ├── parts.rs                                 位置文件和不贴装的零件
│   ├── partsdiff.rs                             两个零件表之间的变化，逐个零件
│   ├── pattern.rs                               用图案标定相机：像素大小、镜头畸变、残差
│   ├── pkggen.rs                                根据名称得到常用封装的尺寸
│   ├── plan.rs                                  逻辑的一部分
│   ├── planner.rs                               了解机器的规划器：等级、顺序、吸嘴头及其搜索、角度和时间
│   ├── polarity.rs                              有方向零件的清单
│   ├── preflight.rs                             作业前的检查，含严重程度及其所指对象
│   ├── preview.rs                               计划图：供料器、位置和贴装头路径
│   ├── queue.rs                                 作业队列：带校验和的文件、状态、确认关卡
│   ├── recipe.rs                                作业配方：其文件、步骤以及按封装、供料器和吸嘴的参数
│   ├── report.rs                                作业报告：阶段、拾取、分组、错误（文本、HTML、CSV、JSON）
│   ├── rotation.rs                              按封装的旋转修正，以及受限吸嘴如何转动
│   ├── setup.rs                                 准备清单：按槽位的供料器、缺件、吸嘴头、校准
│   ├── sim.rs                                   模拟机器：虚拟时钟和可注入的故障
│   ├── stock.rs                                 供料器中的零件与一批板所需零件的对比
│   ├── tips.rs                                  封装可接受的吸嘴，以及为每类零件选择其一
│   ├── units.rs                                 带单位的长度和位置
│   ├── vision.rs                                视觉处理：滤波、阈值、斑块、最小旋转矩形
│   └── visionsim.rs                             供 part-inspect 和 fiducial-check 使用的模拟相机读数
├── tests/
│   ├── align.rs                                 对位步骤的测试：相机、修正后的位置、容差和时间
│   ├── approach.rs                              approach 下计划时间的测试：第二次移动与作业秒数
│   ├── backlash.rs                              模拟机器轴间隙的测试
│   ├── bom.rs                                   物料清单测试：读取、核对、代码、bom-check
│   ├── bom_order.rs                             采购清单的测试
│   ├── calibrate.rs                             校准测试：拟合、文件、配置行、G 代码
│   ├── camera.rs                                相机模型、各贴装头的相机和模拟器相机的测试
│   ├── combined.rs                              联合标定的测试：文件、拟合、模式、命令、工具
│   ├── commands.rs                              使用配置的命令的测试
│   ├── compare.rs                               顺序比较的测试：最快者、文本、建议和命令
│   ├── encoding.rs                              文件编码的测试：UTF-8、UTF-16 和 Latin-1
│   ├── exchange.rs                              作业交换文件测试：写入、读取、各种拒绝、命令
│   ├── feeding.rs                               供料器测试：优先级、托盘和料带
│   ├── feedlib.rs                               供料器库检查的测试
│   ├── feedrep.rs                               每个供料器吸取情况的测试
│   ├── fidcheck.rs                              基准点拟合检查的测试
│   ├── gui.rs                                   无屏幕的窗口测试：状态、规划、作业、地图、表格和向导
│   ├── heads.rs                                 贴装头测试：配置行、可达范围、分配、时钟和 schedule
│   ├── hooks.rs                                 钩子测试：默认关闭、规则、事件、调用、运行、simulate --hooks
│   ├── import.rs                                导入的测试：格式、单位、底面、DNP、范围、带行号的错误
│   ├── inspect.rs                               吸嘴上元件检查的测试
│   ├── job.rs                                   作业测试：步骤、重试和已保存状态
│   ├── jobreport.rs                             作业报告的测试：按供料器统计的损失、吸嘴头寿命、文件
│   ├── jobtab.rs                                无屏幕的作业标签页测试：状态、筛选、进度、对话框
│   ├── journal.rs                               作业日志的测试：每步一行、使其停止的错误、命令
│   ├── kicadmod.rs                              KiCad 封装测试：本体、占位区、损坏文件、行、命令
│   ├── learn.rs                                 学习偏移测试：读数、离群值、上限、文件、作业、命令
│   ├── loadsheet.rs                             上料表和导出的测试
│   ├── looks.rs                                 相机额外查看以及报告中按供料器统计查看的测试
│   ├── marks.rs                                 标记为不良的电路板、作为标记的零件和两级标记的测试
│   ├── model.rs                                 数据模型测试：代码、长度、配置和表格
│   ├── more.rs                                  库和命令行的更多测试
│   ├── names.rs                                 文件名测试：没有仅大小写不同的名称，也没有 Windows 无法使用的名称
│   ├── nozzlelife.rs                            吸嘴头使用情况的测试：文件、作业新增的内容和命令
│   ├── panel.rs                                 拼板及其基准点的测试
│   ├── panelize.rs                              网格拼板与 panelize 的测试
│   ├── partsdiff.rs                             两个零件表差异的测试：各种变化、容差、命令
│   ├── pattern.rs                               图案标定的测试：文件、拟合、限值、命令和工具
│   ├── pkggen.rs                                封装尺寸的测试：各类、错误、表、命令
│   ├── planner.rs                               移动时间和规划器的测试
│   ├── pnp.rs                                   库和命令行的测试
│   ├── polarity.rs                              有方向零件清单的测试
│   ├── preflight.rs                             作业前检查的测试
│   ├── preview.rs                               计划图的测试：内容、转义、尺寸和 --svg
│   ├── queue.rs                                 作业队列的测试：文件、校验和、关卡、命令和工具
│   ├── readme.rs                                验证 README 可运行且完整的测试
│   ├── recipe.rs                                配方的测试：文件、限制、步骤、作业中的规则和已保存的状态
│   ├── report.rs                                报告测试：阶段、拾取、直方图、分组、决定、格式
│   ├── samples.rs                               校准示例与供料器测量文件的测试
│   ├── setup.rs                                 准备清单的测试：列表、语言、命令和工具
│   ├── shot.rs                                  窗口图片模式的测试：在没有屏幕的情况下读取并检查其参数
│   ├── sim.rs                                   模拟机器的测试
│   ├── stock.rs                                 批量库存与待购零件的测试
│   ├── strict.rs                                命令行测试：它不认识的选项和词
│   ├── tables.rs                                不贴装零件、tables-check 以及作业 CSV 报告的测试
│   ├── teach.rs                                 料带与料盘示教的测试
│   ├── tips.rs                                  封装可接受吸嘴的测试：规则、选择和计划
│   ├── toolbox.rs                               工具页的测试：其设置、各工具对示例的处理、所应用与写出的内容
│   ├── vacuum.rs                                真空、执行器和供料器故障窗口的测试
│   ├── vis_cli.rs                               命令 part-inspect、fiducial-check、feeder-check 和 --report-feeders 的测试
│   ├── vision.rs                                视觉处理测试：手算图像、失败和设置
│   ├── vision_job.rs                            作业与 vision-test 的视觉测试：读数、失败、已保存状态
│   └── visionsim.rs                             变差的相机及其给两项检查的读数的测试
├── examples/
│   ├── board.mnt                                EAGLE 文本格式的电路板位置
│   ├── board.pos                                试用的示例
│   ├── board_dnp.csv                            带 dnp 列的电路板：其中两个零件不贴装
│   ├── bom.csv                                  与 board.pos 一致的物料清单
│   ├── camera_combined.csv                      camera-combined 用的、从头的四个位置看到的固定图案观测
│   ├── camera_points.csv                        camera-calibrate 用的标记观测点
│   ├── camera_pattern.csv                       camera-pattern 用的网格图案上的点
│   ├── catalog.csv                              零件表：等级和高度
│   ├── centroid.csv                             带有不贴装零件列和含逗号取值的质心 CSV
│   ├── centroid.map                             centroid_custom.csv 的列映射
│   ├── centroid_custom.csv                      使用其他列名、表头带密耳单位、并用词语表示面的质心 CSV
│   ├── chip_0603.kicad_mod                      用于 package-kicad 的 0603 贴片 KiCad 封装
│   ├── feeder_measures.csv                      feeder-teach 用的料带与料盘测量位置
│   ├── feeders.csv                              试用的示例
│   ├── feeders_kinds.csv                        各种供料器：料带、料条、托盘和一个已关闭的
│   ├── feeders_tubes.csv                        除料带外还含料管和散料供料器的供料器表
│   ├── hooks.csv                                simulate --hooks 的钩子规则：每条含一个事件和一个程序
│   ├── fid_design.csv                           四个基准点的设计位置
│   ├── fid_measured.csv                         在机器上测得的同样四个基准点
│   ├── job.recipe                               含全部步骤以及按封装、供料器和吸嘴规则的配方
│   ├── job_basic.recipe                         最短的配方：准备、拾取和贴装
│   ├── machine.cfg                              机器设置，每个键均有说明
│   ├── marks.csv                                拼板基准点的实测位置
│   ├── nozzle_life.csv                          两个吸嘴头的使用情况，其中一个超过警告阈值
│   ├── nozzle_readings.csv                      nozzle-calibrate 用的多个角度下吸嘴尖端读数
│   ├── nozzle_reads.csv                         供 part-inspect 使用的吸嘴上元件读数
│   ├── package_sizes.csv                        各封装本体的尺寸
│   ├── packages.csv                             封装表：高度、吸嘴头、等级和料带
│   ├── packages_bodies.csv                      含各封装本体尺寸的封装表，供 part-inspect 使用
│   ├── packages_common.csv                      常见封装表（从 0201 电阻到 QFN），请按自己的零件调整
│   ├── panel.json                               由两块板组成的拼板
│   ├── slots.csv                                机器的槽位
│   └── vision.cfg                               视觉处理设置，每个键均有说明
├── docs/
│   ├── architecture.json                        架构图数据
│   ├── structure.json                           结构图数据
│   └── windows-console.md                       在 Windows 上窗口为何是独立程序
├── tools/                                       CI 和 README 检查及绘图工具
├── images/                                      横幅图、架构与结构图片以及窗口截图
├── .github/                                     CI 与发布工作流、Issue 和 PR 模板
├── eh3d.project.json                            清单：版本、角色、成熟度和构建命令
├── Cargo.toml Cargo.lock                        工作区及其锁定的依赖
├── rust-toolchain.toml                          固定的 Rust 工具链
├── CHANGELOG.md                                 每个版本的变更
├── SECURITY.md SUPPORT.md                       如何报告漏洞及在哪里提问
└── CONTRIBUTING.md CODE_OF_CONDUCT.md LICENSE   参与贡献、行为准则和许可证
```

</details>

## 🗺️ 路线图与已知限制

**目前已知的限制**

* 没有人在屏幕上检查过窗口的绘制（`images/screens/` 中的图片由程序用 `--shot` 自己拍摄）；窗口没有点动和真实相机，也还不使用属于电路板零件的标记。
* 没有在机器上试过，也没有用真实文件集合测试过，也没有经过独立审查；它对机器所做的一切都是模拟。
* 它不读取其他贴片程序的文件；CSV 底面角度的规则（`bottom_rotation`）没有与任何 CAD 程序的真实文件比对过。
* 相机和视觉是模拟的：没有真实相机、没有基于真实图像的取料修正，也没有真实机器的驱动；计划是一个 JSON 文件，供其他程序执行。

**下一步**

* 让窗口达到套件其他程序共用的外观，并让它使用属于电路板零件的标记、显示被标记为不良的电路板。
* 用真实相机的图像试用视觉流程，用真实拼板试用两级标记；只有在真实机器上试过之后才把计划发送给机器。
* 用真实文件和套件中的其他程序试用。

## 🌍 语言

README 和文本提供英语、西班牙语、法语、意大利语、德语、汉语、日语和欧洲葡萄牙语版本。

## 🔗 相关项目

* [EH3D-VISION-STUDIO](https://github.com/JuanenRac/EH3D-VISION-STUDIO): 摄像头、基准点和检测
* [EH3D-MOTIONCNC](https://github.com/JuanenRac/EH3D-MOTIONCNC): 机器控制程序
* [EH3D-GERBERVIEWER](https://github.com/JuanenRac/EH3D-GERBERVIEWER): Gerber 层查看器
* [EH3D-TOOL-MANAGER](https://github.com/JuanenRac/EH3D-TOOL-MANAGER): CAM 程序共用的刀具库
* [EH3D-SUITE-LAUNCHER](https://github.com/JuanenRac/EH3D-SUITE-LAUNCHER): 查找并启动套件程序的中心

## 📚 文档

* [更新日志](CHANGELOG.md)
* [架构与结构数据](docs/architecture.json)：绘制两张图所用的数据。
* [为什么在 Windows 上窗口是独立程序](docs/windows-console.md)

## 🤝 参与贡献与社区

* 提交拉取请求前请运行 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test` 和 `python -I tools/ci_validate.py`：必须全部通过。见 [CONTRIBUTING.md](CONTRIBUTING.md) 和[行为准则](CODE_OF_CONDUCT.md)。
* README 有八种语言：修改文字要在八个文件中同时进行，两张图由 `tools/make_architecture.py` 和 `tools/make_structure.py` 根据 `docs/architecture.json` 和 `docs/structure.json` 绘制。
* 请私下报告漏洞：[SECURITY.md](SECURITY.md)。问题、想法和报告：electrohobby3d@gmail.com（[SUPPORT.md](SUPPORT.md)）。

## 👤 作者

**JuanenRac (Electro Hobby 3D)** · electrohobby3d@gmail.com

## 📜 许可证

软件：GPL-3.0-or-later。见 [LICENSE](LICENSE)。套件的硬件设计使用 CERN OHL-S。
