// sentiric-registrar-service/src/main.rs
use anyhow::{Context, Result};
use sentiric_sip_registrar_service::app::App;
use std::process;

fn main() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("Tokio runtime oluşturulamadı")?;

    runtime.block_on(async {
        match App::bootstrap().await {
            Ok(app) => {
                if let Err(e) = app.run().await {
                    // [SUTS v4.0 Compliance]
                    let err_json = serde_json::json!({
                        "schema_v": "1.0.0",
                        "severity": "FATAL",
                        "event": "SERVICE_RUNTIME_CRASH",
                        "message": format!("Uygulama çalışma zamanında çöktü: {:?}", e)
                    });
                    eprintln!("{}", err_json);
                    process::exit(1);
                }
            }
            Err(e) => {
                // [ARCH-COMPLIANCE] ARCH-005: Raw stderr output converted to SUTS JSON
                let err_json = serde_json::json!({
                    "schema_v": "1.0.0",
                    "severity": "FATAL",
                    "event": "BOOTSTRAP_FAILED",
                    "message": format!("Kritik Hata: Uygulama başlatılamadı: {:?}", e)
                });
                eprintln!("{}", err_json);
                process::exit(1);
            }
        }
    });
    Ok(())
}
