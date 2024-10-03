pub mod bitwork;
pub mod error;
pub mod types;

use std::mem::size_of;

use crate::error::DodError;
use base64::engine::general_purpose;
use base64::Engine;
use byteorder::{ByteOrder, LittleEndian};
use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};
use k256::sha2::digest::FixedOutput;
use k256::sha2::{Digest, Sha256};
use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};

struct BufferWriter {}

impl BufferWriter {
    fn varint_buf_num(n: i64) -> Vec<u8> {
        let mut buf = Vec::new();
        if n < 253 {
            buf.push(n as u8);
        } else if n < 0x10000 {
            buf.push(253);
            let mut bytes = [0u8; size_of::<u16>()];
            LittleEndian::write_u16(&mut bytes, n as u16);
            buf.extend_from_slice(&bytes);
        } else if n < 0x100000000 {
            buf.push(254);
            let mut bytes = [0u8; size_of::<u32>()];
            LittleEndian::write_u32(&mut bytes, n as u32);
            buf.extend_from_slice(&bytes);
        } else {
            buf.push(255);
            let mut bytes = [0u8; size_of::<u64>()];
            LittleEndian::write_i32(&mut bytes[0..4], (n & -1) as i32);
            LittleEndian::write_u32(&mut bytes[4..8], (n / 0x100000000) as u32);
            buf.extend_from_slice(&bytes);
        }
        buf
    }
}

const MAGIC_BYTES: &str = "DoD Signed Message:\n";

pub fn _msg_hash(message: String) -> Vec<u8> {
    let prefix1 = BufferWriter::varint_buf_num(MAGIC_BYTES.len() as i64);
    let message_buffer = message.as_bytes().to_vec();
    let prefix2 = BufferWriter::varint_buf_num(message_buffer.len() as i64);
    let mut buf = Vec::new();
    buf.extend_from_slice(&prefix1);
    buf.extend_from_slice(MAGIC_BYTES.as_bytes());
    buf.extend_from_slice(&prefix2);
    buf.extend_from_slice(&message_buffer);

    let _hash = Sha256::new_with_prefix(buf);
    let hash = Sha256::new_with_prefix(_hash.finalize_fixed().to_vec());
    hash.finalize_fixed().to_vec()
}

pub fn verify_message(
    message: String,
    signature: String,
    public_key: String,
) -> Result<Vec<u8>, String> {
    let message_prehashed = _msg_hash(message);
    let signature_bytes = general_purpose::STANDARD
        .decode(signature)
        .map_err(|_| "Invalid b64 signature".to_string())?;
    let public_key_bytes = hex::decode(public_key).map_err(|_| "Invalid public key".to_string())?;
    let recovered_public_key = recover_pub_key_compact(
        signature_bytes.as_slice(),
        message_prehashed.as_slice(),
        None,
    )?;

    if public_key_bytes.clone() != recovered_public_key.clone() {
        Err("public_key_bytes != recovered_public_key".to_string())
    } else {
        Ok(recovered_public_key.clone())
    }
}

pub fn recover_pub_key_compact(
    signature_bytes: &[u8],
    message_hash: &[u8],
    chain_id: Option<u8>,
) -> Result<Vec<u8>, String> {
    let mut v;
    let r: Vec<u8> = signature_bytes[1..33].to_vec();
    let mut s: Vec<u8> = signature_bytes[33..65].to_vec();

    if signature_bytes.len() >= 65 {
        v = signature_bytes[0];
    } else {
        v = signature_bytes[33] >> 7;
        s[0] &= 0x7f;
    };
    if v < 27 {
        v = v + 27;
    }

    let mut bytes = [0u8; 65];
    if r.len() > 32 || s.len() > 32 {
        return Err("Cannot create secp256k1 signature: malformed signature.".to_string());
    }
    let rid = calculate_sig_recovery(v.clone(), chain_id);
    bytes[0..32].clone_from_slice(&r);
    bytes[32..64].clone_from_slice(&s);
    bytes[64] = rid;

    if rid > 3 {
        return Err(format!(
            "Cannot create secp256k1 signature: invalid recovery id. {:?}",
            rid
        ));
    }

    let recovery_id = RecoveryId::try_from(bytes[64]).map_err(|_| DodError::InvalidRecoveryId)?;

    let signature = Signature::from_slice(&bytes[..64]).map_err(|_| DodError::InvalidSignature)?;

    let verifying_key = VerifyingKey::recover_from_prehash(&message_hash, &signature, recovery_id)
        .map_err(|_| DodError::PublicKeyRecoveryFailure)?;

    Ok(verifying_key.to_encoded_point(true).to_bytes().to_vec())
}

pub fn msg_hash(message: String) -> Vec<u8> {
    _msg_hash(message)
}

pub fn calculate_sig_recovery(mut v: u8, chain_id: Option<u8>) -> u8 {
    if v == 0 || v == 1 {
        return v;
    }

    if chain_id.is_none() {
        v = v - 27;
        while v > 3 {
            v = v - 4;
        }
        v
    } else {
        v = v - (chain_id.unwrap() * 2 + 35);
        while v > 3 {
            v = v - 4;
        }
        v
    }
}

pub fn fake_32() -> Vec<u8> {
    let mut r_bytes = [0u8; 32];
    let mut rd_vec = [0u8; 32];
    rd_vec[0..8].clone_from_slice(&ic_cdk::api::time().to_le_bytes());
    rd_vec[8..16].clone_from_slice(&ic_cdk::api::time().to_le_bytes());
    rd_vec[16..24].clone_from_slice(&ic_cdk::api::time().to_le_bytes());
    rd_vec[24..32].clone_from_slice(&ic_cdk::api::time().to_le_bytes());

    StdRng::from_seed(rd_vec).fill_bytes(&mut r_bytes);
    r_bytes.to_vec()
}

pub fn y_parity(prehash: &[u8], sig: &[u8], pubkey: &[u8]) -> u64 {
    let orig_key = VerifyingKey::from_sec1_bytes(pubkey).expect("failed to parse the pubkey");
    let signature = Signature::try_from(sig).unwrap();
    for parity in [0u8, 1] {
        let recid = RecoveryId::try_from(parity).unwrap();
        let recovered_key = VerifyingKey::recover_from_prehash(prehash, &signature, recid)
            .expect("failed to recover key");
        if recovered_key == orig_key {
            return u64::from(parity);
        }
    }

    panic!(
        "failed to recover the parity bit from a signature; sig: {}, pubkey: {}",
        hex::encode(sig),
        hex::encode(pubkey)
    )
}
