use ariadne::{Color, Label, Report, ReportKind, Source};

use crate::common::source::{SourceFile, Span};

#[derive(Debug, Clone, Copy)]
pub enum DiagnosticKind {
    Lex,
    Parse,
    Sema,
    Compile,
    Runtime,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    pub span: Option<Span>,
    pub message: String,
    pub hint: Option<String>,
}

impl Diagnostic {
    pub fn new(kind: DiagnosticKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            span: None,
            message: message.into(),
            hint: None,
        }
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

pub fn render(diag: &Diagnostic, source: Option<&SourceFile>) {
    match (diag.span, source) {
        (Some(span), Some(src)) => {
            let mut report =
                Report::build(ReportKind::Error, (src.name.clone(), span.start..span.end))
                    .with_message(format!("{:?}: {}", diag.kind, diag.message))
                    .with_label(
                        Label::new((src.name.clone(), span.start..span.end))
                            .with_color(Color::Red)
                            .with_message(diag.message.clone()),
                    );
            if let Some(hint) = &diag.hint {
                report = report.with_note(hint.clone());
            }
            let _ = report
                .finish()
                .print((src.name.clone(), Source::from(src.content.clone())));
        }
        _ => {
            eprintln!("{:?}: {}", diag.kind, diag.message);
            if let Some(hint) = &diag.hint {
                eprintln!("  ヒント: {}", hint);
            }
        }
    }
}
