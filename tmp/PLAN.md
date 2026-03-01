# Kotoba v1 実装再構築計画（クリーン再実装・非互換）

## Summary
- 既存実装は構文受理と実行機能の乖離が大きく、v1規範（[言語仕様_v1.md](/home/nusu/Kotoba/docs/言語仕様_v1.md)）に対して全面的な再構築が最短です。
- 方針は「クリーン再実装」「既存CLIを置換」「v1フル準拠」「旧コード削除」「旧テスト全面刷新」。
- 実装は `run/check/test` サブコマンドCLI、`logos + chumsky` フロントエンド、制約ベース型推論、レジスタVM、台帳100%適合ゲートで進めます。

## Current Status Snapshot (2026-03-01)
- 現在は「再構築の中間到達点」。CLI と主要パイプラインは稼働し、`cargo test` と `kotoba test` は通過。
- ただし v1 フル準拠ゲート（台帳 204 ケース / TypedHIR / RegVM）には未達。
- 暫定安全策として、意味解析にステップ上限を導入（`KOTOBA_ANALYZE_STEP_LIMIT`、既定 500000）。
- 暫定安全策として、構文解析にもステップ上限と進捗停止ガードを導入（`KOTOBA_PARSE_STEP_LIMIT`、既定 500000）。

### Milestone Status
1. M0 Bootstrap and Cutover Skeleton: 完了（新ディレクトリ構成 + `run/check/test` 提供）
2. M1 Lexer: 概ね完了（助詞・予約語・インデント・助数詞対応）
3. M2 Parser: 部分完了（実装は再帰下降が主、`chumsky` 本格移行は未完）
4. M3 Module Loader and Graph Resolution: 完了（相対解決、`.kb` 補完、循環検出、公開項目 import）
5. M4 Name Resolution + Particle Call Resolution: 部分完了（助詞集合一致、`これ/それ/あれ` 制約、ループ文脈検査）
6. M5 Constraint-based Type Inference: 未完（現状は簡易検査中心、制約ベース推論は未着手）
7. M6 Control/Exception Semantics: 部分完了（`試す/失敗した場合/必ず行う` とループ制御の基本対応）
8. M7 Register VM + Codegen: 未完（現状はスタックVM運用。RegVM化は未着手）
9. M8 Conformance Harness and CLI `test`: 部分完了（manifest 実行 + filter + 失敗理由表示 + catalog 204件管理。実行ケース数は 219、catalog 204件の全IDを実行ケースへ反映済み）
10. M9 Docs and Final Hardening: 部分完了（README/設計メモ更新済み、最終固定は未完）

### Immediate Resume Priority
1. 仕様本文と台帳の衝突解消を維持（`これ/それ/あれ` は `の` 付き参照のみ許可）
2. 台帳ケース品質の厳密化（プレースホルダ入力ゼロ・同一入力の過剰重複抑制）
3. TypedHIR/制約ベース型推論の本体導入

## Important Public API / Interface / Type Changes
- CLI公開APIを次に固定する。
- `kotoba run <file.kb> [--debug-ir] [--debug-vm]`
- `kotoba check <file.kb>`
- `kotoba test [--filter <CASE_ID>]`
- モジュール解決APIを次に固定する。
- `「モジュール」を 使う` の文字列は「呼び出し元ファイル基準の相対パス」として解決。
- 拡張子がない場合は `.kb` を補完。
- 循環参照はコンパイルエラー。
- 言語インターフェースの必須変更を固定する。
- `これ/それ/あれ` 単独使用は禁止。
- 暗黙引数は「て形チェインの暗黙 `を`」のみ。
- メソッド暗黙 `が` は禁止。
- 呼び出し解決は助詞集合完全一致のみ（型オーバーロードなし）。
- 診断インターフェースを固定する。
- すべての診断は `種別/位置/原因/修正指針` を持つ。
- 主要規範診断（DGN-002..006）を固定文言で実装。

## Repository Restructure (Decision Complete)
- 旧 `src/*.rs` は削除し、新構成へ置換する。
- `src/main.rs`（CLI）
- `src/lib.rs`
- `src/diag/{mod.rs, report.rs}`
- `src/frontend/{token.rs, lexer.rs, parser.rs, ast.rs, grammar.rs}`
- `src/module/{loader.rs, resolver.rs}`
- `src/sema/{symbols.rs, hir.rs, types.rs, infer.rs, particles.rs}`
- `src/backend/{rir.rs, codegen.rs, vm.rs, value.rs, builtins.rs}`
- `src/common/{span.rs, source.rs}`
- テストは `tests/` 配下へ移行する。
- `tests/conformance/{runner.rs, manifest.yaml, cases/...}`
- `tests/unit/{lexer.rs, parser.rs, sema.rs, vm.rs}`
- `tests/e2e/{cli_run.rs, cli_check.rs, cli_test.rs}`
- 仕様連動ドキュメントは次を維持・更新する。
- [言語仕様_v1.md](/home/nusu/Kotoba/docs/言語仕様_v1.md)
- [適合テスト台帳_v1.md](/home/nusu/Kotoba/docs/適合テスト台帳_v1.md)
- [仕様差分_v0.4_to_v1.md](/home/nusu/Kotoba/docs/仕様差分_v0.4_to_v1.md)

## Dependency Plan
- 追加するライブラリを固定する。
- `clap`（サブコマンドCLI）
- `logos`（字句解析）
- `chumsky`（構文解析・回復）
- `ariadne`（スパン付き診断）
- `indexmap`（順序安定マップ）
- 継続利用。
- `thiserror`
- `num-bigint`
- `num-traits`

## Architecture and Data Flow
- パイプラインを固定する。
- SourceGraph構築 -> Lex -> Parse(AST) -> ModuleResolve -> NameResolve(HIR) -> TypeInfer/Check(TypedHIR) -> Lower(RIR) -> Codegen(RegProgram) -> RegVM実行
- 中間表現を固定する。
- AST: 構文忠実・糖衣保持
- HIR: 名前解決済み・助詞引数を正規化
- TypedHIR: 型/助数詞次元確定
- RIR: レジスタ割当前提の制御フロー正規形
- RegProgram: 命令列 + 定数 + 関数シグネチャ
- レジスタVM方式を固定する。
- 命令は `LoadConst/Move/BinOp/Cmp/Jump/Branch/Call/Ret/MakeList/MakeMap/GetProp/Throw/Try/EndTry` を最小核にする。
- 関数呼出しは「呼出しフレーム + レジスタウィンドウ」方式に固定する。

## Milestones (Execution Plan)
1. M0: Bootstrap and Cutover Skeleton
- `Cargo.toml` へ依存追加、CLI雛形と新ディレクトリ構成を導入。
- 旧 `src` を削除し、新 `src` でビルドが通る最小状態を作る。
- Gate: `cargo test` が新テストゼロ件でも成功、`kotoba --help` が新CLIを表示。

2. M1: Lexer (v1 Lexical Rules)
- 助詞三分類、最長一致、予約語優先、インデント、数値+助数詞（ひらがな禁止）を実装。
- `しながら/待つ/背景で` を予約語として受理し、後段で未実装診断へ接続。
- Gate: 字句ユニットテスト + 台帳字句ケース（LXA/LXR）全通過。

3. M2: Parser (Full v1 Grammar)
- `chumsky` で v1 EBNF準拠パーサを実装。
- `返す/試す/使う/公開` の独立規則、`助詞式/束縛式/アクセス式` 分離を実装。
- `これ/それ/あれ` 単独は構文または意味段階で必ず拒否（DGN-002文言固定）。
- Gate: 構文ケース受理52/拒否52を自動化して全通過。

4. M3: Module Loader and Graph Resolution
- 相対パス + `.kb` 補完でモジュールロード。
- 依存グラフDFSで循環検出し、循環はコンパイルエラー。
- `公開` シンボルテーブルを導入し `使う` 解決に適用。
- Gate: MOD系ケース + 循環参照専用テスト全通過。

5. M4: Name Resolution + Particle Call Resolution
- スコープチェーン、こそあど参照、`こう` 再帰参照を実装。
- 呼び出し解決は「手順名 + 助詞集合完全一致」だけで決定。
- て形暗黙 `を` 補完以外は拒否。
- Gate: CAL/SCP系ケース全通過。

6. M5: Constraint-based Type Inference
- 型変数・単一化・制約収集を実装。
- 助数詞を `Num<Dim>` として管理し、次元不一致を静的エラー化。
- 数値昇格規則（整数除算=>小数、混合=>小数）を実装。
- Gate: TYP系ケース全通過。

7. M6: Control/Exception Semantics
- if/match/loop/try を式として実装。
- `必ず行う` 値破棄、finally再送出による上書きを実装。
- ループの `次へ/抜ける` 文脈制約を実装。
- Gate: CTR/EXC系ケース全通過。

8. M7: Register VM + Codegen
- TypedHIR -> RIR -> RegProgram 生成を実装。
- RegVMで算術・比較・呼出し・例外・データ構造・入出力を実装。
- 組み込みは「仕様例で使う最小集合」に固定実装。
- Gate: 実行E2E + RUN系ケース全通過。

9. M8: Conformance Harness and CLI `test`
- 台帳を `tests/conformance/manifest.yaml` へ機械可読化。
- `kotoba test` で全ケース実行・ID絞り込み・失敗詳細表示を実装。
- Gate: 台帳ケース100%通過（受理102/拒否102）。

10. M9: Docs and Final Hardening
- READMEと設計メモを新CLI・モジュール規則・テスト実行手順へ更新。
- 主要診断メッセージをスナップショット固定。
- Gate: ドキュメント更新完了、CIで `fmt/clippy/test/conformance` 全成功。

## Test Cases and Scenarios
- 単体テスト。
- Lexer: 助詞分離・予約語優先・助数詞判定・インデント。
- Parser: 完全文法、禁止構文、優先順位、ブロック境界。
- Sema: 助詞集合一致、こそあど解決、公開境界、型推論。
- VM: 数値規則、制御、例外、データ操作。
- 適合テスト。
- 台帳準拠で受理102/拒否102を自動実行。
- 構文ケースは必須で受理52/拒否52を固定維持。
- E2Eテスト。
- `run/check/test` CLIの正常系・異常系・診断出力フォーマット。
- 回帰テスト。
- DGN-002..006の文言一致スナップショット。

## Acceptance Criteria
- [言語仕様_v1.md](/home/nusu/Kotoba/docs/言語仕様_v1.md) の規範IDすべてに実装対応がある。
- [適合テスト台帳_v1.md](/home/nusu/Kotoba/docs/適合テスト台帳_v1.md) の全ケースが `kotoba test` で100%通過。
- `manifest` で `@` プレースホルダ入力が 0 件である。
- 旧 `src` 実装・旧インラインテストは撤去済み。
- CLIは `run/check/test` を提供し、`check` で実行なし静的検証が可能。
- DGN要件（種別/位置/原因/修正指針）を満たす診断が出力される。

## Assumptions and Defaults (Locked)
- 非互換で進め、v0.4実行互換は維持しない。
- 実装対象は仕様v1フル準拠。
- モジュールはローカルファイルのみ、循環は禁止。
- 組み込みは「仕様例で使う最小集合」を初版範囲とする。
- パフォーマンス最適化は適合達成後の後続フェーズとする。
