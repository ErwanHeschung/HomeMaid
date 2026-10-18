//! KLAP v2 crypto, as spoken by Tapo devices (`encrypt_type: KLAP, lv: 2`).
//! Reference: python-kasa `kasa/transports/klaptransport.py` (`KlapTransportV2`, `KlapEncryptionSession`).
//!
//! Both sides are exposed so tests can run a fake device.

use aes::cipher::{BlockModeDecrypt, BlockModeEncrypt, KeyIvInit, block_padding::Pkcs7};
use sha1::Sha1;
use sha2::{Digest, Sha256};

type Encryptor = cbc::Encryptor<aes::Aes128>;
type Decryptor = cbc::Decryptor<aes::Aes128>;

fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

/// Derived from the TP-Link account credentials.
pub fn auth_hash(username: &str, password: &str) -> [u8; 32] {
    sha256(&[&Sha1::digest(username.as_bytes()), &Sha1::digest(password.as_bytes())])
}

/// Returned by the device in handshake1, proving it knows the same credentials.
pub fn server_hash(local_seed: &[u8], remote_seed: &[u8], auth: &[u8; 32]) -> [u8; 32] {
    sha256(&[local_seed, remote_seed, auth])
}

/// Sent by the client in handshake2.
pub fn client_hash(local_seed: &[u8], remote_seed: &[u8], auth: &[u8; 32]) -> [u8; 32] {
    sha256(&[remote_seed, local_seed, auth])
}

pub struct Session {
    key: [u8; 16],
    iv: [u8; 12],
    sig: [u8; 28],
    seq: i32,
}

impl Session {
    pub fn new(local_seed: &[u8], remote_seed: &[u8], auth: &[u8; 32]) -> Self {
        let derive = |prefix: &[u8]| sha256(&[prefix, local_seed, remote_seed, auth]);
        let full_iv = derive(b"iv");
        Self {
            key: derive(b"lsk")[..16].try_into().unwrap(),
            iv: full_iv[..12].try_into().unwrap(),
            sig: derive(b"ldk")[..28].try_into().unwrap(),
            seq: i32::from_be_bytes(full_iv[28..].try_into().unwrap()),
        }
    }

    /// Encrypts the next request. Returns the body to POST and its `seq` query parameter.
    pub fn encrypt(&mut self, plaintext: &[u8]) -> (Vec<u8>, i32) {
        self.seq = self.seq.wrapping_add(1);
        (self.seal(self.seq, plaintext), self.seq)
    }

    /// `signature (32 bytes) + AES-128-CBC ciphertext`, for message `seq`.
    pub fn seal(&self, seq: i32, plaintext: &[u8]) -> Vec<u8> {
        let ciphertext = Encryptor::new(&self.key.into(), &self.iv(seq).into()).encrypt_padded_vec::<Pkcs7>(plaintext);
        let mut message = self.signature(seq, &ciphertext).to_vec();
        message.extend_from_slice(&ciphertext);
        message
    }

    /// Decrypts message `seq`. The signature is not checked, like python-kasa does for responses.
    pub fn decrypt(&self, seq: i32, message: &[u8]) -> Result<Vec<u8>, String> {
        let ciphertext = message.get(32..).ok_or("encrypted message too short")?;
        Decryptor::new(&self.key.into(), &self.iv(seq).into())
            .decrypt_padded_vec::<Pkcs7>(ciphertext)
            .map_err(|_| "cannot decrypt message (bad key or padding)".to_string())
    }

    pub fn verify(&self, seq: i32, message: &[u8]) -> bool {
        message.len() >= 32 && message[..32] == self.signature(seq, &message[32..])
    }

    fn iv(&self, seq: i32) -> [u8; 16] {
        let mut iv = [0; 16];
        iv[..12].copy_from_slice(&self.iv);
        iv[12..].copy_from_slice(&seq.to_be_bytes());
        iv
    }

    fn signature(&self, seq: i32, ciphertext: &[u8]) -> [u8; 32] {
        sha256(&[&self.sig, &seq.to_be_bytes(), ciphertext])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCAL: [u8; 16] = [1; 16];
    const REMOTE: [u8; 16] = [2; 16];

    #[test]
    fn client_and_device_agree() {
        let auth = auth_hash("me@example.com", "secret");
        let mut client = Session::new(&LOCAL, &REMOTE, &auth);
        let device = Session::new(&LOCAL, &REMOTE, &auth);

        let (body, seq) = client.encrypt(br#"{"method":"get_device_info"}"#);
        assert!(device.verify(seq, &body));
        assert_eq!(device.decrypt(seq, &body).unwrap(), br#"{"method":"get_device_info"}"#);

        let reply = device.seal(seq, b"{}");
        assert_eq!(client.decrypt(seq, &reply).unwrap(), b"{}");
        assert!(!device.verify(seq + 1, &body), "the signature covers seq");
    }

    #[test]
    fn seq_increments_and_wraps() {
        let mut session = Session::new(&LOCAL, &REMOTE, &auth_hash("u", "p"));
        let (_, first) = session.encrypt(b"a");
        let (_, second) = session.encrypt(b"b");
        assert_eq!(second, first.wrapping_add(1));
        session.seq = i32::MAX;
        assert_eq!(session.encrypt(b"c").1, i32::MIN);
    }

    #[test]
    fn wrong_password_fails_the_handshake_check() {
        let device = server_hash(&LOCAL, &REMOTE, &auth_hash("u", "good"));
        assert_eq!(device, server_hash(&LOCAL, &REMOTE, &auth_hash("u", "good")));
        assert_ne!(device, server_hash(&LOCAL, &REMOTE, &auth_hash("u", "bad")));
        assert_ne!(client_hash(&LOCAL, &REMOTE, &auth_hash("u", "good")), device, "handshake2 hash differs from handshake1");
    }
}
