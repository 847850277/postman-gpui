use postman_flow::{compile_flow, ApiCatalog, CompileEnvironment, FlowDefinition, FlowPlan};

pub fn compile_example(source: &FlowDefinition) -> Result<FlowPlan, Box<dyn std::error::Error>> {
    compile_flow(source, &ApiCatalog::new(), &CompileEnvironment::default()).map_err(|errors| {
        errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
            .into()
    })
}
