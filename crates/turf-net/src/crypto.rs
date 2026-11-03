use crate::error::NetError;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use rand::RngCore;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Unambiguous alphanumeric characters for room codes (excludes 0, 1, I, O) conforming to [MD-025].
pub const UNAMBIGUOUS_ALPHABET: &[u8] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";

/// Standard room code prefix.
pub const ROOM_CODE_PREFIX: &str = "TURF-";

/// AES-GCM standard nonce length (96-bit / 12-byte).
pub const NONCE_LEN: usize = 12;

/// AES-GCM authentication tag length (128-bit / 16-byte).
pub const TAG_LEN: usize = 16;

/// Encrypted wire payload with hex-encoded IV, auth tag, and ciphertext.
/// Conforms to [MD-019] and is compatible across Node.js and Rust implementations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptedPayload {
    /// 12-byte hex-encoded initialization vector / nonce (24 hex characters).
    pub iv: String,
    /// 16-byte hex-encoded authentication tag (32 hex characters).
    pub tag: String,
    /// Hex-encoded ciphertext data.
    pub data: String,
}

/// Generate a cryptographically secure room code in `TURF-XXXX` format [MD-025].
/// Uses 4 unambiguous alphanumeric characters (excludes 0, 1, I, O).
pub fn generate_room_code() -> String {
    generate_room_code_with_len(4)
}

/// Generate a cryptographically secure room code with custom alphanumeric suffix length.
pub fn generate_room_code_with_len(suffix_len: usize) -> String {
    let mut rng = rand::thread_rng();
    let mut suffix = String::with_capacity(suffix_len);
    for _ in 0..suffix_len {
        let idx = (rng.next_u32() as usize) % UNAMBIGUOUS_ALPHABET.len();
        suffix.push(UNAMBIGUOUS_ALPHABET[idx] as char);
    }
    format!("{}{}", ROOM_CODE_PREFIX, suffix)
}

/// Check if a room code adheres to the `TURF-XXXX` unambiguous format.
pub fn is_valid_room_code(code: &str) -> bool {
    let normalized = normalize_room_code(code);
    if !normalized.starts_with(ROOM_CODE_PREFIX) {
        return false;
    }
    let suffix = &normalized[ROOM_CODE_PREFIX.len()..];
    if suffix.is_empty() {
        return false;
    }
    suffix.bytes().all(|b| UNAMBIGUOUS_ALPHABET.contains(&b))
}

/// Normalize room code: trims whitespace and converts to uppercase.
pub fn normalize_room_code(code: &str) -> String {
    code.trim().to_uppercase()
}

/// Derive a 32-byte (256-bit) AES-256 key from a room code or secret using SHA-256.
pub fn derive_key(secret: &str) -> [u8; 32] {
    let normalized = normalize_room_code(secret);
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    hasher.finalize().into()
}

/// Encrypt raw bytes using AES-256-GCM authenticated encryption [MD-019].
pub fn encrypt_bytes(plaintext: &[u8], secret: &str) -> Result<EncryptedPayload, NetError> {
    let key_bytes = derive_key(secret);
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);

    let mut iv_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut iv_bytes);
    let nonce = Nonce::from_slice(&iv_bytes);

    let encrypted_with_tag = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| NetError::Crypto(format!("AES-GCM encryption failed: {e}")))?;

    if encrypted_with_tag.len() < TAG_LEN {
        return Err(NetError::Crypto(
            "Ciphertext shorter than authentication tag".into(),
        ));
    }

    let split_idx = encrypted_with_tag.len() - TAG_LEN;
    let data_bytes = &encrypted_with_tag[..split_idx];
    let tag_bytes = &encrypted_with_tag[split_idx..];

    Ok(EncryptedPayload {
        iv: hex::encode(iv_bytes),
        tag: hex::encode(tag_bytes),
        data: hex::encode(data_bytes),
    })
}

/// Decrypt an `EncryptedPayload` into raw bytes using AES-256-GCM authenticated decryption [MD-019].
pub fn decrypt_bytes(payload: &EncryptedPayload, secret: &str) -> Result<Vec<u8>, NetError> {
    let iv_bytes =
        hex::decode(&payload.iv).map_err(|e| NetError::Crypto(format!("Invalid IV hex: {e}")))?;
    if iv_bytes.len() != NONCE_LEN {
        return Err(NetError::Crypto(format!(
            "Invalid IV length: expected {} bytes, got {}",
            NONCE_LEN,
            iv_bytes.len()
        )));
    }

    let tag_bytes =
        hex::decode(&payload.tag).map_err(|e| NetError::Crypto(format!("Invalid tag hex: {e}")))?;
    if tag_bytes.len() != TAG_LEN {
        return Err(NetError::Crypto(format!(
            "Invalid tag length: expected {} bytes, got {}",
            TAG_LEN,
            tag_bytes.len()
        )));
    }

    let data_bytes = hex::decode(&payload.data)
        .map_err(|e| NetError::Crypto(format!("Invalid ciphertext hex: {e}")))?;

    let mut combined = Vec::with_capacity(data_bytes.len() + tag_bytes.len());
    combined.extend_from_slice(&data_bytes);
    combined.extend_from_slice(&tag_bytes);

    let key_bytes = derive_key(secret);
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(&iv_bytes);

    let decrypted = cipher.decrypt(nonce, combined.as_slice()).map_err(|e| {
        NetError::Crypto(format!(
            "AES-GCM decryption failed / tag verification failed: {e}"
        ))
    })?;

    Ok(decrypted)
}

/// Encrypt a UTF-8 string using AES-256-GCM.
pub fn encrypt_str(text: &str, secret: &str) -> Result<EncryptedPayload, NetError> {
    encrypt_bytes(text.as_bytes(), secret)
}

/// Decrypt an `EncryptedPayload` into a UTF-8 string.
pub fn decrypt_str(payload: &EncryptedPayload, secret: &str) -> Result<String, NetError> {
    let bytes = decrypt_bytes(payload, secret)?;
    String::from_utf8(bytes)
        .map_err(|e| NetError::Crypto(format!("Decrypted bytes are not valid UTF-8: {e}")))
}

/// Encrypt any JSON-serializable value using AES-256-GCM.
pub fn encrypt_json<T: Serialize>(value: &T, secret: &str) -> Result<EncryptedPayload, NetError> {
    let json_bytes = serde_json::to_vec(value)?;
    encrypt_bytes(&json_bytes, secret)
}

/// Decrypt an `EncryptedPayload` into a JSON-deserializable value.
pub fn decrypt_json<T: DeserializeOwned>(
    payload: &EncryptedPayload,
    secret: &str,
) -> Result<T, NetError> {
    let bytes = decrypt_bytes(payload, secret)?;
    let value = serde_json::from_slice(&bytes)?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_room_code_generation_and_validation() {
        let code = generate_room_code();
        assert!(code.starts_with("TURF-"));
        assert_eq!(code.len(), 9); // "TURF-" (5) + 4 chars
        assert!(is_valid_room_code(&code));

        // Validation checks
        assert!(!is_valid_room_code(""));
        assert!(!is_valid_room_code("TURF-"));
        assert!(!is_valid_room_code("TURF-1O0I")); // Contains ambiguous chars: 1, O, 0, I
        assert!(is_valid_room_code("TURF-7A9B"));
        assert!(is_valid_room_code("turf-7a9b")); // Case insensitive normalization
    }

    #[test]
    fn test_sha256_key_derivation() {
        let key1 = derive_key("TURF-7A9B");
        let key2 = derive_key("  turf-7a9b  ");
        assert_eq!(key1, key2);
        assert_eq!(key1.len(), 32);

        let key3 = derive_key("TURF-XYZ8");
        assert_ne!(key1, key3);
    }

    #[test]
    fn test_aes_256_gcm_string_roundtrip() {
        let secret = "TURF-7A9B";
        let message = "Hello from TurfCode encrypted P2P tunnel!";

        let encrypted = encrypt_str(message, secret).unwrap();
        assert_eq!(encrypted.iv.len(), 24); // 12 bytes = 24 hex chars
        assert_eq!(encrypted.tag.len(), 32); // 16 bytes = 32 hex chars

        let decrypted = decrypt_str(&encrypted, secret).unwrap();
        assert_eq!(decrypted, message);
    }

    #[test]
    fn test_aes_256_gcm_json_roundtrip() {
        #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
        struct TestCreds {
            username: String,
            role: String,
            token: Option<String>,
        }

        let creds = TestCreds {
            username: "Ayush".into(),
            role: "Peer".into(),
            token: Some("secret_token_12345".into()),
        };

        let secret = "TURF-9K2M";
        let encrypted = encrypt_json(&creds, secret).unwrap();
        let decrypted: TestCreds = decrypt_json(&encrypted, secret).unwrap();
        assert_eq!(creds, decrypted);
    }

    #[test]
    fn test_aes_256_gcm_wrong_key_fails() {
        let secret = "TURF-7A9B";
        let wrong_secret = "TURF-8B0C";
        let message = "Confidential agent instructions";

        let encrypted = encrypt_str(message, secret).unwrap();
        let result = decrypt_str(&encrypted, wrong_secret);
        assert!(result.is_err());
    }

    #[test]
    fn test_aes_256_gcm_tampered_ciphertext_fails() {
        let secret = "TURF-7A9B";
        let message = "Integrity protected state";

        let mut encrypted = encrypt_str(message, secret).unwrap();
        // Tamper with data hex
        let mut tampered_data = encrypted.data.clone();
        if let Some(c) = tampered_data.chars().last() {
            let replacement = if c == '0' { '1' } else { '0' };
            tampered_data.pop();
            tampered_data.push(replacement);
        }
        encrypted.data = tampered_data;

        let result = decrypt_str(&encrypted, secret);
        assert!(result.is_err());
    }
}
