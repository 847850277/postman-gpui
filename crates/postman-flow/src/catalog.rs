use std::collections::BTreeMap;

use crate::HttpRequestTemplate;

#[derive(Debug, Clone, PartialEq)]
pub struct ApiDefinition {
    pub parameters: Vec<String>,
    pub request: HttpRequestTemplate,
}

impl ApiDefinition {
    pub fn new(request: HttpRequestTemplate) -> Self {
        Self {
            parameters: Vec::new(),
            request,
        }
    }

    pub fn parameter(mut self, name: impl Into<String>) -> Self {
        self.parameters.push(name.into());
        self
    }
}

/// Reusable request definitions. Only referenced definitions participate in compilation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ApiCatalog {
    pub(crate) definitions: BTreeMap<String, ApiDefinition>,
}

impl ApiCatalog {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn insert(
        &mut self,
        id: impl Into<String>,
        definition: ApiDefinition,
    ) -> Option<ApiDefinition> {
        self.definitions.insert(id.into(), definition)
    }
    pub fn get(&self, id: &str) -> Option<&ApiDefinition> {
        self.definitions.get(id)
    }
}
