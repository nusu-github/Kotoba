# Kotoba v1 実装

言（ことば）言語 v1 の再構築実装です。

## CLI

```bash
kotoba run <file.kb> [--debug-vm] [--debug-ir]
kotoba check <file.kb>
kotoba test [--filter <CASE_ID>]
```

- `run`: コンパイルして実行
- `check`: 実行せずに静的検証（モジュール解決 + 字句/構文/意味）
- `test`: `tests/conformance/manifest.yaml` のケースを実行

## モジュール解決

- `「モジュール」を 使う` は呼び出し元ファイル基準の相対パスで解決します。
- 拡張子がなければ `.kb` を補完します。
- 循環参照はコンパイルエラー扱いです（解決器で検出）。
- `「モジュール」から 「項目」を 使う` は公開定義のみ取り込みます。
- 公開されていない項目を指定するとコンパイルエラーになります。

## 診断

診断は次の情報を持ちます。

- 種別
- 位置（可能な場合）
- 原因
- 修正指針（ヒント）

`これ/それ/あれ` の単独使用は意味検査で拒否されます。

## テスト

```bash
cargo test
cargo run -- test
cargo run -- test --filter RUN-ACCEPT-001
```

`test` は次を事前検証します。

- `cases` / `catalog` のケースID重複がないこと
- `cases` の `input` が空でないこと
- `cases` に `@` プレースホルダ入力が残っていないこと
- 同一入力の過剰重複（8件超過）がないこと

## 安全ガード（暫定）

異常入力時のハングアップ回避として、次の解析ステップ上限を有効化しています。

- `KOTOBA_PARSE_STEP_LIMIT`（既定: `500000`）
- `KOTOBA_ANALYZE_STEP_LIMIT`（既定: `500000`）

必要時のみ最小限で調整してください。

## 参照仕様

- [言語仕様_v1.md](docs/言語仕様_v1.md)
- [適合テスト台帳_v1.md](docs/適合テスト台帳_v1.md)
- [仕様差分_v0.4_to_v1.md](docs/仕様差分_v0.4_to_v1.md)
