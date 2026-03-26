// src/grpc/service.rs

use std::sync::Arc;
use tokio::sync::Mutex;
use sentiric_contracts::sentiric::sip::v1::{
    registrar_service_server::RegistrarService, 
    RegisterRequest, RegisterResponse, 
    UnregisterRequest, UnregisterResponse, 
    LookupContactRequest, LookupContactResponse
};
use sentiric_contracts::sentiric::user::v1::GetSipCredentialsRequest;
use tonic::{Request, Response, Status};
use tracing::{info, error, warn, instrument, Span};
use crate::grpc::client::InternalClients;
use crate::data::store::RegistrationStore;
use crate::config::AppConfig;

pub struct MyRegistrarService {
    store: RegistrationStore,
    clients: Arc<Mutex<InternalClients>>,
    config: Arc<AppConfig>,
}

impl MyRegistrarService {
    pub fn new(store: RegistrationStore, clients: Arc<Mutex<InternalClients>>, config: Arc<AppConfig>) -> Self {
        Self { store, clients, config }
    }
    
    // Trace ID Çıkarıcı
    fn extract_trace_id<T>(req: &Request<T>) -> String {
        req.metadata().get("x-trace-id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown")
            .to_string()
    }
}

#[tonic::async_trait]
impl RegistrarService for MyRegistrarService {
    
    #[instrument(skip(self, request), fields(trace_id, sip.uri = %request.get_ref().sip_uri))]
    async fn register(
        &self,
        request: Request<RegisterRequest>,
    ) -> Result<Response<RegisterResponse>, Status> {
        let trace_id = Self::extract_trace_id(&request);
        Span::current().record("trace_id", &trace_id);
        
        let req = request.into_inner();
        let username = sentiric_sip_core::utils::extract_username_from_uri(&req.sip_uri);

        if username.is_empty() {
            warn!(event="SIP_REGISTER_BAD_REQUEST", uri=%req.sip_uri, "Geçersiz URI");
            return Err(Status::invalid_argument("Invalid SIP URI"));
        }

        // 1. User Service Sorgusu
        let mut user_client = {
            let guard = self.clients.lock().await;
            guard.user.clone()
        };

        // [ARCH-COMPLIANCE] ARCH-006 & Timeout Kuralları uygulanıyor
        let mut user_req = Request::new(GetSipCredentialsRequest {
            sip_username: username.clone(),
            realm: self.config.sip_realm.clone(),
        });

        // Zorunlu Timeout (resilience.timeouts)
        user_req.set_timeout(std::time::Duration::from_secs(5));

        // Trace ID Yayılımı (observability.tracing context propagation)
        if let Ok(meta_val) = trace_id.parse() {
            user_req.metadata_mut().insert("x-trace-id", meta_val);
        }

        let user_res = user_client.get_sip_credentials(user_req).await;

        match user_res {
            Ok(res) => {
                let inner = res.into_inner();
                //[SUTS v4.0]: REGISTER SUCCESS
                info!(
                    event = "SIP_REGISTER_SUCCESS",
                    trace_id = %trace_id,
                    sip.user = %username,
                    tenant.id = %inner.tenant_id,
                    "Kullanıcı doğrulandı ve kaydediliyor"
                );
                
                // 2. Redis Kaydı
                if let Err(e) = self.store.register_user(&req.sip_uri, &req.contact_uri, req.expires).await {
                    error!(event="SIP_REGISTER_STORE_FAIL", user=%username, error=%e, "Redis yazma hatası");
                    return Err(Status::internal("Location store failure"));
                }
                
                Ok(Response::new(RegisterResponse { success: true }))
            },
            Err(e) => {
                //[SUTS v4.0]: AUTH FAILURE
                warn!(
                    event = "SIP_AUTH_FAILURE",
                    trace_id = %trace_id,
                    sip.user = %username,
                    error = %e,
                    "Kimlik doğrulama başarısız"
                );
                Err(Status::unauthenticated("Invalid credentials"))
            }
        }
    }

    #[instrument(skip(self, request), fields(trace_id, sip.uri = %request.get_ref().sip_uri))]
    async fn unregister(&self, request: Request<UnregisterRequest>) -> Result<Response<UnregisterResponse>, Status> {
        let trace_id = Self::extract_trace_id(&request);
        Span::current().record("trace_id", &trace_id);
        
        let req = request.into_inner();
        info!(event="SIP_UNREGISTER_REQUEST", uri=%req.sip_uri, "Kayıt silme isteği");
        
        if let Err(e) = self.store.unregister_user(&req.sip_uri).await {
            error!(event="SIP_UNREGISTER_FAIL", error=%e, "Silme hatası");
            return Err(Status::internal("Location store failure"));
        }
        
        Ok(Response::new(UnregisterResponse { success: true }))
    }

    #[instrument(skip(self, request), fields(trace_id, sip.uri = %request.get_ref().sip_uri))]
    async fn lookup_contact(&self, request: Request<LookupContactRequest>) -> Result<Response<LookupContactResponse>, Status> {
        let trace_id = Self::extract_trace_id(&request);
        Span::current().record("trace_id", &trace_id);
        
        let req = request.into_inner();
        let contact = self.store.lookup_user(&req.sip_uri).await;

        if let Some(c) = contact {
            info!(event="SIP_LOOKUP_HIT", uri=%req.sip_uri, contact=%c, "Kullanıcı bulundu");
            Ok(Response::new(LookupContactResponse { contact_uris: vec![c] }))
        } else {
            info!(event="SIP_LOOKUP_MISS", uri=%req.sip_uri, "Kullanıcı bulunamadı (Offline)");
            Ok(Response::new(LookupContactResponse { contact_uris: vec!