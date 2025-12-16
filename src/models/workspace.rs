pub struct Workspace {
    pub name: String,
    pub collections: Vec<String>, // List of collection names
    pub requests: Vec<String>,    // List of request names
}

impl Workspace {
    pub fn new(name: String) -> Self {
        Workspace {
            name,
            collections: Vec::new(),
            requests: Vec::new(),
        }
    }

    pub fn add_collection(&mut self, collection_name: String) {
        self.collections.push(collection_name);
    }

    pub fn add_request(&mut self, request_name: String) {
        self.requests.push(request_name);
    }

    pub fn remove_collection(&mut self, collection_name: &str) {
        self.collections.retain(|c| c != collection_name);
    }

    pub fn remove_request(&mut self, request_name: &str) {
        self.requests.retain(|r| r != request_name);
    }
}
