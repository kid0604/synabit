use ed25519_dalek::{Signature, Signer, SigningKey};
use hex::FromHex;
use serde::Serialize;
use std::env;

#[derive(Clone)]
pub struct CryptoService {
    signing_key: SigningKey,
}

impl CryptoService {
    pub fn new() -> Self {
        let hex_key = env::var("LICENSE_PRIVATE_KEY").expect("LICENSE_PRIVATE_KEY must be set");
        let bytes = <[u8; 32]>::from_hex(&hex_key).expect("Invalid hex for private key");
        let signing_key = SigningKey::from_bytes(&bytes);
        Self { signing_key }
    }

    pub fn sign_data<T: Serialize>(&self, data: &T) -> String {
        let serialized = serde_json::to_string(data).expect("Failed to serialize data");
        let signature: Signature = self.signing_key.sign(serialized.as_bytes());
        hex::encode(signature.to_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crypto_signing() {
        // Set fake private key (32 bytes = 64 hex chars)
        unsafe {
            std::env::set_var("LICENSE_PRIVATE_KEY", "0000000000000000000000000000000000000000000000000000000000000000");
        }
        
        let crypto = CryptoService::new();
        let data = "test_data";
        
        let sig1 = crypto.sign_data(&data);
        let sig2 = crypto.sign_data(&data);
        
        assert_eq!(sig1, sig2);
    }
}
