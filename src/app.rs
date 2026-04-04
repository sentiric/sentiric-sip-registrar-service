// Dosya: src/app.rs
use crate::config::AppConfig;
use crate::data::store::{RedisConn, RegistrationStore};
use crate::grpc::client::InternalClients;
use crate::grpc::service::MyRegistrarService;
use crate::telemetry::SutsFormatter;
use crate::tls::load_server_tls_config;
use sentiric_contracts::sentiric::sip::v1::registrar_service_server::RegistrarServiceServer;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tonic::transport::Server as GrpcServer;
use tracing::{error, info, warn};
use tracing_subscriber::{fmt, prelude::*, EnvFilter, Registry};

pub struct App {
    config: Arc<AppConfig>,
}

impl App {
    pub async fn bootstrap() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();
        let config = Arc::new(AppConfig::load_from_env()?);

        // --- SUTS v4.0 LOGGING SETUP ---
        let rust_log_env = std::env::var("RUST_LOG").unwrap_or_else(|_| config.rust_log.clone());
        let env_filter =
            EnvFilter::try_from_default_env().or_else(|_| EnvFilter::try_new(&rust_log_env))?;
        let subscriber = Registry::default().with(env_filter);

        if config.log_format == "json" {
            let suts_formatter = SutsFormatter::new(
                "sip-registrar-service".to_string(),
                config.service_version.clone(),
                config.env.clone(),
                config.node_hostname.clone(),
                config.tenant_id.clone(), // [ARCH-COMPLIANCE] Tenant enjekte edildi
            );
            subscriber
                .with(fmt::layer().event_format(suts_formatter))
                .init();
        } else {
            subscriber.with(fmt::layer().compact()).init();
        }

        info!(
            event = "SYSTEM_STARTUP",
            service_name = "sip-registrar-service",
            version = %config.service_version,
            profile = %config.env,
            "🚀 Registrar Service başlatılıyor (SUTS v4.0 - AutoHealing Redis)"
        );

        Ok(Self { config })
    }

    pub async fn run(self) -> anyhow::Result<()> {
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);

        // 1. Redis Connection
        let redis_conn = self.init_redis().await?;
        let store = RegistrationStore::new(redis_conn);

        // 2. Internal gRPC Clients
        let clients = Arc::new(Mutex::new(InternalClients::connect(&self.config).await?));

        // 3. gRPC Server
        let tls_config = load_server_tls_config(&self.config).await?;
        let grpc_service = MyRegistrarService::new(store, clients, self.config.clone());

        info!(event="GRPC_SERVER_START", addr=%self.config.grpc_listen_addr, "Registrar gRPC aktif.");

        let server_handle = tokio::spawn(async move {
            GrpcServer::builder()
                .tls_config(tls_config)
                .unwrap()
                .add_service(RegistrarServiceServer::new(grpc_service))
                .serve_with_shutdown(self.config.grpc_listen_addr, async {
                    shutdown_rx.recv().await;
                    info!(event = "GRPC_SHUTDOWN", "gRPC sunucusu kapanıyor...");
                })
                .await
        });

        tokio::select! {
            res = server_handle => {
                if let Ok(Err(e)) = res { error!(event="SERVER_ERROR", error=%e, "Sunucu çöktü"); }
            }
            _ = tokio::signal::ctrl_c() => {
                warn!(event="SIGINT", "Kapatma sinyali alındı.");
            }
        }

        let _ = shutdown_tx.send(());
        Ok(())
    }

    async fn init_redis(&self) -> anyhow::Result<RedisConn> {
        loop {
            match redis::Client::open(self.config.redis_url.as_str()) {
                Ok(client) => match redis::aio::ConnectionManager::new(client).await {
                    Ok(conn) => {
                        info!(event="REDIS_CONNECTED", url=%self.config.redis_url, "Redis Auto-Healing ConnectionManager başarıyla başlatıldı.");
                        return Ok(conn);
                    }
                    Err(e) => {
                        error!(event="REDIS_CONNECT_FAIL", error=%e, "Redis asenkron bağlantı hatası. 5sn sonra tekrar...")
                    }
                },
                Err(e) => {
                    error!(event="REDIS_CLIENT_FAIL", error=%e, "Redis istemci oluşturma hatası. 5sn sonra tekrar...")
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    }
}
