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
                // [ARCH-COMPLIANCE] ARCH-005: standart yazdırma makroları (eprintln!) yasaktır.
                // Tracing makroları henüz başlatılamamış olabileceği için, STDERR'e manuel JSON (SUTS v4.0 uyumlu) çıktı basıyoruz.
                let err_msg = format!("{{\"schema_v\":\"1.0.0\",\"severity\":\"FATAL\",\"event\":\"APP_STARTUP_FAILED\",\"message\":\"Kritik Hata: Uygulama başlatılamadı: {:?}\"}}\n", e);
                let _ = std::io::Write::write_all(&mut std::io::stderr(), err_msg.as_bytes());
                process::exit(1);
            }
        }
    })
}