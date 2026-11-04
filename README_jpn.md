<p align="center">
  <img src="images/EH3D_PHOENIX_PNP_BANNER.svg" alt="EH3D-PHOENIX-PNP banner" width="100%">
</p>

# 📖 EH3D-PHOENIX-PNP

<p align="center"><a href="README.md">🇺🇸 English</a> | <a href="README_spa.md">🇪🇸 Español</a> | <a href="README_fra.md">🇫🇷 Français</a> | <a href="README_ita.md">🇮🇹 Italiano</a> | <a href="README_deu.md">🇩🇪 Deutsch</a> | <a href="README_zho.md">🇨🇳 中文</a> | 🇯🇵 <b>日本語</b> | <a href="README_por.md">🇵🇹 Português</a></p>

### ピック＆プレース実装：位置、フィーダー、実装順序

<p align="center">
  <img src="https://img.shields.io/badge/License-GPL%203.0--or--later-blue.svg" alt="GPL 3.0 or later">
  <img src="https://img.shields.io/badge/Stack-Rust-DEA584.svg" alt="Rust">
  <img src="https://img.shields.io/badge/Languages-8-00E5FF.svg" alt="Languages">
  <img src="https://img.shields.io/badge/Version-0.0.4-0f6cbd.svg" alt="Version">
  <img src="https://img.shields.io/badge/Maturity-scaffolding-FFB020.svg" alt="Maturity">
</p>

---

**正直さの確認 - 現在実際に動くもの：** Rust のライブラリとコマンドラインプログラム（`phoenix-pnp`）で、コンピューター上で動く 198 個のテストがあります。基板の位置ファイルを読み、各部品にフィーダーを割り当て（優先度、トレイ、ストリップに対応）、機械の設定をファイルから読み、最大6本のノズルを持つヘッドの配置順を計画し（ランク、ノズルチップ、角度の制限、移動時間、パネル）、搭載シートを書き出し、模擬機械で故障を注入しながら計画を一手ずつ実行します。機械は制御しません。ウィンドウはまだなく、実機では試しておらず、実際のファイル群での確認もしていません。

---

> **安全とセキュリティの状況：** このプログラムは機械を制御せず、安全システムでもありません。出力は、機械で何かを実行する前に人が確認する必要があります。ここにあるものは実機で動かしたことがありません。


## 📋 ひと目で

| | |
|---|---|
| バージョン | `0.0.4` |
| 成熟度 | 足場 |
| テスト | 198 |
| 技術スタック | Rust |
| 実機での動作 | 未試験：ここにあるものは機械で動かしたことがありません |

**移動先：** [概要](#-概要) · [アーキテクチャ](#-アーキテクチャ) · [クイックスタート](#-クイックスタート) · [テスト](#-テスト) · [構成](#-リポジトリの構成) · [関連](#-関連プロジェクト)

## 🚦 プロジェクトの状況

各行はこのリポジトリの一部分で、信号は今日どこまで進んでいるかを示します。計画ではなく、リポジトリが示せるもの（テスト、ビルド、実行）に基づいて書かれています。

🟢 完了・検証済み · 🟡 動作する（既知の制限あり） · 🟠 一部のみ、または未検証 · 🔴 未実装または動作しない

| 領域 | 状態 | ここでの意味 |
|---|---|---|
| コア | 🟡 | 基板の位置ファイルを読み、各部品にフィーダーを割り当て（優先度、トレイ、ストリップ）、最大6本のノズルを持つヘッドの配置順をランク、ノズルチップ、角度の制限、移動時間とともに計画し、パネルを計画し、搭載シートを書き出します。通常の入力と壊れた入力でコンピューター上でテスト済みですが、実際のファイル群ではありません。 |
| コマンドライン | 🟡 | `phoenix-pnp` はライブラリと同じ関数（`parts`、`check`、`plan`、`schedule`、`preflight`、`simulate`、`loadsheet`、`config-check`）を実行して結果を表示します。エラーはすべて `PHOENIX-PNP-NNN` のコード付きメッセージと0以外の終了コードで返ります。 |
| テスト | 🟡 | 198 個のテストが通ります（`cargo test`）。通常のケース、境界のケース、壊れた入力、小さな生成ケースを含みます。 |
| 機械の設定 | 🟡 | バージョン付きの `キー = 値` ファイル、行を示す検証、安全限界（Z の行程、X と Y の限界、再試行、ノズルの角度）。値は実機とは比較していません。 |
| シミュレーターとジョブ | 🟡 | 仮想時計、確率（シード付き）または指定した場所で注入する故障、再試行、警告と保留の方針、故障が続くと止まるフィーダー、一時停止、一手ずつの実行、再開用に保存する状態。模擬のみで、実機は動かしていません。 |
| セキュリティ | 🟡 | 安全な Rust のみ（`forbid(unsafe_code)`）。受け取ったファイルでパニックせず、不正な値は理由付きのエラーになります。独立したレビューはありません。 |
| ウィンドウ | 🔴 | 予定：スイート共通の egui ウィンドウ。現在はコマンドラインのみです。 |
| 実機 | 🔴 | 未試験：ここにあるものは機械で実行したことがありません。 |
| ドキュメント | 🟡 | 8 言語の README、変更履歴、`docs/` から描いた 2 枚の図。 |

## 🎯 概要

* **部品**（`src/parts.rs`）：KiCad の `.pos` ファイルまたは CSV（カンマまたはセミコロン、引用符、名前で見つけるヘッダー）。位置は mm、mil、インチ、角度は 0〜360、面は上か下。意味のない行は行番号付きで拒否され、同じ参照の重複はエラーです。
* **フィーダー**（`src/feeders.rs`）：フィーダーの CSV（ID、値、パッケージ、ピック位置、残数）。各部品には、大文字小文字を区別せず値とパッケージが一致する最初のフィーダーが割り当てられます。フィーダーのない部品と枯渇するフィーダーは報告され、同種の 2 つ目のフィーダーが引き継ぎます。
* **計画**（`src/plan.rs`）：実装位置を近い順に巡り、1〜6 ノズルのバッチに分け、バッチ内のフィーダーも近い順に訪れ、下面は基板幅で鏡像にし、移動距離と所要時間の見積りを出し、JSON のジョブとして書き出します。
* **表と設定**（`src/catalog.rs`、`src/config.rs`、`src/rotation.rs`）：値が継承される部品とパッケージの表（部品、次にパッケージ、次に既定値）、`キー = 値` 形式の機械設定ファイル（`examples/machine.cfg` がすべてのキーを説明）、CSV 位置ファイルの列名の追加、パッケージ別の回転補正。
* **テープ以外も扱うフィーダー**（`src/feeding.rs`）：優先度、トレイ、ストリップ、停止したフィーダー。部品のフィーダーはファイルの順序によらず同じように選ばれます。
* **機械のプランナー**（`src/planner.rs`、`src/motion.rs`）：ランク（順に搭載するブロック）、順序の戦略、ノズルチップとその交換、ノズル角度の制限、加速度とジャークを含む移動時間、より短い経路（2-opt）、部品の原点の補正。
* **パネル、搭載シート、出力**（`src/panel.rs`、`src/loadsheet.rs`、`src/export.rs`）：パネルの中の基板とパネル、各テープのスロット、レポート、不足の一覧、ヘッドだけを動かす G コード。
* **シミュレーターとジョブ**（`src/sim.rs`、`src/job.rs`、`src/preflight.rs`、`src/model.rs`）：ジョブ前の確認、仮想時計と注入できる故障を持つ機械、一時停止・一手ずつの実行・保存・再開ができるジョブ。

## 🧩 EH3D での役割

EH3D-PHOENIX-PNP は EH3D Fabrication Suite のプログラムの一つです。現在はライブラリとコマンドラインとして動作し、スイートの他のプログラムと共通の外観のウィンドウは予定です。単純なファイルを読み書きするため、単独でも他のプログラムの間でも使えます。

## 🏛️ アーキテクチャ

<p align="center">
  <img src="images/architecture.svg" alt="EH3D-PHOENIX-PNP のアーキテクチャ：誰が使い、何を持ち、何に依存するか" width="100%">
</p>


このリポジトリの各部分の関係：誰が使い、何を持ち、何に依存するか。図は `docs/architecture.json` から描かれ、コードと照合されます。

## ✨ 機能

* **ライブラリとコマンドライン：** `phoenix-pnp` の各コマンドはライブラリの関数なので、スイートの他のプログラムがプロセスを起動せずに呼び出せます。
* **厳密なコマンドライン：** 書き間違えたオプション（`--powr`）や余分な語は無視されず、それを名指しするエラーになります。そうでないと、プログラムは既定値で出力を作ってしまいます。
* **破損入力のテスト：** 空のファイル、ゴミデータ、巨大な数値、途中で切れたファイルも与え、どれもパニックしてはいけません。
* **推測しない：** 意味のない値は、どの値かと理由を示すエラーになり、見かけだけ正しい結果にはなりません。
* **unsafe コードなし：** ライブラリとプログラムに `#![forbid(unsafe_code)]`。
* **2 つの表を 1 つのリーダーで読み、大きな基板でも速い：** 位置ファイルとフィーダーファイルは同じリーダーで読み（カンマまたはセミコロン、区切り文字や改行を含む引用符付きフィールド）、5 万点の部品の確認・割り当て・計画が一瞬で終わり、計画は元にしたファイルを上書きせず、未知のオプションはエラーになり、`--origin-x` は下面を鏡像にするときの基板の左端の位置を指定します。

## 🔒 セキュリティと安全

| 脅威 | 対策 |
|---|---|
| 破損・途中切れ・空のファイル | パニックなし：テストは有効な入力をあらゆる位置で切り、ゴミデータも与えます |
| 意味のない数値（負、無限、極端に大きい） | 値を示すエラーで拒否され、もっともらしい値に丸められることはありません |
| 安全でないメモリ使用 | `forbid(unsafe_code)` の安全な Rust のみ |

**防御していないこと：** エラーなく作られた結果は、検証された結果ではありません。ここには機械の限界を知るものはなく、プログラムは独立したレビューを受けていません。脆弱性は非公開で報告してください（[SECURITY.md](SECURITY.md) 参照）。

## 🔌 インターフェース

ライブラリは `eh3d_phoenix_pnp`（`use eh3d_phoenix_pnp::...`）、コマンドラインは `phoenix-pnp` です。コマンドは次のとおりで、結果は標準出力に表示され、エラーは標準エラー出力に 0 以外の終了コードで報告されます。ネットワークアクセスはありません。

```text
EH3D-PHOENIX-PNP/
├── src/
│   ├── catalog.rs                               部品とパッケージの表、およびその属性の継承
│   ├── cli_args.rs                              コマンドライン引数（厳格：未知のオプションはエラー）
│   ├── commands.rs                              設定を使うコマンド：schedule、preflight、simulate、loadsheet
│   ├── config.rs                                機械の設定ファイル：キー、範囲、安全限界
│   ├── error.rs                                 エラーコード PHOENIX-PNP-NNN と8言語での意味
│   ├── export.rs                                レポート、不足の一覧、移動の G コード
│   ├── feeders.rs                               ロジックの一部
│   ├── feeding.rs                               優先度・トレイ・ストリップ付きのフィーダーと、部品のフィーダーの選択
│   ├── job.rs                                   ジョブ：ステップ、一時停止、再試行、方針、統計、保存状態
│   ├── lib.rs                                   ライブラリ：各コマンドが使うもの
│   ├── loadsheet.rs                             搭載シート：各テープのスロット
│   ├── main.rs                                  コマンドライン
│   ├── model.rs                                 機械の部品をトレイトで表現：機械、ノズル、フィーダー、アクチュエーター、カメラ、ドライバー
│   ├── motion.rs                                加速度とジャークを含む移動時間
│   ├── panel.rs                                 パネル：パネルの中の基板とパネル、それぞれの位置
│   ├── parts.rs                                 ロジックの一部
│   ├── plan.rs                                  ロジックの一部
│   ├── planner.rs                               機械を知るプランナー：ランク、順序、チップ、角度、時間
│   ├── preflight.rs                             ジョブ前の確認（重大度と対象つき）
│   ├── rotation.rs                              パッケージ別の回転補正と、制限付きノズルの回し方
│   ├── sim.rs                                   模擬機械：仮想時計と注入できる故障
│   └── units.rs                                 単位を持つ長さと位置
├── tests/
│   ├── commands.rs                              設定を使うコマンドのテスト
│   ├── feeding.rs                               フィーダーのテスト：優先度、トレイ、ストリップ
│   ├── job.rs                                   ジョブのテスト：ステップ、再試行、保存状態
│   ├── loadsheet.rs                             搭載シートと出力のテスト
│   ├── model.rs                                 データモデルのテスト：コード、長さ、設定、表
│   ├── more.rs                                  ライブラリとコマンドラインの追加テスト
│   ├── panel.rs                                 パネルのテスト
│   ├── planner.rs                               移動時間とプランナーのテスト
│   ├── pnp.rs                                   ライブラリとコマンドラインのテスト
│   ├── preflight.rs                             ジョブ前の確認のテスト
│   ├── readme.rs                                README が実行でき、漏れがないことのテスト
│   ├── sim.rs                                   模擬機械のテスト
│   └── strict.rs                                コマンドラインのテスト：知らないオプションと語
├── examples/
│   ├── board.pos                                試すためのサンプル
│   ├── catalog.csv                              部品の表：ランクと高さ
│   ├── feeders.csv                              試すためのサンプル
│   ├── machine.cfg                              機械の設定（すべてのキーを説明）
│   ├── packages.csv                             パッケージの表：高さ、チップ、ランク、テープ
│   ├── panel.json                               2枚の基板のパネル
│   └── slots.csv                                機械のスロット
├── docs/
│   ├── architecture.json                        アーキテクチャ図のデータ
│   └── structure.json                           構成図のデータ
├── tools/                                       CI と README のチェック、図のツール
├── images/                                      バナーとアーキテクチャ・リポジトリ構成の図
├── .github/                                     CI とリリースのワークフロー、Issue と PR のテンプレート
├── eh3d.project.json                            マニフェスト：バージョン、役割、成熟度、ビルドコマンド
├── Cargo.toml Cargo.lock                        ワークスペースと固定された依存関係
├── rust-toolchain.toml                          固定されたRustツールチェーン
├── CHANGELOG.md                                 各バージョンの変更点
├── SECURITY.md SUPPORT.md                       脆弱性の報告方法と問い合わせ先
└── CONTRIBUTING.md CODE_OF_CONDUCT.md LICENSE   貢献方法、行動規範、ライセンス
```

## ⚙️ 設定

機械とプランナーは `キー = 値` の行からなるテキストファイル（`--config ファイル`）で設定します。形式はスイートの他のプログラムと同じです。`examples/machine.cfg` がすべてのキーを既定値と働きとともに載せています。ないキーは既定値のままで、未知のキー、二重に書かれたキー、範囲外のキーは行を示すエラーになります。コマンドラインのオプション（`--nozzles`、`--speed`、`--order`、`--side`）はファイルより優先されます。

パッケージと部品の表（`--packages`、`--catalog`）は空のセルが継承される CSV ファイル、パネル（`--panel`）は JSON ファイル、機械のスロット（`loadsheet`）は `id,x,y,max_width_mm` の CSV です。`max_file_bytes` より大きなファイルは拒否されます。すべてのエラーにはコード `PHOENIX-PNP-NNN` があり、メッセージはキー `language` または `--lang` の言語（`en`、`es`、`de`、`fr`、`it`、`ja`、`zh`、`pt`）で書かれます。

| コード | 意味 |
|---|---|
| `PHOENIX-PNP-001` | ファイルが許容サイズを超えています |
| `PHOENIX-PNP-002` | 部品数が上限を超えています |
| `PHOENIX-PNP-003` | 設定の行が「キー = 値」の形式ではありません |
| `PHOENIX-PNP-004` | 設定に未知のキーがあります |
| `PHOENIX-PNP-005` | 設定の値が無効です |
| `PHOENIX-PNP-006` | 設定の値が範囲外です |
| `PHOENIX-PNP-007` | この設定は新しいバージョンのプログラム用です |
| `PHOENIX-PNP-008` | 設定のキーが重複しています |
| `PHOENIX-PNP-009` | 設定が安全限界を超えています |
| `PHOENIX-PNP-010` | 部品またはパッケージ表の行が無効です |
| `PHOENIX-PNP-011` | 部品またはパッケージが二重に定義されています |
| `PHOENIX-PNP-012` | 回転補正の規則が無効です |
| `PHOENIX-PNP-013` | フィーダーの行が無効です |
| `PHOENIX-PNP-014` | パネルファイルが無効です |
| `PHOENIX-PNP-015` | パネルの入れ子が深すぎるか基板が多すぎます |
| `PHOENIX-PNP-016` | スロット表が無効です |
| `PHOENIX-PNP-017` | 計画の設定が無効です |
| `PHOENIX-PNP-018` | ノズルは制限内でその角度に回転できません |
| `PHOENIX-PNP-019` | ノズルは既に部品を保持しており、別の部品は取れません |
| `PHOENIX-PNP-020` | ノズルに配置する部品がありません |
| `PHOENIX-PNP-021` | この部品に使えるフィーダーがありません |
| `PHOENIX-PNP-022` | すべての試行後も部品を取れませんでした |
| `PHOENIX-PNP-023` | すべての試行後もフィーダーが送れませんでした |
| `PHOENIX-PNP-024` | すべての試行後も部品を配置できませんでした |
| `PHOENIX-PNP-025` | 保存された状態は別のジョブのものです |
| `PHOENIX-PNP-026` | 保存されたジョブの状態が無効です |
| `PHOENIX-PNP-027` | ジョブは一時停止中、完了、またはエラーで停止しています |
| `PHOENIX-PNP-028` | ジョブ前の確認でエラーが見つかりました |
| `PHOENIX-PNP-029` | 長さまたは角度が無効です |
| `PHOENIX-PNP-030` | ドライバーがコマンドを受け付けませんでした |
| `PHOENIX-PNP-031` | シミュレーターが障害を注入しました |

## 🚀 クイックスタート

安定版の Rust ツールチェーン（1.75 以降）が必要です。

```bash
cargo build --release
cargo test
cargo run --release -- plan examples/board.pos examples/feeders.csv examples/plan.json --nozzles 2
cargo run --release -- schedule examples/board.pos examples/feeders.csv examples/schedule.json --config examples/machine.cfg --packages examples/packages.csv --catalog examples/catalog.csv
cargo run --release -- simulate examples/board.pos examples/feeders.csv --config examples/machine.cfg --state examples/job_state.json --steps 5
cargo run --release -- loadsheet examples/board.pos examples/slots.csv examples/loadsheet.csv --packages examples/packages.csv
```

## 🧪 テスト

テストは 198 個あります（`cargo test`、CI の Windows と Linux で実行）。通常のケース、境界のケース、壊れた入力を確認します。空のファイル、ゴミ、途中で切れたファイルでパニックは起きません。固定シードから作る小さな生成ケース（実行のたびに同じ数値になります）で、計画がファイルの順序に依存しないこと、経路が決して長くならないこと、どこで保存して再開してもまったく止まらなかったジョブと同じ結果になること、長さと角度が行き来して変換できることを確認します。CI は `cargo fmt --check`、警告をエラーとする `cargo clippy`、`python -I tools/ci_validate.py` も実行します。

## 🏗️ ビルドとリリース

バージョンは `Cargo.toml` と `eh3d.project.json` にあり、各バージョンは [CHANGELOG.md](CHANGELOG.md) に項目があります。`build.bat` / `build.sh` でビルド、`test.bat` / `test.sh` でテスト、`run.bat` / `run.sh` でサンプル実行します。リリースは GitHub で公開されます。

## 📂 リポジトリの構成

<p align="center">
  <img src="images/repository-structure.svg" alt="EH3D-PHOENIX-PNP のリポジトリ構成" width="100%">
</p>

<details>
<summary>ツリーのテキスト版</summary>

```text
EH3D-PHOENIX-PNP/
├── src/
│   ├── catalog.rs                               部品とパッケージの表、およびその属性の継承
│   ├── cli_args.rs                              コマンドライン引数（厳格：未知のオプションはエラー）
│   ├── commands.rs                              設定を使うコマンド：schedule、preflight、simulate、loadsheet
│   ├── config.rs                                機械の設定ファイル：キー、範囲、安全限界
│   ├── error.rs                                 エラーコード PHOENIX-PNP-NNN と8言語での意味
│   ├── export.rs                                レポート、不足の一覧、移動の G コード
│   ├── feeders.rs                               ロジックの一部
│   ├── feeding.rs                               優先度・トレイ・ストリップ付きのフィーダーと、部品のフィーダーの選択
│   ├── job.rs                                   ジョブ：ステップ、一時停止、再試行、方針、統計、保存状態
│   ├── lib.rs                                   ライブラリ：各コマンドが使うもの
│   ├── loadsheet.rs                             搭載シート：各テープのスロット
│   ├── main.rs                                  コマンドライン
│   ├── model.rs                                 機械の部品をトレイトで表現：機械、ノズル、フィーダー、アクチュエーター、カメラ、ドライバー
│   ├── motion.rs                                加速度とジャークを含む移動時間
│   ├── panel.rs                                 パネル：パネルの中の基板とパネル、それぞれの位置
│   ├── parts.rs                                 ロジックの一部
│   ├── plan.rs                                  ロジックの一部
│   ├── planner.rs                               機械を知るプランナー：ランク、順序、チップ、角度、時間
│   ├── preflight.rs                             ジョブ前の確認（重大度と対象つき）
│   ├── rotation.rs                              パッケージ別の回転補正と、制限付きノズルの回し方
│   ├── sim.rs                                   模擬機械：仮想時計と注入できる故障
│   └── units.rs                                 単位を持つ長さと位置
├── tests/
│   ├── commands.rs                              設定を使うコマンドのテスト
│   ├── feeding.rs                               フィーダーのテスト：優先度、トレイ、ストリップ
│   ├── job.rs                                   ジョブのテスト：ステップ、再試行、保存状態
│   ├── loadsheet.rs                             搭載シートと出力のテスト
│   ├── model.rs                                 データモデルのテスト：コード、長さ、設定、表
│   ├── more.rs                                  ライブラリとコマンドラインの追加テスト
│   ├── panel.rs                                 パネルのテスト
│   ├── planner.rs                               移動時間とプランナーのテスト
│   ├── pnp.rs                                   ライブラリとコマンドラインのテスト
│   ├── preflight.rs                             ジョブ前の確認のテスト
│   ├── readme.rs                                README が実行でき、漏れがないことのテスト
│   ├── sim.rs                                   模擬機械のテスト
│   └── strict.rs                                コマンドラインのテスト：知らないオプションと語
├── examples/
│   ├── board.pos                                試すためのサンプル
│   ├── catalog.csv                              部品の表：ランクと高さ
│   ├── feeders.csv                              試すためのサンプル
│   ├── machine.cfg                              機械の設定（すべてのキーを説明）
│   ├── packages.csv                             パッケージの表：高さ、チップ、ランク、テープ
│   ├── panel.json                               2枚の基板のパネル
│   └── slots.csv                                機械のスロット
├── docs/
│   ├── architecture.json                        アーキテクチャ図のデータ
│   └── structure.json                           構成図のデータ
├── tools/                                       CI と README のチェック、図のツール
├── images/                                      バナーとアーキテクチャ・リポジトリ構成の図
├── .github/                                     CI とリリースのワークフロー、Issue と PR のテンプレート
├── eh3d.project.json                            マニフェスト：バージョン、役割、成熟度、ビルドコマンド
├── Cargo.toml Cargo.lock                        ワークスペースと固定された依存関係
├── rust-toolchain.toml                          固定されたRustツールチェーン
├── CHANGELOG.md                                 各バージョンの変更点
├── SECURITY.md SUPPORT.md                       脆弱性の報告方法と問い合わせ先
└── CONTRIBUTING.md CODE_OF_CONDUCT.md LICENSE   貢献方法、行動規範、ライセンス
```

</details>

## 🗺️ ロードマップと既知の制限

**現時点で分かっている制限**

* ウィンドウはまだありません。ライブラリとコマンドラインだけです。
* 実機でも実際のファイル群でも試しておらず、独立したレビューも受けていません。機械に関することはすべてシミュレーションです。
* 他の実装プログラムのファイルは読みません。CSV の下面の角度の規則（`bottom_rotation`）は、どの CAD プログラムの実ファイルとも比較していません。
* カメラ、視覚によるピックの補正、実機のドライバーはまだありません。計画は別のプログラムが実行する JSON ファイルです。

**次のステップ**

* スイートのプログラムに共通する見た目のウィンドウを8言語で追加する：作業テーブル、部品とフィーダーの表、問題と解決のパネル。
* 基板とパネルのフィデューシャル、EH3D-VISION-STUDIO のカメラによる補正。計画を機械に送るのは、実機で試してからにする。
* 実際のファイルとスイートの他のプログラムで試す。

## 🌍 言語

README とテキストは、英語、スペイン語、フランス語、イタリア語、ドイツ語、中国語、日本語、ヨーロッパポルトガル語で提供されています。

## 🔗 関連プロジェクト

* [EH3D-VISION-STUDIO](https://github.com/JuanenRac/EH3D-VISION-STUDIO): カメラ、基準マーク、検査
* [EH3D-MOTIONCNC](https://github.com/JuanenRac/EH3D-MOTIONCNC): 機械制御プログラム
* [EH3D-GERBERVIEWER](https://github.com/JuanenRac/EH3D-GERBERVIEWER): Gerber レイヤー ビューアー
* [EH3D-TOOL-MANAGER](https://github.com/JuanenRac/EH3D-TOOL-MANAGER): CAM プログラム共通の工具庫
* [EH3D-SUITE-LAUNCHER](https://github.com/JuanenRac/EH3D-SUITE-LAUNCHER): スイートのプログラムを見つけて起動するハブ

## 📚 ドキュメント

* [変更履歴](CHANGELOG.md)
* [アーキテクチャと構成のデータ](docs/architecture.json)：2 枚の図を描くためのデータ。

## 🤝 コントリビュートとコミュニティ

* プルリクエストの前に `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`、`python -I tools/ci_validate.py` を実行し、すべて通してください。[CONTRIBUTING.md](CONTRIBUTING.md) と[行動規範](CODE_OF_CONDUCT.md)を参照。
* README は 8 言語あります。文章の変更は 8 ファイルすべてで行い、2 枚の図は `tools/make_architecture.py` と `tools/make_structure.py` が `docs/architecture.json` と `docs/structure.json` から描きます。
* 脆弱性は非公開で報告してください：[SECURITY.md](SECURITY.md)。質問・提案・報告：electrohobby3d@gmail.com（[SUPPORT.md](SUPPORT.md)）。

## 👤 作者

**JuanenRac (Electro Hobby 3D)** · electrohobby3d@gmail.com

## 📜 ライセンス

ソフトウェア：GPL-3.0-or-later。[LICENSE](LICENSE) を参照。スイートのハードウェア設計には CERN OHL-S を使います。
