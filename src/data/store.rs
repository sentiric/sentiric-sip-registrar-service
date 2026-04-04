// src/data/store.rs
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use sentiric_sip_core::utils as sip_utils;
use tracing::{debug, info, instrument, warn};

// Artık Arc<Mutex<...>> kullanmıyoruz, ConnectionManager kendi içinde güvenlidir ve kopmaları yönetir.
pub type RedisConn = ConnectionManager;

#[derive(Clone)]
pub struct RegistrationStore {
    redis: RedisConn,
}

impl RegistrationStore {
    pub fn new(redis: RedisConn) -> Self {
        Self { redis }
    }

    fn generate_key(&self, raw_uri: &str) -> String {
        let username = sip_utils::extract_username_from_uri(raw_uri);
        if username.is_empty() {
            warn!(event="URI_PARSE_WARN", uri=%raw_uri, "Username extraction failed, using raw URI");
            return format!("sip_reg:{}", raw_uri);
        }
        format!("sip_reg:{}", username)
    }

    #[instrument(skip(self), fields(key))]
    pub async fn register_user(
        &self,
        sip_uri: &str,
        contact_uri: &str,
        expires: i32,
    ) -> anyhow::Result<()> {
        let key = self.generate_key(sip_uri);
        // ConnectionManager ucuz bir şekilde kopyalanabilir (clone), içindeki havuzu paylaşır.
        let mut conn = self.redis.clone();

        if expires <= 0 {
            let _: () = conn.del(&key).await?;
            info!(event="SIP_UNREGISTER_EXPIRE", key=%key, "Kayıt süresi dolduğu için silindi");
        } else {
            let _: () = conn.set_ex(&key, contact_uri, expires as u64).await?;
            debug!(event="SIP_REGISTER_STORED", key=%key, contact=%contact_uri, ttl=%expires, "Kayıt Redis'e yazıldı");
        }
        Ok(())
    }

    #[instrument(skip(self), fields(key))]
    pub async fn unregister_user(&self, sip_uri: &str) -> anyhow::Result<()> {
        let key = self.generate_key(sip_uri);
        let mut conn = self.redis.clone();
        let _: () = conn.del(&key).await?;
        info!(event="SIP_UNREGISTER_MANUAL", key=%key, "Kullanıcı manuel silindi");
        Ok(())
    }

    #[instrument(skip(self), fields(key))]
    pub async fn lookup_user(&self, sip_uri: &str) -> Option<String> {
        let key = self.generate_key(sip_uri);
        let mut conn = self.redis.clone();

        match conn.get::<_, String>(&key).await {
            Ok(contact) => {
                debug!(event="SIP_LOCATION_FOUND", key=%key, contact=%contact, "Konum bulundu");
                Some(contact)
            }
            Err(_) => {
                warn!(event="SIP_LOCATION_MISS", key=%key, "Konum bulunamadı (Offline)");
                None
            }
        }
    }
}
