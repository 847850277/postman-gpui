//! Minimal proof of the Postman Flow execution boundary.
//!
//! This first slice intentionally supports only an ordered sequence of HTTP steps. It proves that
//! one step can export a typed response value and a later step can consume it while the caller
//! observes execution through a stream.

mod model;
mod runtime;

pub use model::{
    BodyTemplate, FlowEvent, FlowInputSpec, FlowInputs, FlowPlan, HttpStepPlan, ResponseCheck,
    ResponseExport, StepOutcome, TemplatePart, TextTemplate,
};
pub use runtime::{execute_flow, FlowError, FlowEventStream, FlowSessionEnvironment};
