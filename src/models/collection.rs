use super::request::Request;

#[derive(Debug, Clone)]
pub struct Collection {
    pub name: String,
    pub requests: Vec<Request>,
}

impl Collection {
    pub fn new(name: String) -> Self {
        Collection {
            name,
            requests: Vec::new(),
        }
    }

    pub fn add_request(&mut self, request: Request) {
        self.requests.push(request);
    }

    pub fn remove_request(&mut self, index: usize) {
        if index < self.requests.len() {
            self.requests.remove(index);
        }
    }

    pub fn get_request(&self, index: usize) -> Option<&Request> {
        self.requests.get(index)
    }
}