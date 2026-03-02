use crate::common::source::Span;
use crate::frontend::ast::*;
use crate::frontend::grammar::normalize_token_stream;
use crate::frontend::token::{Particle, Token, TokenKind};
use tracing::instrument;

const DEFAULT_PARSE_STEP_LIMIT: usize = 500_000;

/// パースエラー
#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// 再帰下降パーサ
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    errors: Vec<ParseError>,
    step_limit: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            errors: Vec::new(),
            step_limit: resolve_parse_step_limit(),
        }
    }

    /// プログラム全体をパースする
    #[instrument(skip(self))]
    pub fn parse(mut self) -> (Program, Vec<ParseError>) {
        let normalized = normalize_token_stream(self.tokens);
        self.tokens = normalized.tokens;
        self.errors
            .extend(normalized.errors.into_iter().map(|error| ParseError {
                message: error.message,
                span: error.span,
            }));
        let start_span = self.current_span();
        let mut statements = Vec::new();
        let mut safety_steps = 0usize;

        self.skip_newlines();

        while !self.is_at_end() {
            safety_steps = safety_steps.saturating_add(1);
            if safety_steps > self.step_limit {
                self.errors.push(ParseError {
                    message: format!(
                        "構文解析回数が上限を超えたため停止しました（上限: {}）",
                        self.step_limit
                    ),
                    span: self.current_span(),
                });
                break;
            }

            if matches!(self.current_kind(), TokenKind::Dedent) {
                self.advance();
                self.skip_newlines();
                continue;
            }

            let before_pos = self.pos;
            match self.parse_statement() {
                Ok(stmt) => statements.push(stmt),
                Err(e) => {
                    self.errors.push(e);
                    self.recover_to_next_statement();
                }
            }
            self.skip_newlines();

            if self.pos == before_pos && !self.is_at_end() {
                self.errors.push(ParseError {
                    message: "構文解析が進行しないため、この位置で解析を打ち切りました".into(),
                    span: self.current_span(),
                });
                self.advance();
            }
        }

        let end_span = self.current_span();
        let program = Program {
            statements,
            span: start_span.merge(end_span),
        };

        (program, self.errors)
    }

    // === ユーティリティ ===

    fn current(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn current_kind(&self) -> &TokenKind {
        &self.current().kind
    }

    fn current_span(&self) -> Span {
        self.current().span
    }

    fn is_at_end(&self) -> bool {
        matches!(self.current_kind(), TokenKind::Eof)
    }

    fn advance(&mut self) -> &Token {
        let tok = &self.tokens[self.pos.min(self.tokens.len() - 1)];
        if !self.is_at_end() {
            self.pos += 1;
        }
        tok
    }

    #[allow(dead_code)]
    fn peek_kind(&self) -> &TokenKind {
        self.current_kind()
    }

    fn peek_ahead(&self, offset: usize) -> &TokenKind {
        let idx = (self.pos + offset).min(self.tokens.len() - 1);
        &self.tokens[idx].kind
    }

    fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(self.current_kind()) == std::mem::discriminant(kind)
    }

    fn eat(&mut self, expected: &TokenKind) -> Result<Token, ParseError> {
        if self.check(expected) {
            Ok(self.advance().clone())
        } else {
            Err(ParseError {
                message: format!(
                    "「{:?}」が必要ですが、「{}」がありました",
                    expected,
                    self.current_kind()
                ),
                span: self.current_span(),
            })
        }
    }

    fn eat_identifier(&mut self) -> Result<(String, Span), ParseError> {
        match self.current_kind().clone() {
            TokenKind::Identifier(name) => {
                let span = self.current_span();
                self.advance();
                Ok((name, span))
            }
            _ => Err(ParseError {
                message: format!(
                    "識別子が必要ですが、「{}」がありました",
                    self.current_kind()
                ),
                span: self.current_span(),
            }),
        }
    }

    fn skip_newlines(&mut self) {
        while matches!(self.current_kind(), TokenKind::Newline | TokenKind::Period) {
            self.advance();
        }
    }

    fn recover_to_next_statement(&mut self) {
        loop {
            match self.current_kind() {
                TokenKind::Newline | TokenKind::Eof => {
                    break;
                }
                TokenKind::Dedent => {
                    self.advance();
                    break;
                }
                _ => {
                    self.advance();
                }
            }
        }
    }

    // === 文のパース ===

    fn parse_statement(&mut self) -> Result<Stmt, ParseError> {
        self.skip_newlines();

        let start = self.current_span();

        match self.current_kind().clone() {
            // 公開 修飾子
            TokenKind::Koukai => {
                self.advance();
                self.skip_newlines();
                let mut stmt = self.parse_statement()?;
                match &mut stmt.kind {
                    StmtKind::ProcDef { is_public, .. }
                    | StmtKind::StructDef { is_public, .. }
                    | StmtKind::TraitDef { is_public, .. } => {
                        if *is_public {
                            return Err(ParseError {
                                message: "「公開」は重複指定できません".into(),
                                span: start,
                            });
                        }
                        *is_public = true;
                    }
                    _ => {
                        return Err(ParseError {
                            message: "DGN-005: 「公開」は手順・組・特性にのみ適用できます".into(),
                            span: start,
                        });
                    }
                }
                stmt.span = start.merge(stmt.span);
                Ok(stmt)
            }

            // 変わる 名前 は 式（可変束縛）
            TokenKind::Kawaru => {
                self.advance();
                let (raw_name, _) = self.eat_identifier()?;
                let name = if matches!(self.current_kind(), TokenKind::Ha) {
                    self.eat(&TokenKind::Ha)?;
                    raw_name
                } else if let Some(stripped) = raw_name.strip_suffix('は') {
                    if stripped.is_empty() {
                        return Err(ParseError {
                            message: "可変束縛の名前が必要です".into(),
                            span: start,
                        });
                    }
                    stripped.to_string()
                } else {
                    self.eat(&TokenKind::Ha)?;
                    unreachable!()
                };
                let value = self.parse_expr()?;
                let span = start.merge(value.span);
                Ok(Stmt {
                    kind: StmtKind::Bind {
                        name,
                        mutable: true,
                        value,
                    },
                    span,
                })
            }

            // 返す
            TokenKind::Kaesu => {
                self.advance();
                let span = start;
                Ok(Stmt {
                    kind: StmtKind::Return(None),
                    span,
                })
            }

            // 次へ
            TokenKind::TsugiHe => {
                self.advance();
                Ok(Stmt {
                    kind: StmtKind::Continue,
                    span: start,
                })
            }

            // 抜ける
            TokenKind::Nukeru => {
                self.advance();
                Ok(Stmt {
                    kind: StmtKind::Break,
                    span: start,
                })
            }

            // もし（条件分岐）
            TokenKind::Moshi => self.parse_if_statement(start),

            // 試す
            TokenKind::Tamesu => {
                let expr = self.parse_try_expr(start)?;
                Ok(Stmt {
                    span: expr.span,
                    kind: StmtKind::ExprStmt(expr),
                })
            }

            // 予約済み（未実装）
            TokenKind::Shinagara | TokenKind::Matsu | TokenKind::Haikeide => Err(ParseError {
                message: "DGN-006: 未実装機能です（しながら/待つ/背景で）".into(),
                span: start,
            }),

            _ => {
                if let Some(loop_stmt) = self.try_parse_loop_statement(start)? {
                    return Ok(loop_stmt);
                }
                // 識別子から始まる場合: 束縛、再束縛、手順定義、組定義、式文の候補
                self.parse_identifier_led_statement(start)
            }
        }
    }

    fn parse_identifier_led_statement(&mut self, start: Span) -> Result<Stmt, ParseError> {
        // 先読みで何が続くかを判定する
        // パターン:
        //   名前 は 式                         → 束縛
        //   名前 という 手順 ...               → 手順定義
        //   名前 という 組 ...                 → 組定義
        //   名前 という 特性 ...               → 特性定義
        //   名前を 式 に変える                 → 再束縛
        //   それ以外                           → 式文

        // まず識別子と「は」のパターンをチェック
        if let TokenKind::Identifier(_) = self.current_kind() {
            if let Some(trait_impl) = self.try_parse_trait_impl_statement(start)? {
                return Ok(trait_impl);
            }

            // `人は 表示できる を持つ` は特性実装ヘッダ。
            // 本体ブロックがない場合は明示エラーにする。
            let looks_like_trait_impl_header = matches!(self.peek_ahead(1), TokenKind::Ha)
                && matches!(self.peek_ahead(2), TokenKind::Identifier(_))
                && matches!(self.peek_ahead(3), TokenKind::Particle(Particle::Wo))
                && (matches!(self.peek_ahead(4), TokenKind::Motsu)
                    || matches!(self.peek_ahead(4), TokenKind::Identifier(s) if s == "持つ"));
            let has_impl_block = matches!(self.peek_ahead(5), TokenKind::Newline)
                && matches!(self.peek_ahead(6), TokenKind::Indent);
            if looks_like_trait_impl_header && !has_impl_block {
                return Err(ParseError {
                    message: "特性実装には本体ブロックが必要です".into(),
                    span: start,
                });
            }

            // 名前 は 式（束縛）
            if matches!(self.peek_ahead(1), TokenKind::Ha) {
                let (name, _) = self.eat_identifier()?;
                self.eat(&TokenKind::Ha)?;

                if matches!(self.current_kind(), TokenKind::Identifier(_))
                    && matches!(self.peek_ahead(1), TokenKind::Identifier(s) if s == "を持つ")
                {
                    return Err(ParseError {
                        message: "特性実装には本体ブロックが必要です".into(),
                        span: start,
                    });
                }
                let value = self.parse_expr()?;
                let span = start.merge(value.span);
                return Ok(Stmt {
                    kind: StmtKind::Bind {
                        name,
                        mutable: false,
                        value,
                    },
                    span,
                });
            }

            // 名前 という 手順/組/特性
            if matches!(self.peek_ahead(1), TokenKind::ToIu) {
                let (name, _) = self.eat_identifier()?;
                self.eat(&TokenKind::ToIu)?;

                match self.current_kind().clone() {
                    TokenKind::Tejun => {
                        self.advance();
                        return self.parse_proc_def(name, start);
                    }
                    TokenKind::Kumi => {
                        self.advance();
                        return self.parse_struct_def(name, start);
                    }
                    TokenKind::Tokusei => {
                        self.advance();
                        return self.parse_trait_def(name, start);
                    }
                    _ => {
                        return Err(ParseError {
                            message: "「という」の後には「手順」「組」「特性」が必要です".into(),
                            span: self.current_span(),
                        });
                    }
                }
            }
        }

        // 式文として解析し、結果を見て再束縛やreturnに変換
        let expr = self.parse_expr()?;

        // 式の後に特定のキーワードがあれば変換
        // `式を 返す` パターン
        if matches!(self.current_kind(), TokenKind::Kaesu) {
            let end_span = self.current_span();
            self.advance();
            return Ok(Stmt {
                kind: StmtKind::Return(Some(expr)),
                span: start.merge(end_span),
            });
        }

        if let Some(use_stmt) = self.try_convert_use_statement(&expr)? {
            return Ok(Stmt {
                kind: use_stmt,
                span: start.merge(expr.span),
            });
        }

        if let ExprKind::Identifier(name) = &expr.kind {
            if !name.ends_with('は') && matches!(self.current_kind(), TokenKind::String(_)) {
                return Err(ParseError {
                    message:
                        "文の区切りが必要です（「。」「改行」または適切な助詞を入れてください）"
                            .into(),
                    span: self.current_span(),
                });
            }
        }

        let span = start.merge(expr.span);
        Ok(Stmt {
            kind: StmtKind::ExprStmt(expr),
            span,
        })
    }

    fn try_parse_trait_impl_statement(&mut self, start: Span) -> Result<Option<Stmt>, ParseError> {
        let checkpoint = self.pos;
        let (raw_type_name, _) = match self.current_kind().clone() {
            TokenKind::Identifier(name) => {
                self.advance();
                (name, start)
            }
            _ => return Ok(None),
        };

        let type_name = if matches!(self.current_kind(), TokenKind::Ha) {
            self.advance();
            raw_type_name
        } else if let Some(stripped) = raw_type_name.strip_suffix('は') {
            if stripped.is_empty() {
                self.pos = checkpoint;
                return Ok(None);
            }
            stripped.to_string()
        } else {
            self.pos = checkpoint;
            return Ok(None);
        };

        let trait_name = match self.current_kind().clone() {
            TokenKind::Identifier(name) => {
                self.advance();
                name
            }
            _ => {
                self.pos = checkpoint;
                return Ok(None);
            }
        };

        let has_impl_marker = if matches!(self.current_kind(), TokenKind::Particle(Particle::Wo)) {
            self.advance();
            match self.current_kind().clone() {
                TokenKind::Motsu => {
                    self.advance();
                    true
                }
                TokenKind::Identifier(s) if s == "持つ" => {
                    self.advance();
                    true
                }
                _ => false,
            }
        } else if matches!(self.current_kind(), TokenKind::Identifier(s) if s == "を持つ") {
            self.advance();
            true
        } else {
            false
        };

        if !has_impl_marker {
            self.pos = checkpoint;
            return Ok(None);
        }

        self.skip_newlines();
        if !matches!(self.current_kind(), TokenKind::Indent) {
            return Err(ParseError {
                message: "特性実装には本体ブロックが必要です".into(),
                span: start,
            });
        }
        let body = self.parse_block()?;
        let body_span = body.span;
        let methods = body.statements;
        if methods.is_empty() {
            return Err(ParseError {
                message: "特性実装には1つ以上のメソッド定義が必要です".into(),
                span: body_span,
            });
        }
        if methods
            .iter()
            .any(|stmt| !matches!(stmt.kind, StmtKind::ProcDef { .. }))
        {
            return Err(ParseError {
                message: "特性実装の本体には手順定義のみ記述できます".into(),
                span: body_span,
            });
        }

        Ok(Some(Stmt {
            kind: StmtKind::TraitImpl {
                type_name,
                trait_name,
                methods,
            },
            span: start.merge(body_span),
        }))
    }

    fn try_convert_use_statement(&self, expr: &Expr) -> Result<Option<StmtKind>, ParseError> {
        let ExprKind::Call { callee, args } = &expr.kind else {
            return Ok(None);
        };
        if callee != "使う" {
            return Ok(None);
        }

        let mut module_from_kara: Option<String> = None;
        let mut wo_items: Vec<String> = Vec::new();

        for arg in args {
            let s = match &arg.value.kind {
                ExprKind::StringLiteral(s) => s.clone(),
                _ => {
                    return Err(ParseError {
                        message: "「使う」の引数は文字列リテラルで指定してください".into(),
                        span: arg.span,
                    });
                }
            };
            match arg.particle {
                Particle::Kara => module_from_kara = Some(s),
                Particle::Wo => wo_items.push(s),
                _ => {
                    return Err(ParseError {
                        message: "「使う」で使える助詞は「を」「から」のみです".into(),
                        span: arg.span,
                    });
                }
            }
        }

        if let Some(module) = module_from_kara {
            if wo_items.is_empty() {
                return Err(ParseError {
                    message: "「Xから Yを 使う」の形では取り込む項目が必要です".into(),
                    span: expr.span,
                });
            }
            return Ok(Some(StmtKind::Use {
                module,
                items: Some(wo_items),
            }));
        }

        if wo_items.len() != 1 {
            return Err(ParseError {
                message: "「Xを 使う」の形ではモジュール名を1つだけ指定してください".into(),
                span: expr.span,
            });
        }

        Ok(Some(StmtKind::Use {
            module: wo_items[0].clone(),
            items: None,
        }))
    }

    // === 手順定義 ===

    fn parse_proc_def(&mut self, name: String, start: Span) -> Result<Stmt, ParseError> {
        // 「返す」は予約キーワードのため手順名として使用不可
        if name == "返す" {
            return Err(ParseError {
                message: "「返す」は予約されたキーワードであり、手順名として使用できません".into(),
                span: start,
            });
        }

        let params = if matches!(self.current_kind(), TokenKind::LBracket) {
            self.parse_params()?
        } else {
            Vec::new()
        };

        let return_type = self.parse_optional_return_type()?;

        self.skip_newlines();
        let body = self.parse_block()?;

        let span = start.merge(body.span);
        Ok(Stmt {
            kind: StmtKind::ProcDef {
                name,
                params,
                return_type,
                body,
                is_public: false,
            },
            span,
        })
    }

    fn parse_optional_return_type(&mut self) -> Result<Option<String>, ParseError> {
        if !matches!(self.current_kind(), TokenKind::Arrow) {
            return Ok(None);
        }
        self.advance();
        match self.current_kind().clone() {
            TokenKind::Identifier(name) => {
                self.advance();
                Ok(Some(name))
            }
            _ => Err(ParseError {
                message: "戻り型の指定には型名が必要です".into(),
                span: self.current_span(),
            }),
        }
    }

    fn parse_params(&mut self) -> Result<Vec<Param>, ParseError> {
        self.eat(&TokenKind::LBracket)?;
        let mut params = Vec::new();

        loop {
            if matches!(self.current_kind(), TokenKind::RBracket) {
                break;
            }

            let param_start = self.current_span();

            // 短縮形 `:を` or 通常形 `名前:を`
            let name = if matches!(self.current_kind(), TokenKind::Colon) {
                None
            } else {
                let (n, _) = self.eat_identifier()?;
                Some(n)
            };

            self.eat(&TokenKind::Colon)?;

            let particle = match self.current_kind() {
                TokenKind::Particle(p) => {
                    let p = *p;
                    self.advance();
                    p
                }
                _ => {
                    return Err(ParseError {
                        message: "引数宣言には助詞が必要です".into(),
                        span: self.current_span(),
                    });
                }
            };

            let param_span = param_start.merge(self.tokens[self.pos - 1].span);
            params.push(Param {
                name,
                particle,
                span: param_span,
            });

            if matches!(self.current_kind(), TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        self.eat(&TokenKind::RBracket)?;
        Ok(params)
    }

    // === 組定義 ===

    fn parse_struct_def(&mut self, name: String, start: Span) -> Result<Stmt, ParseError> {
        self.skip_newlines();
        self.eat(&TokenKind::Indent)?;

        let mut fields = Vec::new();
        let methods = Vec::new();

        while !matches!(self.current_kind(), TokenKind::Dedent | TokenKind::Eof) {
            self.skip_newlines();
            if matches!(self.current_kind(), TokenKind::Dedent | TokenKind::Eof) {
                break;
            }

            if !matches!(self.current_kind(), TokenKind::Identifier(_)) {
                return Err(ParseError {
                    message: "組の本体には `名前は 型名` 形式のフィールド宣言のみ記述できます"
                        .into(),
                    span: self.current_span(),
                });
            }

            let field_start = self.current_span();
            let (raw_name, _) = self.eat_identifier()?;
            let fname = if matches!(self.current_kind(), TokenKind::Ha) {
                self.advance();
                raw_name
            } else if let Some(stripped) = raw_name.strip_suffix('は') {
                if stripped.is_empty() {
                    return Err(ParseError {
                        message: "組の本体には `名前は 型名` 形式のフィールド宣言のみ記述できます"
                            .into(),
                        span: field_start,
                    });
                }
                stripped.to_string()
            } else {
                return Err(ParseError {
                    message: "組の本体には `名前は 型名` 形式のフィールド宣言のみ記述できます"
                        .into(),
                    span: field_start,
                });
            };
            let type_name = if let TokenKind::Identifier(t) = self.current_kind().clone() {
                self.advance();
                Some(t)
            } else {
                return Err(ParseError {
                    message: "フィールド宣言には型名が必要です".into(),
                    span: self.current_span(),
                });
            };
            let field_span = field_start.merge(self.tokens[self.pos - 1].span);
            fields.push(FieldDef {
                name: fname,
                type_name,
                span: field_span,
            });
            self.skip_newlines();
        }

        let end_span = self.current_span();
        if matches!(self.current_kind(), TokenKind::Dedent) {
            self.advance();
        }

        if fields.is_empty() {
            return Err(ParseError {
                message: "組には1つ以上のフィールド宣言が必要です".into(),
                span: start,
            });
        }

        Ok(Stmt {
            kind: StmtKind::StructDef {
                name,
                fields,
                methods,
                is_public: false,
            },
            span: start.merge(end_span),
        })
    }

    // === 特性定義 ===

    fn parse_trait_def(&mut self, name: String, start: Span) -> Result<Stmt, ParseError> {
        self.skip_newlines();
        self.eat(&TokenKind::Indent)?;

        let mut methods = Vec::new();
        while !matches!(self.current_kind(), TokenKind::Dedent | TokenKind::Eof) {
            self.skip_newlines();
            if matches!(self.current_kind(), TokenKind::Dedent | TokenKind::Eof) {
                break;
            }
            if !(matches!(self.current_kind(), TokenKind::Identifier(_))
                && matches!(self.peek_ahead(1), TokenKind::ToIu)
                && matches!(self.peek_ahead(2), TokenKind::Tejun))
            {
                return Err(ParseError {
                    message: "特性の本体には `名前 という 手順` 形式のシグネチャのみ記述できます"
                        .into(),
                    span: self.current_span(),
                });
            }

            let method_start = self.current_span();
            let (method_name, _) = self.eat_identifier()?;
            self.eat(&TokenKind::ToIu)?;
            self.eat(&TokenKind::Tejun)?;
            let params = if matches!(self.current_kind(), TokenKind::LBracket) {
                self.parse_params()?
            } else {
                Vec::new()
            };
            let return_type = self.parse_optional_return_type()?;
            let method_end = self.tokens[self.pos.saturating_sub(1)].span;
            methods.push(Stmt {
                kind: StmtKind::ProcDef {
                    name: method_name,
                    params,
                    return_type,
                    body: Block {
                        statements: Vec::new(),
                        span: method_end,
                    },
                    is_public: false,
                },
                span: method_start.merge(method_end),
            });
            self.skip_newlines();
        }

        let end_span = self.current_span();
        if matches!(self.current_kind(), TokenKind::Dedent) {
            self.advance();
        }

        if methods.is_empty() {
            return Err(ParseError {
                message: "特性には1つ以上のメソッドシグネチャが必要です".into(),
                span: start,
            });
        }

        Ok(Stmt {
            kind: StmtKind::TraitDef {
                name,
                methods,
                is_public: false,
            },
            span: start.merge(end_span),
        })
    }

    // === 条件分岐 ===

    fn parse_if_statement(&mut self, start: Span) -> Result<Stmt, ParseError> {
        let if_expr = self.parse_if_expr(start)?;
        let span = if_expr.span;
        Ok(Stmt {
            kind: StmtKind::ExprStmt(if_expr),
            span,
        })
    }

    fn parse_if_expr(&mut self, start: Span) -> Result<Expr, ParseError> {
        self.eat(&TokenKind::Moshi)?;
        let condition = self.parse_expr()?;
        self.eat(&TokenKind::Naraba)?;
        self.skip_newlines();
        let then_block = self.parse_block()?;

        let mut elif_clauses = Vec::new();
        let mut else_block = None;

        self.skip_newlines();

        while matches!(self.current_kind(), TokenKind::Moshikuha) {
            self.advance();
            let elif_cond = self.parse_expr()?;
            self.eat(&TokenKind::Naraba)?;
            self.skip_newlines();
            let elif_block = self.parse_block()?;
            elif_clauses.push((elif_cond, elif_block));
            self.skip_newlines();
        }

        if matches!(self.current_kind(), TokenKind::SouDenakereba) {
            self.advance();
            self.skip_newlines();
            else_block = Some(self.parse_block()?);
        }

        let end_span = self.tokens[self.pos.saturating_sub(1)].span;

        Ok(Expr {
            kind: ExprKind::If {
                condition: Box::new(condition),
                then_block,
                elif_clauses,
                else_block,
            },
            span: start.merge(end_span),
        })
    }

    // === 試す ===

    fn parse_try_expr(&mut self, start: Span) -> Result<Expr, ParseError> {
        self.eat(&TokenKind::Tamesu)?;
        self.skip_newlines();
        let body = self.parse_block()?;

        self.skip_newlines();
        let mut catch_param = None;
        let mut catch_body = None;

        if matches!(self.current_kind(), TokenKind::ShippaiShitaBaai) {
            self.advance();
            if matches!(self.current_kind(), TokenKind::LBracket) {
                self.advance();
                let (param_name, _) = self.eat_identifier()?;
                self.eat(&TokenKind::Colon)?;
                // catch 引数助詞は `で` 固定
                match self.current_kind() {
                    TokenKind::Particle(Particle::De) => {
                        self.advance();
                    }
                    _ => {
                        return Err(ParseError {
                            message: "失敗した場合の引数助詞は `で` のみ使用できます".into(),
                            span: self.current_span(),
                        });
                    }
                }
                self.eat(&TokenKind::RBracket)?;
                catch_param = Some(param_name);
            }
            self.skip_newlines();
            catch_body = Some(self.parse_block()?);
        }

        self.skip_newlines();
        let mut finally_body = None;
        if matches!(self.current_kind(), TokenKind::KanarazuOkonau) {
            self.advance();
            self.skip_newlines();
            finally_body = Some(self.parse_block()?);
        }

        let end_span = self.tokens[self.pos.saturating_sub(1)].span;

        Ok(Expr {
            kind: ExprKind::TryCatch {
                body,
                catch_param,
                catch_body,
                finally_body,
            },
            span: start.merge(end_span),
        })
    }

    fn try_parse_loop_statement(&mut self, start: Span) -> Result<Option<Stmt>, ParseError> {
        let checkpoint = self.pos;

        if let Some(stmt) = self.try_parse_times_loop(start)? {
            return Ok(Some(stmt));
        }
        self.pos = checkpoint;

        if let Some(stmt) = self.try_parse_range_loop(start)? {
            return Ok(Some(stmt));
        }
        self.pos = checkpoint;

        if let Some(stmt) = self.try_parse_foreach_loop(start)? {
            return Ok(Some(stmt));
        }
        self.pos = checkpoint;

        if let Some(stmt) = self.try_parse_while_loop(start)? {
            return Ok(Some(stmt));
        }
        self.pos = checkpoint;

        Ok(None)
    }

    fn try_parse_times_loop(&mut self, start: Span) -> Result<Option<Stmt>, ParseError> {
        let checkpoint = self.pos;
        let count = match self.current_kind() {
            TokenKind::Integer(_) | TokenKind::Float(_) | TokenKind::Identifier(_) => {
                self.parse_primary()?
            }
            _ => return Ok(None),
        };

        let mut matched_counter = false;
        match self.current_kind().clone() {
            TokenKind::Counter(c) if c == "回" => {
                matched_counter = true;
                self.advance();
            }
            TokenKind::Kai => {
                matched_counter = true;
                self.advance();
            }
            _ => {}
        }

        if !matched_counter {
            self.pos = checkpoint;
            return Ok(None);
        }

        self.eat(&TokenKind::KuriKaesu)?;
        let var = self.parse_loop_var()?;
        self.skip_newlines();
        let body = self.parse_block()?;
        let end = body.span;
        Ok(Some(Stmt {
            kind: StmtKind::ExprStmt(Expr {
                kind: ExprKind::Loop(Box::new(LoopKind::Times { count, var, body })),
                span: start.merge(end),
            }),
            span: start.merge(end),
        }))
    }

    fn try_parse_range_loop(&mut self, start: Span) -> Result<Option<Stmt>, ParseError> {
        let checkpoint = self.pos;
        let from = match self.current_kind() {
            TokenKind::Integer(_)
            | TokenKind::Float(_)
            | TokenKind::Identifier(_)
            | TokenKind::Kore
            | TokenKind::Sore
            | TokenKind::Are => self.parse_primary()?,
            _ => return Ok(None),
        };

        if !matches!(self.current_kind(), TokenKind::Particle(Particle::Kara)) {
            self.pos = checkpoint;
            return Ok(None);
        }
        self.advance(); // から

        let to = self.parse_primary()?;
        self.eat(&TokenKind::Particle(Particle::Made))?;
        self.eat(&TokenKind::KuriKaesu)?;
        let var = self.parse_loop_var()?;
        self.skip_newlines();
        let body = self.parse_block()?;
        let end = body.span;
        Ok(Some(Stmt {
            kind: StmtKind::ExprStmt(Expr {
                kind: ExprKind::Loop(Box::new(LoopKind::Range {
                    from,
                    to,
                    var,
                    body,
                })),
                span: start.merge(end),
            }),
            span: start.merge(end),
        }))
    }

    fn try_parse_while_loop(&mut self, start: Span) -> Result<Option<Stmt>, ParseError> {
        let checkpoint = self.pos;
        let condition = self.parse_expr()?;

        if !matches!(self.current_kind(), TokenKind::Aida) {
            self.pos = checkpoint;
            return Ok(None);
        }
        self.advance(); // 間
        self.eat(&TokenKind::KuriKaesu)?;
        self.skip_newlines();
        let body = self.parse_block()?;
        let end = body.span;
        Ok(Some(Stmt {
            kind: StmtKind::ExprStmt(Expr {
                kind: ExprKind::Loop(Box::new(LoopKind::While { condition, body })),
                span: start.merge(end),
            }),
            span: start.merge(end),
        }))
    }

    fn try_parse_foreach_loop(&mut self, start: Span) -> Result<Option<Stmt>, ParseError> {
        let checkpoint = self.pos;
        let iterable = match self.current_kind() {
            TokenKind::Identifier(_) | TokenKind::Kore | TokenKind::Sore | TokenKind::Are => {
                self.parse_primary()?
            }
            _ => return Ok(None),
        };

        if !matches!(self.current_kind(), TokenKind::AccessParticle) {
            self.pos = checkpoint;
            return Ok(None);
        }
        self.advance();
        if !matches!(self.current_kind(), TokenKind::SorezoreNiTsuite) {
            self.pos = checkpoint;
            return Ok(None);
        }
        self.advance();

        let var = self.parse_loop_var()?.ok_or(ParseError {
            message: "「それぞれについて」には繰り返し変数が必要です".into(),
            span: self.current_span(),
        })?;
        self.skip_newlines();
        let body = self.parse_block()?;
        let end = body.span;
        Ok(Some(Stmt {
            kind: StmtKind::ExprStmt(Expr {
                kind: ExprKind::Loop(Box::new(LoopKind::ForEach {
                    iterable,
                    var,
                    body,
                })),
                span: start.merge(end),
            }),
            span: start.merge(end),
        }))
    }

    fn parse_loop_var(&mut self) -> Result<Option<String>, ParseError> {
        if !matches!(self.current_kind(), TokenKind::LBracket) {
            return Ok(None);
        }
        self.advance();
        let (name, _) = self.eat_identifier()?;
        self.eat(&TokenKind::RBracket)?;
        Ok(Some(name))
    }

    // === ブロック ===

    fn parse_block(&mut self) -> Result<Block, ParseError> {
        let start = self.current_span();
        self.eat(&TokenKind::Indent)?;

        let mut statements = Vec::new();
        let mut safety_steps = 0usize;
        while !matches!(self.current_kind(), TokenKind::Dedent | TokenKind::Eof) {
            safety_steps = safety_steps.saturating_add(1);
            if safety_steps > self.step_limit {
                self.errors.push(ParseError {
                    message: format!(
                        "構文解析回数が上限を超えたため停止しました（上限: {}）",
                        self.step_limit
                    ),
                    span: self.current_span(),
                });
                break;
            }

            self.skip_newlines();
            if matches!(self.current_kind(), TokenKind::Dedent | TokenKind::Eof) {
                break;
            }
            let before_pos = self.pos;
            match self.parse_statement() {
                Ok(stmt) => statements.push(stmt),
                Err(e) => {
                    self.errors.push(e);
                    self.recover_to_next_statement();
                }
            }
            self.skip_newlines();

            if self.pos == before_pos
                && !matches!(self.current_kind(), TokenKind::Dedent | TokenKind::Eof)
            {
                self.errors.push(ParseError {
                    message: "構文解析が進行しないため、この位置で解析を打ち切りました".into(),
                    span: self.current_span(),
                });
                self.advance();
            }
        }

        let end_span = self.current_span();
        if matches!(self.current_kind(), TokenKind::Dedent) {
            self.advance();
        }

        Ok(Block {
            statements,
            span: start.merge(end_span),
        })
    }

    // === 式のパース ===

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_logical_or()
    }

    /// 論理 OR: `a または b`
    fn parse_logical_or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_logical_and()?;

        while matches!(self.current_kind(), TokenKind::Mataha) {
            self.advance();
            let right = self.parse_logical_and()?;
            let span = left.span.merge(right.span);
            left = Expr {
                kind: ExprKind::Logical {
                    op: LogicalOp::Or,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            };
        }

        Ok(left)
    }

    /// 論理 AND: `a かつ b`
    fn parse_logical_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_comparison_or_call()?;

        while matches!(self.current_kind(), TokenKind::Katsu) {
            self.advance();
            let right = self.parse_comparison_or_call()?;
            let span = left.span.merge(right.span);
            left = Expr {
                kind: ExprKind::Logical {
                    op: LogicalOp::And,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            };
        }

        Ok(left)
    }

    /// 比較式 or 助詞式（手順呼び出し）
    fn parse_comparison_or_call(&mut self) -> Result<Expr, ParseError> {
        // まずプライマリ式をパースし、その後に助詞・比較・呼び出しがあるかを見る
        let mut expr = self.parse_primary()?;

        // 助数詞付き数値
        if let TokenKind::Counter(c) = self.current_kind().clone() {
            let end_span = self.current_span();
            self.advance();
            let span = expr.span.merge(end_span);
            expr = Expr {
                kind: ExprKind::WithCounter {
                    value: Box::new(expr),
                    counter: c,
                },
                span,
            };
        }

        if let Some(chain_expr) = self.try_parse_te_chain_expr(expr.clone())? {
            return Ok(chain_expr);
        }

        // 助詞が続く場合 → 助詞式（呼び出し）構築
        if matches!(
            self.current_kind(),
            TokenKind::Particle(_) | TokenKind::AccessParticle
        ) && !matches!(self.current_kind(), TokenKind::Particle(Particle::Ga))
        {
            expr = self.parse_particle_expr(expr)?;
        }

        // 比較は専用パスで解釈する
        if matches!(self.current_kind(), TokenKind::Particle(Particle::Ga)) {
            expr = self.parse_comparison_tail(expr)?;
        }

        // match 式: `値は どれかを調べる ...`
        if matches!(self.current_kind(), TokenKind::DorekaWoShiraberu) {
            expr = self.parse_match_tail(expr)?;
        }

        // `でない` (NOT)
        if matches!(self.current_kind(), TokenKind::DeNai) {
            let end_span = self.current_span();
            let span = expr.span.merge(end_span);
            self.advance();
            return Ok(Expr {
                kind: ExprKind::UnaryOp {
                    op: UnaryOp::Not,
                    operand: Box::new(expr),
                },
                span,
            });
        }

        Ok(expr)
    }

    fn try_parse_te_chain_expr(&mut self, head_expr: Expr) -> Result<Option<Expr>, ParseError> {
        let checkpoint = self.pos;
        let chain_start = head_expr.span;

        if !matches!(self.current_kind(), TokenKind::Particle(Particle::Wo)) {
            self.pos = checkpoint;
            return Ok(None);
        }

        let Some((head_call, head_is_te)) =
            self.try_parse_chain_call_step(Some(head_expr), true)?
        else {
            self.pos = checkpoint;
            return Ok(None);
        };

        if !head_is_te {
            self.pos = checkpoint;
            return Ok(None);
        }

        if !matches!(self.current_kind(), TokenKind::Comma) {
            if matches!(
                self.current_kind(),
                TokenKind::Identifier(_)
                    | TokenKind::HyoujiSuru
                    | TokenKind::NyuuryokuSuru
                    | TokenKind::BunkiShite
            ) {
                return Err(ParseError {
                    message: "て形連鎖には「、」が必要です".into(),
                    span: self.current_span(),
                });
            }
            self.pos = checkpoint;
            return Ok(None);
        }

        self.advance(); // first comma
        let mut steps = vec![head_call];

        loop {
            self.skip_newlines();

            if matches!(self.current_kind(), TokenKind::BunkiShite) {
                let branch_step = self.parse_chain_branch_step()?;
                steps.push(branch_step);

                self.skip_newlines();
                if matches!(self.current_kind(), TokenKind::Comma) {
                    self.advance();
                    continue;
                }

                let Some((call, is_te)) = self.try_parse_chain_call_step(None, false)? else {
                    return Err(ParseError {
                        message: "分岐して の後には終止形ステップが必要です".into(),
                        span: self.current_span(),
                    });
                };
                if is_te {
                    return Err(ParseError {
                        message: "て形連鎖の最後は終止形が必要です".into(),
                        span: self.current_span(),
                    });
                }
                steps.push(call);
                break;
            }

            let Some((call, is_te)) = self.try_parse_chain_call_step(None, false)? else {
                return Err(ParseError {
                    message: "て形連鎖のステップが必要です".into(),
                    span: self.current_span(),
                });
            };
            steps.push(call);

            if is_te {
                if !matches!(self.current_kind(), TokenKind::Comma) {
                    return Err(ParseError {
                        message: "て形連鎖には「、」が必要です".into(),
                        span: self.current_span(),
                    });
                }
                self.advance();
                continue;
            }
            break;
        }

        let end_span = self.tokens[self.pos.saturating_sub(1)].span;
        Ok(Some(Expr {
            kind: ExprKind::TeChain { steps },
            span: chain_start.merge(end_span),
        }))
    }

    fn try_parse_chain_call_step(
        &mut self,
        first_arg: Option<Expr>,
        allow_first_wo: bool,
    ) -> Result<Option<(ChainStep, bool)>, ParseError> {
        let checkpoint = self.pos;
        let mut args: Vec<ParticleArg> = Vec::new();
        let has_first_arg = first_arg.is_some();

        if let Some(expr) = first_arg {
            match self.current_kind().clone() {
                TokenKind::Particle(p) => {
                    if !allow_first_wo && p == Particle::Wo {
                        return Err(ParseError {
                            message: "て形ステップでは明示「を」は使えません".into(),
                            span: self.current_span(),
                        });
                    }
                    let particle_span = self.current_span();
                    self.advance();
                    args.push(ParticleArg {
                        value: expr.clone(),
                        particle: p,
                        span: expr.span.merge(particle_span),
                    });
                }
                _ => {
                    self.pos = checkpoint;
                    return Ok(None);
                }
            }
        }

        let parse_additional_args = !has_first_arg || !allow_first_wo;
        if parse_additional_args {
            loop {
                if !self.is_primary_start_for_chain() {
                    break;
                }
                if matches!(self.current_kind(), TokenKind::Identifier(_))
                    && matches!(self.peek_ahead(1), TokenKind::Particle(Particle::De))
                    && matches!(
                        self.peek_ahead(2),
                        TokenKind::Comma | TokenKind::Newline | TokenKind::Dedent | TokenKind::Eof
                    )
                {
                    break;
                }

                let arg_checkpoint = self.pos;
                let arg_expr = self.parse_primary()?;
                let TokenKind::Particle(particle) = self.current_kind().clone() else {
                    self.pos = arg_checkpoint;
                    break;
                };
                if particle == Particle::Wo {
                    return Err(ParseError {
                        message: "て形ステップでは明示「を」は使えません".into(),
                        span: self.current_span(),
                    });
                }
                let particle_span = self.current_span();
                self.advance();
                args.push(ParticleArg {
                    value: arg_expr.clone(),
                    particle,
                    span: arg_expr.span.merge(particle_span),
                });
            }
        }

        let (callee, is_te, end_span) = match self.current_kind().clone() {
            TokenKind::Identifier(name) => {
                let start_span = self.current_span();
                self.advance();
                if matches!(self.current_kind(), TokenKind::Particle(Particle::De))
                    && matches!(
                        self.peek_ahead(1),
                        TokenKind::Comma
                            | TokenKind::Newline
                            | TokenKind::Dedent
                            | TokenKind::Eof
                            | TokenKind::BunkiShite
                    )
                {
                    let end = self.current_span();
                    self.advance();
                    (format!("{name}で"), true, start_span.merge(end))
                } else if name.ends_with('て') {
                    (name, true, start_span)
                } else {
                    (name, false, start_span)
                }
            }
            TokenKind::HyoujiSuru => {
                let s = self.current_span();
                self.advance();
                ("表示する".to_string(), false, s)
            }
            TokenKind::NyuuryokuSuru => {
                let s = self.current_span();
                self.advance();
                ("入力する".to_string(), false, s)
            }
            TokenKind::Kaeru => {
                let s = self.current_span();
                self.advance();
                ("変える".to_string(), false, s)
            }
            TokenKind::Tsukau => {
                let s = self.current_span();
                self.advance();
                ("使う".to_string(), false, s)
            }
            TokenKind::Tsukuru => {
                let s = self.current_span();
                self.advance();
                ("作る".to_string(), false, s)
            }
            _ => {
                self.pos = checkpoint;
                return Ok(None);
            }
        };

        let _ = end_span;
        Ok(Some((ChainStep::Call { callee, args }, is_te)))
    }

    fn parse_chain_branch_step(&mut self) -> Result<ChainStep, ParseError> {
        self.eat(&TokenKind::BunkiShite)?;
        self.skip_newlines();
        let branch_block = self.parse_block()?;
        if branch_block.statements.len() != 1 {
            return Err(ParseError {
                message: "分岐して ブロックには分岐式を1つだけ書いてください".into(),
                span: branch_block.span,
            });
        }

        let StmtKind::ExprStmt(if_expr) = &branch_block.statements[0].kind else {
            return Err(ParseError {
                message: "分岐して ブロックには「もし ...」式が必要です".into(),
                span: branch_block.span,
            });
        };
        if !matches!(if_expr.kind, ExprKind::If { .. }) {
            return Err(ParseError {
                message: "分岐して ブロックには「もし ...」式が必要です".into(),
                span: if_expr.span,
            });
        }
        if !self.if_expr_returns_value(if_expr) {
            return Err(ParseError {
                message: "分岐して の各分岐は値を返す必要があります".into(),
                span: if_expr.span,
            });
        }

        Ok(ChainStep::Branch {
            if_expr: if_expr.clone(),
        })
    }

    fn if_expr_returns_value(&self, if_expr: &Expr) -> bool {
        let ExprKind::If {
            then_block,
            elif_clauses,
            else_block,
            ..
        } = &if_expr.kind
        else {
            return false;
        };

        if !self.block_ends_with_return(then_block) {
            return false;
        }
        if elif_clauses
            .iter()
            .any(|(_, block)| !self.block_ends_with_return(block))
        {
            return false;
        }
        match else_block {
            Some(block) => self.block_ends_with_return(block),
            None => false,
        }
    }

    fn block_ends_with_return(&self, block: &Block) -> bool {
        matches!(
            block.statements.last().map(|s| &s.kind),
            Some(StmtKind::Return(_))
        )
    }

    fn is_primary_start_for_chain(&self) -> bool {
        matches!(
            self.current_kind(),
            TokenKind::Integer(_)
                | TokenKind::Float(_)
                | TokenKind::String(_)
                | TokenKind::StringInterpStart
                | TokenKind::Bool(_)
                | TokenKind::Kore
                | TokenKind::Sore
                | TokenKind::Are
                | TokenKind::Kou
                | TokenKind::Koko
                | TokenKind::Soko
                | TokenKind::DoreDemoNai
                | TokenKind::LBracket
                | TokenKind::LBrace
                | TokenKind::LParen
                | TokenKind::Moshi
                | TokenKind::Tamesu
                | TokenKind::Identifier(_)
        )
    }

    fn parse_match_tail(&mut self, target: Expr) -> Result<Expr, ParseError> {
        let start = target.span;
        self.eat(&TokenKind::DorekaWoShiraberu)?;
        self.skip_newlines();
        self.eat(&TokenKind::Indent)?;

        let mut arms = Vec::new();
        while !matches!(self.current_kind(), TokenKind::Dedent | TokenKind::Eof) {
            self.skip_newlines();
            if matches!(self.current_kind(), TokenKind::Dedent | TokenKind::Eof) {
                break;
            }

            let arm_start = self.current_span();
            let pattern = if matches!(self.current_kind(), TokenKind::DoreDemoNaiBaai) {
                self.advance();
                Pattern::Default
            } else {
                let p = self.parse_match_pattern()?;
                self.eat(&TokenKind::AccessParticle)?;
                self.eat(&TokenKind::NoBaai)?;
                p
            };

            self.skip_newlines();
            let body = self.parse_block()?;
            let arm_span = arm_start.merge(body.span);
            arms.push(MatchArm {
                pattern,
                body,
                span: arm_span,
            });
            self.skip_newlines();
        }

        let end_span = self.current_span();
        if matches!(self.current_kind(), TokenKind::Dedent) {
            self.advance();
        }

        Ok(Expr {
            kind: ExprKind::Match {
                target: Box::new(target),
                arms,
            },
            span: start.merge(end_span),
        })
    }

    fn parse_match_pattern(&mut self) -> Result<Pattern, ParseError> {
        match self.current_kind().clone() {
            TokenKind::Doreka => {
                self.advance();
                Ok(Pattern::Wildcard)
            }
            TokenKind::Identifier(name) => {
                self.advance();
                Ok(Pattern::Binding(name))
            }
            TokenKind::LBracket => self.parse_match_list_pattern(),
            TokenKind::Integer(_)
            | TokenKind::Float(_)
            | TokenKind::String(_)
            | TokenKind::Bool(_) => {
                let lit = self.parse_primary()?;
                Ok(Pattern::Literal(lit))
            }
            _ => Err(ParseError {
                message: "match の場合パターンが必要です".into(),
                span: self.current_span(),
            }),
        }
    }

    fn parse_match_list_pattern(&mut self) -> Result<Pattern, ParseError> {
        self.eat(&TokenKind::LBracket)?;
        let mut items = Vec::new();

        while !matches!(self.current_kind(), TokenKind::RBracket | TokenKind::Eof) {
            let item = match self.current_kind().clone() {
                TokenKind::Doreka => {
                    self.advance();
                    Pattern::Wildcard
                }
                TokenKind::Identifier(name) => {
                    self.advance();
                    Pattern::Binding(name)
                }
                TokenKind::Integer(_)
                | TokenKind::Float(_)
                | TokenKind::String(_)
                | TokenKind::Bool(_) => {
                    let lit = self.parse_primary()?;
                    Pattern::Literal(lit)
                }
                _ => {
                    return Err(ParseError {
                        message: "分解パターン要素が必要です".into(),
                        span: self.current_span(),
                    });
                }
            };
            items.push(item);
            if matches!(self.current_kind(), TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        self.eat(&TokenKind::RBracket)?;
        Ok(Pattern::List(items))
    }

    fn parse_comparison_tail(&mut self, left: Expr) -> Result<Expr, ParseError> {
        self.eat(&TokenKind::Particle(Particle::Ga))?;
        let right = self.parse_comparison_operand()?;
        let (op, end_span) = match self.current_kind().clone() {
            TokenKind::Identifier(w) if w == "より大きい" => {
                let s = self.current_span();
                self.advance();
                (CompOp::Gt, s)
            }
            TokenKind::Identifier(w) if w == "より小さい" => {
                let s = self.current_span();
                self.advance();
                (CompOp::Lt, s)
            }
            TokenKind::Identifier(w) if w == "と等しい" => {
                let s = self.current_span();
                self.advance();
                (CompOp::Eq, s)
            }
            TokenKind::Identifier(w) if w == "と等しくない" => {
                let s = self.current_span();
                self.advance();
                (CompOp::Ne, s)
            }
            TokenKind::Particle(Particle::Yori) => {
                self.advance();
                match self.current_kind().clone() {
                    TokenKind::Identifier(w) if w == "大きい" => {
                        let s = self.current_span();
                        self.advance();
                        (CompOp::Gt, s)
                    }
                    TokenKind::Identifier(w) if w == "小さい" => {
                        let s = self.current_span();
                        self.advance();
                        (CompOp::Lt, s)
                    }
                    _ => {
                        return Err(ParseError {
                            message: "「より」の後には「大きい」または「小さい」が必要です".into(),
                            span: self.current_span(),
                        });
                    }
                }
            }
            TokenKind::Particle(Particle::To) => {
                self.advance();
                match self.current_kind().clone() {
                    TokenKind::Identifier(w) if w == "等しい" => {
                        let s = self.current_span();
                        self.advance();
                        (CompOp::Eq, s)
                    }
                    TokenKind::Identifier(w) if w == "等しくない" => {
                        let s = self.current_span();
                        self.advance();
                        (CompOp::Ne, s)
                    }
                    _ => {
                        return Err(ParseError {
                            message: "「と」の後には「等しい」または「等しくない」が必要です"
                                .into(),
                            span: self.current_span(),
                        });
                    }
                }
            }
            TokenKind::Identifier(w) if w == "以上" => {
                let s = self.current_span();
                self.advance();
                (CompOp::Ge, s)
            }
            TokenKind::Identifier(w) if w == "以下" => {
                let s = self.current_span();
                self.advance();
                (CompOp::Le, s)
            }
            _ => {
                return Err(ParseError {
                    message: "比較式は「Aが Bより大きい|小さい」「Aが Bと等しい|等しくない」「Aが B以上|以下」の形が必要です".into(),
                    span: self.current_span(),
                });
            }
        };
        let span = left.span.merge(end_span);
        Ok(Expr {
            kind: ExprKind::Comparison {
                op,
                left: Box::new(left),
                right: Box::new(right),
            },
            span,
        })
    }

    fn parse_comparison_operand(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;
        if let TokenKind::Counter(c) = self.current_kind().clone() {
            let end_span = self.current_span();
            let span = expr.span.merge(end_span);
            self.advance();
            expr = Expr {
                kind: ExprKind::WithCounter {
                    value: Box::new(expr),
                    counter: c,
                },
                span,
            };
        }

        if matches!(self.current_kind(), TokenKind::AccessParticle) {
            expr = self.parse_particle_expr(expr)?;
        }
        Ok(expr)
    }

    /// 助詞式: `(式 助詞)+ 動詞`
    /// または算術のパターンを認識
    fn parse_particle_expr(&mut self, first_expr: Expr) -> Result<Expr, ParseError> {
        let mut args: Vec<ParticleArg> = Vec::new();
        let mut current_expr = first_expr;

        loop {
            match self.current_kind() {
                TokenKind::Particle(p) => {
                    if *p == Particle::Ga && args.is_empty() {
                        return Ok(current_expr);
                    }

                    let particle = *p;
                    let particle_span = self.current_span();
                    self.advance();

                    // 通常の助詞引数を追加
                    let arg_span = current_expr.span.merge(particle_span);
                    args.push(ParticleArg {
                        value: current_expr,
                        particle,
                        span: arg_span,
                    });

                    // 次の式をパース（動詞位置かもしれない）
                    if self.is_verb_position() {
                        break;
                    }

                    // 「返す」キーワードが来た場合はここでループ終了
                    if matches!(self.current_kind(), TokenKind::Kaesu) {
                        break;
                    }

                    current_expr = self.parse_primary()?;
                    if let TokenKind::Counter(c) = self.current_kind().clone() {
                        let end_span = self.current_span();
                        self.advance();
                        let span = current_expr.span.merge(end_span);
                        current_expr = Expr {
                            kind: ExprKind::WithCounter {
                                value: Box::new(current_expr),
                                counter: c,
                            },
                            span,
                        };
                    }
                }
                TokenKind::AccessParticle => {
                    let _particle_span = self.current_span();
                    self.advance();

                    // 次が識別子の場合は属性アクセス or 算術パターン
                    if let TokenKind::Identifier(prop) = self.current_kind().clone() {
                        let prop_span = self.current_span();
                        self.advance();

                        if matches!(current_expr.kind, ExprKind::StringLiteral(_))
                            && prop.ends_with("する")
                        {
                            return Err(ParseError {
                                message: "「の」はアクセス助詞です。役割助詞としては使えません"
                                    .into(),
                                span: prop_span,
                            });
                        }

                        // 算術チェック: `aとbの和/差/積` パターン（固定糖衣構文）
                        if is_arithmetic_word(&prop) && !args.is_empty() {
                            let op = arithmetic_op(&prop).unwrap();
                            let right = args.pop().unwrap();
                            let span = right.value.span.merge(prop_span);
                            // オペランド順序: 左=第1引数(との前), 右=第2引数(との後)
                            current_expr = Expr {
                                kind: ExprKind::BinaryOp {
                                    op,
                                    left: Box::new(right.value),
                                    right: Box::new(current_expr),
                                },
                                span,
                            };
                            continue;
                        }

                        // 通常の属性アクセス
                        let span = current_expr.span.merge(prop_span);
                        current_expr = Expr {
                            kind: ExprKind::PropertyAccess {
                                object: Box::new(current_expr),
                                property: prop,
                            },
                            span,
                        };
                        continue;
                    } else if let TokenKind::Integer(_) = self.current_kind() {
                        // `リストの3番目` のようなインデックスアクセス
                        let idx_expr = self.parse_primary()?;
                        if let TokenKind::Identifier(s) = self.current_kind() {
                            if s == "番目" || s == "番目以降" {
                                self.advance();
                            }
                        }
                        let span = current_expr.span.merge(idx_expr.span);
                        current_expr = Expr {
                            kind: ExprKind::PropertyAccess {
                                object: Box::new(current_expr),
                                property: "__index".to_string(),
                            },
                            span,
                        };
                        continue;
                    }

                    // 「の」の後に識別子も整数もない場合
                    if args.is_empty() {
                        return Ok(current_expr);
                    }
                    break;
                }
                _ => {
                    // 助詞が来なかった場合、current_exprをそのまま返す
                    if args.is_empty() {
                        return Ok(current_expr);
                    }
                    break;
                }
            }
        }

        if args.is_empty() {
            // ループ内で全てが処理済み（助詞引数として消費された）
            // ここには到達しないが、コンパイラの安全性のため
            return Err(ParseError {
                message: "式が期待されましたが、見つかりませんでした".into(),
                span: self.current_span(),
            });
        }

        // 「返す」キーワードが続く場合、助詞引数の値を返す
        // （呼び出し側で Return 文に変換される）
        if matches!(self.current_kind(), TokenKind::Kaesu) {
            if let Some(arg) = args.pop() {
                return Ok(arg.value);
            }
        }

        // `<値を ...> 手順と 動詞` パターンを、手順呼び出しの結果を動詞の `と` 引数に渡す形に解釈する
        if args.len() >= 2 {
            if let Some(last) = args.last() {
                if last.particle == Particle::To
                    && matches!(last.value.kind, ExprKind::Identifier(_))
                    && args[..args.len() - 1]
                        .iter()
                        .all(|a| a.particle != Particle::To)
                {
                    let ExprKind::Identifier(func_name) = &last.value.kind else {
                        unreachable!("checked by matches! above");
                    };

                    let call_args = args[..args.len() - 1].to_vec();
                    let call_span = call_args
                        .first()
                        .map(|a| a.span)
                        .unwrap()
                        .merge(last.value.span);
                    let call_expr = Expr {
                        kind: ExprKind::Call {
                            callee: func_name.clone(),
                            args: call_args,
                        },
                        span: call_span,
                    };
                    let new_span = call_span.merge(last.span);
                    args = vec![ParticleArg {
                        value: call_expr,
                        particle: Particle::To,
                        span: new_span,
                    }];
                }
            }
        }

        // 動詞（呼び出し先）をパース
        // 現在位置が動詞（識別子 or キーワード動詞）ならそれを消費
        let callee = self.parse_verb()?;
        let end_span = self.tokens[self.pos - 1].span;
        let start_span = args.first().map(|a| a.span).unwrap_or(end_span);

        if args.iter().any(|a| {
            a.particle == Particle::De
                && matches!(&a.value.kind, ExprKind::Identifier(s) if s.ends_with('ん'))
        }) {
            return Err(ParseError {
                message: "て形連鎖には「、」が必要です".into(),
                span: start_span.merge(end_span),
            });
        }

        if callee == "訴える" {
            if args.len() != 1 || args[0].particle != Particle::To {
                return Err(ParseError {
                    message: "「訴える」は「式と 訴える」の形で使います".into(),
                    span: start_span.merge(end_span),
                });
            }
            let expr = args.pop().unwrap().value;
            let span = expr.span.merge(end_span);
            return Ok(Expr {
                kind: ExprKind::Throw(Box::new(expr)),
                span,
            });
        }

        if callee == "作る" {
            if matches!(self.current_kind(), TokenKind::LBrace) {
                return Err(ParseError {
                    message: "組の初期化は `【名前: 値】` 形式で記述してください（`｛｝` は廃止されました）"
                        .into(),
                    span: self.current_span(),
                });
            }

            if args.len() == 1
                && args[0].particle == Particle::Wo
                && matches!(args[0].value.kind, ExprKind::Identifier(_))
                && matches!(self.current_kind(), TokenKind::LBracket)
            {
                let type_name = match &args[0].value.kind {
                    ExprKind::Identifier(name) => name.clone(),
                    _ => unreachable!(),
                };
                let fields = self.parse_construct_fields()?;
                let end = self.tokens[self.pos.saturating_sub(1)].span;
                return Ok(Expr {
                    kind: ExprKind::Construct { type_name, fields },
                    span: start_span.merge(end),
                });
            }
        }

        Ok(Expr {
            kind: ExprKind::Call { callee, args },
            span: start_span.merge(end_span),
        })
    }

    fn parse_construct_fields(&mut self) -> Result<Vec<(String, Expr)>, ParseError> {
        self.eat(&TokenKind::LBracket)?;
        let mut fields = Vec::new();
        while !matches!(self.current_kind(), TokenKind::RBracket | TokenKind::Eof) {
            let (name, _) = self.eat_identifier()?;
            self.eat(&TokenKind::Colon)?;
            let value = self.parse_expr()?;
            fields.push((name, value));
            if matches!(self.current_kind(), TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        self.eat(&TokenKind::RBracket)?;
        Ok(fields)
    }

    /// 動詞位置にあるかを判定
    fn is_verb_position(&self) -> bool {
        matches!(
            self.current_kind(),
            TokenKind::Identifier(_)
                | TokenKind::HyoujiSuru
                | TokenKind::NyuuryokuSuru
                | TokenKind::Kaeru
                | TokenKind::KuriKaesu
                | TokenKind::Tsukau
                | TokenKind::Tsukuru
                | TokenKind::Uttaeru
                | TokenKind::Kou
        ) && !matches!(
            self.peek_ahead(1),
            TokenKind::Particle(_) | TokenKind::AccessParticle
        )
    }

    /// 動詞をパースする
    fn parse_verb(&mut self) -> Result<String, ParseError> {
        match self.current_kind().clone() {
            TokenKind::Identifier(name) => {
                self.advance();
                Ok(name)
            }
            TokenKind::HyoujiSuru => {
                self.advance();
                Ok("表示する".into())
            }
            TokenKind::NyuuryokuSuru => {
                self.advance();
                Ok("入力する".into())
            }
            TokenKind::Kaeru => {
                self.advance();
                Ok("変える".into())
            }
            TokenKind::KuriKaesu => {
                self.advance();
                Ok("繰り返す".into())
            }
            TokenKind::Tsukau => {
                self.advance();
                Ok("使う".into())
            }
            TokenKind::Tsukuru => {
                self.advance();
                Ok("作る".into())
            }
            TokenKind::Uttaeru => {
                self.advance();
                Ok("訴える".into())
            }
            TokenKind::Kou => {
                self.advance();
                Ok("こう".into())
            }
            TokenKind::Shinagara | TokenKind::Matsu | TokenKind::Haikeide => Err(ParseError {
                message: "DGN-006: 未実装機能です（しながら/待つ/背景で）".into(),
                span: self.current_span(),
            }),
            _ => Err(ParseError {
                message: format!("動詞が必要ですが、「{}」がありました", self.current_kind()),
                span: self.current_span(),
            }),
        }
    }

    // === プライマリ式 ===

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        let start = self.current_span();

        match self.current_kind().clone() {
            TokenKind::Integer(n) => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Integer(n),
                    span: start,
                })
            }
            TokenKind::Float(n) => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Float(n),
                    span: start,
                })
            }
            TokenKind::String(s) => {
                self.advance();
                // 式展開チェック
                if matches!(self.current_kind(), TokenKind::StringInterpStart) {
                    self.parse_string_interp(s, start)
                } else {
                    Ok(Expr {
                        kind: ExprKind::StringLiteral(s),
                        span: start,
                    })
                }
            }
            TokenKind::StringInterpStart => {
                // 文字列先頭が式展開から始まる場合
                self.parse_string_interp(String::new(), start)
            }
            TokenKind::Bool(b) => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Bool(b),
                    span: start,
                })
            }
            TokenKind::Kore => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::KosoAdo(KosoAdoKind::Kore),
                    span: start,
                })
            }
            TokenKind::Sore => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::KosoAdo(KosoAdoKind::Sore),
                    span: start,
                })
            }
            TokenKind::Are => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::KosoAdo(KosoAdoKind::Are),
                    span: start,
                })
            }
            TokenKind::Kou => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::KosoAdo(KosoAdoKind::Kou),
                    span: start,
                })
            }
            TokenKind::Koko => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::KosoAdo(KosoAdoKind::Koko),
                    span: start,
                })
            }
            TokenKind::Soko => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::KosoAdo(KosoAdoKind::Soko),
                    span: start,
                })
            }
            TokenKind::DoreDemoNai => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::None,
                    span: start,
                })
            }
            TokenKind::LBracket => self.parse_list_literal(),
            TokenKind::LBrace => self.parse_map_literal(),
            TokenKind::LParen => self.parse_paren_or_lambda(),
            TokenKind::Moshi => self.parse_if_expr(start),
            TokenKind::Tamesu => self.parse_try_expr(start),
            TokenKind::Identifier(name) => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Identifier(name),
                    span: start,
                })
            }
            TokenKind::HyoujiSuru => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Call {
                        callee: "表示する".into(),
                        args: Vec::new(),
                    },
                    span: start,
                })
            }
            TokenKind::NyuuryokuSuru => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Call {
                        callee: "入力する".into(),
                        args: Vec::new(),
                    },
                    span: start,
                })
            }
            TokenKind::Kaeru => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Call {
                        callee: "変える".into(),
                        args: Vec::new(),
                    },
                    span: start,
                })
            }
            TokenKind::Tsukau => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Call {
                        callee: "使う".into(),
                        args: Vec::new(),
                    },
                    span: start,
                })
            }
            TokenKind::Tsukuru => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Call {
                        callee: "作る".into(),
                        args: Vec::new(),
                    },
                    span: start,
                })
            }
            _ => Err(ParseError {
                message: format!("式が必要ですが、「{}」がありました", self.current_kind()),
                span: start,
            }),
        }
    }

    fn parse_string_interp(&mut self, initial: String, start: Span) -> Result<Expr, ParseError> {
        let mut parts = Vec::new();
        if !initial.is_empty() {
            parts.push(StringPart::Literal(initial));
        }

        loop {
            match self.current_kind() {
                TokenKind::StringInterpStart => {
                    self.advance();
                    let expr = self.parse_expr()?;
                    parts.push(StringPart::Expr(expr));
                    self.eat(&TokenKind::StringInterpEnd)?;
                }
                TokenKind::String(s) => {
                    let s = s.clone();
                    self.advance();
                    if !s.is_empty() {
                        parts.push(StringPart::Literal(s));
                    }
                    // 文字列が続けば続く
                    if !matches!(self.current_kind(), TokenKind::StringInterpStart) {
                        break;
                    }
                }
                _ => break,
            }
        }

        let end_span = self.tokens[self.pos.saturating_sub(1)].span;
        Ok(Expr {
            kind: ExprKind::StringInterp(parts),
            span: start.merge(end_span),
        })
    }

    fn parse_list_literal(&mut self) -> Result<Expr, ParseError> {
        let start = self.current_span();
        self.eat(&TokenKind::LBracket)?;

        let mut elements = Vec::new();
        while !matches!(self.current_kind(), TokenKind::RBracket | TokenKind::Eof) {
            let elem = self.parse_expr()?;
            elements.push(elem);
            if matches!(self.current_kind(), TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        let end_span = self.current_span();
        self.eat(&TokenKind::RBracket)?;

        Ok(Expr {
            kind: ExprKind::List(elements),
            span: start.merge(end_span),
        })
    }

    fn parse_map_literal(&mut self) -> Result<Expr, ParseError> {
        let start = self.current_span();
        self.eat(&TokenKind::LBrace)?;

        let mut entries = Vec::new();
        while !matches!(self.current_kind(), TokenKind::RBrace | TokenKind::Eof) {
            let (key, _) = self.eat_identifier()?;
            self.eat(&TokenKind::Colon)?;
            let value = self.parse_expr()?;
            entries.push((key, value));
            if matches!(self.current_kind(), TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        let end_span = self.current_span();
        self.eat(&TokenKind::RBrace)?;

        Ok(Expr {
            kind: ExprKind::Map(entries),
            span: start.merge(end_span),
        })
    }

    fn parse_paren_or_lambda(&mut self) -> Result<Expr, ParseError> {
        let checkpoint = self.pos;
        self.eat(&TokenKind::LParen)?;
        if matches!(self.current_kind(), TokenKind::LBracket) {
            self.pos = checkpoint;
            return self.parse_lambda();
        }

        let expr = self.parse_expr()?;
        self.eat(&TokenKind::RParen)?;
        Ok(expr)
    }

    fn parse_lambda(&mut self) -> Result<Expr, ParseError> {
        let start = self.current_span();
        self.eat(&TokenKind::LParen)?;

        let params = if matches!(self.current_kind(), TokenKind::LBracket) {
            self.parse_params()?
        } else {
            Vec::new()
        };

        // ラムダの本体: 単一式 or ブロック
        let body_start = self.current_span();
        let mut stmts = Vec::new();

        // RParen が出るまで文を読む
        while !matches!(self.current_kind(), TokenKind::RParen | TokenKind::Eof) {
            self.skip_newlines();
            if matches!(self.current_kind(), TokenKind::RParen | TokenKind::Eof) {
                break;
            }
            let stmt = self.parse_statement()?;
            stmts.push(stmt);
            self.skip_newlines();
        }

        let end_span = self.current_span();
        self.eat(&TokenKind::RParen)?;

        Ok(Expr {
            kind: ExprKind::Lambda {
                params,
                body: Block {
                    statements: stmts,
                    span: body_start.merge(end_span),
                },
            },
            span: start.merge(end_span),
        })
    }
}

// === ヘルパー関数 ===

fn resolve_parse_step_limit() -> usize {
    std::env::var("KOTOBA_PARSE_STEP_LIMIT")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_PARSE_STEP_LIMIT)
}

fn is_arithmetic_word(word: &str) -> bool {
    matches!(word, "和" | "差" | "積")
}

fn arithmetic_op(word: &str) -> Option<BinOp> {
    match word {
        "和" => Some(BinOp::Add),
        "差" => Some(BinOp::Sub),
        "積" => Some(BinOp::Mul),
        _ => None,
    }
}
