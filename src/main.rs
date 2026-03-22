// sentiric-registrar-service/src/main.rs
use anyhow::{Context, Result};
use sentiric_sip_registrar_service::app::App;
use std::process;

fn main() -> Result<()> {
    // Rust'taki tonic/tokio yapısı gereği main asenkron olamaz.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("Tokio runtime oluşturulamadı")?;

    runtime.block_on(async {
        match App::bootstrap().await {
            Ok(app) => app.run().await,
            Err(e) => {
                eprintln!("Kritik Hata: Uygulama başlatılamadı: {:?}", e);
                process::exit(1);
            }
        }
    })
}