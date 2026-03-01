use miette::{LabeledSpan, MietteDiagnostic, NamedSource, Report, Severity};

use crate::common::source::{SourceFile, Span};

#[derive(Debug, Clone, Copy)]
pub enum DiagnosticKind {
    Lex,
    Parse,
    Sema,
    Compile,
    Runtime,
}

impl DiagnosticKind {
    fn code(self) -> &'static str {
        match self {
            DiagnosticKind::Lex => "kotoba::lex",
            DiagnosticKind::Parse => "kotoba::parse",
            DiagnosticKind::Sema => "kotoba::sema",
            DiagnosticKind::Compile => "kotoba::compile",
            DiagnosticKind::Runtime => "kotoba::runtime",
        }
    }
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
    let mut base = MietteDiagnostic::new(format!("{:?}: {}", diag.kind, diag.message))
        .with_code(diag.kind.code())
        .with_severity(Severity::Error);

    if let Some(hint) = &diag.hint {
        base = base.with_help(hint.clone());
    }

    if let Some(span) = diag.span {
        let len = span.end.saturating_sub(span.start).max(1);
        base = base.with_label(LabeledSpan::new(
            Some(diag.message.clone()),
            span.start,
            len,
        ));
    }

    match source {
        Some(src) => {
            let named = NamedSource::new(src.name.clone(), src.content.clone());
            let report = Report::new(base).with_source_code(named);
            print_report(report);
        }
        None => {
            let report = Report::new(base);
            print_report(report);
        }
    }
}

fn print_report(report: Report) {
    eprintln!("{report:?}");
}
