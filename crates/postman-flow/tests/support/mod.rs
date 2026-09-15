#![allow(dead_code)]
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use futures::TryStreamExt;
use postman_flow::{execute_flow, FlowEvent, FlowInputs, FlowPlan, FlowSessionEnvironment};
use postman_http::{
    request::{Request, RequestOptions},
    HttpError, HttpResponse, HttpTransport,
};
use serde_json::Value;

#[derive(Clone)]
pub struct FakeTransport {
    state: Arc<Mutex<State>>,
}

struct State {
    responses: VecDeque<Result<HttpResponse, HttpError>>,
    requests: Vec<(Request, RequestOptions)>,
}

impl FakeTransport {
    pub fn new(responses: impl IntoIterator<Item = Result<HttpResponse, HttpError>>) -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                responses: responses.into_iter().collect(),
                requests: Vec::new(),
            })),
        }
    }
    pub fn requests(&self) -> Vec<(Request, RequestOptions)> {
        self.state.lock().unwrap().requests.clone()
    }
}

impl HttpTransport for FakeTransport {
    async fn execute(
        &self,
        request: Request,
        options: RequestOptions,
    ) -> Result<HttpResponse, HttpError> {
        let mut state = self.state.lock().unwrap();
        state.requests.push((request, options));
        state
            .responses
            .pop_front()
            .expect("unexpected HTTP request")
    }
}

pub fn response(value: Value) -> Result<HttpResponse, HttpError> {
    Ok(HttpResponse::new(200, Vec::new(), value.to_string()))
}

pub async fn run(plan: FlowPlan, inputs: FlowInputs, transport: FakeTransport) -> Vec<FlowEvent> {
    execute_flow(plan, transport, FlowSessionEnvironment::new(inputs))
        .unwrap()
        .try_collect()
        .await
        .unwrap()
}
