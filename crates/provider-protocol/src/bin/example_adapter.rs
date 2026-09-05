//! Executable conformance fixture for the external provider protocol.
#![forbid(unsafe_code)]

use inferqos_provider_protocol::{ExternalProviderService, ProviderAdapterServer, v1};
use std::pin::Pin;
use tokio_stream::{Stream, wrappers::ReceiverStream};
use tonic::{Request, Response, Status};

#[derive(Default)]
struct ExampleAdapter;

#[tonic::async_trait]
impl ExternalProviderService for ExampleAdapter {
    async fn estimate(
        &self,
        request: Request<v1::EstimateRequest>,
    ) -> Result<Response<v1::WorkEstimate>, Status> {
        let body = request.into_inner().body;
        let input = ((body.len() as u64) / 4).max(1);
        Ok(Response::new(v1::WorkEstimate {
            input_tokens: input,
            output_tokens: 32,
            cached_input_tokens: 0,
            normalized_units: (input + 32) as f64,
            confidence: 0.5,
            source: "approximation".into(),
            provider_cost_coefficient: 1.0,
        }))
    }

    type DispatchStream = Pin<Box<dyn Stream<Item = Result<v1::DispatchResponse, Status>> + Send>>;

    async fn dispatch(
        &self,
        request: Request<v1::DispatchRequest>,
    ) -> Result<Response<Self::DispatchStream>, Status> {
        let request = request.into_inner();
        let (sender, receiver) = tokio::sync::mpsc::channel(4);
        tokio::spawn(async move {
            let body = serde_json::json!({
                "id": request.request_id,
                "object": "chat.completion",
                "choices": [{"message":{"role":"assistant","content":"external provider protocol is live"}}],
                "usage": {"prompt_tokens": 8, "completion_tokens": 6, "total_tokens": 14}
            }).to_string().into_bytes();
            let _ = sender
                .send(Ok(v1::DispatchResponse {
                    status: 200,
                    safe_headers: [("content-type".into(), "application/json".into())].into(),
                    data: body.into(),
                    end: true,
                    usage: Some(v1::WorkEstimate {
                        input_tokens: 8,
                        output_tokens: 6,
                        cached_input_tokens: 0,
                        normalized_units: 14.0,
                        confidence: 1.0,
                        source: "provider_metadata".into(),
                        provider_cost_coefficient: 1.0,
                    }),
                }))
                .await;
        });
        Ok(Response::new(Box::pin(ReceiverStream::new(receiver))))
    }

    async fn health(
        &self,
        _request: Request<v1::HealthRequest>,
    ) -> Result<Response<v1::HealthResponse>, Status> {
        Ok(Response::new(v1::HealthResponse {
            healthy: true,
            detail: "example adapter ready".into(),
        }))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listen = std::env::var("INFERQOS_ADAPTER_LISTEN")
        .unwrap_or_else(|_| "127.0.0.1:50051".into())
        .parse()?;
    println!("InferQoS example external provider adapter listening on {listen}");
    tonic::transport::Server::builder()
        .add_service(ProviderAdapterServer::new(ExampleAdapter))
        .serve(listen)
        .await?;
    Ok(())
}
