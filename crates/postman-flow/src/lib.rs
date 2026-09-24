//! Format-independent HTTP flow definitions, YAML v1 documents, compilation and execution.
//!
//! Parse or construct a FlowDefinition, compile an immutable FlowPlan, and execute it with
//! a caller-supplied HTTP transport and session. Version 1 supports ordered HTTP steps,
//! conditions, bounded collection iteration and bounded polling; source documents and optional
//! editor layout remain separate from runtime state.

pub mod calc;
mod catalog;
mod compiler;
mod document;
mod json_path;
mod model;
mod plan;
mod runtime;

pub use catalog::{ApiCatalog, ApiDefinition};
pub use compiler::{
    compile_flow, CompileEnvironment, Diagnostic, DiagnosticCode, DiagnosticLocation,
};
pub use document::{
    parse_flow_yaml, write_flow_yaml, DocumentError, DocumentErrorCode, EditorLayout, FlowDocument,
    NodePosition, FLOW_DOCUMENT_SCHEMA_JSON, FLOW_DOCUMENT_VERSION,
};
pub use model::{
    ApiCall, AuthTemplate, BodyTemplate, ConditionExpr, ExpectedError, FlowDefinition, FlowEvent,
    FlowInputSpec, FlowInputs, FlowOutputSpec, FlowOutputs, FlowValue, ForEachDefinition,
    HttpRequestSource, HttpRequestTemplate, HttpStepDefinition, JsonTemplate, LoopCarry,
    LoopCollection, LoopErrorPolicy, LoopFinishReason, LoopKind, RepeatUntilDefinition,
    RequestOptionOverrides, ResponseCheck, ResponseExport, StepOutcome, TemplatePart, TextTemplate,
    ValueReference,
};
pub use plan::FlowPlan;
pub use runtime::{execute_flow, is_builtin_variable, FlowError, FlowSessionEnvironment};
