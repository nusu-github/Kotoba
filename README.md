# Kotoba v1 実装

言（ことば）言語 v1 の再構築実装です。

## CLI

```bash
kotoba run <file.kb> [--debug-vm] [--debug-ir]
kotoba check <file.kb>
kotoba test [--filter <CASE_ID>]
```

- `run`: コンパイルして実行
- `check`: 実行せずに静的検証（字句/構文/意味/コンパイル）
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

## 参照仕様

- [言語仕様_v1.md](docs/言語仕様_v1.md)
- [適合テスト台帳_v1.md](docs/適合テスト台帳_v1.md)
- [仕様差分_v0.4_to_v1.md](docs/仕様差分_v0.4_to_v1.md)
