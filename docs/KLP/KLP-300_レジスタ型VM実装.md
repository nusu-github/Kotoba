# KLP-300 — レジスタ型VM実装

> 種別: Kotoba Language Proposal (KLP) — Standards Track（実装記録）  
> 番号: KLP-300  
> 題名: レジスタ型VM（RegVM）の実装とスタック型VMからの移行  
> 状態: 実装済み（Implemented）  
> 注記: 本文書はバックエンド実装に関する記録である。言語の意味論規範（`[規範]` ラベル付き）は含まない。

---

## 要約

言の実行器を**スタック型VM**から**レジスタ型VM（RegVM）**へ移行した。  
移行は段階的に進められ、互換シムを経由した過渡期を経て完全な置き換えを達成した。  
現在の実行パイプラインは `AST → TypedHIR → RIR → RegProgram → RegVM` である。

---

## 動機

初期実装はスタック型VMを採用した。スタック型は実装が単純で、バイトコードコンパイラの初期プロトタイピングに適しているが、以下の課題があった。

- **最適化の難しさ**: スタック操作が多く、冗長な `Push`/`Pop` の連鎖が生じる
- **IRとの親和性の低さ**: 型推論・意味解析の結果（TypedHIR）を静的に扱うには、レジスタ割り当て済みの中間表現（RIR）の方が自然
- **将来のバックエンド移行**: LLVMなどの低レベルバックエンドへの移行を見据えると、レジスタ型IRが出発点として望ましい

---

## 移行経緯

### フェーズ 1 — スタック型VM時代（初期実装）

最初の実装（`init` コミット以降）ではスタック型VMが唯一の実行器だった。

- コンパイラが `OpCode` 列を直接スタックに積む形式で出力
- 呼び出し規約・変数参照もすべてスタックオフセットで管理
- 構造はシンプルだが、最適化・デバッグ・将来拡張の余地が限られていた

### フェーズ 2 — RegVM 導入・`StackCompat` シム期（過渡期）

`AST → TypedHIR → RIR → RegProgram → RegVM` パイプラインを新設した。  
ただし `RegVM` は内部で旧スタック型VMに処理を委譲する `StackCompat` シムを持ち、  
既存テストをすべて通しながら新パイプラインの骨格だけを先行して確立した。

この設計により「フラッグデー的な全書き換え」を避け、段階的な移行が可能になった。

### フェーズ 3 — RegVM 単独実行への完全移行（現状）

`StackCompat` シムを撤去し、RegVM が RIR/RegProgram を直接実行する唯一の実行器となった。  
移行に伴い、以下も整理された。

- 過渡期のみ必要だった `lib.rs` の互換再エクスポートを削除
- 出力バッファをアクセサ経由で公開（APIの衛生化）
- チャンク列の反復をプログラム型経由に統一（内部コレクションの隠蔽）

---

## 現在のアーキテクチャ

### 実行パイプライン

```
ソースファイル
  ↓ 字句解析（logos）
Token列
  ↓ 文法正規化（chumsky）
正規化Token列
  ↓ 構文解析（Parser）
AST
  ↓ 意味解析（Sema）
TypedHIR（型付き高水準IR）
  ↓ コード生成（Codegen）
RIR（レジスタIR命令列）
  ↓ プログラム構築
RegProgram（チャンク群）
  ↓ 実行
RegVM（レジスタ型VM）
```

### RegVM の主要構造

```
RegVM
├── chunks: Vec<Chunk>         全チャンク（手順ごとのバイトコード）
├── stack: Vec<Value>          値スタック（ローカル変数 + 評価スタック）
├── frames: Vec<CallFrame>     コールフレームスタック
│   ├── chunk_id: usize        実行中チャンクのインデックス
│   ├── ip: usize              命令ポインタ
│   └── base: usize            ローカル変数のスタック開始位置
├── globals: HashMap<String, Value>  グローバル変数
├── try_stack: Vec<TryFrame>   例外ハンドラフレームスタック
│   ├── catch_target: usize    catch ジャンプ先
│   ├── stack_depth: usize     try 開始時のスタック深さ
│   └── chunk_id: usize        try 開始時のチャンクID
└── output: Vec<String>        出力バッファ（テスト用）
```

### 主要 OpCode 一覧

| カテゴリ       | OpCode 例                                                           |
| -------------- | ------------------------------------------------------------------- |
| 定数ロード     | `PushInt`, `PushFloat`, `PushString`, `PushBool`, `PushNone`        |
| ローカル変数   | `LoadLocal(idx)`, `StoreLocal(idx)`                                 |
| グローバル変数 | `LoadGlobal(name)`, `StoreGlobal(name)`                             |
| 算術・比較     | `Add`, `Sub`, `Mul`, `Div`, `Mod`, `Negate`                         |
| 論理比較       | `Equal`, `NotEqual`, `Greater`, `Less`, `GreaterEqual`, `LessEqual` |
| 制御フロー     | `Jump(target)`, `JumpIfFalse(target)`, `JumpIfTrue(target)`         |
| 手順呼び出し   | `Call(arg_count)`, `Return`                                         |
| スタック操作   | `Pop`, `Dup`                                                        |
| I/O            | `Print`, `ReadInput`, `ReadFile`, `WriteFile`                       |
| 例外処理       | `SetupTry(catch)`, `PopTry`, `Throw`                                |
| コレクション   | `MakeList(n)`, `MakeMap(n)`, `Index`, `SetIndex`                    |

### 値型（Value）

| 型名        | 内部表現                  | 言の表記     |
| ----------- | ------------------------- | ------------ |
| `Integer`   | `BigInt`（任意精度）      | 整数リテラル |
| `Float`     | `f64`                     | 小数リテラル |
| `String`    | `String`                  | 「文字列」   |
| `Bool`      | `bool`                    | 真 / 偽      |
| `List`      | `Vec<Value>`              | 一覧         |
| `Map`       | `HashMap<String, Value>`  | 対応表       |
| `Procedure` | `ProcRef`（チャンク参照） | 手順         |
| `None`      | —                         | 無           |

---

## 実装状況

| 項目                                | 状態        |
| ----------------------------------- | ----------- |
| スタック型VMから RegVM への移行     | ✅ 完了     |
| `StackCompat` シムの撤去            | ✅ 完了     |
| `AST→TypedHIR→RIR→RegProgram→RegVM` | ✅ 完了     |
| 全適合テストの通過                  | ✅ 確認済み |
| 出力バッファのアクセサ経由公開      | ✅ 完了     |
| `lib.rs` 過渡期再エクスポートの削除 | ✅ 完了     |

---

## 既存仕様への影響

本実装は言語の意味論には影響しない。実行モデル仕様（`docs/仕様/09_実行モデル.md`）の規範（RUN-001..008）はそのまま維持される。  
実装方針の詳細は `docs/実装/設計メモ_v1.md` を参照。

---

## 関連

| 文書                         | 関係                                 |
| ---------------------------- | ------------------------------------ |
| `docs/仕様/09_実行モデル.md` | 実行モデル規範（RUN-001..008）       |
| `docs/実装/設計メモ_v1.md`   | 実装方針・パイプライン設計メモ       |
| KLP-301                      | 次段階: LLVMバックエンドへの移行計画 |
